use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Everything we do to the picture on the PC side, as opposed to settings the camera keeps itself.
/// Same fields and JSON as the Mac app's options.json.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FrameOptions {
    /// Apps already mirror your self view, so mirroring here flips what everyone else sees.
    pub mirror: bool,
    /// Digital zoom, 1 means the full live view.
    pub zoom: f64,
    /// Where the zoomed frame sits, from -1 (left or bottom edge) to 1 (right or top edge).
    pub pan_x: f64,
    pub pan_y: f64,
    // Picture adjustments all run from -1 to 1 with 0 meaning untouched
    pub brightness: f64,
    pub contrast: f64,
    pub saturation: f64,
    pub warmth: f64,
    pub tint: f64,
    /// How much crispness to add back after upscaling, from 0 to 1.
    pub sharpness: f64,
    /// Frames per second sent to apps. 0 means every camera frame as it arrives (24 on the T7).
    pub frame_rate: u32,
}

impl Default for FrameOptions {
    fn default() -> Self {
        Self {
            mirror: false,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            warmth: 0.0,
            tint: 0.0,
            sharpness: 0.4,
            frame_rate: 0,
        }
    }
}

pub const ZOOM_RANGE: std::ops::RangeInclusive<f64> = 1.0..=3.0;
pub const ADJUSTMENT_RANGE: std::ops::RangeInclusive<f64> = -1.0..=1.0;
pub const FRAME_RATES: [u32; 3] = [0, 30, 60];
pub const DEFAULT_SHARPNESS: f64 = 0.4;

pub const ADJUSTABLE_NAMES: [&str; 13] = [
    "mirror",
    "zoom",
    "pan-x",
    "pan-y",
    "brightness",
    "contrast",
    "saturation",
    "warmth",
    "tint",
    "sharpness",
    "fps",
    "picture",
    "framing",
];

impl FrameOptions {
    /// True when the colors are untouched, so the renderer can skip that work entirely.
    pub fn has_color_adjustments(&self) -> bool {
        [self.brightness, self.contrast, self.saturation, self.warmth, self.tint].iter().any(|v| *v != 0.0)
    }

    /// Applies one change like ("brightness", "0.2") or ("picture", "reset"), or explains what's wrong with it.
    pub fn applying(&self, name: &str, value: &str) -> Result<FrameOptions, String> {
        let mut changed = self.clone();
        match name {
            "mirror" => changed.mirror = parse_switch(value).ok_or("mirror takes on or off")?,
            "zoom" => {
                let zoom: f64 = value.replace('x', "").parse().map_err(|_| "zoom goes from 1 to 3, like 1.5")?;
                if !ZOOM_RANGE.contains(&zoom) {
                    return Err("zoom goes from 1 to 3, like 1.5".into());
                }
                changed.zoom = zoom;
            }
            "pan-x" | "pan-y" | "brightness" | "contrast" | "saturation" | "warmth" | "tint" => {
                let amount = value.parse::<f64>().ok().filter(|a| ADJUSTMENT_RANGE.contains(a));
                let amount = amount.ok_or_else(|| format!("{name} goes from -1 to 1, like 0.2"))?;
                *changed.adjustment_mut(name) = amount;
            }
            "sharpness" => {
                changed.sharpness = value
                    .parse::<f64>()
                    .ok()
                    .filter(|a| (0.0..=1.0).contains(a))
                    .ok_or("sharpness goes from 0 to 1")?;
            }
            "fps" => {
                let rate = if value == "camera" { Some(0) } else { value.parse().ok() };
                changed.frame_rate = rate.filter(|r| FRAME_RATES.contains(r)).ok_or("fps takes camera, 30 or 60")?;
            }
            "picture" if value == "reset" => changed.reset_picture(),
            "framing" if value == "reset" => changed.reset_framing(),
            _ => return Err(format!("\"{name}\" isn't something I can change")),
        }
        Ok(changed)
    }

    /// A short confirmation for the log and the CLI.
    pub fn describe(&self, name: &str) -> String {
        match name {
            "mirror" => format!("Mirror is {}.", if self.mirror { "on" } else { "off" }),
            "zoom" => format!("Zoom is {:.1}x.", self.zoom),
            "fps" if self.frame_rate == 0 => "Sending every camera frame as it arrives.".into(),
            "fps" => format!("Sending {} frames per second.", self.frame_rate),
            "picture" => "Picture adjustments are back to normal.".into(),
            "framing" => "Framing is back to the full picture.".into(),
            _ => format!("{name} updated."),
        }
    }

