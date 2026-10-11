use std::collections::HashMap;

use crate::bytes::packed;
use crate::codes::{CanonProperty, LensDrive, LiveViewOutput, Operation, ResponseCode};
use crate::device_info::model_name;
use crate::error::{PtpError, Result};
use crate::events::{CameraEvent, parse_canon_events};
use crate::transport::{DataPhase, Transport};
use crate::viewfinder::jpeg_from_viewfinder;

/// What the camera has told us so far through its event stream.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CameraState {
    pub values: HashMap<u32, u32>,
    pub choices: HashMap<u32, Vec<u32>>,
    pub lens_name: Option<String>,
}

impl CameraState {
    pub fn value(&self, property: CanonProperty) -> Option<u32> {
        self.values.get(&property.code()).copied()
    }

    pub fn choices(&self, property: CanonProperty) -> &[u32] {
        self.choices.get(&property.code()).map_or(&[], Vec::as_slice)
    }

    pub fn apply(&mut self, events: &[CameraEvent]) {
        for event in events {
            match event {
                CameraEvent::PropertyChanged { property, value } => {
                    self.values.insert(*property, *value);
                }
                CameraEvent::TextPropertyChanged { property, text } => {
                    if *property == CanonProperty::LensName.code() {
                        self.lens_name = Some(text.clone());
                    }
                }
                CameraEvent::ChoicesChanged { property, choices } => {
                    self.choices.insert(*property, choices.clone());
                }
            }
        }
    }
}

/// A Canon EOS body in remote mode, speaking just enough PTP to stream live view and take orders.
pub struct CanonCamera<T: Transport> {
    transport: T,
    state: CameraState,
    model: String,
    live_view_output_before_us: Option<u32>,
}

impl<T: Transport> CanonCamera<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            state: CameraState::default(),
            model: "Canon camera".into(),
            live_view_output_before_us: None,
        }
    }

    pub fn state(&self) -> &CameraState {
        &self.state
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn connect(&mut self) -> Result<()> {
        self.transport.open_session()?;
        if let Some(name) = self.transport.model_name().or_else(|| {
            let info = self.transport.require(Operation::GetDeviceInfo, &[], DataPhase::Read).ok()?;
            model_name(&info.data)
        }) {
            self.model = name;
        }
        self.transport.require(Operation::CanonSetRemoteMode, &[1], DataPhase::None)?;
        self.transport.require(Operation::CanonSetEventMode, &[1], DataPhase::None)?;
        // The first GetEvent dumps every property and its allowed values
        self.poll_events()?;
        Ok(())
    }

    pub fn start_live_view(&mut self) -> Result<()> {
        self.live_view_output_before_us = self.state.value(CanonProperty::LiveViewOutput);
        if self.state.value(CanonProperty::LiveViewMode) != Some(1) {
            // Movie mode answers busy here but streams anyway, so busy is fine
            let payload = property_payload(CanonProperty::LiveViewMode, 1);
            let reply = self.transport.transact(Operation::CanonSetPropertyValue, &[], DataPhase::Write(&payload))?;
            if !reply.is_ok() && reply.code != ResponseCode::DeviceBusy.code() {
                return Err(PtpError::Response { code: reply.code, during: Operation::CanonSetPropertyValue.code() });
            }
        }
        // Computer only, so the rear screen goes dark and the sensor runs a bit cooler
        self.set(CanonProperty::LiveViewOutput, LiveViewOutput::COMPUTER)
    }

    /// One attempt at a frame. None means the camera hasn't produced a new one yet, so ask again shortly.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>> {
        let reply =
            self.transport.transact(Operation::CanonGetViewFinderData, &[0x0020_0000, 0, 0], DataPhase::Read)?;
        if reply.code == ResponseCode::CanonNotReady.code() || reply.code == ResponseCode::DeviceBusy.code() {
            return Ok(None);
        }
        if !reply.is_ok() {
            return Err(PtpError::Response { code: reply.code, during: Operation::CanonGetViewFinderData.code() });
        }
        Ok(jpeg_from_viewfinder(&reply.data).map(<[u8]>::to_vec))
    }

    pub fn poll_events(&mut self) -> Result<Vec<CameraEvent>> {
        let reply = self.transport.require(Operation::CanonGetEvent, &[], DataPhase::Read)?;
        let events = parse_canon_events(&reply.data);
        self.state.apply(&events);
        Ok(events)
    }

    pub fn set(&mut self, property: CanonProperty, value: u32) -> Result<()> {
        let payload = property_payload(property, value);
        self.transport.require(Operation::CanonSetPropertyValue, &[], DataPhase::Write(&payload))?;
        self.state.values.insert(property.code(), value);
        Ok(())
    }

    /// Stops the camera from powering itself off mid meeting.
    pub fn keep_awake(&mut self) -> Result<()> {
        self.transport.transact(Operation::CanonKeepDeviceOn, &[], DataPhase::None).map(|_| ())
    }

    pub fn autofocus(&mut self) -> Result<()> {
        self.transport.require(Operation::CanonDoAutofocus, &[], DataPhase::None)?;
        self.transport.transact(Operation::CanonCancelAutofocus, &[], DataPhase::None)?;
        Ok(())
    }

    pub fn drive_focus(&mut self, step: LensDrive) -> Result<()> {
        self.transport.require(Operation::CanonDriveLens, &[step as u32], DataPhase::None).map(|_| ())
    }

    /// Hands the camera back the way we found it. Errors are ignored since the cable may already be gone.
    pub fn disconnect(&mut self) {
        let output = self.live_view_output_before_us.unwrap_or(LiveViewOutput::CAMERA_SCREEN);
        let _ = self.set(CanonProperty::LiveViewOutput, output);
        let _ = self.transport.transact(Operation::CanonSetEventMode, &[0], DataPhase::None);
        let _ = self.transport.transact(Operation::CanonSetRemoteMode, &[0], DataPhase::None);
        let _ = self.transport.close_session();
    }
}

