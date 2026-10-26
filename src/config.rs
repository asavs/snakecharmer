//! Configuration for the Snakecharmer daemon.
//!
//! Persisted as TOML at `%LOCALAPPDATA%\Snakecharmer\config.toml`. Defaults are
//! written if the file is missing. The settings window edits the same file.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TemporaryConfig(PathBuf);

impl Drop for TemporaryConfig {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Per-zone lighting override (`[zones.wheel]`, `[zones.logo]`). Absent fields
/// fall back to the device-wide `lighting` / `color`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ZoneConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lighting: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    /// DPI applied (and locked) at startup.
    pub dpi: u16,
    /// Optional separate Y DPI; defaults to `dpi` when omitted.
    pub dpi_y: Option<u16>,
    /// Action for the front DPI button (vendor code 0x20).
    pub dpi_up: String,
    /// Action for the rear DPI button (vendor code 0x21).
    pub dpi_down: String,
    /// Action for the Back thumb button (XBUTTON1). `none` = passthrough
    /// (native Back kept). Any other action suppresses native Back.
    pub thumb_back: String,
    /// Action for the Forward thumb button (XBUTTON2). `none` = passthrough.
    pub thumb_forward: String,
    /// Lighting effect applied at startup: `keep` (don't touch), `static`,
    /// `breathing`, `spectrum`, or `off`.
    pub lighting: String,
    /// Color (`#RRGGBB`) used by the `static`/`breathing` lighting effects.
    pub color: String,
    /// Optional polling rate in Hz, applied (and re-asserted) at startup.
    /// Absent = leave the hardware setting alone.
    pub polling_rate: Option<u16>,
    /// How often (seconds) to re-assert driver mode + DPI.
    pub reassert_interval_secs: u64,
    /// Whether first-run autostart registration has already been attempted.
    /// This one-shot latch stops us recreating a removed login registration.
    /// `platform::autostart` reads that registration; it does not query the
    /// separate Windows startup setting that can suppress the registered app.
    pub autostart_bootstrapped: bool,
    /// Per-zone lighting overrides, keyed by zone name (`wheel`, `logo`).
    /// A zone without an entry (or with a field unset) uses the device-wide
    /// `lighting` / `color`. Kept last: TOML tables must follow plain values.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub zones: BTreeMap<String, ZoneConfig>,
    /// Binds written to the mouse's own memory (`[onboard]`), keyed by button
    /// name: `back = "ctrl+c"`, `wheel_click = "default"`. They work with the
    /// daemon closed. Only mice with a recorded keymap (`razer_proto::keymap`);
    /// a button not listed keeps whatever it stores.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub onboard: BTreeMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            dpi: 1800,
            dpi_y: None,
            dpi_up: "copy".to_string(),
            dpi_down: "paste".to_string(),
            thumb_back: "none".to_string(),
            thumb_forward: "none".to_string(),
            lighting: "keep".to_string(),
            color: "#00ff00".to_string(),
            polling_rate: None,
            reassert_interval_secs: 60,
            autostart_bootstrapped: false,
            zones: BTreeMap::new(),
            onboard: BTreeMap::new(),
        }
    }
}

impl Config {
    /// The resolved (x, y) DPI, filling Y from X when unset.
    pub fn dpi_xy(&self) -> (u16, u16) {
        (self.dpi, self.dpi_y.unwrap_or(self.dpi))
    }

    /// Effective lighting effect for a zone: its override, else device-wide.
    pub fn zone_lighting(&self, zone: &str) -> &str {
        self.zones
            .get(zone)
            .and_then(|z| z.lighting.as_deref())
            .unwrap_or(&self.lighting)
    }

    /// Effective color for a zone: its override, else device-wide.
    pub fn zone_color(&self, zone: &str) -> &str {
        self.zones
            .get(zone)
            .and_then(|z| z.color.as_deref())
            .unwrap_or(&self.color)
    }

