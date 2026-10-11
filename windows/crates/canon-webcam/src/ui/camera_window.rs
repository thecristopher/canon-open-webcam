//! The main window: the picture front and center, every control in a tidy sidebar.

use std::time::Duration;

use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Label, Layout, Margin, Rect, RichText, Sense, Stroke, TextureHandle,
    TextureOptions, UiBuilder, Vec2, pos2, vec2,
};

use super::model::{CameraModel, SettingControl, number};
use super::preview::Preview;
use super::style::{self, QuitSignal};
use crate::options::{DEFAULT_SHARPNESS, FrameOptions};
use crate::panel::{CREDITS, PanelState};

pub struct CameraWindow {
    model: CameraModel,
    preview: Preview,
    texture: Option<TextureHandle>,
    exposure_draft: Option<usize>,
    quit: QuitSignal,
}

impl CameraWindow {
    pub fn new(ctx: &egui::Context) -> Self {
        style::apply(ctx);
        Self {
            model: CameraModel::new(ctx.clone()),
            preview: Preview::start(ctx.clone()),
            texture: None,
            exposure_draft: None,
            quit: QuitSignal::open(),
        }
    }
}

impl eframe::App for CameraWindow {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // The uninstaller asks every Canon Webcam window to leave so it can remove the files
        if self.quit.raised() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.model.tick();
        if let Some(image) = self.preview.take() {
            match &mut self.texture {
                Some(texture) => texture.set(image, TextureOptions::LINEAR),
                None => self.texture = Some(ctx.load_texture("preview", image, TextureOptions::LINEAR)),
            }
        }

        egui::Panel::right("inspector")
            .exact_size(340.0)
            .resizable(false)
            .frame(Frame::new().fill(style::PANEL).inner_margin(Margin::same(0)))
            .show(ui, |ui| self.inspector(ui));
        egui::CentralPanel::default().frame(Frame::new().fill(style::STAGE)).show(ui, |ui| self.stage(ui));
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}

// Stage

