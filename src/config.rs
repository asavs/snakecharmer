//! Configuration for the Snakecharmer daemon.
//!
//! Persisted as TOML at `%LOCALAPPDATA%\Snakecharmer\config.toml`. Defaults are
//! written if the file is missing. This seeds the Phase-4 settings window.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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
    /// How often (seconds) to re-assert driver mode + DPI.
    pub reassert_interval_secs: u64,
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
            reassert_interval_secs: 60,
        }
    }
}

impl Config {
    /// The resolved (x, y) DPI, filling Y from X when unset.
    pub fn dpi_xy(&self) -> (u16, u16) {
        (self.dpi, self.dpi_y.unwrap_or(self.dpi))
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
    /// On a parse error, returns defaults with an explanatory message rather
    /// than failing (a daemon should still start on a corrupt config).
    pub fn load_or_create(path: &Path) -> (Config, Option<String>) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::read_to_string(path) {
            Ok(text) => match toml::from_str::<Config>(&text) {
                Ok(cfg) => (cfg, None),
                Err(e) => (Config::default(), Some(format!("config parse error ({e}); using defaults"))),
            },
            Err(_) => {
                let cfg = Config::default();
                let note = match cfg.save(path) {
                    Ok(()) => Some(format!("wrote default config to {}", path.display())),
                    Err(e) => Some(format!("could not write default config: {e}")),
                };
                (cfg, note)
            }
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)
            .unwrap_or_else(|_| "# failed to serialize config\n".to_string());
        std::fs::write(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            reassert_interval_secs: 30,
        };
        let text = toml::to_string_pretty(&c).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(c, back);
        assert_eq!(back.dpi_xy(), (1600, 800));
    }

    #[test]
    fn partial_config_fills_defaults() {
        // Only dpi specified; the rest should come from Default via serde(default).
        let back: Config = toml::from_str("dpi = 1200\n").unwrap();
        assert_eq!(back.dpi, 1200);
        assert_eq!(back.dpi_up, "copy");
        assert_eq!(back.reassert_interval_secs, 60);
    }
}
