//! How the CLI and the window reach the running engine: one JSON object per line over a named pipe.
//! Same requests, responses and status as the Mac app's control socket.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::io::FromRawHandle;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use canon_ptp::{CameraSetting, LensDrive};
use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::{ERROR_PIPE_CONNECTED, GetLastError, HANDLE};
use windows::Win32::Storage::FileSystem::{FlushFileBuffers, PIPE_ACCESS_DUPLEX};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_core::HSTRING;

use crate::options::{ADJUSTABLE_NAMES, FrameOptions};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlRequest {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

impl ControlRequest {
    pub fn status() -> Self {
        Self { command: "status".into(), name: None, value: None }
    }

    pub fn set(name: &str, value: &str) -> Self {
        Self { command: "set".into(), name: Some(name.into()), value: Some(value.into()) }
    }

    pub fn focus(action: &str, steps: u32) -> Self {
        Self { command: "focus".into(), name: Some(action.into()), value: Some(steps.to_string()) }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ControlResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<EngineStatus>,
}

impl ControlResponse {
    pub fn success(message: Option<String>, status: Option<EngineStatus>) -> Self {
        Self { ok: true, message, status }
    }

    pub fn failure(message: impl Into<String>) -> Self {
        Self { ok: false, message: Some(message.into()), status: None }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingStatus {
    pub value: String,
    pub choices: Vec<String>,
}

/// Where the engine is at, in a form an app can switch on instead of parsing English.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PhaseKind {
    Idle,
    WaitingForCamera,
    Starting,
    Live,
    Trouble,
}

/// A snapshot of the engine for `CanonWebcam status`, the window and the tray.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub state: String,
    pub kind: PhaseKind,
    pub trouble: Option<String>,
    pub model: Option<String>,
    pub lens: Option<String>,
    pub frames_per_second: f64,
    pub apps_watching: bool,
    pub options: FrameOptions,
    pub settings: BTreeMap<String, SettingStatus>,
}

/// The work a control request turns into, run on the camera thread between frames.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineCommand {
    SetCamera(CameraSetting, String),
    SetOption(String, String),
    Autofocus,
    Focus(LensDrive),
}

impl EngineCommand {
    /// Turns a request into a command, or explains what's wrong with it.
    pub fn from_request(request: &ControlRequest) -> Result<Self, String> {
        let name = request.name.clone().unwrap_or_default();
        let value = request.value.clone().unwrap_or_default();
        match request.command.as_str() {
            "set" if ADJUSTABLE_NAMES.contains(&name.as_str()) => {
                // Check it against defaults now, so a typo fails here instead of on the camera thread
                FrameOptions::default().applying(&name, &value)?;
                Ok(Self::SetOption(name, value))
            }
            "set" => {
                let setting = CameraSetting::from_name(&name).ok_or_else(|| {
                    let names: Vec<&str> =
                        CameraSetting::ALL.iter().map(|s| s.name()).chain(ADJUSTABLE_NAMES).collect();
                    format!("\"{name}\" isn't a setting. Try one of: {}", names.join(", "))
                })?;
                Ok(Self::SetCamera(setting, value))
            }
            "focus" => {
                // Canon only knows three step sizes, so 1, 2 and 3 pick small, medium and large
                let size = value.parse::<usize>().unwrap_or(1).clamp(1, 3) - 1;
                match name.as_str() {
                    "" | "auto" => Ok(Self::Autofocus),
                    "near" => {
                        Ok(Self::Focus([LensDrive::NearSmall, LensDrive::NearMedium, LensDrive::NearLarge][size]))
                    }
                    "far" => Ok(Self::Focus([LensDrive::FarSmall, LensDrive::FarMedium, LensDrive::FarLarge][size])),
                    _ => Err("focus takes auto, near or far".into()),
                }
            }
            other => Err(format!("I don't know the command \"{other}\"")),
        }
    }
}

/// One pipe per Windows session, so two people signed in at once each talk to their own engine.
fn pipe_name() -> String {
    let mut session = 0u32;
    unsafe {
        let _ = ProcessIdToSessionId(std::process::id(), &mut session);
    }
    format!(r"\\.\pipe\CanonWebcam-{session}")
}

pub type Handler = Arc<dyn Fn(ControlRequest) -> ControlResponse + Send + Sync>;

/// Serves the pipe on background threads for as long as the process lives.
pub fn serve(handler: Handler) -> Result<()> {
    serve_on(&pipe_name(), handler)
}

fn serve_on(name: &str, handler: Handler) -> Result<()> {
    let name = HSTRING::from(name);
    // Make the first instance now, so a failure shows up here instead of silently on a thread
    // Handles aren't Send, so the first one crosses into the thread as a plain number
    let first = create_instance(&name)?.0 as isize;
    std::thread::Builder::new().name("control pipe".into()).spawn(move || {
        let mut next = Some(HANDLE(first as _));
        loop {
            let instance = match next.take().map_or_else(|| create_instance(&name), Ok) {
                Ok(instance) => instance,
                Err(e) => {
                    crate::log::log!("control pipe stopped: {e:#}");
                    return;
                }
            };
            let connected = unsafe { ConnectNamedPipe(instance, None) };
            if connected.is_err() && unsafe { GetLastError() } != ERROR_PIPE_CONNECTED {
                drop(unsafe { std::fs::File::from_raw_handle(instance.0) });
                continue;
            }
            let handler = handler.clone();
            let raw = instance.0 as isize;
            // Each client on its own thread, so a slow camera command never holds up status polling
            std::thread::spawn(move || serve_client(HANDLE(raw as _), &handler));
        }
    })?;
    Ok(())
}

fn create_instance(name: &HSTRING) -> Result<HANDLE> {
    let handle = unsafe {
        CreateNamedPipeW(
            name,
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            64 * 1024,
            64 * 1024,
            0,
            None,
        )
    };
    if handle.is_invalid() {
        return Err(windows_core::Error::from_thread()).context("couldn't open the control pipe");
    }
    Ok(handle)
}

fn serve_client(pipe: HANDLE, handler: &Handler) {
    let file = unsafe { std::fs::File::from_raw_handle(pipe.0) };
    let mut reader = BufReader::new(&file);
    let mut line = String::new();
    let response = match reader.read_line(&mut line) {
        Ok(_) => match serde_json::from_str::<ControlRequest>(&line) {
            Ok(request) => handler(request),
            Err(_) => ControlResponse::failure("couldn't read that request"),
        },
        Err(_) => return,
    };
    if let Ok(mut reply) = serde_json::to_vec(&response) {
        reply.push(b'\n');
        let mut writer = &file;
        let _ = writer.write_all(&reply);
        unsafe {
            let _ = FlushFileBuffers(pipe);
            let _ = DisconnectNamedPipe(pipe);
        }
    }
}

/// Sends one request to the running engine and waits for its answer.
pub fn send(request: &ControlRequest) -> Result<ControlResponse> {
    send_to(&pipe_name(), request)
}

fn send_to(name: &str, request: &ControlRequest) -> Result<ControlResponse> {
    let pipe = std::fs::OpenOptions::new().read(true).write(true).open(name).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            anyhow!("Canon Webcam isn't running. Start it from the Start menu, or run `CanonWebcam start`.")
        }
        _ => anyhow!("couldn't reach Canon Webcam: {e}"),
    })?;
    let mut line = serde_json::to_vec(request)?;
    line.push(b'\n');
    (&pipe).write_all(&line)?;
    let mut reply = String::new();
    BufReader::new(&pipe).read_line(&mut reply)?;
    serde_json::from_str(&reply).context("no sensible reply from Canon Webcam")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_use_the_mac_wire_format() {
        let json = serde_json::to_string(&ControlRequest::set("iso", "400")).unwrap();
        assert_eq!(json, r#"{"command":"set","name":"iso","value":"400"}"#);
    }

    #[test]
    fn picture_options_become_option_commands() {
        let command = EngineCommand::from_request(&ControlRequest::set("brightness", "0.2")).unwrap();
        assert_eq!(command, EngineCommand::SetOption("brightness".into(), "0.2".into()));
    }

    #[test]
    fn a_bad_option_value_fails_before_reaching_the_camera() {
        assert!(EngineCommand::from_request(&ControlRequest::set("zoom", "9")).is_err());
    }

    #[test]
    fn camera_settings_become_camera_commands() {
        let command = EngineCommand::from_request(&ControlRequest::set("white-balance", "daylight")).unwrap();
        assert_eq!(command, EngineCommand::SetCamera(CameraSetting::WhiteBalance, "daylight".into()));
    }

    #[test]
    fn unknown_settings_list_what_exists() {
        let error = EngineCommand::from_request(&ControlRequest::set("vibes", "on")).unwrap_err();
        assert!(error.contains("iso") && error.contains("brightness"), "{error}");
    }

    #[test]
    fn focus_steps_pick_canons_three_sizes() {
        assert_eq!(
            EngineCommand::from_request(&ControlRequest::focus("near", 2)).unwrap(),
            EngineCommand::Focus(LensDrive::NearMedium)
        );
        assert_eq!(
            EngineCommand::from_request(&ControlRequest::focus("far", 9)).unwrap(),
            EngineCommand::Focus(LensDrive::FarLarge)
        );
        assert_eq!(EngineCommand::from_request(&ControlRequest::focus("auto", 1)).unwrap(), EngineCommand::Autofocus);
    }

    #[test]
    fn a_request_and_reply_cross_the_pipe() {
        let handler: Handler =
            Arc::new(|request| ControlResponse::success(Some(format!("got {}", request.command)), None));
        // Its own pipe, so the test never talks to a Canon Webcam that's actually running
        let name = format!(r"\\.\pipe\CanonWebcam-test-{}", std::process::id());
        serve_on(&name, handler).unwrap();
        let reply = send_to(&name, &ControlRequest::status()).unwrap();
        assert_eq!(reply.message.as_deref(), Some("got status"));
    }
}
