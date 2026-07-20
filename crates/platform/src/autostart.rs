//! "Start with Windows" registration via the per-user Run key
//! (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`), which is also what
//! makes the app appear in the OS's own startup UI (Task Manager → Startup
//! apps, Settings → Apps → Startup).
//!
//! The public surface deliberately exposes no path, key name, or registry
//! concept, so the eventual MSIX/Store build can reimplement this one file on
//! the WinRT `StartupTask` API without touching any caller. The OS is the
//! single source of truth: the user can revoke the entry from Task Manager at
//! any time (and a `StartupTask` enable can be refused by user or policy), so
//! [`enable`]/[`disable`] return the *actual* resulting state and callers must
//! render UI from that — never optimistically from what was requested.
//!
//! Only the daemon (`snakecharmer.exe`) may call this module: the registered
//! command comes from `current_exe()`, which is the wrong binary in
//! `charmctl.exe` or `diagram-editor.exe`. There is deliberately no way to
//! override the command string.

use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
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

/// Whether the app is registered to start at login *right now*. The OS is the
/// single source of truth — the user can revoke the registration from Task
/// Manager at any time. Never fails: absent / unreadable / wrong-typed all
/// mean "off".
pub fn is_enabled() -> bool {
    // SAFETY: read_command is a plain registry read; see its own comment.
    unsafe { read_command().is_some() }
}

/// Register the current exe to start at login. Idempotent. Returns the ACTUAL
/// state after the attempt (a genuine read-back), not the request.
pub fn enable() -> Result<bool, Error> {
    let command = current_command()?;
    // SAFETY: write_command validates its own preconditions; see its comment.
    unsafe { write_command(&command)? };
    Ok(is_enabled())
}

/// Remove the login registration. Idempotent (already-absent is success).
/// Returns the ACTUAL state after the attempt, not the request.
pub fn disable() -> Result<bool, Error> {
    // SAFETY: delete_command is a plain registry delete; see its own comment.
    unsafe { delete_command()? };
    Ok(is_enabled())
}

/// Re-point an already-enabled registration at the current exe (after a move
/// or rebuild). Never enables anything; `Ok(false)` when autostart is off.
pub fn refresh() -> Result<bool, Error> {
    // SAFETY: read_command is a plain registry read; see its own comment.
    let Some(existing) = (unsafe { read_command() }) else {
        return Ok(false);
    };
    let want = current_command()?;
    if !command_matches(&existing, &want) {
        // SAFETY: write_command validates its own preconditions; see its comment.
        unsafe { write_command(&want)? };
    }
    Ok(true)
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
    a.trim_matches('"').eq_ignore_ascii_case(b.trim_matches('"'))
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Read the registered command, or `None` when absent, unreadable, or not a
/// string value (`RRF_RT_REG_SZ` rejects wrong-typed data).
///
/// SAFETY: two-call `RegGetValueW` sizing — the first call only writes the
/// byte count, the second writes into a buffer allocated to exactly that
/// count. All string arguments are NUL-terminated UTF-16 that outlive the
/// calls.
unsafe fn read_command() -> Option<String> {
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
    if ok != 0 || size == 0 {
        return None;
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
    if ok != 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
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
        return Err(Error::Os { call: "RegCreateKeyExW", status: ok });
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
        return Err(Error::Os { call: "RegSetValueExW", status: ok });
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
        return Err(Error::Os { call: "RegOpenKeyExW", status: ok });
    }
    let val = to_wide(VALUE_NAME);
    let ok = RegDeleteValueW(hkey, val.as_ptr());
    RegCloseKey(hkey);
    if ok != 0 && ok != ERROR_FILE_NOT_FOUND {
        return Err(Error::Os { call: "RegDeleteValueW", status: ok });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_matching_ignores_quotes_and_case() {
        assert!(command_matches("\"C:\\Apps\\snakecharmer.exe\"", "\"c:\\apps\\SNAKECHARMER.EXE\""));
        assert!(command_matches("C:\\Apps\\snakecharmer.exe", "\"C:\\Apps\\snakecharmer.exe\""));
        assert!(!command_matches("\"C:\\old\\snakecharmer.exe\"", "\"C:\\new\\snakecharmer.exe\""));
    }

    #[test]
    fn current_command_is_quoted() {
        let cmd = current_command().unwrap();
        assert!(cmd.starts_with('"') && cmd.ends_with('"'), "unquoted: {cmd}");
        assert!(cmd.to_ascii_lowercase().contains(".exe"), "not an exe path: {cmd}");
    }

    /// Round-trips the real HKCU Run value, restoring whatever was there.
    #[test]
    #[ignore = "writes to the developer's real HKCU Run key"]
    fn hkcu_roundtrip_restores_prior_state() {
        // SAFETY: same registry internals the public API uses.
        let prior = unsafe { read_command() };
        assert!(enable().unwrap(), "enable must read back as on");
        assert!(is_enabled());
        assert!(!disable().unwrap(), "disable must read back as off");
        assert!(!is_enabled());
        if let Some(cmd) = prior {
            unsafe { write_command(&cmd).unwrap() };
        }
    }
}
