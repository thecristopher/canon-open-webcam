use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::control::{self, ControlRequest, EngineStatus, SettingStatus};
use crate::options::FrameOptions;
use crate::panel::PanelState;

// Fast enough to look live while dragging, slow enough not to swamp the engine
const SEND_INTERVAL: Duration = Duration::from_millis(80);
// While you're dragging, the engine's older idea of the options shouldn't yank the slider back
const LOCAL_EDIT_WINS_FOR: Duration = Duration::from_millis(1500);

/// A camera setting the way the inspector shows it.
#[derive(Clone, Debug, PartialEq)]
pub enum SettingControl {
    Unavailable,
    LockedByMode(String),
    Choices(Vec<String>, String),
}

impl SettingControl {
    fn from(setting: Option<&SettingStatus>) -> Self {
        match setting {
            None => SettingControl::Unavailable,
            Some(s) if s.choices.is_empty() => SettingControl::LockedByMode(s.value.clone()),
            Some(s) => SettingControl::Choices(s.choices.clone(), s.value.clone()),
        }
    }
}

struct Shared {
    status: Option<EngineStatus>,
    problem: Option<String>,
}

/// The window's view of the engine: polls its status, and turns slider drags into a calm stream of changes.
pub struct CameraModel {
    shared: Arc<Mutex<Shared>>,
    requests: Sender<ControlRequest>,
    /// What the controls show. Changes land here first, so dragging feels instant even before the engine answers.
    pub options: FrameOptions,
    last_local_edit: Option<Instant>,
    last_sent: HashMap<String, Instant>,
    pending: HashMap<String, String>,
}

impl CameraModel {
    pub fn new(ctx: eframe::egui::Context) -> Self {
        let shared = Arc::new(Mutex::new(Shared { status: None, problem: None }));
        let (requests, inbox) = channel();
        let worker = shared.clone();
        let repaint = ctx.clone();
        std::thread::Builder::new().name("engine talk".into()).spawn(move || talk(inbox, &worker, &repaint)).ok();
        let poller = requests.clone();
        std::thread::Builder::new()
            .name("status poll".into())
            .spawn(move || {
                while poller.send(ControlRequest::status()).is_ok() {
                    std::thread::sleep(Duration::from_secs(1));
                }
            })
            .ok();
        Self {
            shared,
            requests,
            options: FrameOptions::default(),
            last_local_edit: None,
            last_sent: HashMap::new(),
            pending: HashMap::new(),
        }
    }

    /// Called every frame: picks up fresh status and sends whatever changes have waited long enough.
    pub fn tick(&mut self) {
        let fresh = self.shared.lock().unwrap().status.as_ref().map(|s| s.options.clone());
        if let Some(options) = fresh
            && self.last_local_edit.is_none_or(|t| t.elapsed() > LOCAL_EDIT_WINS_FOR)
        {
            self.options = options;
        }
        let due: Vec<String> = self
            .pending
            .keys()
            .filter(|name| self.last_sent.get(*name).is_none_or(|t| t.elapsed() >= SEND_INTERVAL))
            .cloned()
            .collect();
        for name in due {
            if let Some(value) = self.pending.remove(&name) {
                self.last_sent.insert(name.clone(), Instant::now());
                let _ = self.requests.send(ControlRequest::set(&name, &value));
            }
        }
    }

    pub fn state(&self) -> PanelState {
        PanelState::from(self.shared.lock().unwrap().status.as_ref())
    }

    pub fn problem(&self) -> Option<String> {
        self.shared.lock().unwrap().problem.clone()
    }

    pub fn control(&self, setting: &str) -> SettingControl {
        SettingControl::from(self.shared.lock().unwrap().status.as_ref().and_then(|s| s.settings.get(setting)))
    }

    /// Sends right away if we haven't lately, otherwise once the interval passes, always with the latest value.
    pub fn set_soon(&mut self, name: &str, value: String) {
        self.last_local_edit = Some(Instant::now());
        self.pending.insert(name.to_string(), value);
    }

    /// A one off change like mirror or a reset: applied here and sent now.
    pub fn set_option(&mut self, name: &str, value: &str) {
        if let Ok(changed) = self.options.applying(name, value) {
            self.options = changed;
            self.last_local_edit = Some(Instant::now());
            self.pending.remove(name);
            self.send(ControlRequest::set(name, value));
        }
    }

    pub fn set_camera(&mut self, name: &str, value: &str) {
        self.send(ControlRequest::set(name, value));
    }

    pub fn focus(&mut self, action: &str, steps: u32) {
        self.send(ControlRequest::focus(action, steps));
    }

    /// Drag on the preview to slide the zoomed frame around.
    pub fn pan_by(&mut self, delta_x: f64, delta_y: f64) {
        self.options.pan_x = (self.options.pan_x + delta_x).clamp(-1.0, 1.0);
        self.options.pan_y = (self.options.pan_y + delta_y).clamp(-1.0, 1.0);
        self.set_soon("pan-x", number(self.options.pan_x));
        self.set_soon("pan-y", number(self.options.pan_y));
    }

    pub fn zoom_to(&mut self, zoom: f64) {
        self.options.zoom = zoom.clamp(1.0, 3.0);
        self.set_soon("zoom", number(self.options.zoom));
    }

    fn send(&self, request: ControlRequest) {
        self.shared.lock().unwrap().problem = None;
        let _ = self.requests.send(request);
    }
}

pub fn number(value: f64) -> String {
    format!("{value:.3}")
}

/// Runs every request on one thread, in order, so the window never blocks on the pipe.
fn talk(inbox: Receiver<ControlRequest>, shared: &Mutex<Shared>, ctx: &eframe::egui::Context) {
    for request in inbox {
        let is_status = request.command == "status";
        let result = control::send(&request);
        let mut state = shared.lock().unwrap();
        match result {
            Ok(response) => {
                if !response.ok && !is_status {
                    state.problem = response.message;
                }
                if let Some(status) = response.status {
                    state.status = Some(status);
                }
            }
            Err(_) if is_status => state.status = None,
            Err(e) => state.problem = Some(format!("{e:#}")),
        }
        drop(state);
        ctx.request_repaint();
    }
}
