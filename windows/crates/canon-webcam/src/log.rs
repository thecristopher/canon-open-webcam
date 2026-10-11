use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 1024 * 1024;

/// %LOCALAPPDATA%\CanonWebcam, where the app keeps its log.
pub fn folder() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map_or_else(std::env::temp_dir, PathBuf::from);
    base.join("CanonWebcam")
}

pub fn path() -> PathBuf {
    folder().join("canon-webcam.log")
}

pub fn write(message: &str) {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let line = format!("[{seconds}] {message}\n");
    if crate::console::attached() {
        eprint!("{line}");
    }
    let _ = std::fs::create_dir_all(folder());
    let too_big = std::fs::metadata(path()).is_ok_and(|m| m.len() > MAX_BYTES);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(!too_big)
        .write(true)
        .truncate(too_big)
        .open(path());
    if let Ok(mut file) = file {
        let _ = file.write_all(line.as_bytes());
    }
}

macro_rules! log {
    ($($arg:tt)*) => { $crate::log::write(&format!($($arg)*)) };
}
pub(crate) use log;
