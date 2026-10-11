use std::sync::Mutex;
use std::time::{Duration, Instant};

use canon_shm::FRAME_BYTES;
use windows::Win32::Foundation::S_OK;
use windows::Win32::Media::KernelStreaming::{IKsControl, IKsControl_Impl, KSIDENTIFIER};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows_core::{ComObject, GUID, IUnknown, Ref, implement};

use crate::feed::Feed;
use crate::log::log;
use crate::source::ks_not_found;

// In "every camera frame" mode, never wait longer than this for one, so apps don't think we froze
const LONGEST_WAIT: Duration = Duration::from_millis(100);
// ...and never hand them out faster than 60 a second
const SHORTEST_GAP: Duration = Duration::from_nanos(1_000_000_000 / 60);

#[implement(IMFMediaStream2, IKsControl)]
pub struct MediaStream {
    inner: Mutex<Inner>,
    feed: Mutex<Feed>,
    next_due: Mutex<Option<Instant>>,
}

struct Inner {
    queue: Option<IMFMediaEventQueue>,
    // Strong on purpose so GetMediaSource works; Shutdown breaks the cycle
    source: Option<IMFMediaSource>,
    descriptor: IMFStreamDescriptor,
    state: MF_STREAM_STATE,
}

impl MediaStream {
    pub fn create(source: IMFMediaSource, descriptor: IMFStreamDescriptor) -> windows_core::Result<ComObject<Self>> {
        let queue = unsafe { MFCreateEventQueue()? };
        Ok(ComObject::new(Self {
            inner: Mutex::new(Inner {
                queue: Some(queue),
                source: Some(source),
                descriptor,
                state: MF_STREAM_STATE_STOPPED,
            }),
            feed: Mutex::new(Feed::new()),
            next_due: Mutex::new(None),
        }))
    }

    pub fn descriptor(&self) -> IMFStreamDescriptor {
        self.inner.lock().unwrap().descriptor.clone()
    }

    pub fn start(&self, position: *const PROPVARIANT) -> windows_core::Result<()> {
        let queue = self.set_state(MF_STREAM_STATE_RUNNING)?;
        unsafe { queue.QueueEventParamVar(MEStreamStarted.0 as u32, &GUID::zeroed(), S_OK, position) }
    }

    pub fn stop(&self) -> windows_core::Result<()> {
        let queue = self.set_state(MF_STREAM_STATE_STOPPED)?;
        unsafe { queue.QueueEventParamVar(MEStreamStopped.0 as u32, &GUID::zeroed(), S_OK, std::ptr::null()) }
    }

    pub fn shutdown(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.source = None;
        if let Some(queue) = inner.queue.take() {
            let _ = unsafe { queue.Shutdown() };
        }
    }

    fn set_state(&self, state: MF_STREAM_STATE) -> windows_core::Result<IMFMediaEventQueue> {
        let mut inner = self.inner.lock().unwrap();
        inner.state = state;
        inner.queue.clone().ok_or_else(|| MF_E_SHUTDOWN.into())
    }

    fn queue(&self) -> windows_core::Result<IMFMediaEventQueue> {
        self.inner.lock().unwrap().queue.clone().ok_or_else(|| MF_E_SHUTDOWN.into())
    }

    /// Waits until the next frame is due: a steady beat at 30 or 60, or the moment the camera's next frame lands.
    /// Returns how long this frame will be on screen, in 100ns units, for the sample's duration.
    fn pace(&self) -> i64 {
        let rate = self.feed.lock().unwrap().frame_rate();
        let mut next_due = self.next_due.lock().unwrap();
        let now = Instant::now();
        if rate == 0 {
            let earliest = next_due.unwrap_or(now);
            let deadline = now + LONGEST_WAIT;
            while Instant::now() < deadline
                && (Instant::now() < earliest || !self.feed.lock().unwrap().fresh_frame_waiting())
            {
                std::thread::sleep(Duration::from_millis(2));
            }
            let delivered = Instant::now();
            *next_due = Some(delivered + SHORTEST_GAP);
            return (delivered - now).max(SHORTEST_GAP).as_nanos() as i64 / 100;
        }
        let interval = Duration::from_nanos(1_000_000_000 / u64::from(rate));
        let due = next_due.filter(|due| *due > now - interval).unwrap_or(now);
        if due > now {
            std::thread::sleep(due - now);
        }
        *next_due = Some(due + interval);
        interval.as_nanos() as i64 / 100
    }

