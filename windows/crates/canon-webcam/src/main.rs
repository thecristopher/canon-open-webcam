// A GUI app, so double clicking doesn't flash a console. Run from a terminal it borrows that one.
#![cfg_attr(not(test), windows_subsystem = "windows")]

mod console;
mod diagnose;
mod engine;
mod log;
mod picture;
mod setup;
mod tray;

use std::path::PathBuf;

use crate::log::log;

const USAGE: &str = "\
Canon Webcam: use a Canon EOS camera as a webcam

  CanonWebcam                    run the app (offers to install itself the first time)
  CanonWebcam install [--quiet]  install for every app on this PC (asks for admin once)
  CanonWebcam uninstall [--quiet]
  CanonWebcam check [out.bmp]    open the virtual camera like Zoom would and save a frame
  CanonWebcam list               show every portable device Windows sees
  CanonWebcam probe [out.jpg]    talk to the first Canon directly and save one live view frame";

fn main() {
    console::attach();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let (quiet, elevated) = (flag("--quiet"), flag("--elevated"));
    let path_arg = |default: &str| {
        args.get(1)
            .filter(|a| !a.starts_with("--"))
            .map_or_else(|| PathBuf::from(default), PathBuf::from)
    };

    let code = match args.first().map(String::as_str) {
        None => start(),
        Some("install") => setup::report(setup::install(quiet, elevated), quiet),
        Some("uninstall") => setup::report(setup::uninstall(quiet, elevated), quiet),
        Some("check") => setup::report(diagnose::check(&path_arg("canon-webcam-check.bmp")), true),
        Some("list") => setup::report(diagnose::list(), true),
        Some("probe") => setup::report(diagnose::probe(&path_arg("frame.jpg")), true),
        Some("run") => setup::report(tray::run(), true),
        Some(_) => {
            console::tell(USAGE, false);
            2
        }
    };
    std::process::exit(code);
}

/// Double click: run if this is the installed copy, otherwise offer to install (or update) first.
fn start() -> i32 {
    if setup::running_installed_copy() {
        return setup::report(tray::run(), false);
    }
    let question = if setup::is_installed() {
        "Canon Webcam is already installed. Replace it with this copy?"
    } else {
        "Install Canon Webcam?\n\nIt adds a camera called \"Canon Webcam\" that Zoom, Teams, Meet and other apps can pick, and starts when you sign in. Windows will ask for permission once."
    };
    if console::attached() {
        console::tell(USAGE, true);
        return 0;
    }
    if console::ask(question) {
        log!(
            "installing from {}",
            std::env::current_exe()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        );
        return setup::report(setup::install(false, false), false);
    }
    0
}