    pub fn reset_picture(&mut self) {
        let defaults = FrameOptions::default();
        self.brightness = defaults.brightness;
        self.contrast = defaults.contrast;
        self.saturation = defaults.saturation;
        self.warmth = defaults.warmth;
        self.tint = defaults.tint;
        self.sharpness = defaults.sharpness;
    }

    pub fn reset_framing(&mut self) {
        self.zoom = 1.0;
        self.pan_x = 0.0;
        self.pan_y = 0.0;
    }

    fn adjustment_mut(&mut self, name: &str) -> &mut f64 {
        match name {
            "pan-x" => &mut self.pan_x,
            "pan-y" => &mut self.pan_y,
            "brightness" => &mut self.brightness,
            "contrast" => &mut self.contrast,
            "saturation" => &mut self.saturation,
            "warmth" => &mut self.warmth,
            _ => &mut self.tint,
        }
    }
}

pub fn parse_switch(value: &str) -> Option<bool> {
    match value.to_lowercase().as_str() {
        "on" | "true" | "yes" | "1" => Some(true),
        "off" | "false" | "no" | "0" => Some(false),
        _ => None,
    }
}

/// Keeps options between runs in %APPDATA%\CanonWebcam\options.json.
pub struct OptionsStore {
    path: PathBuf,
}

impl OptionsStore {
    pub fn new() -> Self {
        let base = std::env::var_os("APPDATA").map_or_else(std::env::temp_dir, PathBuf::from);
        Self { path: base.join("CanonWebcam").join("options.json") }
    }

    #[cfg(test)]
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    /// A missing or broken file just means defaults.
    pub fn load(&self) -> FrameOptions {
        std::fs::read(&self.path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    pub fn save(&self, options: &FrameOptions) -> std::io::Result<()> {
        if let Some(folder) = self.path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        std::fs::write(&self.path, serde_json::to_vec_pretty(options)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_takes_a_number_in_range() {
        let changed = FrameOptions::default().applying("brightness", "0.2").unwrap();
        assert_eq!(changed.brightness, 0.2);
    }

    #[test]
    fn out_of_range_values_explain_themselves() {
        let error = FrameOptions::default().applying("contrast", "4").unwrap_err();
        assert!(error.contains("-1 to 1"), "{error}");
    }

    #[test]
    fn zoom_accepts_an_x_suffix() {
        assert_eq!(FrameOptions::default().applying("zoom", "1.5x").unwrap().zoom, 1.5);
    }

    #[test]
    fn fps_takes_camera_or_a_rate() {
        assert_eq!(FrameOptions::default().applying("fps", "60").unwrap().frame_rate, 60);
        assert_eq!(FrameOptions::default().applying("fps", "camera").unwrap().frame_rate, 0);
        assert!(FrameOptions::default().applying("fps", "24").is_err());
    }

    #[test]
    fn picture_reset_keeps_the_framing() {
        let mut options = FrameOptions { brightness: 0.5, zoom: 2.0, sharpness: 0.9, ..Default::default() };
        options = options.applying("picture", "reset").unwrap();
        assert_eq!((options.brightness, options.sharpness, options.zoom), (0.0, DEFAULT_SHARPNESS, 2.0));
    }

    #[test]
    fn framing_reset_keeps_the_picture() {
        let options = FrameOptions { brightness: 0.5, zoom: 2.0, pan_x: 1.0, ..Default::default() };
        let options = options.applying("framing", "reset").unwrap();
        assert_eq!((options.brightness, options.zoom, options.pan_x), (0.5, 1.0, 0.0));
    }

    #[test]
    fn unknown_names_are_refused() {
        assert!(FrameOptions::default().applying("vibes", "on").is_err());
    }

    #[test]
    fn a_mac_options_file_with_only_mirror_and_zoom_still_loads() {
        let options: FrameOptions = serde_json::from_str(r#"{"mirror": true, "zoom": 1.5}"#).unwrap();
        assert!(options.mirror);
        assert_eq!((options.zoom, options.sharpness), (1.5, DEFAULT_SHARPNESS));
    }

    #[test]
    fn options_survive_a_save_and_load() {
        let path = std::env::temp_dir().join(format!("canon-webcam-options-test-{}.json", std::process::id()));
        let store = OptionsStore::at(path.clone());
        let options = FrameOptions { warmth: -0.3, frame_rate: 30, ..Default::default() };
        store.save(&options).unwrap();
        assert_eq!(store.load(), options);
        let _ = std::fs::remove_file(path);
    }
}
