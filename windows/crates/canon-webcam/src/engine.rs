use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use canon_ptp::{CameraSetting, CanonCamera};
use canon_shm::{CAMERA_NAME, SharedFrames};
use canon_wpd::{ComApartment, WpdTransport, list_devices};
use windows::Win32::Foundation::{E_ACCESSDENIED, ERROR_BUSY, ERROR_FILE_NOT_FOUND, ERROR_SHARING_VIOLATION};

use crate::control::{ControlRequest, ControlResponse, EngineCommand, EngineStatus, PhaseKind, SettingStatus};
use crate::log::log;
use crate::options::{FrameOptions, OptionsStore};
use crate::picture::{Picture, card};
use crate::timing::{Every, FramePacer, FrameRateMeter, TimingStats};

// An app counts as watching while it asked the virtual camera for a frame this recently
const WATCHING_MS: u64 = 2000;
// After the last app lets go, the camera stays warm this long in case it comes right back
const REST_AFTER: Duration = Duration::from_secs(30);
const RETRY_CAMERA: Duration = Duration::from_secs(2);
// No frames for this long means live view quietly died
const STALL_LIMIT: Duration = Duration::from_secs(5);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

/// Where the engine is at, and the card apps see when there's no live picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    WaitingForCamera,
    Starting,
    Live,
    Trouble(String),
}

impl Phase {
    fn name(&self) -> String {
        match self {
            Phase::Idle => "idle (no app is using the camera)".into(),
            Phase::WaitingForCamera => "waiting for the camera".into(),
            Phase::Starting => "starting live view".into(),
            Phase::Live => "live".into(),
            Phase::Trouble(reason) => format!("trouble: {reason}"),
        }
    }

    fn kind(&self) -> PhaseKind {
        match self {
            Phase::Idle => PhaseKind::Idle,
            Phase::WaitingForCamera => PhaseKind::WaitingForCamera,
            Phase::Starting => PhaseKind::Starting,
            Phase::Live => PhaseKind::Live,
            Phase::Trouble(_) => PhaseKind::Trouble,
        }
    }

