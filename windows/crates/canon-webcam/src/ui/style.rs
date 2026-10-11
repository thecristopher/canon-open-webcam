use std::sync::Arc;

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame, Margin, Stroke};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{OpenEventW, SYNCHRONIZATION_SYNCHRONIZE, WaitForSingleObject};

use crate::setup::QUIT_EVENT;

pub const STAGE: Color32 = Color32::from_rgb(22, 23, 26);
pub const PANEL: Color32 = Color32::from_rgb(32, 33, 37);
pub const CARD: Color32 = Color32::from_rgb(42, 44, 49);
pub const ACCENT: Color32 = Color32::from_rgb(76, 158, 255);
pub const SECONDARY: Color32 = Color32::from_gray(185);
pub const FAINT: Color32 = Color32::from_gray(120);
pub const WARNING: Color32 = Color32::from_rgb(255, 176, 64);
pub const GOOD: Color32 = Color32::from_rgb(52, 199, 89);

/// Dark, rounded and in Segoe UI, so the window feels like it belongs on Windows 11.
pub fn apply(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let windows_fonts =
        std::env::var_os("WINDIR").map_or_else(|| r"C:\Windows".into(), std::path::PathBuf::from).join("Fonts");
    for (name, file) in [("segoe", "segoeui.ttf"), ("segoe-semibold", "seguisb.ttf")] {
        if let Ok(bytes) = std::fs::read(windows_fonts.join(file)) {
            fonts.font_data.insert(name.into(), Arc::new(FontData::from_owned(bytes)));
        }
    }
    if fonts.font_data.contains_key("segoe") {
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "segoe".into());
    }
    let mut semibold = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    if fonts.font_data.contains_key("segoe-semibold") {
        semibold.insert(0, "segoe-semibold".into());
    }
    fonts.families.insert(FontFamily::Name("semibold".into()), semibold);
    ctx.set_fonts(fonts);

    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.55);
    visuals.hyperlink_color = ACCENT;
    for widgets in [&mut visuals.widgets.inactive, &mut visuals.widgets.hovered, &mut visuals.widgets.active] {
        widgets.corner_radius = CornerRadius::same(6);
    }
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(58, 60, 66);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(58, 60, 66);
    visuals.slider_trailing_fill = true;
    // Always dark, like the Mac window: the picture reads better on a dark stage
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, visuals);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style.spacing.item_spacing = egui::vec2(8.0, 7.0);
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
        style.spacing.interact_size.y = 24.0;
        style.text_styles.insert(egui::TextStyle::Body, FontId::proportional(14.0));
        style.text_styles.insert(egui::TextStyle::Button, FontId::proportional(14.0));
    });
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}

/// A grouped box of rows, like a Settings card.
pub fn card<R>(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    Frame::new()
        .fill(CARD)
        .corner_radius(CornerRadius::same(10))
        .stroke(Stroke::new(1.0, Color32::from_white_alpha(10)))
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            contents(ui)
        })
        .inner
}

/// The event the uninstaller raises when it needs every Canon Webcam window gone.
pub struct QuitSignal(Option<HANDLE>);

impl QuitSignal {
    pub fn open() -> Self {
        Self(unsafe { OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, false, QUIT_EVENT) }.ok())
    }

    pub fn raised(&self) -> bool {
        self.0.is_some_and(|event| unsafe { WaitForSingleObject(event, 0) } == WAIT_OBJECT_0)
    }
}

impl Drop for QuitSignal {
    fn drop(&mut self) {
        if let Some(event) = self.0 {
            unsafe {
                let _ = CloseHandle(event);
            }
        }
    }
}
