//! Just enough PTP to stream live view from a Canon EOS body and take orders.
//!
//! Nothing here touches an OS. How bytes reach the camera is a [`Transport`]: Windows'
//! WPD driver on Windows, or raw PTP containers over USB anywhere else ([`ContainerSession`]).

mod bytes;
mod camera;
mod codes;
mod container;
mod device_info;
mod error;
mod events;
mod settings;
mod transport;
mod viewfinder;

pub use camera::{CameraState, CanonCamera};
pub use codes::{CanonEvent, CanonProperty, ContainerType, LensDrive, LiveViewOutput, Operation, ResponseCode};
pub use container::{Container, ContainerIo, ContainerSession};
pub use device_info::model_name;
pub use error::{PtpError, Result};
pub use events::{CameraEvent, parse_canon_events};
pub use settings::CameraSetting;
pub use transport::{DataPhase, Reply, Transport};
pub use viewfinder::jpeg_from_viewfinder;
