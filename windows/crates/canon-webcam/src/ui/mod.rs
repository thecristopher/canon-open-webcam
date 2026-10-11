//! The windows: the camera window, and the setup and uninstall checklists. Each runs as its own
//! process next to the tray, the same split the Mac app has between its windows and the engine.

mod camera_window;
mod model;
mod preview;
mod setup_window;
mod style;

use anyhow::{Result, anyhow};
use eframe::egui;
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SW_RESTORE, SetForegroundWindow, ShowWindow};
use windows_core::{HSTRING, w};

use crate::panel::icon_rgba;

const CAMERA_WINDOW_TITLE: &str = "Canon Webcam";

pub fn run_camera_window() -> Result<()> {
    // One camera window at a time: a second click just brings the open one forward
    let _single = unsafe { CreateMutexW(None, true, w!("Local\\CanonWebcamWindow"))? };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        bring_forward(CAMERA_WINDOW_TITLE);
        return Ok(());
    }
    run(CAMERA_WINDOW_TITLE, [1180.0, 700.0], [980.0, 600.0], true, |ctx| {
        Box::new(camera_window::CameraWindow::new(ctx))
    })
}

pub fn run_setup_window() -> Result<()> {
    run("Set Up Canon Webcam", [560.0, 600.0], [520.0, 520.0], false, |ctx| {
        Box::new(setup_window::SetupWindow::new(ctx))
    })
}

pub fn run_uninstall_window() -> Result<()> {
    run("Uninstall Canon Webcam", [520.0, 420.0], [480.0, 360.0], false, |ctx| {
        Box::new(setup_window::UninstallWindow::new(ctx))
    })
}

fn run(
    title: &str,
    size: [f32; 2],
    min_size: [f32; 2],
    resizable: bool,
    make: impl FnOnce(&egui::Context) -> Box<dyn eframe::App> + 'static,
) -> Result<()> {
    let icon = egui::IconData { rgba: icon_rgba(64), width: 64, height: 64 };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size(size)
            .with_min_inner_size(min_size)
            .with_resizable(resizable)
            .with_maximize_button(resizable)
            .with_icon(icon),
        centered: true,
        ..Default::default()
    };
    eframe::run_native(title, options, Box::new(move |cc| Ok(make(&cc.egui_ctx)))).map_err(|e| anyhow!("{e}"))
}

fn bring_forward(title: &str) {
    unsafe {
        if let Ok(window) = FindWindowW(None, &HSTRING::from(title)) {
            let _ = ShowWindow(window, SW_RESTORE);
            let _ = SetForegroundWindow(window);
        }
    }
}