impl CameraWindow {
    /// The live picture, exactly what everyone on the call sees, with framing gestures and a floating toolbar.
    fn stage(&mut self, ui: &mut egui::Ui) {
        let picture = fitted_16x9(ui.available_rect_before_wrap(), 28.0);
        let response = ui.allocate_rect(picture, Sense::click_and_drag());
        let painter = ui.painter_at(picture.expand(40.0));
        let state = self.model.state();
        let radius = CornerRadius::same(14);

        let shadow = egui::Shadow { offset: [0, 10], blur: 28, spread: 0, color: Color32::from_black_alpha(110) };
        painter.add(shadow.as_shape(picture, radius));
        painter.rect_filled(picture, radius, Color32::BLACK);
        if let Some(texture) = &self.texture {
            egui::Image::new((texture.id(), picture.size())).corner_radius(radius).paint_at(ui, picture);
        }

        // Dragging moves the picture under your finger, like moving a photo inside a frame
        if response.dragged() {
            let delta = response.drag_delta();
            let overflow = self.model.options.zoom - 1.0;
            if overflow > 0.01 {
                let mirrored = if self.model.options.mirror { -1.0 } else { 1.0 };
                self.model.pan_by(
                    -(delta.x as f64) / (overflow * picture.width() as f64 / 2.0) * mirrored,
                    delta.y as f64 / (overflow * picture.height() as f64 / 2.0),
                );
            }
        }
        // Pinch or ctrl+scroll zooms, a plain scroll nudges it
        if response.hovered() {
            let (pinch, scroll) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta.y));
            if pinch != 1.0 {
                self.model.zoom_to(self.model.options.zoom * pinch as f64);
            } else if scroll != 0.0 {
                self.model.zoom_to(self.model.options.zoom + scroll as f64 * 0.004);
            }
        }

        // The engine's own card already says what's going on, so only speak up when there's no picture at all
        if !state.is_live() && (self.texture.is_none() || state == PanelState::NotRunning) {
            self.stage_message(ui, picture, &state);
        }
        status_pill(ui, picture, &state);
        self.toolbar(ui, picture, &state);
    }

    /// Shown over the picture while there's nothing live to look at yet.
    fn stage_message(&self, ui: &mut egui::Ui, picture: Rect, state: &PanelState) {
        ui.painter().rect_filled(picture, CornerRadius::same(14), Color32::from_black_alpha(150));
        // This window is itself an app watching, so resting here really means waking up
        let (title, subtitle) = match state {
            PanelState::Resting => ("Waking up your camera".to_string(), PanelState::Starting.subtitle()),
            _ => (state.title(), state.subtitle()),
        };
        let busy = matches!(state, PanelState::Resting | PanelState::Starting);
        let inner = Rect::from_center_size(picture.center(), vec2(picture.width().min(520.0), 140.0));
        ui.scope_builder(UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Center)), |ui| {
            if busy {
                ui.add(egui::Spinner::new().size(28.0).color(Color32::WHITE));
                ui.add_space(8.0);
            }
            ui.label(RichText::new(title).font(style::semibold(20.0)).color(Color32::WHITE));
            ui.add_space(4.0);
            ui.add(Label::new(RichText::new(subtitle).size(14.0).color(Color32::from_white_alpha(180))).wrap());
            if *state == PanelState::NotRunning && crate::setup::is_installed() {
                ui.add_space(10.0);
                if ui.button("Start Canon Webcam").clicked() {
                    let _ = crate::setup::start_app(false);
                }
            }
        });
    }

    /// The handful of things you reach for mid call, floating over the bottom of the picture.
    fn toolbar(&mut self, ui: &mut egui::Ui, picture: Rect, state: &PanelState) {
        let size = vec2(392.0, 40.0);
        let rect = Rect::from_center_size(pos2(picture.center().x, picture.bottom() - 16.0 - size.y / 2.0), size);
        ui.painter().rect(
            rect,
            CornerRadius::same(20),
            Color32::from_rgba_unmultiplied(28, 30, 34, 225),
            Stroke::new(1.0, Color32::from_white_alpha(18)),
            egui::StrokeKind::Inside,
        );
        let inner = rect.shrink2(vec2(12.0, 4.0));
        ui.scope_builder(UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let zoom = self.model.options.zoom;
            if tool_button(ui, "−", false).on_hover_text("Zoom out").clicked() {
                self.model.zoom_to(zoom - 0.25);
            }
            ui.add_sized([44.0, 24.0], Label::new(RichText::new(format!("{zoom:.1}×")).monospace().size(13.0)));
            if tool_button(ui, "+", false).on_hover_text("Zoom in").clicked() {
                self.model.zoom_to(zoom + 0.25);
            }
            ui.add(egui::Separator::default().vertical().spacing(10.0));
            if tool_button(ui, "Fit", false).on_hover_text("Show the whole picture").clicked() {
                self.model.set_option("framing", "reset");
            }
            let mirror = self.model.options.mirror;
            if tool_button(ui, "Mirror", mirror).on_hover_text("Mirror for everyone").clicked() {
                self.model.set_option("mirror", if mirror { "off" } else { "on" });
            }
            let focus = ui.add_enabled(state.is_live(), tool_button_widget("Autofocus", false));
            if focus.on_hover_text("Autofocus").clicked() {
                self.model.focus("auto", 1);
            }
        });
    }
}

fn tool_button_widget(text: &str, active: bool) -> egui::Button<'static> {
    let color = if active { style::ACCENT } else { Color32::from_gray(235) };
    egui::Button::new(RichText::new(text).size(14.0).color(color)).frame(false).min_size(vec2(30.0, 26.0))
}

fn tool_button(ui: &mut egui::Ui, text: &str, active: bool) -> egui::Response {
    ui.add(tool_button_widget(text, active))
}

/// A frosted pill in the corner: a colored dot and what the camera is up to.
fn status_pill(ui: &mut egui::Ui, picture: Rect, state: &PanelState) {
    let text = match state {
        PanelState::Live { model, detail } if detail.is_empty() => model.clone(),
        PanelState::Live { model, detail } => format!("{model} · {}", detail.rsplit(" · ").next().unwrap_or_default()),
        other => other.title(),
    };
    let dot = match state {
        PanelState::Live { .. } => Color32::from_rgb(52, 199, 89),
        PanelState::Starting => Color32::from_rgb(10, 132, 255),
        PanelState::Trouble(_) | PanelState::NotRunning => Color32::from_rgb(255, 159, 10),
        PanelState::Resting | PanelState::WaitingForCamera => Color32::from_gray(150),
    };
    let galley = ui.painter().layout_no_wrap(text, style::semibold(13.0), Color32::WHITE);
    let size = vec2(galley.size().x + 36.0, 28.0);
    let rect = Rect::from_min_size(picture.min + vec2(14.0, 14.0), size);
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(14), Color32::from_rgba_unmultiplied(20, 22, 26, 200));
    painter.circle_filled(pos2(rect.left() + 15.0, rect.center().y), 4.0, dot);
    painter.galley(pos2(rect.left() + 26.0, rect.center().y - galley.size().y / 2.0), galley, Color32::WHITE);
}

