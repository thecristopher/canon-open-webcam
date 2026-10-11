use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use canon_ptp::CanonCamera;
use canon_shm::SharedFrames;
use canon_wpd::{ComApartment, WpdTransport, list_devices};
use windows::Win32::Foundation::{E_ACCESSDENIED, ERROR_BUSY, ERROR_SHARING_VIOLATION};

use crate::log::log;
use crate::picture::{Picture, card};

// An app counts as watching while it asked for a frame this recently
const WATCHING_MS: u64 = 2000;
// After the last call ends the camera gets to rest, like on the Mac
const REST_AFTER: Duration = Duration::from_secs(30);
const RETRY_CAMERA: Duration = Duration::from_secs(2);
const KEEP_AWAKE: Duration = Duration::from_secs(20);
const REFRESH_CARD: Duration = Duration::from_millis(500);

/// What the tray shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Waiting,
    Resting,
    Live(String),
    NoCamera,
    Busy,
    Trouble(String),
}

impl Status {
    pub fn describe(&self) -> String {
        match self {
            Status::Waiting => "Ready, waiting for an app to open the camera".into(),
            Status::Resting => "Camera resting".into(),
            Status::Live(model) => format!("Live from {model}"),
            Status::NoCamera => "No Canon found, turn it on and plug it in".into(),
            Status::Busy => "Your camera is busy in another app".into(),
            Status::Trouble(detail) => format!("Trouble: {detail}"),
        }
    }
}

/// The background thread that wakes the camera when an app is watching and feeds frames to the virtual camera.
pub struct Engine {
    status: Arc<Mutex<Status>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Engine {
    pub fn start() -> Self {
        let status = Arc::new(Mutex::new(Status::Waiting));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (status, stop) = (status.clone(), stop.clone());
            std::thread::Builder::new()
                .name("engine".into())
                .spawn(move || run(&status, &stop))
                .expect("engine thread")
        };
        Self {
            status,
            stop,
            thread: Some(thread),
        }
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Live {
    camera: CanonCamera<WpdTransport>,
    last_keep_awake: Instant,
}

fn run(status: &Mutex<Status>, stop: &AtomicBool) {
    let _com = ComApartment::enter().expect("COM");
    let mut picture = Picture::new();
    let mut shared: Option<SharedFrames> = None;
    let mut live: Option<Live> = None;
    let mut last_watched: Option<Instant> = None;
    let mut last_attempt: Option<Instant> = None;
    let mut last_card: Option<(Instant, Vec<u8>)> = None;
    let set = |new: Status| {
        let mut current = status.lock().unwrap();
        if *current != new {
            log!("{}", new.describe());
            *current = new;
        }
    };

    while !stop.load(Ordering::Relaxed) {
        // Frame Server creates the buffer the first time an app opens the camera
        if shared.is_none() {
            shared = SharedFrames::open_or_create().ok();
        }
        let watching = shared
            .as_ref()
            .and_then(SharedFrames::consumer_idle_ms)
            .is_some_and(|idle| idle < WATCHING_MS);

        if !watching {
            if live.is_some() && last_watched.is_some_and(|t| t.elapsed() > REST_AFTER) {
                if let Some(mut gone) = live.take() {
                    gone.camera.disconnect();
                }
                set(Status::Resting);
            } else if live.is_none() && !matches!(*status.lock().unwrap(), Status::Resting) {
                set(Status::Waiting);
            }
            last_card = None;
            std::thread::sleep(Duration::from_millis(250));
            continue;
        }
        last_watched = Some(Instant::now());
        let shared = shared.as_ref().unwrap();

        if live.is_none() && last_attempt.is_none_or(|t| t.elapsed() > RETRY_CAMERA) {
            last_attempt = Some(Instant::now());
            match wake_camera() {
                Ok(camera) => {
                    set(Status::Live(camera.model().to_string()));
                    live = Some(Live {
                        camera,
                        last_keep_awake: Instant::now(),
                    });
                    last_card = None;
                }
                Err(problem) => {
                    let (title, detail) = match &problem {
                        Status::NoCamera => (
                            "Your camera is off",
                            "Turn on your Canon and plug it in. The picture comes back on its own.",
                        ),
                        Status::Busy => (
                            "Your camera is busy",
                            "Another app is using your Canon. Close it and the picture comes back on its own.",
                        ),
                        _ => (
                            "Can't reach your camera",
                            "Try unplugging it and plugging it back in.",
                        ),
                    };
                    set(problem);
                    last_card = Some((Instant::now(), card(title, detail)));
                }
            }
        }

        if let Some(current) = live.as_mut() {
            match current.camera.next_frame() {
                Ok(Some(jpeg)) => match picture.jpeg_to_nv12(&jpeg) {
                    Ok(nv12) => shared.write_frame(nv12),
                    Err(e) => log!("skipping a frame: {e:#}"),
                },
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Err(e) => {
                    log!("lost the camera: {e}");
                    live = None;
                    set(Status::Trouble(e.to_string()));
                    last_card = Some((
                        Instant::now(),
                        card(
                            "Lost your camera",
                            "Check the cable and that the camera is still on.",
                        ),
                    ));
                    continue;
                }
            }
            if current.last_keep_awake.elapsed() > KEEP_AWAKE {
                let _ = current.camera.keep_awake();
                current.last_keep_awake = Instant::now();
            }
        } else if let Some((written, frame)) = last_card.as_mut() {
            // Keep rewriting the card so the virtual camera knows we're alive
            if written.elapsed() > REFRESH_CARD {
                shared.write_frame(frame);
                *written = Instant::now();
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    if let Some(mut current) = live {
        current.camera.disconnect();
    }
}

fn wake_camera() -> Result<CanonCamera<WpdTransport>, Status> {
    let devices = list_devices().map_err(|e| Status::Trouble(format!("listing devices: {e}")))?;
    let entry = devices
        .iter()
        .find(|d| d.is_canon())
        .ok_or(Status::NoCamera)?;
    let transport = WpdTransport::open(entry).map_err(|e| {
        let busy = [
            E_ACCESSDENIED,
            ERROR_BUSY.to_hresult(),
            ERROR_SHARING_VIOLATION.to_hresult(),
        ];
        if busy.contains(&e.code()) {
            Status::Busy
        } else {
            Status::Trouble(format!("opening the camera: {e}"))
        }
    })?;
    let mut camera = CanonCamera::new(transport);
    camera
        .connect()
        .map_err(|e| Status::Trouble(format!("remote mode: {e}")))?;
    camera
        .start_live_view()
        .map_err(|e| Status::Trouble(format!("live view: {e}")))?;
    log!("connected to {}", camera.model());
    Ok(camera)
}
