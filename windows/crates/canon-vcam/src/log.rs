use std::io::Write;

use windows::Win32::System::Diagnostics::Debug::OutputDebugStringW;
use windows_core::HSTRING;

// The installer creates this folder and lets LOCAL SERVICE write to it
const LOG_PATH: &str = r"C:\ProgramData\CanonWebcam\camera.log";
const MAX_BYTES: u64 = 512 * 1024;

/// Best effort logging from inside the camera service: the debugger always, the file when allowed.
pub fn write(message: &str) {
    let line = format!("[{}] {message}\n", canon_shm::tick_ms());
    unsafe { OutputDebugStringW(&HSTRING::from(format!("CanonWebcam: {line}"))) };
    let too_big = std::fs::metadata(LOG_PATH).is_ok_and(|m| m.len() > MAX_BYTES);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(!too_big)
        .write(true)
        .truncate(too_big)
        .open(LOG_PATH);
    if let Ok(mut file) = file {
        let _ = file.write_all(line.as_bytes());
    }
}

macro_rules! log {
    ($($arg:tt)*) => { $crate::log::write(&format!($($arg)*)) };
}
pub(crate) use log;
