use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONINFORMATION, MB_ICONWARNING, MB_OK, MessageBoxW};
use windows_core::HSTRING;

// The exe is a GUI app so double clicking it doesn't flash a console.
// Run from a terminal, it borrows that terminal so the commands can still print.
static ATTACHED: AtomicBool = AtomicBool::new(false);

pub fn attach() {
    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }.is_ok() {
        ATTACHED.store(true, Ordering::Relaxed);
    }
}

pub fn attached() -> bool {
    ATTACHED.load(Ordering::Relaxed)
}

/// Prints when there's a terminal, shows a message box when there isn't.
pub fn tell(text: &str, quiet: bool) {
    if attached() {
        println!("{text}");
    }
    if !quiet && !attached() {
        unsafe { MessageBoxW(None, &HSTRING::from(text), &HSTRING::from("Canon Webcam"), MB_OK | MB_ICONINFORMATION) };
    }
}

pub fn warn(text: &str, quiet: bool) {
    if attached() {
        eprintln!("Error: {text}");
    }
    if !quiet && !attached() {
        unsafe { MessageBoxW(None, &HSTRING::from(text), &HSTRING::from("Canon Webcam"), MB_OK | MB_ICONWARNING) };
    }
}