fn fitted_16x9(space: Rect, padding: f32) -> Rect {
    let available = (space.size() - Vec2::splat(padding * 2.0)).max(vec2(100.0, 56.0));
    let size = if available.x / available.y > 16.0 / 9.0 {
        vec2(available.y * 16.0 / 9.0, available.y)
    } else {
        vec2(available.x, available.x * 9.0 / 16.0)
    };
    Rect::from_center_size(space.center(), size)
}

// Inspector

impl CameraWindow {
    /// Every knob, grouped the way Settings groups things.
    fn inspector(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            Frame::new().inner_margin(Margin::symmetric(18, 18)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.adjust_section(ui);
                self.framing_section(ui);
                self.camera_section(ui);
                self.output_section(ui);
                if let Some(problem) = self.model.problem() {
                    style::card(ui, |ui| {
                        ui.add(Label::new(RichText::new(problem).color(style::WARNING)).wrap());
                    });
                }
                ui.add_space(12.0);
                ui.add(Label::new(RichText::new(CREDITS).size(11.0).color(style::FAINT)).wrap());
            });
        });
    }

    fn adjust_section(&mut self, ui: &mut egui::Ui) {
        if section_header(ui, "Adjust", Some("Reset")) {
            self.model.set_option("picture", "reset");
        }
        style::card(ui, |ui| {
            self.adjustment(ui, "Brightness", "brightness", -1.0..=1.0, 0.0, None);
            self.adjustment(ui, "Contrast", "contrast", -1.0..=1.0, 0.0, None);
            self.adjustment(ui, "Saturation", "saturation", -1.0..=1.0, 0.0, None);
            self.adjustment(ui, "Warmth", "warmth", -1.0..=1.0, 0.0, Some(("Cool", "Warm")));
            self.adjustment(ui, "Tint", "tint", -1.0..=1.0, 0.0, Some(("Green", "Magenta")));
            self.adjustment(ui, "Sharpness", "sharpness", 0.0..=1.0, DEFAULT_SHARPNESS, None);
        });
    }

    fn framing_section(&mut self, ui: &mut egui::Ui) {
        if section_header(ui, "Framing", Some("Center")) {
            self.model.set_option("framing", "reset");
        }
        style::card(ui, |ui| {
            self.adjustment(ui, "Zoom", "zoom", 1.0..=3.0, 1.0, None);
            let mut mirror = self.model.options.mirror;
            if ui.checkbox(&mut mirror, "Mirror for everyone").changed() {
                self.model.set_option("mirror", if mirror { "on" } else { "off" });
            }
        });
        footnote(ui, "Drag the picture to reframe it, scroll or pinch to zoom.");
    }

    fn camera_section(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "Camera", None);
        let live = self.model.state().is_live();
        style::card(ui, |ui| {
            self.exposure_row(ui);
            for (title, name) in
                [("White balance", "white-balance"), ("ISO", "iso"), ("Aperture", "aperture"), ("Shutter", "shutter")]
            {
                self.picker_row(ui, title, name);
            }
            ui.horizontal(|ui| {
                ui.label("Focus");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_enabled_ui(live, |ui| {
                        if ui.button("»").on_hover_text("Focus farther").clicked() {
                            self.model.focus("far", 2);
                        }
                        if ui.button("Auto").clicked() {
                            self.model.focus("auto", 1);
                        }
                        if ui.button("«").on_hover_text("Focus closer").clicked() {
                            self.model.focus("near", 2);
                        }
                    });
                });
            });
        });
        let note = if !live {
            "These come from the camera itself, so they unlock while it's live."
        } else if ["iso", "aperture", "shutter"]
            .iter()
            .all(|n| matches!(self.model.control(n), SettingControl::LockedByMode(_)))
        {
            "Turn the camera's dial to M to set ISO, aperture and shutter yourself."
        } else {
            "Your camera remembers these on its own."
        };
        footnote(ui, note);
    }

    fn output_section(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "Output", None);
        style::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Frame rate");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let current = self.model.options.frame_rate;
                    for (rate, label) in [(60, "60 fps"), (30, "30 fps"), (0, "Camera")] {
                        if ui.selectable_label(current == rate, label).clicked() && current != rate {
                            let value = if rate == 0 { "camera".to_string() } else { rate.to_string() };
                            self.model.set_option("fps", &value);
                        }
                    }
                });
            });
            value_row(ui, "Resolution", "1920 × 1080");
            value_row(ui, "Shows up as", canon_shm::CAMERA_NAME);
        });
        footnote(ui, "Camera sends every frame as it arrives (24 a second on the T7).");
    }

    /// A labeled slider with its value on the right. Double click the label to put it back.
    fn adjustment(
        &mut self,
        ui: &mut egui::Ui,
        title: &str,
        name: &str,
        range: std::ops::RangeInclusive<f64>,
        reset: f64,
        ends: Option<(&str, &str)>,
    ) {
        let mut value = *option_mut(&mut self.model.options, name);
        ui.horizontal(|ui| {
            if ui.add(Label::new(title).sense(Sense::click())).on_hover_text("Double click to reset").double_clicked() {
                value = reset;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let shown = if name == "zoom" {
                    format!("{value:.1}×")
                } else {
                    let percent = (value * 100.0).round() as i32;
                    if *range.start() < 0.0 && percent > 0 { format!("+{percent}") } else { percent.to_string() }
                };
                let color = if (value - reset).abs() < 1e-6 { style::FAINT } else { style::SECONDARY };
                ui.label(RichText::new(shown).monospace().color(color));
            });
        });
        ui.spacing_mut().slider_width = ui.available_width();
        ui.add(egui::Slider::new(&mut value, range).show_value(false));
        if let Some((low, high)) = ends {
            ui.horizontal(|ui| {
                ui.label(RichText::new(low).size(11.0).color(style::FAINT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(high).size(11.0).color(style::FAINT));
                });
            });
        }
        ui.add_space(4.0);
        if value != *option_mut(&mut self.model.options, name) {
            *option_mut(&mut self.model.options, name) = value;
            self.model.set_soon(name, number(value));
        }
    }

    /// Exposure is an ordered range, so it's a slider that sends when you let go.
    fn exposure_row(&mut self, ui: &mut egui::Ui) {
        match self.model.control("exposure") {
            SettingControl::Choices(choices, current) => {
                let current_index = choices.iter().position(|c| *c == current).unwrap_or(choices.len() / 2);
                let mut index = self.exposure_draft.unwrap_or(current_index);
                value_row(ui, "Exposure", &choices[index.min(choices.len() - 1)]);
                ui.spacing_mut().slider_width = ui.available_width();
                let response =
                    ui.add(egui::Slider::new(&mut index, 0..=choices.len().saturating_sub(1)).show_value(false));
                if response.dragged() || response.changed() {
                    self.exposure_draft = Some(index);
                }
                if (response.drag_stopped() || (response.changed() && !response.dragged()))
                    && let Some(picked) = self.exposure_draft.take()
                {
                    self.model.set_camera("exposure", &choices[picked]);
                }
            }
            other => self.locked_row(ui, "Exposure", other),
        }
    }

    /// A camera setting with named values, shown as a menu. Says so plainly when the mode dial is in charge.
    fn picker_row(&mut self, ui: &mut egui::Ui, title: &str, name: &str) {
        match self.model.control(name) {
            SettingControl::Choices(choices, current) => {
                ui.horizontal(|ui| {
                    ui.label(title);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        egui::ComboBox::from_id_salt(name).selected_text(&current).width(130.0).show_ui(ui, |ui| {
                            for choice in &choices {
                                if ui.selectable_label(*choice == current, choice).clicked() && *choice != current {
                                    self.model.set_camera(name, choice);
                                }
                            }
                        });
                    });
                });
            }
            other => self.locked_row(ui, title, other),
        }
    }

    fn locked_row(&self, ui: &mut egui::Ui, title: &str, control: SettingControl) {
        let shown = match control {
            SettingControl::LockedByMode(current) => format!("{current} · set by the camera"),
            _ => "–".into(),
        };
        value_row(ui, title, &shown);
    }
}

fn option_mut<'a>(options: &'a mut FrameOptions, name: &str) -> &'a mut f64 {
    match name {
        "brightness" => &mut options.brightness,
        "contrast" => &mut options.contrast,
        "saturation" => &mut options.saturation,
        "warmth" => &mut options.warmth,
        "tint" => &mut options.tint,
        "sharpness" => &mut options.sharpness,
        _ => &mut options.zoom,
    }
}

/// A section title with a small action on the right, like Reset. True when the action was clicked.
fn section_header(ui: &mut egui::Ui, title: &str, action: Option<&str>) -> bool {
    ui.add_space(10.0);
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).font(style::semibold(14.0)).color(style::SECONDARY));
        if let Some(action) = action {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                clicked = ui.link(RichText::new(action).size(12.0)).clicked();
            });
        }
    });
    ui.add_space(4.0);
    clicked
}

fn value_row(ui: &mut egui::Ui, title: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(title);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).color(style::SECONDARY));
        });
    });
}

fn footnote(ui: &mut egui::Ui, text: &str) {
    ui.add_space(2.0);
    ui.add(Label::new(RichText::new(text).size(11.5).color(style::FAINT)).wrap());
}
