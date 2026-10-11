use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

/// Joins the calling thread to the multithreaded COM apartment until dropped.
/// WPD objects are free threaded, so one per thread that touches the camera is enough.
pub struct ComApartment(());

impl ComApartment {
    pub fn enter() -> windows::core::Result<Self> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
        Ok(Self(()))
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}
