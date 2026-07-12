//! Event-driven PC Vitals health capsule. No polling thread is created: the daemon
//! publishes only at lifecycle, connection, failure, recovery, configuration, and quit.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA_VERSION: &str = "1.0";
const PROVIDER_ID: &str = "snakecharmer";
const PROVIDER_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_CAPSULE_BYTES: usize = 32 * 1024;
const MAX_INCIDENTS: usize = 8;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ProviderStatus {
    Healthy,
    Degraded,
    Unavailable,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Privacy {
    classification: String,
    contains_user_content: bool,
    contains_usernames: bool,
    contains_paths: bool,
    contains_command_lines: bool,
    contains_window_titles: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Authority {
    may_set_pc_severity: bool,
    may_recommend_restart: bool,
    may_execute_actions: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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
struct Metric {
    code: String,
    subject_id: Option<String>,
    kind: MetricKind,
    value: f64,
    unit: String,
    window_seconds: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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
    polling_rate_hz: Option<u16>,
    session_restarts_total: u64,
    consecutive_failures: u32,
    retry_delay_seconds: u32,
    incidents: Vec<Incident>,
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
            polling_rate_hz: None,
            session_restarts_total: 0,
            consecutive_failures: 0,
            retry_delay_seconds: 0,
            incidents: Vec::new(),
        };
        reporter.restore_previous();
        reporter
    }

    pub fn starting(&mut self, polling_rate_hz: Option<u16>) {
        self.status = ProviderStatus::Unknown;
        self.polling_rate_hz = polling_rate_hz;
        self.publish();
    }

    pub fn connected(&mut self, product_name: &str, vendor_id: u16, product_id: u16) {
        let now = unix_time_ms();
        let subject_id = subject_id(&self.salt, vendor_id, product_id, product_name);
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
        self.publish();
    }

    pub fn session_failed(&mut self, error: &razer_hid::Error, retry_delay_seconds: u32) {
        self.status = ProviderStatus::Degraded;
        self.session_restarts_total = self.session_restarts_total.saturating_add(1);
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.retry_delay_seconds = retry_delay_seconds;
        let Some(subject) = &self.subject else {
            self.publish();
            return;
        };
        let code = error_code(error).to_string();
        let native_error_code = error.native_error_code();
        let now = unix_time_ms();
        if let Some(existing) = self
            .incidents
            .iter_mut()
            .find(|incident| incident.code == code && incident.subject_id == subject.subject_id)
        {
            existing.occurred_at_unix_ms = now;
            existing.recovered_at_unix_ms = None;
            existing.occurrence_count = existing.occurrence_count.saturating_add(1);
            existing.native_error_code = native_error_code;
        } else {
            self.incidents.push(Incident {
                incident_id: format!("session-failure-{}", self.sequence.saturating_add(1)),
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
        self.publish();
    }

    pub fn polling_rate_changed(&mut self, polling_rate_hz: Option<u16>) {
        self.polling_rate_hz = polling_rate_hz;
        self.publish();
    }

    pub fn stopped(&mut self) {
        self.status = ProviderStatus::Unavailable;
        self.retry_delay_seconds = 0;
        self.publish();
    }

    fn restore_previous(&mut self) {
        let Ok(bytes) = fs::read(&self.capsule_path) else {
            return;
        };
        let Ok(capsule) = serde_json::from_slice::<Capsule>(&bytes) else {
            return;
        };
        self.sequence = capsule.sequence;
        self.subject = capsule.subjects.into_iter().next();
        self.incidents = capsule
            .recent_incidents
            .into_iter()
            .take(MAX_INCIDENTS)
            .collect();
        for metric in capsule.metrics {
            match metric.code.as_str() {
                "session_restarts_total" => {
                    self.session_restarts_total = metric.value.max(0.0) as u64
                }
                "consecutive_session_failures" => {
                    self.consecutive_failures = metric.value.max(0.0) as u32
                }
                _ => {}
            }
        }
    }

    fn publish(&mut self) {
        self.sequence = self.sequence.saturating_add(1).max(1);
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
        let capsule = Capsule {
            schema_version: SCHEMA_VERSION.to_string(),
            provider_id: PROVIDER_ID.to_string(),
            provider_version: PROVIDER_VERSION.to_string(),
            generated_at_unix_ms: unix_time_ms(),
            sequence: self.sequence,
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
            subjects: self.subject.clone().into_iter().collect(),
            metrics,
            recent_incidents: self.incidents.clone(),
        };
        let _ = persist_capsule(&self.capsule_path, &capsule);
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
        reporter.starting(Some(4_000));
        reporter.connected("Razer Viper V3", 0x1532, 0x00b2);
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
}
