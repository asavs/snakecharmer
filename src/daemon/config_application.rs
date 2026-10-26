//! Configuration application boundary shared by session startup and reload.
//! Validation and migration complete before the application callback runs; persistence
//! happens only after the candidate becomes the active configuration.

use razer_proto::DeviceSpec;

use crate::config::Config;
use crate::logger::Logger;

#[derive(Debug)]
pub(super) enum SessionError {
    Config(String),
    Device(razer_hid::Error),
}

impl From<razer_hid::Error> for SessionError {
    fn from(error: razer_hid::Error) -> Self {
        Self::Device(error)
    }
}

pub(super) struct ConfigApplication {
    migration_notes: Vec<String>,
    migrated: bool,
}

/// Startup and reload use the same ordering: normalize and validate the whole
/// candidate, apply it, then replace the active configuration. A rejected
/// candidate cannot start a partial device update or unwind a healthy session.
pub(super) fn apply_config_candidate(
    active: &mut Config,
    mut candidate: Config,
    spec: DeviceSpec,
    apply: impl FnOnce(&Config) -> Result<(), razer_hid::Error>,
) -> Result<ConfigApplication, SessionError> {
    let original = candidate.clone();
    let migration_notes = razer_proto::keymap::spec_for(spec.product_id)
        .map(|km| crate::onboard::migrate_thumbs(km, &mut candidate))
        .unwrap_or_default();
    candidate.validate_for(spec).map_err(SessionError::Config)?;
    apply(&candidate)?;
    let migrated = candidate != original;
    *active = candidate;
    Ok(ConfigApplication { migration_notes, migrated })
}

pub(super) fn report_config_application(outcome: ConfigApplication, cfg: &Config, log: &Logger) {
    for note in outcome.migration_notes {
        log.log(&format!("Config: {note}."));
    }
    if outcome.migrated {
        save_config(cfg, log);
    }
}

pub(super) fn save_config(cfg: &Config, log: &Logger) {
    if let Err(e) = cfg.save(&Config::config_path()) {
        log.log(&format!("WARN could not save config: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_config_never_applies_or_replaces_the_healthy_config() {
        let original = Config::default();
        let mut active = original.clone();
        let mut candidates = vec![
            Config { dpi: 99, ..original.clone() },
            Config { polling_rate: Some(250), ..original.clone() },
            Config { thumb_back: "bad-action".into(), ..original.clone() },
        ];
        let mut bad_onboard = Config { thumb_back: "copy".into(), ..original.clone() };
        bad_onboard.onboard.insert("underside".into(), "disabled".into());
        candidates.push(bad_onboard);
        for candidate in candidates {
            let result = apply_config_candidate(&mut active, candidate, razer_proto::DEATHADDER_V3,
                |_| panic!("a rejected reload reached device application"));
            assert!(matches!(result, Err(SessionError::Config(_))));
            assert_eq!(active, original, "including migrations from a rejected candidate");
        }
    }

    #[test]
    fn valid_config_applies_the_normalized_candidate_before_committing() {
        let mut active = Config::default();
        let candidate = Config { dpi: 2400, thumb_back: "copy".into(), ..Config::default() };
        let mut applied = None;
        let outcome = apply_config_candidate(&mut active, candidate, razer_proto::DEATHADDER_V3,
            |candidate| {
                assert_eq!(candidate.thumb_back, "none");
                assert_eq!(candidate.onboard.get("back").map(String::as_str), Some("copy"));
                applied = Some(candidate.clone());
                Ok(())
            }).unwrap();
        assert_eq!(Some(&active), applied.as_ref());
        assert!(outcome.migrated);
        assert_eq!(outcome.migration_notes.len(), 1);
    }

    #[test]
    fn device_failure_keeps_the_old_config_and_remains_terminal() {
        let original = Config::default();
        let mut active = original.clone();
        let candidate = Config { dpi: 2400, ..original.clone() };
        let result = apply_config_candidate(&mut active, candidate, razer_proto::DEATHADDER_V3,
            |_| Err(razer_hid::Error::Verify("simulated read-back mismatch".into())));
        assert!(matches!(result, Err(SessionError::Device(ref error)) if error.requires_stop()));
        assert_eq!(active, original);
    }

}
