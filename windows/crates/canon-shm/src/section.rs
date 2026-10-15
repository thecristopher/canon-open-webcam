use std::sync::atomic::{AtomicU32, AtomicU64, Ordering, fence};

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HLOCAL, INVALID_HANDLE_VALUE, LocalFree,
};
use windows::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::System::Memory::{
    CreateFileMappingW, FILE_MAP_READ, FILE_MAP_WRITE, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, OpenFileMappingW,
    PAGE_READWRITE, UnmapViewOfFile,
};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::core::w;

use crate::{FRAME_BYTES, FRAME_OFFSET, HEIGHT, MAGIC, SECTION_BYTES, VERSION, WIDTH};

// SYSTEM, LOCAL SERVICE (Frame Server) and admins get everything, people signed in at the console read and write.
// So both sides ask for read and write only, never all access, or the user's side gets turned away.
const SDDL: windows::core::PCWSTR = w!("D:(A;;GA;;;SY)(A;;GA;;;LS)(A;;GA;;;BA)(A;;GRGW;;;IU)");
const NAME: windows::core::PCWSTR = w!("Global\\CanonWebcamFrames");

/// Milliseconds since boot, the same clock in every session.
pub fn tick_ms() -> u64 {
    unsafe { GetTickCount64() }
}

#[repr(C)]
struct Header {
    magic: AtomicU32,
    version: AtomicU32,
    width: AtomicU32,
    height: AtomicU32,
    /// Seqlock: odd while the engine is writing a frame, bumped to even when it's done
    sequence: AtomicU64,
    /// When an app last asked the virtual camera for a frame
    consumer_tick: AtomicU64,
    /// When the engine last wrote anything, so the camera side can tell it's alive
    writer_tick: AtomicU64,
    /// Frames per second the person picked: 30, 60, or 0 for every camera frame as it arrives
    frame_rate: AtomicU32,
}

pub struct SharedFrames {
    handle: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    created: bool,
}

// The view is plain shared memory guarded by atomics, so handing it between threads is fine
unsafe impl Send for SharedFrames {}
unsafe impl Sync for SharedFrames {}

impl SharedFrames {
    /// Opens the section if it's there, creates it if we're allowed to.
    pub fn open_or_create() -> windows::core::Result<Self> {
        Self::open().or_else(|_| Self::create())
    }

    /// Opens the section only if the other side already made it.
    pub fn open() -> windows::core::Result<Self> {
        unsafe {
            let handle = OpenFileMappingW((FILE_MAP_READ | FILE_MAP_WRITE).0, false, NAME)?;
            Self::map(handle, false)
        }
    }

    /// Makes the section, which takes admin or a service.
    pub fn create() -> windows::core::Result<Self> {
        unsafe {
            let mut descriptor = PSECURITY_DESCRIPTOR::default();
            ConvertStringSecurityDescriptorToSecurityDescriptorW(SDDL, SDDL_REVISION_1, &mut descriptor, None)?;
            let attributes = SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor.0,
                bInheritHandle: false.into(),
            };
            let handle = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                Some(&attributes),
                PAGE_READWRITE,
                0,
                SECTION_BYTES as u32,
                NAME,
            );
            let raced = GetLastError() == ERROR_ALREADY_EXISTS;
            LocalFree(Some(HLOCAL(descriptor.0)));
            Self::map(handle?, !raced)
        }
    }

    unsafe fn map(handle: HANDLE, created: bool) -> windows::core::Result<Self> {
        let view = unsafe { MapViewOfFile(handle, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, SECTION_BYTES) };
        if view.Value.is_null() {
            let error = windows::core::Error::from_thread();
            unsafe { CloseHandle(handle)? };
            return Err(error);
        }
        let frames = Self { handle, view, created };
        let header = frames.header();
        if created || header.magic.load(Ordering::Acquire) != MAGIC {
            header.width.store(WIDTH, Ordering::Relaxed);
            header.height.store(HEIGHT, Ordering::Relaxed);
            header.version.store(VERSION, Ordering::Relaxed);
            header.magic.store(MAGIC, Ordering::Release);
        }
        Ok(frames)
    }

    /// True when this side made the section rather than finding it.
    pub fn created(&self) -> bool {
        self.created
    }

    fn header(&self) -> &Header {
        unsafe { &*(self.view.Value as *const Header) }
    }

    fn frame_ptr(&self) -> *mut u8 {
        unsafe { (self.view.Value as *mut u8).add(FRAME_OFFSET) }
    }

    /// Engine side: publish one NV12 frame.
    pub fn write_frame(&self, nv12: &[u8]) {
        assert_eq!(nv12.len(), FRAME_BYTES, "frame must be {WIDTH}x{HEIGHT} NV12");
        let header = self.header();
        let start = header.sequence.load(Ordering::Relaxed);
        let writing = start | 1;
        header.sequence.store(writing, Ordering::Relaxed);
        fence(Ordering::Release);
        unsafe { std::ptr::copy_nonoverlapping(nv12.as_ptr(), self.frame_ptr(), FRAME_BYTES) };
        header.sequence.store(writing + 1, Ordering::Release);
        header.writer_tick.store(tick_ms(), Ordering::Release);
    }

    /// Camera side: copy the newest frame into `out` if it changed since `last_sequence`.
    /// Returns the sequence of the frame now in `out`, or None if there was nothing newer.
    pub fn read_frame(&self, last_sequence: u64, out: &mut [u8]) -> Option<u64> {
        let header = self.header();
        for _ in 0..3 {
            let before = header.sequence.load(Ordering::Acquire);
            if before == last_sequence || before & 1 == 1 || before == 0 {
                if before & 1 == 1 {
                    std::thread::yield_now();
                    continue;
                }
                return None;
            }
            unsafe { std::ptr::copy_nonoverlapping(self.frame_ptr(), out.as_mut_ptr(), FRAME_BYTES.min(out.len())) };
            fence(Ordering::Acquire);
            if header.sequence.load(Ordering::Relaxed) == before {
                return Some(before);
            }
        }
        None
    }

    /// The newest frame's sequence, so the camera side can tell a fresh frame is waiting without copying it.
    pub fn sequence(&self) -> u64 {
        self.header().sequence.load(Ordering::Acquire)
    }

    /// Engine side: how fast the virtual camera should hand frames to apps.
    pub fn set_frame_rate(&self, frames_per_second: u32) {
        self.header().frame_rate.store(frames_per_second, Ordering::Release);
    }

    pub fn frame_rate(&self) -> u32 {
        self.header().frame_rate.load(Ordering::Acquire)
    }

    /// Camera side: an app just asked for a frame.
    pub fn touch_consumer(&self) {
        self.header().consumer_tick.store(tick_ms(), Ordering::Release);
    }

    /// How long since an app last asked for a frame, or None if none ever did.
    pub fn consumer_idle_ms(&self) -> Option<u64> {
        age(self.header().consumer_tick.load(Ordering::Acquire))
    }

    /// How long since the engine last wrote a frame, or None if it never has.
    pub fn writer_idle_ms(&self) -> Option<u64> {
        age(self.header().writer_tick.load(Ordering::Acquire))
    }
}

fn age(stamp: u64) -> Option<u64> {
    (stamp != 0).then(|| tick_ms().saturating_sub(stamp))
}

impl Drop for SharedFrames {
    fn drop(&mut self) {
        unsafe {
            let _ = UnmapViewOfFile(self.view);
            let _ = CloseHandle(self.handle);
        }
    }
}