fn property_payload(property: CanonProperty, value: u32) -> Vec<u8> {
    packed(&[12, property.code(), value])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::ContainerSession;
    use crate::container::tests::FakeCable;

    fn jpeg_blob(jpeg: &[u8]) -> Vec<u8> {
        let mut blob = packed(&[(jpeg.len() + 8) as u32, 1]);
        blob.extend(jpeg);
        blob
    }

    #[test]
    fn connecting_puts_the_camera_in_remote_mode_and_reads_its_state() {
        let mut cable = FakeCable::default();
        cable.answer(ResponseCode::Ok, None); // OpenSession
        cable.answer(ResponseCode::GeneralError, None); // GetDeviceInfo is optional
        cable.answer(ResponseCode::Ok, None); // SetRemoteMode
        cable.answer(ResponseCode::Ok, None); // SetEventMode
        cable.answer(ResponseCode::Ok, Some(packed(&[16, 0xC189, 0xD1B1, 1, 8, 0]))); // GetEvent
        let mut camera = CanonCamera::new(ContainerSession::new(cable));
        camera.connect().unwrap();
        assert_eq!(camera.state().value(CanonProperty::LiveViewMode), Some(1));
        assert_eq!(camera.model(), "Canon camera");
    }

    #[test]
    fn not_ready_means_try_again_not_an_error() {
        let mut cable = FakeCable::default();
        cable.answer(ResponseCode::CanonNotReady, None);
        let mut camera = CanonCamera::new(ContainerSession::new(cable));
        assert_eq!(camera.next_frame().unwrap(), None);
    }

    #[test]
    fn a_frame_comes_back_as_bare_jpeg() {
        let mut cable = FakeCable::default();
        cable.answer(ResponseCode::Ok, Some(jpeg_blob(&[0xFF, 0xD8, 0xFF, 0xD9])));
        let mut camera = CanonCamera::new(ContainerSession::new(cable));
        assert_eq!(camera.next_frame().unwrap(), Some(vec![0xFF, 0xD8, 0xFF, 0xD9]));
    }

    #[test]
    fn movie_mode_busy_still_starts_live_view() {
        let mut cable = FakeCable::default();
        cable.answer(ResponseCode::DeviceBusy, None); // set live view mode
        cable.answer(ResponseCode::Ok, None); // set output to computer
        let mut camera = CanonCamera::new(ContainerSession::new(cable));
        camera.start_live_view().unwrap();
        assert_eq!(camera.state().value(CanonProperty::LiveViewOutput), Some(LiveViewOutput::COMPUTER));
    }
}
