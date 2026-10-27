//! A process lease backed by the lifetime of a named kernel object.
//!
//! Unlike an owned Win32 mutex, the lease can be dropped by any thread. We
//! never wait on or own the mutex: only the process that creates the object
//! may proceed, and Windows destroys it when its last handle closes, including
//! on process exit. Callers share one lease inside their own process.

use std::os::windows::io::{FromRawHandle, OwnedHandle};

use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows_sys::Win32::System::Threading::CreateMutexW;

#[derive(Debug)]
pub enum Error {
    Busy,
    Os(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => write!(f, "another process holds the lease"),
            Self::Os(error) => write!(f, "could not acquire the lease: {error}"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug)]
pub struct ProcessLease {
    _handle: OwnedHandle,
}

impl ProcessLease {
    /// Acquire without waiting. Existing objects and OS failures both deny access.
    pub fn acquire(name: &str) -> Result<Self, Error> {
        if name.is_empty() || name.contains('\0') {
            return Err(Error::Os(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "lease name must be nonempty and contain no NUL",
            )));
        }
        let name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: name is NUL-terminated and lives through the call. Null
        // security attributes make the handle non-inheritable; FALSE avoids
        // thread ownership. Read last-error immediately after CreateMutexW.
        let (raw, error) = unsafe {
            let raw = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            (raw, GetLastError())
        };
        if raw.is_null() {
            return Err(Error::Os(std::io::Error::from_raw_os_error(error as i32)));
        }
        // SAFETY: CreateMutexW returned a new, valid handle owned by this call.
        // OwnedHandle closes it exactly once, on any thread, including on Busy.
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        if error == ERROR_ALREADY_EXISTS {
            return Err(Error::Busy);
        }
        Ok(Self { _handle: handle })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn name() -> String {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        format!(
            "Global\\SnakecharmerLeaseTest-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn child(name: &str, expected: &str) {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "process_lease::tests::child_probe",
                "--nocapture",
            ])
            .env("SNAKECHARMER_TEST_LEASE_NAME", name)
            .env("SNAKECHARMER_TEST_LEASE_EXPECT", expected)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn child_probe() {
        let Ok(name) = std::env::var("SNAKECHARMER_TEST_LEASE_NAME") else {
            return;
        };
        match std::env::var("SNAKECHARMER_TEST_LEASE_EXPECT")
            .unwrap()
            .as_str()
        {
            "busy" => assert!(matches!(ProcessLease::acquire(&name), Err(Error::Busy))),
            "free" => {
                ProcessLease::acquire(&name).unwrap();
            }
            "exit" => {
                let _lease = ProcessLease::acquire(&name).unwrap();
                // No Rust destructors: Windows must release the lease itself.
                std::process::exit(0);
            }
            other => panic!("unexpected lease probe {other}"),
        }
    }

    #[test]
    fn excludes_another_process_until_the_last_shared_handle_drops() {
        let name = name();
        let lease = std::sync::Arc::new(ProcessLease::acquire(&name).unwrap());
        let other = std::sync::Arc::clone(&lease);
        child(&name, "busy");
        drop(lease);
        child(&name, "busy");
        std::thread::spawn(move || drop(other)).join().unwrap();
        child(&name, "free");
    }

    #[test]
    fn process_exit_releases_the_lease_without_destructors() {
        let name = name();
        child(&name, "exit");
        ProcessLease::acquire(&name).unwrap();
    }

    #[test]
    fn invalid_names_fail_closed() {
        assert!(matches!(ProcessLease::acquire(""), Err(Error::Os(_))));
        assert!(matches!(
            ProcessLease::acquire("invalid\0name"),
            Err(Error::Os(_))
        ));
        assert!(matches!(
            ProcessLease::acquire("invalid\\namespace"),
            Err(Error::Os(_))
        ));
    }
}
