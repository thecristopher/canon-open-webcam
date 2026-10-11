//! What the app says about the engine, shared by the tray, the window and the setup screens.

use crate::control::{EngineStatus, PhaseKind};

#[derive(Clone, Debug, PartialEq)]
pub enum PanelState {
    NotRunning,
    Resting,
    WaitingForCamera,
    Starting,
    Live { model: String, detail: String },
    Trouble(String),
}

impl PanelState {
    pub fn from(status: Option<&EngineStatus>) -> Self {
        let Some(status) = status else { return PanelState::NotRunning };
        match status.kind {
            PhaseKind::Idle => PanelState::Resting,
            PhaseKind::WaitingForCamera => PanelState::WaitingForCamera,
            PhaseKind::Starting => PanelState::Starting,
            PhaseKind::Trouble => PanelState::Trouble(status.trouble.clone().unwrap_or_default()),
            PhaseKind::Live => {
                let mut parts = Vec::new();
                if let Some(lens) = &status.lens {
                    parts.push(lens.clone());
                }
                if status.frames_per_second > 0.0 {
                    parts.push(format!("{:.0} fps", status.frames_per_second));
                } else if !status.apps_watching {
                    parts.push("warm, no app watching".into());
                }
                PanelState::Live {
                    model: status.model.clone().unwrap_or_else(|| "Canon camera".into()),
                    detail: parts.join(" · "),
                }
            }
        }
    }

    pub fn is_live(&self) -> bool {
        matches!(self, PanelState::Live { .. })
    }

    pub fn title(&self) -> String {
        match self {
            PanelState::NotRunning => "Canon Webcam isn't running".into(),
            PanelState::Resting => "Resting".into(),
            PanelState::WaitingForCamera => "Waiting for your camera".into(),
            PanelState::Starting => "Starting camera".into(),
            PanelState::Live { model, .. } => model.clone(),
            PanelState::Trouble(_) => "Can't reach the camera".into(),
        }
    }

    pub fn subtitle(&self) -> String {
        match self {
            PanelState::NotRunning => "Start it from the Start menu to use your Canon as a webcam".into(),
            PanelState::Resting => "The camera wakes up when an app opens Canon Webcam".into(),
            PanelState::WaitingForCamera => "Plug in the Canon over USB and turn it on".into(),
            PanelState::Starting => "Live view takes a second or two".into(),
            PanelState::Live { detail, .. } if detail.is_empty() => "Live".into(),
            PanelState::Live { detail, .. } => format!("Live · {detail}"),
            PanelState::Trouble(reason) => reason.clone(),
        }
    }
}

pub const CREDITS: &str = "Made by Cristopher Cervantes.\n\nAn independent project, not affiliated with or endorsed by Canon Inc. \
Canon and EOS are trademarks of Canon Inc., used here only to say which cameras this works with.";

/// The app's badge: a white camera on a blue rounded square, readable on light and dark taskbars alike.
/// RGBA rows, drawn here so there's no icon file to ship.
pub fn icon_rgba(size: u32) -> Vec<u8> {
    let s = size as f32 / 32.0;
    let inside_round_rect = |x: f32, y: f32, (l, t, r, b): (f32, f32, f32, f32), radius: f32| {
        let (l, t, r, b, radius) = (l * s, t * s, r * s, b * s, radius * s);
        let cx = x.clamp(l + radius, r - radius);
        let cy = y.clamp(t + radius, b - radius);
        (x - cx).powi(2) + (y - cy).powi(2) <= radius * radius && x >= l && x <= r && y >= t && y <= b
    };
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let i = ((y * size + x) * 4) as usize;
            if !inside_round_rect(fx, fy, (0.0, 0.0, 32.0, 32.0), 7.0) {
                continue;
            }
            // A gentle top to bottom blue
            let shade = fy / size as f32;
            let mut color =
                [(52.0 - 18.0 * shade) as u8, (120.0 - 40.0 * shade) as u8, (246.0 - 40.0 * shade) as u8, 255];
            let body = inside_round_rect(fx, fy, (6.0, 11.0, 26.0, 25.0), 3.0)
                || inside_round_rect(fx, fy, (12.0, 8.0, 20.0, 13.0), 1.5);
            let distance = ((fx - 16.0 * s).powi(2) + (fy - 18.0 * s).powi(2)).sqrt() / s;
            if body && !(3.2..5.2).contains(&distance) {
                color = [255, 255, 255, 255];
            }
            pixels[i..i + 4].copy_from_slice(&color);
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::FrameOptions;

    fn status(kind: PhaseKind) -> EngineStatus {
        EngineStatus {
            state: String::new(),
            kind,
            trouble: Some("cable".into()),
            model: Some("Canon EOS 1500D".into()),
            lens: Some("EF-S18-55mm".into()),
            frames_per_second: 24.0,
            apps_watching: true,
            options: FrameOptions::default(),
            settings: Default::default(),
        }
    }

    #[test]
    fn live_shows_the_model_and_speed() {
        let state = PanelState::from(Some(&status(PhaseKind::Live)));
        assert_eq!(state.title(), "Canon EOS 1500D");
        assert_eq!(state.subtitle(), "Live · EF-S18-55mm · 24 fps");
    }

    #[test]
    fn no_status_means_not_running() {
        assert_eq!(PanelState::from(None), PanelState::NotRunning);
    }

    #[test]
    fn trouble_explains_itself() {
        assert_eq!(PanelState::from(Some(&status(PhaseKind::Trouble))).subtitle(), "cable");
    }
}