    /// Validate every setting that this device will apply, without device I/O.
    /// Unsupported lighting, DPI-button actions and on-board tables remain ignored.
    pub fn validate_for(&self, spec: razer_proto::DeviceSpec) -> Result<(), String> {
        let mut errors = Vec::new();
        let (x, y) = self.dpi_xy();
        if let Err(e) = razer_proto::set_dpi_report(
            spec.transaction_id, spec.dpi_min, spec.dpi_max, x, y,
        ) {
            errors.push(format!("dpi/dpi_y: {e}"));
        }
        if let Some(hz) = self.polling_rate {
            if let Err(e) = razer_proto::set_polling_rate_report(spec.transaction_id, spec.polling, hz) {
                errors.push(format!("polling_rate: {e}"));
            }
        }
        for (name, value, active) in [
            ("dpi_up", &self.dpi_up, spec.dpi_buttons.is_some()),
            ("dpi_down", &self.dpi_down, spec.dpi_buttons.is_some()),
            ("thumb_back", &self.thumb_back, true),
            ("thumb_forward", &self.thumb_forward, true),
        ] {
            if active {
                if let Err(e) = crate::actions::parse(value) {
                    errors.push(format!("{name}: {e}"));
                }
            }
        }
        for &zone in spec.rgb_zones {
            let name = razer_proto::led::name(zone);
            if let Err(e) = crate::lighting::EffectSpec::from_config(
                self.zone_lighting(name), self.zone_color(name),
            ) {
                errors.push(format!("lighting ({name}): {e}"));
            }
        }
        if let Some(km) = razer_proto::keymap::spec_for(spec.product_id) {
            if let Err(e) = crate::onboard::parse_config(km, &self.onboard) {
                errors.push(format!("[onboard]: {e}"));
            }
        }
        if errors.is_empty() { Ok(()) } else { Err(errors.join("; ")) }
    }

    /// Config directory: `%LOCALAPPDATA%\Snakecharmer` (falls back to CWD).
    pub fn dir() -> PathBuf {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("Snakecharmer")
    }

    pub fn config_path() -> PathBuf {
        Self::dir().join("config.toml")
    }

    pub fn log_path() -> PathBuf {
        Self::dir().join("daemon.log")
    }