    fn card(&self) -> (&str, &str) {
        match self {
            Phase::Idle | Phase::WaitingForCamera => {
                ("Waiting for your camera", "Plug in the Canon over USB and turn it on")
            }
            Phase::Starting => ("Starting camera", "Live view takes a second or two"),
            Phase::Live => ("", ""),
            Phase::Trouble(reason) => ("Can't reach the camera", reason),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct EngineConfig {
    /// Stream even when no app is watching. Costs camera battery and heat, saves the warm up.
    pub always_on: bool,
}

type Inbox = Vec<(EngineCommand, Sender<ControlResponse>)>;

/// What the camera thread and everyone asking it things share.
struct Shared {
    status: Mutex<EngineStatus>,
    inbox: Mutex<Inbox>,
    stop: AtomicBool,
}

/// Camera in, virtual camera out, on its own thread. Status and control requests can come from any thread.
pub struct Engine {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Engine {
    pub fn start(config: EngineConfig) -> Self {
        let options = OptionsStore::new().load();
        let shared = Arc::new(Shared {
            status: Mutex::new(status_for(&Phase::Idle, None, 0.0, false, &options)),
            inbox: Mutex::new(Vec::new()),
            stop: AtomicBool::new(false),
        });
        let thread = {
            let shared = shared.clone();
            std::thread::Builder::new()
                .name("engine".into())
                .spawn(move || Runner::new(config, options, shared).run())
                .expect("engine thread")
        };
        Self { shared, thread: Some(thread) }
    }

    pub fn status(&self) -> EngineStatus {
        self.shared.status.lock().unwrap().clone()
    }

    /// Answers a control request. Status comes back right away, everything else waits its turn on the camera thread.
    pub fn handle(&self, request: ControlRequest) -> ControlResponse {
        Self::handle_shared(&self.shared, request)
    }

    /// A handle the control pipe can keep on its own threads.
    pub fn handler(&self) -> crate::control::Handler {
        let shared = self.shared.clone();
        Arc::new(move |request| Self::handle_shared(&shared, request))
    }

    fn handle_shared(shared: &Shared, request: ControlRequest) -> ControlResponse {
        match request.command.as_str() {
            "status" => return ControlResponse::success(None, Some(shared.status.lock().unwrap().clone())),
            // The Mac needs this after a preview closes. Here the virtual camera's heartbeat already says so.
            "recheck" => return ControlResponse::success(Some("Apps are tracked as they come and go.".into()), None),
            _ => {}
        }
        let command = match EngineCommand::from_request(&request) {
            Ok(command) => command,
            Err(message) => return ControlResponse::failure(message),
        };
        let (reply, answer) = channel();
        shared.inbox.lock().unwrap().push((command, reply));
        answer
            .recv_timeout(COMMAND_TIMEOUT)
            .unwrap_or_else(|_| ControlResponse::failure("the engine didn't answer in time"))
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Runner {
    config: EngineConfig,
    shared: Arc<Shared>,
    store: OptionsStore,
    options: FrameOptions,
    picture: Picture,
    frames: Option<SharedFrames>,
    /// Why the frame buffer last wouldn't open, so each new reason is logged once
    frames_problem: Option<String>,
    camera: Option<CanonCamera<WpdTransport>>,
    phase: Phase,
    watching: bool,
    meter: FrameRateMeter,
    pacer: FramePacer,
    stats: TimingStats,
    last_frame_at: Instant,
    last_watched: Option<Instant>,
    last_attempt: Option<Instant>,
    last_logged_trouble: Option<String>,
    event_poll: Every,
    keep_alive: Every,
    status_refresh: Every,
    card_refresh: Every,
    card: Option<(Phase, Vec<u8>)>,
    driver_check: Every,
    hijacked: bool,
}

impl Runner {
    fn new(config: EngineConfig, options: FrameOptions, shared: Arc<Shared>) -> Self {
        Self {
            config,
            shared,
            store: OptionsStore::new(),
            options,
            picture: Picture::new(),
            frames: None,
            frames_problem: None,
            camera: None,
            phase: Phase::Idle,
            watching: false,
            meter: FrameRateMeter::default(),
            pacer: FramePacer::default(),
            stats: TimingStats::new(std::env::var_os("CANON_WEBCAM_DEBUG").is_some()),
            last_frame_at: Instant::now(),
            last_watched: None,
            last_attempt: None,
            last_logged_trouble: None,
            event_poll: Every::new(Duration::from_millis(500)),
            keep_alive: Every::new(Duration::from_secs(20)),
            status_refresh: Every::new(Duration::from_millis(500)),
            card_refresh: Every::new(Duration::from_millis(500)),
            card: None,
            driver_check: Every::new(Duration::from_secs(30)),
            hijacked: false,
        }
    }

    fn run(mut self) {
        let _com = ComApartment::enter().expect("COM");
        if self.config.always_on {
            log!("Streaming all the time (--always-on).");
        } else {
            log!("Ready. The camera wakes up when an app opens {CAMERA_NAME}.");
        }
        self.publish();
        while !self.shared.stop.load(Ordering::Relaxed) {
            self.run_pending_commands();
            self.update_watching();
            if !self.config.always_on && !self.watching && self.rest_if_due() {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            if self.camera.is_none() {
                self.connect_camera();
            } else {
                self.step_live_view();
            }
        }
        self.release_camera();
        log!("Stopped. The camera is back to normal.");
    }

    // Who's watching

    fn update_watching(&mut self) {
        // Frame Server makes the buffer the first time an app opens the camera
        if self.frames.is_none() {
            self.open_frames();
        }
        let watching =
            self.frames.as_ref().and_then(SharedFrames::consumer_idle_ms).is_some_and(|idle| idle < WATCHING_MS);
        if watching {
            self.last_watched = Some(Instant::now());
        }
        if watching != self.watching {
            self.watching = watching;
            log!("{}", if watching { "An app opened the camera." } else { "No app is watching anymore." });
            self.publish();
        }
    }

    /// Finds the buffer the virtual camera shares frames through. Not there yet is normal until an app opens
    /// the camera, but one that's there and won't open means apps get the fallback frame forever, so say why.
    fn open_frames(&mut self) {
        let opened = SharedFrames::open().or_else(|e| {
            if e.code() == ERROR_FILE_NOT_FOUND.to_hresult() {
                // Only an admin may make it, which `start` from an admin terminal is
                SharedFrames::create().map_err(|_| None)
            } else {
                Err(Some(e))
            }
        });
        match opened {
            Ok(frames) => {
                frames.set_frame_rate(self.options.frame_rate);
                log!(
                    "{} the frame buffer the virtual camera reads from.",
                    if frames.created() { "Made" } else { "Found" }
                );
                self.frames = Some(frames);
                self.frames_problem = None;
            }
            Err(Some(e)) => {
                let problem = e.message();
                if self.frames_problem.as_ref() != Some(&problem) {
                    log!(
                        "The virtual camera's frame buffer is there but won't open, so apps only get the fallback frame: {problem}"
                    );
                    self.frames_problem = Some(problem);
                }
            }
            Err(None) => {}
        }
    }

    /// With nobody watching: rests the camera once the grace period is over.
    /// Returns true when there's nothing to do, false while the camera is still being kept warm.
    fn rest_if_due(&mut self) -> bool {
        let waited = self.last_watched.is_none_or(|t| t.elapsed() > REST_AFTER);
        if self.camera.is_some() && waited {
            self.release_camera();
            log!("No app is using the camera anymore, so it's resting.");
        }
        if self.camera.is_none() && self.phase != Phase::Idle {
            self.set_phase(Phase::Idle);
        }
        self.camera.is_none()
    }

    // Camera

    fn connect_camera(&mut self) {
        if self.last_attempt.is_some_and(|t| t.elapsed() < RETRY_CAMERA) {
            self.show_card();
            std::thread::sleep(Duration::from_millis(50));
            return;
        }
        self.last_attempt = Some(Instant::now());
        match self.open_camera() {
            Ok(camera) => {
                let lens = camera.state().lens_name.clone().unwrap_or_else(|| "an unknown lens".into());
                log!("Live from {} with {lens}.", camera.model());
                self.camera = Some(camera);
                self.last_frame_at = Instant::now();
                self.meter.reset();
                self.event_poll.reset();
                self.keep_alive.reset();
                self.last_logged_trouble = None;
            }
            Err(phase) => {
                // Retrying every couple of seconds shouldn't write the same line to the log forever
                if let Phase::Trouble(reason) = &phase
                    && self.last_logged_trouble.as_ref() != Some(reason)
                {
                    log!("Couldn't start the camera: {reason} Retrying until it works.");
                    self.last_logged_trouble = Some(reason.clone());
                }
                self.set_phase(phase);
                self.show_card();
            }
        }
    }

    fn open_camera(&mut self) -> Result<CanonCamera<WpdTransport>, Phase> {
        let devices = list_devices().map_err(|e| Phase::Trouble(format!("Windows wouldn't list devices ({e}).")))?;
        let Some(entry) = devices.iter().find(|d| d.is_canon()) else {
            return Err(self.why_no_camera());
        };
        self.set_phase(Phase::Starting);
        self.show_card();
        let transport = WpdTransport::open(entry).map_err(|e| {
            let busy = [E_ACCESSDENIED, ERROR_BUSY.to_hresult(), ERROR_SHARING_VIOLATION.to_hresult()];
            if busy.contains(&e.code()) {
                Phase::Trouble(
                    "Another app is using your camera. Close EOS Utility, Photos or Canon's EOS Webcam Utility.".into(),
                )
            } else {
                Phase::Trouble(format!("Windows wouldn't open the camera ({e})."))
            }
        })?;
        let mut camera = CanonCamera::new(transport);
        camera.connect().map_err(|e| Phase::Trouble(format!("{e}.")))?;
        camera.start_live_view().map_err(|e| Phase::Trouble(format!("{e}.")))?;
        Ok(camera)
    }

    /// No Canon in WPD: either it's simply not there, or a libusb tool took it over. Checking that costs
    /// a PowerShell start, so it's done when the card first goes up and then every half minute.
    fn why_no_camera(&mut self) -> Phase {
        if self.driver_check.is_due(Instant::now()) {
            self.hijacked = crate::setup::canon_on_generic_usb_driver().is_some();
        }
        if self.hijacked {
            Phase::Trouble(
                "Your Canon is on a WinUSB driver from Zadig. Open Canon Webcam's Set Up to switch it back.".into(),
            )
        } else {
            Phase::WaitingForCamera
        }
    }

    fn step_live_view(&mut self) {
        if let Err(e) = self.try_step() {
            log!("Lost the camera: {e:#}. Reconnecting.");
            self.release_camera();
            self.set_phase(Phase::Trouble("Lost the camera, reconnecting".into()));
            self.last_attempt = Some(Instant::now());
            self.show_card();
        }
    }

    fn try_step(&mut self) -> Result<()> {
        let now = Instant::now();
        let camera = self.camera.as_mut().ok_or_else(|| anyhow!("no camera"))?;
        if self.event_poll.is_due(now) {
            let started = Instant::now();
            camera.poll_events()?;
            self.stats.record("events", started);
        }
        if self.keep_alive.is_due(now) {
            camera.keep_awake()?;
        }

        // Live but nobody watching (the rest window, or --always-on): keep it warm without pulling frames
        if !self.watching {
            if self.status_refresh.is_due(now) {
                self.publish();
            }
            std::thread::sleep(Duration::from_millis(100));
            return Ok(());
        }

        let wait = self.pacer.wait_before_asking(self.last_frame_at.elapsed());
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
        let asked = Instant::now();
        let answer = camera.next_frame()?;
        self.stats.record("ask", asked);
        let Some(jpeg) = answer else {
            self.stats.count("not ready");
            if self.last_frame_at.elapsed() > STALL_LIMIT {
                bail!("live view stalled");
            }
            std::thread::sleep(self.pacer.retry_delay);
            return Ok(());
        };
        if self.stats.enabled && self.last_frame_at.elapsed() > Duration::from_millis(60) {
            log!("slow frame: {}ms since the last one", self.last_frame_at.elapsed().as_millis());
        }
        self.last_frame_at = Instant::now();
        self.meter.frame_arrived(self.last_frame_at);
        if self.phase != Phase::Live {
            self.set_phase(Phase::Live);
        }

        let rendered = Instant::now();
        match self.picture.jpeg_to_nv12(&jpeg, &self.options) {
            Ok(nv12) => {
                if let Some(frames) = &self.frames {
                    frames.write_frame(nv12);
                }
                self.stats.record("render", rendered);
            }
            Err(e) => log!("Skipped a frame: {e:#}"),
        }
        if let Some(summary) = self.stats.summary_if_due(Instant::now()) {
            log!("{summary}");
        }
        if self.status_refresh.is_due(Instant::now()) {
            self.publish();
        }
        Ok(())
    }

    fn release_camera(&mut self) {
        if let Some(mut camera) = self.camera.take() {
            camera.disconnect();
        }
        self.meter.reset();
    }

    /// Keeps the card on screen while there's no live picture, so apps don't freeze on an old frame.
    fn show_card(&mut self) {
        let Some(frames) = &self.frames else { return };
        if self.card.as_ref().is_none_or(|(phase, _)| *phase != self.phase) {
            let (title, detail) = self.phase.card();
            self.card = Some((self.phase.clone(), card(title, detail)));
            self.card_refresh.reset();
        }
        if self.card_refresh.is_due(Instant::now())
            && let Some((_, frame)) = &self.card
        {
            frames.write_frame(frame);
        }
    }

    // Control requests

    fn run_pending_commands(&mut self) {
        let pending: Inbox = std::mem::take(&mut *self.shared.inbox.lock().unwrap());
        for (command, reply) in pending {
            let response = match self.execute(&command) {
                Ok(message) => {
                    // Sliders send a stream of option changes, which would bury the log
                    if !matches!(command, EngineCommand::SetOption(..)) {
                        log!("{message}");
                    }
                    self.publish();
                    ControlResponse::success(Some(message), Some(self.shared.status.lock().unwrap().clone()))
                }
                Err(e) => ControlResponse::failure(format!("{e:#}")),
            };
            let _ = reply.send(response);
        }
    }

    fn execute(&mut self, command: &EngineCommand) -> Result<String> {
        match command {
            EngineCommand::SetOption(name, value) => {
                self.options = self.options.applying(name, value).map_err(|e| anyhow!(e))?;
                self.store.save(&self.options)?;
                if let Some(frames) = &self.frames {
                    frames.set_frame_rate(self.options.frame_rate);
                }
                Ok(self.options.describe(name))
            }
            EngineCommand::SetCamera(setting, input) => {
                let camera = self.live_camera(setting.name())?;
                let choices = camera.state().choices(setting.property()).to_vec();
                if choices.is_empty() {
                    // Movie mode with auto exposure, for example, owns ISO and shutter itself
                    bail!(
                        "The camera's current mode controls {} itself. Switch the dial to manual (M) to set it.",
                        setting.name()
                    );
                }
                let code = setting.code(input, &choices).filter(|c| choices.contains(c)).ok_or_else(|| {
                    let labels: Vec<String> = choices.iter().map(|c| setting.label(*c)).collect();
                    anyhow!(
                        "{input} isn't available for {} right now. Pick from: {}",
                        setting.name(),
                        labels.join(", ")
                    )
                })?;
                camera.set(setting.property(), code)?;
                Ok(format!("{} is now {}.", setting.name(), setting.label(code)))
            }
            EngineCommand::Autofocus => {
                self.live_camera("focus")?.autofocus()?;
                Ok("Focused.".into())
            }
            EngineCommand::Focus(step) => {
                self.live_camera("focus")?.drive_focus(*step)?;
                Ok("Nudged focus.".into())
            }
        }
    }

    fn live_camera(&mut self, what: &str) -> Result<&mut CanonCamera<WpdTransport>> {
        match (&mut self.camera, &self.phase) {
            (Some(camera), Phase::Live) => Ok(camera),
            _ => bail!("The camera isn't live right now, so {what} can't change. Open the camera in an app first."),
        }
    }

    // Status

    fn set_phase(&mut self, phase: Phase) {
        if phase != self.phase {
            self.phase = phase;
            self.publish();
        }
    }

    fn publish(&self) {
        let fps = if self.phase == Phase::Live && self.watching { self.meter.frames_per_second() } else { 0.0 };
        let status = status_for(&self.phase, self.camera.as_ref(), fps, self.watching, &self.options);
        *self.shared.status.lock().unwrap() = status;
    }
}

fn status_for(
    phase: &Phase,
    camera: Option<&CanonCamera<WpdTransport>>,
    frames_per_second: f64,
    watching: bool,
    options: &FrameOptions,
) -> EngineStatus {
    let mut settings = BTreeMap::new();
    if let Some(state) = camera.map(|c| c.state()) {
        for setting in CameraSetting::ALL {
            let Some(value) = state.value(setting.property()) else { continue };
            let choices = state.choices(setting.property()).iter().map(|c| setting.label(*c)).collect();
            settings.insert(setting.name().to_string(), SettingStatus { value: setting.label(value), choices });
        }
    }
    EngineStatus {
        state: phase.name(),
        kind: phase.kind(),
        trouble: if let Phase::Trouble(reason) = phase { Some(reason.clone()) } else { None },
        model: camera.map(|c| c.model().to_string()),
        lens: camera.and_then(|c| c.state().lens_name.clone()),
        frames_per_second,
        apps_watching: watching,
        options: options.clone(),
        settings,
    }
}
