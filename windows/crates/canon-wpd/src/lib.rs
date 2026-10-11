//! PTP over Windows Portable Devices.
//!
//! Windows binds its own MTP driver to a Canon when it's plugged in. That driver opens the PTP
//! session and frames every container itself, but it will pass vendor operations through
//! untouched (the MTP vendor extension commands). Canon's own EDSDK reaches the camera the same
//! way, so nobody has to swap drivers with Zadig.

#[cfg(windows)]
mod com;
#[cfg(windows)]
mod device;

#[cfg(windows)]
pub use com::ComApartment;
#[cfg(windows)]
pub use device::{DeviceEntry, WpdTransport, list_devices};