    /// Load config from `path`, writing defaults if the file is missing.
    ///
    /// Existing files must load successfully before any startup side effect or
    /// automatic save. Falling back to defaults would let autostart bootstrap
    /// overwrite a malformed original, including its first-run latch.
    pub fn load_or_create(path: &Path) -> io::Result<(Config, Option<String>)> {
        match fs::read_to_string(path) {
            Ok(text) => {
                let cfg = toml::from_str::<Config>(&text).map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidData,
                        format!("could not parse config {}: {e}", path.display()))
                })?;
                Ok((cfg, None))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let cfg = Config::default();
                cfg.save(path)?;
                Ok((cfg, Some(format!("wrote default config to {}", path.display()))))
            }
            Err(e) => Err(io::Error::new(e.kind(),
                format!("could not read config {}: {e}", path.display()))),
        }
    }

    /// Publish a complete, flushed file in one same-directory replacement.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let text = toml::to_string_pretty(self)
            .map_err(io::Error::other)?;
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let (temporary, mut file) = loop {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let temporary = parent.join(format!(".config.tmp.{}.{sequence}", std::process::id()));
            match OpenOptions::new().write(true).create_new(true).open(&temporary) {
                Ok(file) => break (TemporaryConfig(temporary), file),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        };
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        platform::atomic_replace_file(&temporary.0, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;

    #[test]
    fn validates_both_dpi_axes_and_polling_before_application() {
        let cfg = Config { dpi: 1600, dpi_y: Some(30001), polling_rate: Some(250), ..Config::default() };
        let error = cfg.validate_for(razer_proto::DEATHADDER_V3).unwrap_err();
        assert!(error.contains("dpi/dpi_y"));
        assert!(error.contains("polling_rate"));
        assert!(Config { dpi: 16001, ..Config::default() }
            .validate_for(razer_proto::DEATHADDER_ELITE).is_err());
        assert!(Config { dpi: 45000, polling_rate: Some(8000), ..Config::default() }
            .validate_for(razer_proto::DEATHADDER_V4_PRO).is_ok());
    }

    #[test]
    fn validates_actions_that_the_device_uses() {
        let mut cfg = Config { dpi_up: "not-an-action".into(), ..Config::default() };
        assert!(cfg.validate_for(razer_proto::DEATHADDER_ELITE).unwrap_err().contains("dpi_up"));
        assert!(cfg.validate_for(razer_proto::DEATHADDER_V3).is_ok(), "no DPI buttons on the V3");
        cfg.thumb_forward = "not-an-action".into();
        assert!(cfg.validate_for(razer_proto::DEATHADDER_V3).unwrap_err().contains("thumb_forward"));
    }

    #[test]
    fn validates_effective_zone_effects_and_colors() {
        let mut cfg = Config { lighting: "static".into(), color: "bad-color".into(), ..Config::default() };
        assert!(cfg.validate_for(razer_proto::DEATHADDER_ELITE).unwrap_err().contains("lighting (wheel)"));
        for name in ["wheel", "logo"] {
            cfg.zones.insert(name.into(), ZoneConfig { lighting: Some("off".into()), color: None });
        }
        assert!(cfg.validate_for(razer_proto::DEATHADDER_ELITE).is_ok(), "off ignores unused color");
        cfg.zones.get_mut("logo").unwrap().lighting = Some("not-an-effect".into());
        assert!(cfg.validate_for(razer_proto::DEATHADDER_ELITE).unwrap_err().contains("lighting (logo)"));
    }

    #[test]
    fn validates_captured_onboard_entries_and_keeps_unsupported_features_ignored() {
        let mut cfg = Config { lighting: "not-an-effect".into(), ..Config::default() };
        cfg.onboard.insert("left".into(), "disabled".into());
        assert!(cfg.validate_for(razer_proto::DEATHADDER_V3).unwrap_err().contains("[onboard]"));
        assert!(cfg.validate_for(razer_proto::DEATHADDER_V4_PRO).is_ok(), "no lighting or keymap on V4 Pro");
        cfg.onboard.clear();
        cfg.onboard.insert("wheel_click".into(), "ctrl+c".into());
        assert!(cfg.validate_for(razer_proto::DEATHADDER_V3).is_ok());
    }

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "snakecharmer-config-test-{}-{sequence}", std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn config(&self) -> PathBuf {
            self.0.join("config.toml")
        }

        fn assert_no_temporaries(&self) {
            let names: Vec<_> = fs::read_dir(&self.0).unwrap()
                .map(|entry| entry.unwrap().file_name()).collect();
            assert_eq!(names, vec![std::ffi::OsString::from("config.toml")]);
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn missing_config_is_created_once() {
        let dir = TestDir::new();
        let (cfg, note) = Config::load_or_create(&dir.config()).unwrap();
        assert_eq!(cfg, Config::default());
        assert!(note.is_some());
        let (loaded, note) = Config::load_or_create(&dir.config()).unwrap();
        assert_eq!(loaded, cfg);
        assert!(note.is_none());
        dir.assert_no_temporaries();
    }

    #[test]
    fn malformed_config_is_an_error_and_keeps_the_original() {
        let dir = TestDir::new();
        let original = b"dpi = 1200\n# keep my settings\n[onboard\nback = 'ctrl+c'\n";
        fs::write(dir.config(), original).unwrap();
        let error = Config::load_or_create(&dir.config()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("could not parse config"));
        assert_eq!(fs::read(dir.config()).unwrap(), original);
        dir.assert_no_temporaries();
    }

    #[test]
    fn non_utf8_config_is_not_replaced_with_defaults() {
        let dir = TestDir::new();
        let original = [0xff, 0xfe, 0x80];
        fs::write(dir.config(), original).unwrap();
        assert_eq!(Config::load_or_create(&dir.config()).unwrap_err().kind(),
            io::ErrorKind::InvalidData);
        assert_eq!(fs::read(dir.config()).unwrap(), original);
        dir.assert_no_temporaries();
    }

    #[test]
    fn unreadable_config_is_not_treated_as_missing() {
        let dir = TestDir::new();
        let original = b"dpi = 1200\n";
        fs::write(dir.config(), original).unwrap();
        let locked = OpenOptions::new().read(true).share_mode(0).open(dir.config()).unwrap();
        let error = Config::load_or_create(&dir.config()).unwrap_err();
        assert!(error.to_string().contains("could not read config"));
        drop(locked);
        assert_eq!(fs::read(dir.config()).unwrap(), original);
        dir.assert_no_temporaries();
    }

    #[test]
    fn failed_replacement_keeps_previous_config_and_cleans_temporary() {
        let dir = TestDir::new();
        let original = b"dpi = 1200\n# preserve this even if saving fails\n";
        fs::write(dir.config(), original).unwrap();
        let locked = OpenOptions::new().read(true).share_mode(0).open(dir.config()).unwrap();
        assert!(Config::default().save(&dir.config()).is_err());
        drop(locked);
        assert_eq!(fs::read(dir.config()).unwrap(), original);
        dir.assert_no_temporaries();
    }

    #[test]
    fn save_replaces_an_existing_file_with_a_complete_config() {
        let dir = TestDir::new();
        fs::write(dir.config(), "dpi = 1200\n").unwrap();
        let cfg = Config { dpi: 2400, ..Config::default() };
        cfg.save(&dir.config()).unwrap();
        assert_eq!(Config::load_or_create(&dir.config()).unwrap().0, cfg);
        dir.assert_no_temporaries();
    }

    #[test]
    fn defaults_are_sane() {
        let c = Config::default();
        assert_eq!(c.dpi, 1800);
        assert_eq!(c.dpi_xy(), (1800, 1800));
        assert_eq!(c.dpi_up, "copy");
        assert_eq!(c.dpi_down, "paste");
        assert_eq!(c.reassert_interval_secs, 60);
    }

    #[test]
    fn roundtrips_through_toml() {
        let c = Config {
            dpi: 1600,
            dpi_y: Some(800),
            dpi_up: "key:9".into(),
            dpi_down: "key:0".into(),
            thumb_back: "cut".into(),
            thumb_forward: "none".into(),
            lighting: "static".into(),
            color: "#ff8800".into(),
            polling_rate: Some(500),
            reassert_interval_secs: 30,
            autostart_bootstrapped: true,
            zones: BTreeMap::from([(
                "logo".to_string(),
                ZoneConfig { lighting: Some("breathing".into()), color: None },
            )]),
            onboard: BTreeMap::from([("back".to_string(), "ctrl+c".to_string())]),
        };
        let text = toml::to_string_pretty(&c).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(c, back);
        assert_eq!(back.dpi_xy(), (1600, 800));
    }

    #[test]
    fn zone_overrides_fall_back_to_device_wide() {
        let mut c = Config::default(); // lighting=keep, color=#00ff00
        assert_eq!(c.zone_lighting("wheel"), "keep");
        assert_eq!(c.zone_color("wheel"), "#00ff00");

        c.zones.insert(
            "wheel".into(),
            ZoneConfig { lighting: Some("static".into()), color: Some("#ff0000".into()) },
        );
        c.zones.insert("logo".into(), ZoneConfig { lighting: Some("spectrum".into()), color: None });
        assert_eq!(c.zone_lighting("wheel"), "static");
        assert_eq!(c.zone_color("wheel"), "#ff0000");
        assert_eq!(c.zone_lighting("logo"), "spectrum");
        assert_eq!(c.zone_color("logo"), "#00ff00", "unset color falls back");
    }

    #[test]
    fn empty_zones_stay_out_of_the_toml() {
        let text = toml::to_string_pretty(&Config::default()).unwrap();
        assert!(!text.contains("zones"), "no empty [zones] table:\n{text}");
        assert!(!text.contains("onboard"), "no empty [onboard] table:\n{text}");
    }

    #[test]
    fn partial_config_fills_defaults() {
        // Only dpi specified; the rest should come from Default via serde(default).
        let back: Config = toml::from_str("dpi = 1200\n").unwrap();
        assert_eq!(back.dpi, 1200);
        assert_eq!(back.dpi_up, "copy");
        assert_eq!(back.polling_rate, None, "absent polling_rate must mean leave-as-is");
        assert_eq!(back.reassert_interval_secs, 60);
        assert!(
            !back.autostart_bootstrapped,
            "a pre-latch config must read as never-bootstrapped so it gets its one registration"
        );
    }

    #[test]
    fn autostart_latch_precedes_tables() {
        let mut c = Config::default();
        c.zones.insert(
            "logo".into(),
            ZoneConfig { lighting: Some("static".into()), color: None },
        );
        let text = toml::to_string_pretty(&c).unwrap();
        let latch = text.find("autostart_bootstrapped").expect("latch must serialize");
        let table = text.find('[').expect("zones table must serialize");
        assert!(latch < table, "TOML scalars must precede tables:\n{text}");
    }
}
