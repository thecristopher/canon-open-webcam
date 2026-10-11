//! The installer checklist and the uninstaller, one button per step like the Mac versions.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui::{self, Align, Color32, Layout, RichText, TextureHandle, TextureOptions, vec2};

use super::style;
use crate::panel::icon_rgba;
use crate::setup;

#[derive(Clone, PartialEq)]
enum Job {
    Idle,
    Working,
    Done,
    Failed(String),
}

fn run_job(job: &Arc<Mutex<Job>>, ctx: &egui::Context, work: impl FnOnce() -> anyhow::Result<()> + Send + 'static) {
    *job.lock().unwrap() = Job::Working;
    let (job, ctx) = (job.clone(), ctx.clone());
    std::thread::spawn(move || {
        let outcome = match work() {
            Ok(()) => Job::Done,
            Err(e) => Job::Failed(format!("{e:#}")),
        };
        *job.lock().unwrap() = outcome;
        ctx.request_repaint();
    });
}

fn badge(ctx: &egui::Context) -> TextureHandle {
    let image = egui::ColorImage::from_rgba_unmultiplied([64, 64], &icon_rgba(64));
    ctx.load_texture("badge", image, TextureOptions::LINEAR)
}

fn header(ui: &mut egui::Ui, badge: &TextureHandle, title: &str, subtitle: &str) {
    ui.horizontal(|ui| {
        ui.add(egui::Image::new((badge.id(), vec2(52.0, 52.0))));
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new(title).font(style::semibold(22.0)).color(Color32::WHITE));
            ui.label(RichText::new(subtitle).color(style::SECONDARY));
        });
    });
    ui.add_space(14.0);
}

/// A numbered step that turns into a green check once it's done.
fn step(ui: &mut egui::Ui, number: usize, done: bool, title: &str, contents: impl FnOnce(&mut egui::Ui)) {
    style::card(ui, |ui| {
        ui.horizontal_top(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(26.0, 26.0), egui::Sense::hover());
            let painter = ui.painter();
            if done {
                painter.circle_filled(rect.center(), 13.0, style::GOOD);
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "✔",
                    egui::FontId::proportional(14.0),
                    Color32::WHITE,
                );
            } else {
                painter.circle_stroke(rect.center(), 12.0, egui::Stroke::new(1.5, style::SECONDARY));
                let label = number.to_string();
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    style::semibold(13.0),
                    style::SECONDARY,
                );
            }
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(title).font(style::semibold(15.0)).color(Color32::WHITE));
                contents(ui);
            });
        });
    });
    ui.add_space(8.0);
}

pub struct SetupWindow {
    badge: TextureHandle,
    install: Arc<Mutex<Job>>,
    camera: Arc<Mutex<Option<String>>>,
    hijacked: Arc<Mutex<Option<setup::HijackedCamera>>>,
    driver_fix: Arc<Mutex<Job>>,
    build: u32,
    canon_utility: bool,
}

