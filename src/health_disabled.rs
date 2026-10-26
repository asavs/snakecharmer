//! Disabled PC Vitals integration. The zero-sized reporter keeps lifecycle
//! wiring shared while release optimization removes its calls and lease branch.

use std::io;
use std::time::Duration;

#[derive(Default)]
pub struct HealthReporter;

impl HealthReporter {
    pub fn new() -> Self { Self }

    #[cfg(test)]
    pub(crate) fn with_paths(_: std::path::PathBuf, _: std::path::PathBuf) -> Self { Self }

    pub fn starting(&mut self, _: Option<u16>) -> io::Result<()> { Ok(()) }

    pub fn connected(&mut self, _: &str, _: u16, _: u16) -> io::Result<()> { Ok(()) }

    pub fn session_failed(&mut self, _: &razer_hid::Error, _: u32) -> io::Result<()> { Ok(()) }

    pub fn polling_rate_changed(&mut self, _: Option<u16>) -> io::Result<()> { Ok(()) }

    pub fn stopped(&mut self) -> io::Result<()> { Ok(()) }

    pub fn refresh_due_in(&self) -> Duration { Duration::MAX }

    pub fn refresh_if_due(&mut self) -> io::Result<bool> { Ok(false) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_lifecycle_has_no_lease_or_filesystem_side_effects() {
        let root = std::env::temp_dir().join(format!(
            "snakecharmer-health-disabled-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        assert!(!root.exists());
        let mut reporter = HealthReporter::with_paths(
            root.join("PCVitals/providers/snakecharmer.json"), root.join("provider-salt")
        );
        reporter.starting(Some(1000)).unwrap();
        reporter.connected("Test mouse", 0x1532, 0x00B2).unwrap();
        reporter.session_failed(&razer_hid::Error::DeviceNotFound, 3).unwrap();
        reporter.polling_rate_changed(None).unwrap();
        assert_eq!(reporter.refresh_due_in(), Duration::MAX);
        assert!(!reporter.refresh_if_due().unwrap());
        reporter.stopped().unwrap();
        assert!(!root.exists(), "disabled reporting must not create capsule or salt directories");
        assert_eq!(std::mem::size_of::<HealthReporter>(), 0);
    }
}
