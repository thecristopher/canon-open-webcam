use crate::codes::CanonProperty;

/// A camera setting a person would want to change mid call, with Canon's codes translated to words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CameraSetting {
    Iso,
    Aperture,
    Shutter,
    Exposure,
    WhiteBalance,
    ColorTemperature,
}

impl CameraSetting {
    pub const ALL: [CameraSetting; 6] = [
        CameraSetting::Iso,
        CameraSetting::Aperture,
        CameraSetting::Shutter,
        CameraSetting::Exposure,
        CameraSetting::WhiteBalance,
        CameraSetting::ColorTemperature,
    ];

    /// The name the CLI and the control pipe use.
    pub fn name(self) -> &'static str {
        match self {
            CameraSetting::Iso => "iso",
            CameraSetting::Aperture => "aperture",
            CameraSetting::Shutter => "shutter",
            CameraSetting::Exposure => "exposure",
            CameraSetting::WhiteBalance => "white-balance",
            CameraSetting::ColorTemperature => "color-temp",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }

    pub fn property(self) -> CanonProperty {
        match self {
            CameraSetting::Iso => CanonProperty::Iso,
            CameraSetting::Aperture => CanonProperty::Aperture,
            CameraSetting::Shutter => CanonProperty::ShutterSpeed,
            CameraSetting::Exposure => CanonProperty::ExposureCompensation,
            CameraSetting::WhiteBalance => CanonProperty::WhiteBalance,
            CameraSetting::ColorTemperature => CanonProperty::ColorTemperature,
        }
    }

    pub fn label(self, code: u32) -> String {
        match self {
            CameraSetting::ColorTemperature => format!("{code}K"),
            CameraSetting::Aperture if code == 0 => "auto".into(),
            CameraSetting::Aperture => self.lookup(code).map_or_else(|| hex(code), |l| format!("f/{l}")),
            _ => self.lookup(code).map_or_else(|| hex(code), str::to_string),
        }
    }

    /// Accepts what a person would type: "400", "auto", "f/5.6", "1/60", "+1", "daylight", "5000K".
    pub fn code(self, input: &str, choices: &[u32]) -> Option<u32> {
        let typed = normalized(input);
        if self == CameraSetting::ColorTemperature {
            return typed.replace('k', "").parse().ok();
        }
        // Prefer codes the camera says it accepts, since tables repeat labels for 1/3 and 1/2 stops
        let candidates: Vec<u32> =
            self.table().iter().filter(|(label, _)| normalized(label) == typed).map(|&(_, code)| code).collect();
        candidates.iter().copied().find(|c| choices.contains(c)).or_else(|| candidates.first().copied())
    }

    fn lookup(self, code: u32) -> Option<&'static str> {
        self.table().iter().find(|&&(_, c)| c == code).map(|&(label, _)| label)
    }

    fn table(self) -> &'static [(&'static str, u32)] {
        match self {
            CameraSetting::Iso => ISO,
            CameraSetting::Aperture => APERTURE,
            CameraSetting::Shutter => SHUTTER,
            CameraSetting::Exposure => EXPOSURE,
            CameraSetting::WhiteBalance => WHITE_BALANCE,
            CameraSetting::ColorTemperature => &[],
        }
    }
}

fn normalized(text: &str) -> String {
    let mut cleaned = text.to_lowercase().replace(' ', "");
    for prefix in ["f/", "+", "iso"] {
        if let Some(rest) = cleaned.strip_prefix(prefix) {
            cleaned = rest.to_string();
        }
    }
    cleaned
}

fn hex(code: u32) -> String {
    format!("0x{code:X}")
}

// Tables lifted from libgphoto2's config.c, trimmed to what an EOS body reports

#[rustfmt::skip]
const ISO: &[(&str, u32)] = &[
    ("auto", 0x00), ("100", 0x48), ("125", 0x4B), ("160", 0x4D), ("200", 0x50), ("250", 0x53),
    ("320", 0x55), ("400", 0x58), ("500", 0x5B), ("640", 0x5D), ("800", 0x60), ("1000", 0x63),
    ("1250", 0x65), ("1600", 0x68), ("2000", 0x6B), ("2500", 0x6D), ("3200", 0x70), ("4000", 0x73),
    ("5000", 0x75), ("6400", 0x78), ("8000", 0x7B), ("10000", 0x7D), ("12800", 0x80), ("25600", 0x88),
];

#[rustfmt::skip]
const APERTURE: &[(&str, u32)] = &[
    ("1", 0x08), ("1.1", 0x0B), ("1.2", 0x0C), ("1.2", 0x0D), ("1.4", 0x10), ("1.6", 0x13),
    ("1.8", 0x14), ("1.8", 0x15), ("2", 0x18), ("2.2", 0x1B), ("2.5", 0x1C), ("2.5", 0x1D),
    ("2.8", 0x20), ("3.2", 0x23), ("3.5", 0x24), ("3.5", 0x25), ("4", 0x28), ("4.5", 0x2B),
    ("4.5", 0x2C), ("5", 0x2D), ("5.6", 0x30), ("6.3", 0x33), ("6.7", 0x34), ("7.1", 0x35),
    ("8", 0x38), ("9", 0x3B), ("9.5", 0x3C), ("10", 0x3D), ("11", 0x40), ("13", 0x43),
    ("13", 0x44), ("14", 0x45), ("16", 0x48), ("18", 0x4B), ("19", 0x4C), ("20", 0x4D),
    ("22", 0x50), ("25", 0x53), ("27", 0x54), ("29", 0x55), ("32", 0x58), ("36", 0x5B),
];

