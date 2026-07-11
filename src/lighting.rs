//! Lighting effect spec shared by the daemon, tray, and CLI.

use razer_hid::Mouse;
use razer_proto::Rgb;

/// A lighting effect to apply to all lit zones of the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectSpec {
    Static(Rgb),
    Breathing(Rgb),
    Spectrum,
    Off,
}

impl EffectSpec {
    /// Apply this effect to the device.
    pub fn apply(&self, dev: &Mouse) -> Result<(), razer_hid::Error> {
        match self {
            EffectSpec::Static(c) => dev.set_color(*c),
            EffectSpec::Breathing(c) => dev.set_breathing(*c),
            EffectSpec::Spectrum => dev.set_spectrum(),
            EffectSpec::Off => dev.set_lighting_off(),
        }
    }

    /// A short human-readable description (for logs).
    pub fn describe(&self) -> String {
        match self {
            EffectSpec::Static(c) => format!("static #{:02x}{:02x}{:02x}", c.r, c.g, c.b),
            EffectSpec::Breathing(c) => format!("breathing #{:02x}{:02x}{:02x}", c.r, c.g, c.b),
            EffectSpec::Spectrum => "spectrum".to_string(),
            EffectSpec::Off => "off".to_string(),
        }
    }

    /// The (lighting, color) config strings that reproduce this effect.
    pub fn to_config(&self) -> (String, Option<String>) {
        let hex = |c: &Rgb| format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b);
        match self {
            EffectSpec::Static(c) => ("static".into(), Some(hex(c))),
            EffectSpec::Breathing(c) => ("breathing".into(), Some(hex(c))),
            EffectSpec::Spectrum => ("spectrum".into(), None),
            EffectSpec::Off => ("off".into(), None),
        }
    }

    /// Parse a `lighting` config string (+ its `color`) into an effect.
    /// Returns `Ok(None)` for `keep` (leave lighting untouched).
    pub fn from_config(lighting: &str, color: &str) -> Result<Option<EffectSpec>, String> {
        let rgb = || Rgb::parse_hex(color).map_err(|e| e.to_string());
        match lighting.trim().to_ascii_lowercase().as_str() {
            "keep" | "" => Ok(None),
            "static" => Ok(Some(EffectSpec::Static(rgb()?))),
            "breathing" => Ok(Some(EffectSpec::Breathing(rgb()?))),
            "spectrum" => Ok(Some(EffectSpec::Spectrum)),
            "off" | "none" => Ok(Some(EffectSpec::Off)),
            other => Err(format!("unknown lighting {other:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrip() {
        let e = EffectSpec::Static(Rgb::new(0x11, 0x22, 0x33));
        let (l, c) = e.to_config();
        assert_eq!(l, "static");
        assert_eq!(c.as_deref(), Some("#112233"));
        assert_eq!(EffectSpec::from_config("static", "#112233").unwrap(), Some(e));
    }

    #[test]
    fn keep_means_none() {
        assert_eq!(EffectSpec::from_config("keep", "#000000").unwrap(), None);
    }

    #[test]
    fn parses_all_effects() {
        assert_eq!(EffectSpec::from_config("spectrum", "#000000").unwrap(), Some(EffectSpec::Spectrum));
        assert_eq!(EffectSpec::from_config("off", "#000000").unwrap(), Some(EffectSpec::Off));
        assert!(EffectSpec::from_config("bogus", "#000000").is_err());
    }
}
