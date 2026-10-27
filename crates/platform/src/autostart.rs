//! "Start with Windows" registration via the per-user Run key
//! (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`), which is also what
//! makes the app appear in the OS's own startup UI (Task Manager → Startup
//! apps, Settings → Apps → Startup).
//!
//! This module observes registration, not Windows' effective startup policy.
//! Task Manager can disable startup independently of the Run value. The
//! documented `StartupTask.State` API requires a packaged startup task; this
//! unpackaged app does not decode or write undocumented StartupApproved data.
//! See Microsoft's "Startup apps" and "StartupTask" documentation.
//!
//! Only the daemon (`snakecharmer.exe`) may call this module: the registered
//! command comes from `current_exe()`, which is the wrong binary in
//! `charmctl.exe` or `diagram-editor.exe`. There is deliberately no way to
//! override the command string.

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_INVALID_DATA};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    RRF_RT_REG_SZ,
};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "Snakecharmer";

/// Error type for autostart registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// `current_exe()` failed, so there is no command to register.
    ExePath(String),
    /// A registry call returned a non-zero status.
    Os { call: &'static str, status: u32 },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::ExePath(e) => write!(f, "could not resolve the daemon's exe path: {e}"),
            Error::Os { call, status } => write!(f, "{call} failed (status {status})"),
        }
    }
}

impl std::error::Error for Error {}

/// What the Run value says. None of these states establishes whether Windows
/// will launch the app: startup settings and policy can override registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registration {
    Registered,
    Absent,
    /// The value could not be read or was not a valid string value.
    Unknown,
}

impl Registration {
    fn from_command(command: Option<String>) -> Self {
        if command.is_some() {
            Self::Registered
        } else {
            Self::Absent
        }
    }
}

/// Read registration without treating an unreadable value as an absent one.
pub fn registration() -> Registration {
    read_registration().unwrap_or(Registration::Unknown)
}

fn read_registration() -> Result<Registration, Error> {
    // SAFETY: read_command is a plain registry read; see its own comment.
    unsafe { read_command() }.map(Registration::from_command)
}

/// Register the current exe, then read back registration. This does not change
/// or verify the user's separate Windows startup setting.
pub fn register() -> Result<Registration, Error> {
    let command = current_command()?;
    // SAFETY: write_command validates its own preconditions; see its comment.
    unsafe { write_command(&command)? };
    read_registration()
}

/// Remove the login registration. Idempotent (already-absent is success).
/// Returns registration read-back; it does not alter Windows startup policy.
pub fn unregister() -> Result<Registration, Error> {
    // SAFETY: delete_command is a plain registry delete; see its own comment.
    unsafe { delete_command()? };
    read_registration()
}

/// Re-point an existing registration at the current exe after a move/rebuild.
/// Does not create an absent value or change Windows' separate startup policy.
pub fn refresh() -> Result<Registration, Error> {
    // SAFETY: read_command is a plain registry read; see its own comment.
    let Some(existing) = (unsafe { read_command()? }) else {
        return Ok(Registration::Absent);
    };
    let want = current_command()?;
    if !command_matches(&existing, &want) {
        // SAFETY: write_command validates its own preconditions; see its comment.
        unsafe { write_command(&want)? };
    }
    read_registration()
}

/// The command line to register: the current exe, quoted. Run values are
/// handed to `CreateProcess` as-is, so an unquoted path with spaces mis-parses.
fn current_command() -> Result<String, Error> {
    let exe = std::env::current_exe().map_err(|e| Error::ExePath(e.to_string()))?;
    Ok(format!("\"{}\"", exe.display()))
}

