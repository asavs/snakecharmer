//! Event-driven PC Vitals health capsule. No polling thread is created: the daemon
//! publishes at lifecycle changes and piggybacks a bounded lease refresh on its existing
//! session wakeups.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SCHEMA_VERSION: &str = "1.0";
const PROVIDER_ID: &str = "snakecharmer";
const PROVIDER_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_CAPSULE_BYTES: usize = 32 * 1024;
const MAX_SUBJECTS: usize = 8;
const MAX_INCIDENTS: usize = 8;
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(45 * 60);
const INCIDENT_RETENTION_MS: u64 = 24 * 60 * 60 * 1000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ProviderStatus {
    Healthy,
    Degraded,
    Unavailable,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Privacy {
    classification: String,
    contains_user_content: bool,
    contains_usernames: bool,
    contains_paths: bool,
    contains_command_lines: bool,
    contains_window_titles: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Authority {
    may_set_pc_severity: bool,
    may_recommend_restart: bool,
    may_execute_actions: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Subject {
    subject_id: String,
    kind: String,
    product_name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum MetricKind {
    Gauge,
    Counter,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Metric {
    code: String,
    subject_id: Option<String>,
    kind: MetricKind,
    value: f64,
    unit: String,
    window_seconds: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Incident {
    incident_id: String,
    code: String,
    subject_id: String,
    occurred_at_unix_ms: u64,
    recovered_at_unix_ms: Option<u64>,
    occurrence_count: u32,
    native_error_code: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Capsule {
    schema_version: String,
    provider_id: String,
    provider_version: String,
    generated_at_unix_ms: u64,
    sequence: u64,
    status: ProviderStatus,
    privacy: Privacy,
    authority: Authority,
    subjects: Vec<Subject>,
    metrics: Vec<Metric>,
    recent_incidents: Vec<Incident>,
}

pub struct HealthReporter {
    capsule_path: PathBuf,
    salt: String,
    sequence: u64,
    status: ProviderStatus,
    subject: Option<Subject>,
    historical_subjects: Vec<Subject>,
    polling_rate_hz: Option<u16>,
    session_restarts_total: u64,
    consecutive_failures: u32,
    retry_delay_seconds: u32,
    incidents: Vec<Incident>,
    last_publish_attempt: Instant,
}

impl Default for HealthReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthReporter {
    pub fn new() -> Self {
        let snake_dir = crate::config::Config::dir();
        let capsule_path = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("PCVitals")
            .join("providers")
            .join("snakecharmer.json");
        Self::with_paths(capsule_path, snake_dir.join("provider-salt"))
    }

    fn with_paths(capsule_path: PathBuf, salt_path: PathBuf) -> Self {
        let salt = load_or_create_salt(&salt_path);
        let mut reporter = Self {
            capsule_path,
            salt,
            sequence: 0,
            status: ProviderStatus::Unknown,
            subject: None,
            historical_subjects: Vec::new(),
            polling_rate_hz: None,
            session_restarts_total: 0,
            consecutive_failures: 0,
            retry_delay_seconds: 0,
            incidents: Vec::new(),
            last_publish_attempt: Instant::now(),
        };
        reporter.restore_previous();
        reporter
    }

    pub fn starting(&mut self, polling_rate_hz: Option<u16>) -> std::io::Result<()> {
        self.status = ProviderStatus::Unknown;
        self.polling_rate_hz = polling_rate_hz;
        self.publish()
    }

    pub fn connected(
        &mut self,
        product_name: &str,
        vendor_id: u16,
        product_id: u16,
    ) -> std::io::Result<()> {
        let now = unix_time_ms();
        let subject_id = subject_id(&self.salt, vendor_id, product_id, product_name);
        if let Some(previous) = self.subject.take() {
            if previous.subject_id != subject_id
                && self
                    .incidents
                    .iter()
                    .any(|incident| incident.subject_id == previous.subject_id)
                && !self
                    .historical_subjects
                    .iter()
                    .any(|subject| subject.subject_id == previous.subject_id)
            {
                self.historical_subjects.push(previous);
            }
        }
        self.historical_subjects
            .retain(|subject| subject.subject_id != subject_id);
        self.subject = Some(Subject {
            subject_id: subject_id.clone(),
            kind: "hid_device".to_string(),
            product_name: sanitize_product_name(product_name),
        });
        for incident in &mut self.incidents {
            if incident.subject_id == subject_id && incident.recovered_at_unix_ms.is_none() {
                incident.recovered_at_unix_ms = Some(now);
            }
        }
        self.status = ProviderStatus::Healthy;
        self.consecutive_failures = 0;
        self.retry_delay_seconds = 0;
        self.publish()
    }

    pub fn session_failed(
        &mut self,
        error: &razer_hid::Error,
        retry_delay_seconds: u32,
    ) -> std::io::Result<()> {
        self.status = ProviderStatus::Degraded;
        self.session_restarts_total = self.session_restarts_total.saturating_add(1);
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.retry_delay_seconds = retry_delay_seconds;
        let Some(subject) = &self.subject else {
            return self.publish();
        };
        let code = error_code(error).to_string();
        let native_error_code = error.native_error_code();
        let now = unix_time_ms();
        if let Some(existing) = self.incidents.iter_mut().find(|incident| {
            incident.code == code
                && incident.subject_id == subject.subject_id
                && incident.recovered_at_unix_ms.is_none()
        }) {
            existing.occurrence_count = existing.occurrence_count.saturating_add(1);
            existing.native_error_code = native_error_code;
        } else {
            self.incidents.push(Incident {
                incident_id: format!("session-failure-{now}-{}", self.sequence.saturating_add(1)),
                code,
                subject_id: subject.subject_id.clone(),
                occurred_at_unix_ms: now,
                recovered_at_unix_ms: None,
                occurrence_count: 1,
                native_error_code,
            });
        }
        if self.incidents.len() > MAX_INCIDENTS {
            self.incidents.drain(..self.incidents.len() - MAX_INCIDENTS);
        }
        self.publish()
    }

    pub fn polling_rate_changed(&mut self, polling_rate_hz: Option<u16>) -> std::io::Result<()> {
        self.polling_rate_hz = polling_rate_hz;
        self.publish()
    }

    pub fn stopped(&mut self) -> std::io::Result<()> {
        self.status = ProviderStatus::Unavailable;
        self.retry_delay_seconds = 0;
        self.publish()
    }

    /// Refresh the capsule lease without adding a timer or polling thread. The daemon calls
    /// this from its existing event/reassert loop; repeated calls are an in-memory time check.
    pub fn refresh_due_in(&self) -> Duration {
        HEARTBEAT_INTERVAL.saturating_sub(self.last_publish_attempt.elapsed())
    }

    pub fn refresh_if_due(&mut self) -> std::io::Result<bool> {
        self.refresh_if_due_at(Instant::now())
    }

    fn refresh_if_due_at(&mut self, now: Instant) -> std::io::Result<bool> {
        if now.duration_since(self.last_publish_attempt) >= HEARTBEAT_INTERVAL {
            self.publish_at(now)?;
            return Ok(true);
        }
        Ok(false)
    }

    fn restore_previous(&mut self) {
        let Ok(bytes) = fs::read(&self.capsule_path) else {
            return;
        };
        let Ok(capsule) = serde_json::from_slice::<Capsule>(&bytes) else {
            return;
        };
        if capsule.schema_version == SCHEMA_VERSION
            && capsule.provider_id == PROVIDER_ID
            && capsule.sequence > 0
            && capsule.sequence < u64::MAX
            && capsule.privacy.classification == "operational_only"
            && !capsule.privacy.contains_user_content
            && !capsule.privacy.contains_usernames
            && !capsule.privacy.contains_paths
            && !capsule.privacy.contains_command_lines
            && !capsule.privacy.contains_window_titles
            && !capsule.authority.may_set_pc_severity
            && !capsule.authority.may_recommend_restart
            && !capsule.authority.may_execute_actions
        {
            // The shared capsule is output, not durable factual memory. Carry only its
            // non-semantic sequence forward; connection identity, counters, and incidents
            // must be observed again by this process.
            self.sequence = capsule.sequence;
        }
    }

    fn publish(&mut self) -> std::io::Result<()> {
        self.publish_at(Instant::now())
    }

    fn publish_at(&mut self, attempted_at: Instant) -> std::io::Result<()> {
        self.last_publish_attempt = attempted_at;
        let next_sequence = self.sequence.saturating_add(1).max(1);
        let generated_at_unix_ms = unix_time_ms();
        self.incidents.retain(|incident| {
            incident.recovered_at_unix_ms.is_none()
                || incident.recovered_at_unix_ms.is_some_and(|recovered| {
                    generated_at_unix_ms.saturating_sub(recovered) <= INCIDENT_RETENTION_MS
                })
        });
        let current_subject_id = self
            .subject
            .as_ref()
            .map(|subject| subject.subject_id.as_str());
        self.historical_subjects.retain(|subject| {
            Some(subject.subject_id.as_str()) != current_subject_id
                && self
                    .incidents
                    .iter()
                    .any(|incident| incident.subject_id == subject.subject_id)
        });
        while self.historical_subjects.len() + usize::from(self.subject.is_some()) > MAX_SUBJECTS {
            let removed_subject_id = self.historical_subjects.remove(0).subject_id;
            self.incidents
                .retain(|incident| incident.subject_id != removed_subject_id);
        }
        let subject_id = self
            .subject
            .as_ref()
            .map(|subject| subject.subject_id.clone());
        let mut metrics = vec![
            Metric {
                code: "session_restarts_total".to_string(),
                subject_id: subject_id.clone(),
                kind: MetricKind::Counter,
                value: self.session_restarts_total as f64,
                unit: "count".to_string(),
                window_seconds: None,
            },
            Metric {
                code: "consecutive_session_failures".to_string(),
                subject_id: subject_id.clone(),
                kind: MetricKind::Gauge,
                value: self.consecutive_failures as f64,
                unit: "count".to_string(),
                window_seconds: None,
            },
            Metric {
                code: "retry_delay_seconds".to_string(),
                subject_id: subject_id.clone(),
                kind: MetricKind::Gauge,
                value: self.retry_delay_seconds as f64,
                unit: "seconds".to_string(),
                window_seconds: None,
            },
        ];
        if let Some(rate) = self.polling_rate_hz {
            metrics.push(Metric {
                code: "configured_polling_rate_hz".to_string(),
                subject_id,
                kind: MetricKind::Gauge,
                value: rate as f64,
                unit: "hertz".to_string(),
                window_seconds: None,
            });
        }
        let mut subjects = self.historical_subjects.clone();
        subjects.extend(self.subject.clone());
        let capsule = Capsule {
            schema_version: SCHEMA_VERSION.to_string(),
            provider_id: PROVIDER_ID.to_string(),
            provider_version: PROVIDER_VERSION.to_string(),
            generated_at_unix_ms,
            sequence: next_sequence,
            status: self.status.clone(),
            privacy: Privacy {
                classification: "operational_only".to_string(),
                contains_user_content: false,
                contains_usernames: false,
                contains_paths: false,
                contains_command_lines: false,
                contains_window_titles: false,
            },
            authority: Authority {
                may_set_pc_severity: false,
                may_recommend_restart: false,
                may_execute_actions: false,
            },
            subjects,
            metrics,
            recent_incidents: self.incidents.clone(),
        };
        persist_capsule(&self.capsule_path, &capsule)?;
        self.sequence = next_sequence;
        Ok(())
    }
}

fn error_code(error: &razer_hid::Error) -> &'static str {
    match error {
        razer_hid::Error::Hid(_) => "hid_io_failed",
        razer_hid::Error::DeviceNotFound => "device_not_found",
        razer_hid::Error::Proto(_) => "device_protocol_failed",
        razer_hid::Error::Busy(_) => "device_busy_timeout",
        razer_hid::Error::Verify(_) => "device_verification_failed",
    }
}

fn persist_capsule(path: &Path, capsule: &Capsule) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("capsule path has no parent"))?;
    fs::create_dir_all(parent)?;
    let bytes = serde_json::to_vec(capsule)?;
    if bytes.len() > MAX_CAPSULE_BYTES {
        return Err(std::io::Error::other("health capsule exceeds 32 KiB"));
    }
    let temporary = parent.join(format!(".snakecharmer.tmp.{}", std::process::id()));
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    platform::atomic_replace_file(&temporary, path)
}

fn load_or_create_salt(path: &Path) -> String {
    if let Ok(value) = fs::read_to_string(path) {
        if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return value.to_lowercase();
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(unix_time_ms().to_le_bytes());
    hasher.update(std::process::id().to_le_bytes());
    hasher.update(format!("{:p}", path).as_bytes());
    let value = format!("{:x}", hasher.finalize());
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, &value);
    value
}

fn subject_id(salt: &str, vendor_id: u16, product_id: u16, product_name: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(vendor_id.to_le_bytes());
    hasher.update(product_id.to_le_bytes());
    hasher.update(product_name.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

fn sanitize_product_name(value: &str) -> String {
    let filtered: String = value
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, ' ' | '-' | '_' | '.' | '(' | ')')
        })
        .take(80)
        .collect();
    if filtered.is_empty() {
        "HID device".to_string()
    } else {
        filtered
    }
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |duration| {
            duration.as_millis().try_into().unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "snakecharmer-health-{label}-{}-{}",
            std::process::id(),
            unix_time_ms()
        ))
    }

    #[test]
    fn subject_ids_are_salted_stable_and_opaque() {
        let first = subject_id("a", 0x1532, 0x00b2, "Razer Viper V3");
        assert_eq!(first, subject_id("a", 0x1532, 0x00b2, "Razer Viper V3"));
        assert_ne!(first, subject_id("b", 0x1532, 0x00b2, "Razer Viper V3"));
        assert_eq!(first.len(), 71);
        assert!(!first.contains("Razer"));
    }

    #[test]
    fn product_names_are_bounded_and_content_free() {
        assert_eq!(
            sanitize_product_name(r"Mouse C:\Users\asa\secret"),
            "Mouse CUsersasasecret"
        );
        assert!(sanitize_product_name(&"x".repeat(100)).len() <= 80);
    }

    #[test]
    fn producer_emits_the_fail_closed_pc_vitals_contract() {
        let root = temp_root("contract");
        let path = root.join("providers").join("snakecharmer.json");
        let mut reporter = HealthReporter::with_paths(path.clone(), root.join("salt"));
        reporter.starting(Some(4_000)).unwrap();
        reporter
            .connected("Razer Viper V3", 0x1532, 0x00b2)
            .unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(bytes.len() < MAX_CAPSULE_BYTES);
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["schema_version"], "1.0");
        assert_eq!(value["provider_id"], "snakecharmer");
        assert_eq!(value["status"], "healthy");
        assert_eq!(value["authority"]["may_set_pc_severity"], false);
        assert_eq!(value["authority"]["may_recommend_restart"], false);
        assert_eq!(value["authority"]["may_execute_actions"], false);
        assert_eq!(value["privacy"]["contains_paths"], false);
        assert_eq!(
            value["subjects"][0]["subject_id"].as_str().unwrap().len(),
            71
        );
        assert!(value["metrics"].as_array().unwrap().iter().any(|metric| {
            metric["code"] == "configured_polling_rate_hz"
                && metric["value"].as_f64() == Some(4_000.0)
        }));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn heartbeat_refreshes_the_lease_without_changing_health() {
        let root = temp_root("heartbeat");
        let path = root.join("providers").join("snakecharmer.json");
        let mut reporter = HealthReporter::with_paths(path.clone(), root.join("salt"));
        reporter
            .connected("Razer Viper V3", 0x1532, 0x00b2)
            .unwrap();
        let first: Capsule = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();

        assert!(!reporter
            .refresh_if_due_at(
                reporter.last_publish_attempt + HEARTBEAT_INTERVAL - Duration::from_millis(1),
            )
            .unwrap());
        let early: Capsule = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(early.sequence, first.sequence);

        assert!(reporter
            .refresh_if_due_at(reporter.last_publish_attempt + HEARTBEAT_INTERVAL)
            .unwrap());
        let refreshed: Capsule = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(refreshed.sequence, first.sequence + 1);
        assert!(matches!(refreshed.status, ProviderStatus::Healthy));
        assert_eq!(refreshed.metrics.len(), first.metrics.len());
        assert_eq!(
            refreshed.recent_incidents.len(),
            first.recent_incidents.len()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn failed_publication_does_not_advance_committed_sequence() {
        let root = temp_root("publish-failure");
        fs::create_dir_all(&root).unwrap();
        let blocked_parent = root.join("not-a-directory");
        fs::write(&blocked_parent, b"blocked").unwrap();
        let mut reporter =
            HealthReporter::with_paths(blocked_parent.join("snakecharmer.json"), root.join("salt"));
        assert!(reporter.starting(Some(1_000)).is_err());
        assert_eq!(reporter.sequence, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn shared_capsule_cannot_restore_incidents_or_subject_facts() {
        let root = temp_root("restore-boundary");
        let path = root.join("providers").join("snakecharmer.json");
        let mut first = HealthReporter::with_paths(path.clone(), root.join("salt"));
        first.connected("Razer Viper V3", 0x1532, 0x00b2).unwrap();
        first
            .session_failed(&razer_hid::Error::DeviceNotFound, 3)
            .unwrap();
        let prior_sequence = first.sequence;

        let mut restored = HealthReporter::with_paths(path.clone(), root.join("salt"));
        assert_eq!(restored.sequence, prior_sequence);
        assert!(restored.subject.is_none());
        assert!(restored.incidents.is_empty());
        assert_eq!(restored.session_restarts_total, 0);
        restored.starting(Some(1_000)).unwrap();
        let capsule: Capsule = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(capsule.subjects.is_empty());
        assert!(capsule.recent_incidents.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn recovered_episode_identity_and_start_time_are_immutable() {
        let root = temp_root("episodes");
        let path = root.join("providers").join("snakecharmer.json");
        let mut reporter = HealthReporter::with_paths(path.clone(), root.join("salt"));
        reporter
            .connected("Razer Viper V3", 0x1532, 0x00b2)
            .unwrap();
        reporter
            .session_failed(&razer_hid::Error::DeviceNotFound, 3)
            .unwrap();
        let first_id = reporter.incidents[0].incident_id.clone();
        let first_start = reporter.incidents[0].occurred_at_unix_ms;
        reporter
            .session_failed(&razer_hid::Error::DeviceNotFound, 6)
            .unwrap();
        assert_eq!(reporter.incidents[0].occurred_at_unix_ms, first_start);
        assert_eq!(reporter.incidents[0].occurrence_count, 2);
        reporter
            .connected("Razer Viper V3", 0x1532, 0x00b2)
            .unwrap();
        reporter
            .session_failed(&razer_hid::Error::DeviceNotFound, 3)
            .unwrap();
        assert_eq!(reporter.incidents.len(), 2);
        assert_ne!(reporter.incidents[1].incident_id, first_id);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn switching_devices_retains_the_subject_cited_by_prior_incidents() {
        let root = temp_root("subject-citations");
        let path = root.join("providers").join("snakecharmer.json");
        let mut reporter = HealthReporter::with_paths(path.clone(), root.join("salt"));
        reporter
            .connected("Razer Viper V3", 0x1532, 0x00b2)
            .unwrap();
        reporter
            .session_failed(&razer_hid::Error::DeviceNotFound, 3)
            .unwrap();
        reporter
            .connected("DeathAdder Elite", 0x1532, 0x005c)
            .unwrap();

        let capsule: Capsule = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(capsule.subjects.len(), 2);
        assert!(capsule.recent_incidents.iter().all(|incident| {
            capsule
                .subjects
                .iter()
                .any(|subject| subject.subject_id == incident.subject_id)
        }));
        let _ = fs::remove_dir_all(root);
    }
}