    fn sample(&self, duration: i64) -> windows_core::Result<IMFSample> {
        unsafe {
            let buffer = MFCreateMemoryBuffer(FRAME_BYTES as u32)?;
            let mut data = std::ptr::null_mut();
            buffer.Lock(&mut data, None, None)?;
            {
                let mut feed = self.feed.lock().unwrap();
                std::ptr::copy_nonoverlapping(feed.next().as_ptr(), data, FRAME_BYTES);
            }
            buffer.Unlock()?;
            buffer.SetCurrentLength(FRAME_BYTES as u32)?;

            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(MFGetSystemTime())?;
            sample.SetSampleDuration(duration)?;
            Ok(sample)
        }
    }
}

impl IMFMediaEventGenerator_Impl for MediaStream_Impl {
    fn GetEvent(&self, flags: MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS) -> windows_core::Result<IMFMediaEvent> {
        unsafe { self.queue()?.GetEvent(flags.0) }
    }
    fn BeginGetEvent(&self, callback: Ref<IMFAsyncCallback>, state: Ref<IUnknown>) -> windows_core::Result<()> {
        unsafe { self.queue()?.BeginGetEvent(callback.as_ref(), state.as_ref()) }
    }
    fn EndGetEvent(&self, result: Ref<IMFAsyncResult>) -> windows_core::Result<IMFMediaEvent> {
        unsafe { self.queue()?.EndGetEvent(result.as_ref()) }
    }
    fn QueueEvent(
        &self,
        met: u32,
        extended: *const GUID,
        status: windows_core::HRESULT,
        value: *const PROPVARIANT,
    ) -> windows_core::Result<()> {
        unsafe { self.queue()?.QueueEventParamVar(met, extended, status, value) }
    }
}

impl IMFMediaStream_Impl for MediaStream_Impl {
    fn GetMediaSource(&self) -> windows_core::Result<IMFMediaSource> {
        self.inner.lock().unwrap().source.clone().ok_or_else(|| MF_E_SHUTDOWN.into())
    }

    fn GetStreamDescriptor(&self) -> windows_core::Result<IMFStreamDescriptor> {
        let inner = self.inner.lock().unwrap();
        inner.queue.as_ref().ok_or(MF_E_SHUTDOWN)?;
        Ok(inner.descriptor.clone())
    }

    fn RequestSample(&self, token: Ref<IUnknown>) -> windows_core::Result<()> {
        {
            let inner = self.inner.lock().unwrap();
            inner.queue.as_ref().ok_or(MF_E_SHUTDOWN)?;
            if inner.state != MF_STREAM_STATE_RUNNING {
                return Err(MF_E_MEDIA_SOURCE_WRONGSTATE.into());
            }
        }
        let duration = self.pace();
        let sample = self.sample(duration).inspect_err(|e| log!("couldn't build a sample: {e}"))?;
        unsafe {
            if let Some(token) = token.as_ref() {
                sample.SetUnknown(&MFSampleExtension_Token, token)?;
            }
            self.queue()?.QueueEventParamUnk(MEMediaSample.0 as u32, &GUID::zeroed(), S_OK, &sample)
        }
    }
}

impl IMFMediaStream2_Impl for MediaStream_Impl {
    fn SetStreamState(&self, state: MF_STREAM_STATE) -> windows_core::Result<()> {
        self.set_state(state).map(|_| ())
    }

    fn GetStreamState(&self) -> windows_core::Result<MF_STREAM_STATE> {
        Ok(self.inner.lock().unwrap().state)
    }
}

impl IKsControl_Impl for MediaStream_Impl {
    fn KsProperty(
        &self,
        _: *const KSIDENTIFIER,
        _: u32,
        _: *mut std::ffi::c_void,
        _: u32,
        _: *mut u32,
    ) -> windows_core::Result<()> {
        Err(ks_not_found())
    }
    fn KsMethod(
        &self,
        _: *const KSIDENTIFIER,
        _: u32,
        _: *mut std::ffi::c_void,
        _: u32,
        _: *mut u32,
    ) -> windows_core::Result<()> {
        Err(ks_not_found())
    }
    fn KsEvent(
        &self,
        _: *const KSIDENTIFIER,
        _: u32,
        _: *mut std::ffi::c_void,
        _: u32,
        _: *mut u32,
    ) -> windows_core::Result<()> {
        Err(ks_not_found())
    }
}