/// Whether two registered commands point at the same exe: compared
/// case-insensitively (Windows paths) after stripping the quoting.
fn command_matches(a: &str, b: &str) -> bool {
    a.trim_matches('"')
        .eq_ignore_ascii_case(b.trim_matches('"'))
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn value_present(status: u32) -> Result<bool, Error> {
    match status {
        0 => Ok(true),
        ERROR_FILE_NOT_FOUND => Ok(false),
        status => Err(Error::Os {
            call: "RegGetValueW",
            status,
        }),
    }
}

/// Read the registered command. Only a missing key/value means `None`;
/// unreadable or wrong-typed data is an error (`RRF_RT_REG_SZ` checks the type).
///
/// SAFETY: two-call `RegGetValueW` sizing — the first call only writes the
/// byte count, the second writes into a buffer allocated to exactly that
/// count. All string arguments are NUL-terminated UTF-16 that outlive the
/// calls.
unsafe fn read_command() -> Result<Option<String>, Error> {
    let sub = to_wide(RUN_KEY);
    let val = to_wide(VALUE_NAME);
    let mut size: u32 = 0;
    let ok = RegGetValueW(
        HKEY_CURRENT_USER,
        sub.as_ptr(),
        val.as_ptr(),
        RRF_RT_REG_SZ,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        &mut size,
    );
    if !value_present(ok)? {
        return Ok(None);
    }
    if size == 0 {
        return Err(Error::Os {
            call: "RegGetValueW",
            status: ERROR_INVALID_DATA,
        });
    }
    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    let ok = RegGetValueW(
        HKEY_CURRENT_USER,
        sub.as_ptr(),
        val.as_ptr(),
        RRF_RT_REG_SZ,
        std::ptr::null_mut(),
        buf.as_mut_ptr() as *mut _,
        &mut size,
    );
    if !value_present(ok)? {
        return Ok(None);
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let command = String::from_utf16(&buf[..len]).map_err(|_| Error::Os {
        call: "RegGetValueW",
        status: ERROR_INVALID_DATA,
    })?;
    Ok(Some(command))
}

/// Write `command` as the Run value.
///
/// SAFETY: standard create-then-set registry sequence; the opened key is
/// always closed. `cbData` must include the terminating NUL, hence
/// `wide.len() * 2` on a buffer `to_wide` already NUL-terminated.
unsafe fn write_command(command: &str) -> Result<(), Error> {
    let sub = to_wide(RUN_KEY);
    let mut hkey: HKEY = std::ptr::null_mut();
    let ok = RegCreateKeyExW(
        HKEY_CURRENT_USER,
        sub.as_ptr(),
        0,
        std::ptr::null(),
        REG_OPTION_NON_VOLATILE,
        KEY_SET_VALUE | KEY_QUERY_VALUE,
        std::ptr::null(),
        &mut hkey,
        std::ptr::null_mut(),
    );
    if ok != 0 {
        return Err(Error::Os {
            call: "RegCreateKeyExW",
            status: ok,
        });
    }
    let val = to_wide(VALUE_NAME);
    let wide = to_wide(command);
    let ok = RegSetValueExW(
        hkey,
        val.as_ptr(),
        0,
        REG_SZ,
        wide.as_ptr() as *const u8,
        (wide.len() * 2) as u32,
    );
    RegCloseKey(hkey);
    if ok != 0 {
        return Err(Error::Os {
            call: "RegSetValueExW",
            status: ok,
        });
    }
    Ok(())
}

/// Delete the Run value. An already-absent value (or key) is the desired
/// state, so `ERROR_FILE_NOT_FOUND` maps to `Ok`.
///
/// SAFETY: standard open-then-delete registry sequence; the opened key is
/// always closed. All string arguments are NUL-terminated UTF-16.
unsafe fn delete_command() -> Result<(), Error> {
    let sub = to_wide(RUN_KEY);
    let mut hkey: HKEY = std::ptr::null_mut();
    let ok = RegOpenKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, KEY_SET_VALUE, &mut hkey);
    if ok == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    if ok != 0 {
        return Err(Error::Os {
            call: "RegOpenKeyExW",
            status: ok,
        });
    }
    let val = to_wide(VALUE_NAME);
    let ok = RegDeleteValueW(hkey, val.as_ptr());
    RegCloseKey(hkey);
    if ok != 0 && ok != ERROR_FILE_NOT_FOUND {
        return Err(Error::Os {
            call: "RegDeleteValueW",
            status: ok,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_matching_ignores_quotes_and_case() {
        assert!(command_matches(
            "\"C:\\Apps\\snakecharmer.exe\"",
            "\"c:\\apps\\SNAKECHARMER.EXE\""
        ));
        assert!(command_matches(
            "C:\\Apps\\snakecharmer.exe",
            "\"C:\\Apps\\snakecharmer.exe\""
        ));
        assert!(!command_matches(
            "\"C:\\old\\snakecharmer.exe\"",
            "\"C:\\new\\snakecharmer.exe\""
        ));
    }

    #[test]
    fn current_command_is_quoted() {
        let cmd = current_command().unwrap();
        assert!(
            cmd.starts_with('"') && cmd.ends_with('"'),
            "unquoted: {cmd}"
        );
        assert!(
            cmd.to_ascii_lowercase().contains(".exe"),
            "not an exe path: {cmd}"
        );
    }

    #[test]
    fn read_failures_do_not_mean_absent_registration() {
        use windows_sys::Win32::Foundation::{
            ERROR_ACCESS_DENIED, ERROR_MORE_DATA, ERROR_UNSUPPORTED_TYPE,
        };
        assert_eq!(value_present(0), Ok(true));
        assert_eq!(value_present(ERROR_FILE_NOT_FOUND), Ok(false));
        for status in [ERROR_ACCESS_DENIED, ERROR_MORE_DATA, ERROR_UNSUPPORTED_TYPE] {
            assert_eq!(
                value_present(status),
                Err(Error::Os {
                    call: "RegGetValueW",
                    status
                })
            );
        }
    }

    /// Round-trips the real HKCU Run value, restoring whatever was there.
    #[test]
    #[ignore = "writes to the developer's real HKCU Run key"]
    fn hkcu_roundtrip_restores_prior_state() {
        // SAFETY: same registry internals the public API uses.
        let prior = unsafe { read_command().unwrap() };
        assert_eq!(register().unwrap(), Registration::Registered);
        assert_eq!(registration(), Registration::Registered);
        assert_eq!(unregister().unwrap(), Registration::Absent);
        assert_eq!(registration(), Registration::Absent);
        if let Some(cmd) = prior {
            unsafe { write_command(&cmd).unwrap() };
        }
    }
}
