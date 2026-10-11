// A GUI app, so double clicking doesn't flash a console. Run from a terminal it borrows that one.
#![cfg_attr(not(test), windows_subsystem = "windows")]

mod cli;
mod console;
mod control;
mod diagnose;
mod engine;
mod log;
mod options;
mod panel;
mod picture;
mod setup;
mod timing;
mod tray;
mod ui;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, bail};
use windows::Win32::System::Console::SetConsoleCtrlHandler;

use crate::control::ControlRequest;
use crate::engine::{Engine, EngineConfig};
use crate::log::log;

const USAGE: &str = "\
Canon Webcam: use a Canon EOS camera as a webcam

  CanonWebcam                         open the app (offers to set itself up the first time)
  CanonWebcam status                  what's going on, plus every setting
  CanonWebcam set brightness 0.2      picture: brightness, contrast, saturation, warmth, tint (-1 to 1)
  CanonWebcam set sharpness 0.6       0 to 1
  CanonWebcam set picture reset
  CanonWebcam set zoom 1.5            framing: zoom (1 to 3), pan-x and pan-y (-1 to 1)
  CanonWebcam set framing reset
  CanonWebcam set mirror on
  CanonWebcam set fps 30              output: camera, 30 or 60
  CanonWebcam set exposure +1         camera: exposure, white-balance, iso, aperture, shutter, color-temp
  CanonWebcam focus                   autofocus, or: focus near 2, focus far 1
  CanonWebcam doctor                  check everything is in place
  CanonWebcam install [--quiet]       install for every app on this PC (asks for admin once)
  CanonWebcam uninstall [--quiet] [--restore-canon]
                                      add --window to either for the checklist window instead
  CanonWebcam start [--always-on]     run the engine in this terminal instead of the tray
  CanonWebcam bench [seconds]         measure the camera's raw live view rate
  CanonWebcam check [out.bmp]         open the virtual camera like Zoom would and save a frame
  CanonWebcam list                    every portable device Windows sees
  CanonWebcam probe [out.jpg]         talk to the Canon directly and save one live view frame";

fn main() {
    console::attach();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let (quiet, elevated, always_on) = (flag("--quiet"), flag("--elevated"), flag("--always-on"));
    let positional: Vec<&str> = args.iter().skip(1).filter(|a| !a.starts_with("--")).map(String::as_str).collect();
    let path_arg = |default: &str| positional.first().map_or_else(|| PathBuf::from(default), PathBuf::from);
    // Opened from Explorer, Settings or the tray there's no console, so install and uninstall show their window.
    // From a terminal they act straight away like the Mac commands, unless --window asks for the window.
    let gui = (!console::attached() || flag("--window")) && !quiet;

    let code = match args.first().map(String::as_str) {
        None => start(),
        Some(setup::BACKGROUND_FLAG) | Some("--always-on") => {
            report(tray::run(EngineConfig { always_on }, false), false)
        }
        Some("window") => report(ui::run_camera_window(), false),
        Some("setup") => report(ui::run_setup_window(), false),
        Some("install") if gui && !elevated => report(ui::run_setup_window(), false),
        Some("install") => report(setup::install(quiet, elevated), quiet),
        Some("uninstall") if gui && !elevated => report(ui::run_uninstall_window(), false),
        Some("uninstall") => report(setup::uninstall(quiet, elevated, flag("--restore-canon")), quiet),
        Some("status") => report(cli::status(), true),
        Some("set") => report(set(&positional), true),
        Some("focus") => report(focus(&positional), true),
        Some("doctor") => report(cli::doctor(), true),
        Some("start") => report(run_in_terminal(EngineConfig { always_on }), true),
        Some("bench") => report(cli::bench(positional.first().and_then(|s| s.parse().ok()).unwrap_or(10.0)), true),
        Some("check") => report(diagnose::check(&path_arg("canon-webcam-check.bmp")), true),
        Some("list") => report(diagnose::list(), true),
        Some("probe") => report(diagnose::probe(&path_arg("frame.jpg")), true),
        Some("help" | "--help" | "-h") => {
            console::tell(USAGE, false);
            0
        }
        Some(other) => {
            console::warn(&format!("I don't know \"{other}\".\n\n{USAGE}"), false);
            2
        }
    };
    std::process::exit(code);
}

/// Opened by hand: set up first if this copy isn't the installed one, otherwise the tray plus the window.
fn start() -> i32 {
    if console::attached() {
        console::tell(USAGE, true);
        return 0;
    }
    if !setup::running_installed_copy() {
        return report(ui::run_setup_window(), false);
    }
    if tray::already_running() {
        tray::open_window();
        return 0;
    }
    report(tray::run(EngineConfig::default(), true), false)
}

fn set(positional: &[&str]) -> Result<()> {
    let [name, value] = positional else { bail!("set takes a name and a value, like: set brightness 0.2") };
    cli::send(ControlRequest::set(name, value))
}

fn focus(positional: &[&str]) -> Result<()> {
    let action = positional.first().copied().unwrap_or("auto");
    let steps = positional.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    cli::send(ControlRequest::focus(action, steps))
}

static STOP: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn on_ctrl_c(_: u32) -> windows_core::BOOL {
    STOP.store(true, Ordering::Relaxed);
    true.into()
}

/// The engine without the tray, for watching the log scroll by. Ctrl+C hands the camera back and leaves.
fn run_in_terminal(config: EngineConfig) -> Result<()> {
    if tray::already_running() {
        bail!("Canon Webcam is already running in the tray. Quit it from the tray icon first.");
    }
    let engine = Engine::start(config);
    control::serve(engine.handler())?;
    unsafe { SetConsoleCtrlHandler(Some(on_ctrl_c), true)? };
    log!("Press Ctrl+C to stop.");
    while !STOP.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    drop(engine);
    Ok(())
}

fn report(result: Result<()>, quiet: bool) -> i32 {
    match result {
        Ok(()) => 0,
        Err(e) => {
            log::record(&format!("{e:#}"));
            console::warn(&format!("{e:#}"), quiet);
            1
        }
    }
}
