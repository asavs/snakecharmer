//! Tiny thread-safe file+stdout logger with UTC timestamps and size-based
//! rotation. No external crates (footprint ethos); dependency-free time
//! formatting via a civil-date conversion.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_LOG_BYTES: u64 = 512 * 1024;

#[derive(Clone)]
pub struct Logger {
    inner: Arc<Inner>,
}

struct Inner {
    path: PathBuf,
    lock: Mutex<()>,
}

impl Logger {
    pub fn new(path: PathBuf) -> Logger {
        Logger {
            inner: Arc::new(Inner {
                path,
                lock: Mutex::new(()),
            }),
        }
    }

    pub fn log(&self, msg: &str) {
        let line = format!("{} {}", utc_timestamp(), msg);
        println!("{line}");
        let _guard = self.inner.lock.lock().unwrap_or_else(|p| p.into_inner());
        // Rotate if oversized.
        if let Ok(meta) = fs::metadata(&self.inner.path) {
            if meta.len() > MAX_LOG_BYTES {
                let _ = fs::rename(&self.inner.path, self.inner.path.with_extension("log.old"));
            }
        }
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&self.inner.path) {
            let _ = writeln!(f, "{line}");
        }
    }
}

/// Format the current time as `YYYY-MM-DD HH:MM:SS UTC` with no dependencies.
fn utc_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, mo, d, h, mi, s) = civil_from_epoch(secs);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02} UTC")
}

/// Convert Unix seconds to (year, month, day, hour, min, sec) in UTC using
/// Howard Hinnant's civil-from-days algorithm.
fn civil_from_epoch(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = (secs % 86_400) as u32;
    let (hour, min, sec) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = y + if month <= 2 { 1 } else { 0 };
    (year, month, day, hour, min, sec)
}

#[cfg(test)]
mod tests {
    use super::civil_from_epoch;

    #[test]
    fn known_epochs() {
        // 2021-01-01 00:00:00 UTC = 1609459200
        assert_eq!(civil_from_epoch(1_609_459_200), (2021, 1, 1, 0, 0, 0));
        // 2000-02-29 12:34:56 UTC = 951827696 (leap day)
        assert_eq!(civil_from_epoch(951_827_696), (2000, 2, 29, 12, 34, 56));
        // Epoch itself
        assert_eq!(civil_from_epoch(0), (1970, 1, 1, 0, 0, 0));
    }
}