#[rustfmt::skip]
const SHUTTER: &[(&str, u32)] = &[
    ("auto", 0x00), ("30", 0x10), ("15", 0x18), ("8", 0x20), ("4", 0x28), ("2", 0x30), ("1", 0x38),
    ("0.5", 0x40), ("1/4", 0x48), ("1/5", 0x4B), ("1/6", 0x4C), ("1/8", 0x50), ("1/10", 0x53),
    ("1/10", 0x54), ("1/13", 0x55), ("1/15", 0x58), ("1/20", 0x5B), ("1/20", 0x5C), ("1/25", 0x5D),
    ("1/30", 0x60), ("1/40", 0x63), ("1/45", 0x64), ("1/50", 0x65), ("1/60", 0x68), ("1/80", 0x6B),
    ("1/90", 0x6C), ("1/100", 0x6D), ("1/125", 0x70), ("1/160", 0x73), ("1/180", 0x74),
    ("1/200", 0x75), ("1/250", 0x78), ("1/320", 0x7B), ("1/350", 0x7C), ("1/400", 0x7D),
    ("1/500", 0x80), ("1/640", 0x83), ("1/750", 0x84), ("1/800", 0x85), ("1/1000", 0x88),
    ("1/1250", 0x8B), ("1/1500", 0x8C), ("1/1600", 0x8D), ("1/2000", 0x90), ("1/2500", 0x93),
    ("1/3000", 0x94), ("1/3200", 0x95), ("1/4000", 0x98),
];

#[rustfmt::skip]
const EXPOSURE: &[(&str, u32)] = &[
    ("3", 0x18), ("2.6", 0x15), ("2.5", 0x14), ("2.3", 0x13), ("2", 0x10), ("1.6", 0x0D),
    ("1.5", 0x0C), ("1.3", 0x0B), ("1", 0x08), ("0.6", 0x05), ("0.5", 0x04), ("0.3", 0x03),
    ("0", 0x00), ("-0.3", 0xFD), ("-0.5", 0xFC), ("-0.6", 0xFB), ("-1", 0xF8), ("-1.3", 0xF5),
    ("-1.5", 0xF4), ("-1.6", 0xF3), ("-2", 0xF0), ("-2.3", 0xED), ("-2.5", 0xEC), ("-2.6", 0xEB),
    ("-3", 0xE8),
];

#[rustfmt::skip]
const WHITE_BALANCE: &[(&str, u32)] = &[
    ("auto", 0), ("daylight", 1), ("cloudy", 2), ("tungsten", 3), ("fluorescent", 4),
    ("flash", 5), ("custom", 6), ("shade", 8), ("kelvin", 9), ("auto-white", 23),
];

#[cfg(test)]
mod tests {
    use super::CameraSetting::*;

    #[test]
    fn iso_codes_read_as_numbers() {
        assert_eq!(Iso.label(0x58), "400");
    }

    #[test]
    fn aperture_labels_get_the_f_stop_prefix() {
        assert_eq!(Aperture.label(0x30), "f/5.6");
    }

    #[test]
    fn aperture_zero_means_the_camera_picks() {
        assert_eq!(Aperture.label(0), "auto");
    }

    #[test]
    fn unknown_codes_show_up_as_hex_instead_of_lying() {
        assert_eq!(Iso.label(0x99), "0x99");
    }

    #[test]
    fn typing_an_f_stop_finds_its_code() {
        assert_eq!(Aperture.code("f/5.6", &[]), Some(0x30));
    }

    #[test]
    fn duplicate_labels_prefer_what_the_camera_accepts() {
        assert_eq!(Aperture.code("4.5", &[0x2C]), Some(0x2C));
    }

    #[test]
    fn plus_signs_are_optional_for_exposure() {
        assert_eq!(Exposure.code("+1", &[]), Some(0x08));
    }

    #[test]
    fn white_balance_names_ignore_case() {
        assert_eq!(WhiteBalance.code("Daylight", &[]), Some(1));
    }

    #[test]
    fn color_temperature_takes_kelvin() {
        assert_eq!(ColorTemperature.code("5200K", &[]), Some(5200));
    }

    #[test]
    fn nonsense_input_finds_nothing() {
        assert_eq!(Iso.code("over 9000", &[]), None);
    }

    #[test]
    fn names_round_trip() {
        for setting in super::CameraSetting::ALL {
            assert_eq!(super::CameraSetting::from_name(setting.name()), Some(setting));
        }
    }
}