impl SetupWindow {
    pub fn new(ctx: &egui::Context) -> Self {
        style::apply(ctx);
        let camera = Arc::new(Mutex::new(None));
        let hijacked = Arc::new(Mutex::new(None));
        // Watch for the Canon, so plugging it in (or fixing its driver) ticks the last step on its own
        let (seen, taken, repaint) = (camera.clone(), hijacked.clone(), ctx.clone());
        std::thread::spawn(move || {
            loop {
                let found = crate::cli::canon_plugged_in();
                let generic = if found.is_none() { setup::canon_on_generic_usb_driver() } else { None };
                let changed = *seen.lock().unwrap() != found || *taken.lock().unwrap() != generic;
                *seen.lock().unwrap() = found;
                *taken.lock().unwrap() = generic;
                if changed {
                    repaint.request_repaint();
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        });
        let install = Arc::new(Mutex::new(if setup::is_installed() { Job::Done } else { Job::Idle }));
        Self {
            badge: badge(ctx),
            install,
            camera,
            hijacked,
            driver_fix: Arc::new(Mutex::new(Job::Idle)),
            build: setup::windows_build(),
            canon_utility: setup::canon_utility_present(),
        }
    }
}

impl eframe::App for SetupWindow {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let job = self.install.lock().unwrap().clone();
        let installed = job == Job::Done;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(style::PANEL).inner_margin(24)).show(ui, |ui| {
            header(ui, &self.badge, "Set Up Canon Webcam", "Use your Canon as a webcam in Zoom, Teams, Meet and every other app.");

            let windows_ok = self.build >= 22000;
            step(ui, 1, windows_ok, "Windows 11", |ui| {
                let text = if windows_ok {
                    format!("Build {}, which has virtual cameras built in.", self.build)
                } else {
                    format!("This PC runs build {}. Canon Webcam needs Windows 11.", self.build)
                };
                ui.label(RichText::new(text).color(style::SECONDARY));
            });

            step(ui, 2, installed, "Canon Webcam", |ui| {
                let mut text = String::from("Adds a camera called \"Canon Webcam\" that every app can pick, and starts it when you sign in. Windows asks for permission once.");
                if self.canon_utility {
                    text.push_str(" It also switches off Canon's EOS Webcam Utility so it stops grabbing the camera.");
                }
                ui.add(egui::Label::new(RichText::new(text).color(style::SECONDARY)).wrap());
                ui.add_space(4.0);
                ui.horizontal(|ui| match &job {
                    Job::Working => {
                        ui.spinner();
                        ui.label("Installing…");
                    }
                    Job::Done => {
                        ui.label(RichText::new("Installed.").color(style::GOOD));
                        if ui.small_button("Reinstall").clicked() {
                            run_job(&self.install, &ctx, setup::request_install);
                        }
                    }
                    Job::Idle | Job::Failed(_) => {
                        if ui.add_enabled(windows_ok, egui::Button::new("Install")).clicked() {
                            run_job(&self.install, &ctx, setup::request_install);
                        }
                    }
                });
                if let Job::Failed(reason) = &job {
                    ui.add(egui::Label::new(RichText::new(reason).color(style::WARNING)).wrap());
                }
            });

            let camera = self.camera.lock().unwrap().clone();
            let hijacked = self.hijacked.lock().unwrap().clone();
            let fix = self.driver_fix.lock().unwrap().clone();
            step(ui, 3, camera.is_some(), "Your camera", |ui| {
                match (&camera, &hijacked) {
                    (Some(name), _) => {
                        ui.label(RichText::new(format!("{name} is plugged in.")).color(style::SECONDARY));
                    }
                    (None, Some(taken)) => {
                        let text = format!(
                            "Your Canon is on the {} driver that Zadig installs for libusb tools, which hides it from \
                             Windows' camera system. Switching it back takes a moment, and Zadig can always redo it.",
                            taken.driver
                        );
                        ui.add(egui::Label::new(RichText::new(text).color(style::WARNING)).wrap());
                        ui.horizontal(|ui| {
                            if fix == Job::Working {
                                ui.spinner();
                                ui.label("Switching…");
                            } else if ui.button("Switch back to the Windows camera driver").clicked() {
                                run_job(&self.driver_fix, &ctx, setup::request_camera_driver_fix);
                            }
                        });
                        if let Job::Failed(reason) = &fix {
                            ui.add(egui::Label::new(RichText::new(reason).color(style::WARNING)).wrap());
                        }
                    }
                    (None, None) => {
                        let text = "Plug in your Canon over USB and turn it on. You can do this later too.";
                        ui.label(RichText::new(text).color(style::SECONDARY));
                    }
                }
            });

            ui.add_space(8.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let open = ui.add_enabled(installed, egui::Button::new(RichText::new("Open Canon Webcam").strong()).min_size(vec2(170.0, 30.0)));
                if open.clicked() && setup::start_app(true).is_ok() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button("Close").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}

pub struct UninstallWindow {
    badge: TextureHandle,
    job: Arc<Mutex<Job>>,
    canon_utility: bool,
    restore_canon: bool,
}

impl UninstallWindow {
    pub fn new(ctx: &egui::Context) -> Self {
        style::apply(ctx);
        Self {
            badge: badge(ctx),
            job: Arc::new(Mutex::new(Job::Idle)),
            canon_utility: setup::canon_utility_present(),
            restore_canon: false,
        }
    }
}

impl eframe::App for UninstallWindow {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let job = self.job.lock().unwrap().clone();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(style::PANEL).inner_margin(24)).show(ui, |ui| {
            header(
                ui,
                &self.badge,
                "Uninstall Canon Webcam",
                "Your Canon and its own settings stay exactly as they are.",
            );
            if job == Job::Done {
                style::card(ui, |ui| {
                    ui.label(RichText::new("Canon Webcam is gone.").font(style::semibold(15.0)).color(style::GOOD));
                    let note = if self.restore_canon {
                        "Canon's EOS Webcam Utility is back on."
                    } else if self.canon_utility {
                        "Canon's EOS Webcam Utility stays off."
                    } else {
                        "Everything it put on this PC has been removed."
                    };
                    ui.label(RichText::new(note).color(style::SECONDARY));
                });
                ui.add_space(12.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                return;
            }

            style::card(ui, |ui| {
                ui.label(RichText::new("This removes").font(style::semibold(15.0)).color(Color32::WHITE));
                for item in setup::removal_list() {
                    ui.label(RichText::new(format!("•  {item}")).color(style::SECONDARY));
                }
            });
            ui.add_space(8.0);
            if self.canon_utility {
                ui.checkbox(&mut self.restore_canon, "Turn Canon's EOS Webcam Utility back on");
                ui.add_space(4.0);
            }
            if let Job::Failed(reason) = &job {
                ui.add(egui::Label::new(RichText::new(reason).color(style::WARNING)).wrap());
            }
            ui.add_space(8.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if job == Job::Working {
                    ui.label("Removing…");
                    ui.spinner();
                } else {
                    let remove = egui::Button::new(RichText::new("Uninstall").strong().color(Color32::WHITE))
                        .fill(Color32::from_rgb(196, 43, 28))
                        .min_size(vec2(120.0, 30.0));
                    if ui.add(remove).clicked() {
                        let restore = self.restore_canon;
                        run_job(&self.job, &ctx, move || setup::request_uninstall(restore));
                    }
                    if ui.button("Cancel").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            });
        });
    }
}
