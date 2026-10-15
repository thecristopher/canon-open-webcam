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
