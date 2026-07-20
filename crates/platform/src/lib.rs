//! Isolated Windows platform FFI for Snakecharmer.
//!
//! Every `unsafe` block in the project lives here, contained and documented:
//!   * [`send_chord`] — synthesize a keystroke via `SendInput`.
//!   * [`acquire_single_instance`] — a named-mutex single-instance guard.
//!   * [`alert_retry`] — a blocking Retry/Cancel message box via `MessageBoxW`.
//!   * key-code helpers ([`vk_for_char`], [`vk_function`], and the `VK_*` consts).
//!   * [`autostart`] — "Start with Windows" registration via the HKCU Run key.
//!
//! The rest of the codebase (`razer-proto`, `razer-hid`, the daemon logic) stays
//! safe Rust and never touches Win32 directly.

pub mod autostart;
pub mod diagram;
pub mod icon;
pub mod mouse_hook;
pub mod settings;
pub mod tray;

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows_sys::Win32::Storage::FileSystem::{
    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, VkKeyScanW, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDRETRY, MB_ICONINFORMATION, MB_RETRYCANCEL, MB_SETFOREGROUND,
};

/// Common virtual-key codes (a small, hand-picked subset).
pub const VK_CONTROL: u16 = 0x11;
pub const VK_SHIFT: u16 = 0x10;
pub const VK_MENU: u16 = 0x12; // Alt
pub const VK_LWIN: u16 = 0x5B;

/// Error type for keystroke injection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InjectError {
    /// No keys were supplied.
    Empty,
    /// `SendInput` accepted fewer events than we submitted.
    Partial { sent: u32, wanted: u32 },
}

impl std::fmt::Display for InjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InjectError::Empty => write!(f, "no keys to send"),
            InjectError::Partial { sent, wanted } => {
                write!(f, "SendInput injected {sent}/{wanted} events")
            }
        }
    }
}

impl std::error::Error for InjectError {}

/// Resolve a printable character to a virtual-key code via `VkKeyScanW`.
///
/// Returns `None` if the character has no key on the current layout. The
/// returned code is the low byte only; modifier requirements in the high byte
/// are intentionally ignored (our default actions — `c`, `v`, `x`, `9`, `0` —
/// need no shift).
pub fn vk_for_char(c: char) -> Option<u16> {
    let mut buf = [0u16; 2];
    let encoded = c.encode_utf16(&mut buf);
    if encoded.len() != 1 {
        return None; // non-BMP characters have no single VK
    }
    // SAFETY: VkKeyScanW is a pure lookup over a u16 code point; no pointers,
    // no state mutated. It returns -1 when there is no mapping.
    let res = unsafe { VkKeyScanW(encoded[0]) };
    if res == -1 {
        None
    } else {
        Some((res as u16) & 0x00FF)
    }
}

/// Virtual-key code for a function key F1..=F24 (`n` is 1-based). `None` if out of range.
pub fn vk_function(n: u8) -> Option<u16> {
    // VK_F1 = 0x70 ... VK_F24 = 0x87
    if (1..=24).contains(&n) {
        Some(0x70 + (n as u16) - 1)
    } else {
        None
    }
}

fn key_input(vk: u16, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if key_up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Inject a keystroke chord: press every vk in order, then release in reverse.
///
/// Example: `send_chord(&[VK_CONTROL, vk_for_char('c').unwrap()])` sends Ctrl+C.
pub fn send_chord(vks: &[u16]) -> Result<(), InjectError> {
    if vks.is_empty() {
        return Err(InjectError::Empty);
    }
    let mut events: Vec<INPUT> = Vec::with_capacity(vks.len() * 2);
    for &vk in vks {
        events.push(key_input(vk, false));
    }
    for &vk in vks.iter().rev() {
        events.push(key_input(vk, true));
    }
    let wanted = events.len() as u32;
    // SAFETY: `events` is a valid, contiguous slice of `wanted` INPUT structs
    // that outlives the call; we pass the correct count and element size, exactly
    // as the SendInput contract requires. SendInput does not retain the pointer.
    let sent = unsafe {
        SendInput(
            wanted,
            events.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if sent == wanted {
        Ok(())
    } else {
        Err(InjectError::Partial { sent, wanted })
    }
}

/// Acquire a process-lifetime single-instance guard backed by a named mutex.
///
/// Returns `true` if this is the first/only instance, `false` if another
/// process already holds the named mutex. The mutex handle is intentionally
/// leaked so it lives for the whole process; Windows releases it on exit.
pub fn acquire_single_instance(name: &str) -> bool {
    let wide: Vec<u16> = OsStr::new(name).encode_wide().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is a valid NUL-terminated UTF-16 buffer that outlives the
    // call. A null security-attributes pointer is explicitly allowed. We never
    // close the returned handle, so the mutex persists for the process lifetime.
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, wide.as_ptr()) };
    if handle.is_null() {
        // Could not create the mutex at all; fail open (allow running) rather
        // than block the daemon on an unexpected OS error.
        return true;
    }
    // SAFETY: GetLastError just reads this thread's last-error slot.
    let already = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    !already
}

/// Show a blocking informational message box with Retry/Cancel buttons.
/// Returns `true` if Retry was clicked, `false` on Cancel (or close). Blocks
/// the calling thread until dismissed — callers that must not stall (e.g. the
/// daemon's retry loop) should invoke this from a throwaway thread.
pub fn alert_retry(title: &str, text: &str) -> bool {
    let title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let text: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: both buffers are valid NUL-terminated UTF-16 strings that outlive
    // the call; a null owner HWND is explicitly allowed and makes the box
    // free-standing. MessageBoxW does not retain the pointers.
    let clicked = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_RETRYCANCEL | MB_ICONINFORMATION | MB_SETFOREGROUND,
        )
    };
    clicked == IDRETRY
}

/// Atomically replace a same-directory destination with a fully written temporary file.
pub fn atomic_replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_vks_resolve() {
        // Digits and common letters must resolve on any US-like layout.
        assert!(vk_for_char('c').is_some());
        assert!(vk_for_char('v').is_some());
        assert!(vk_for_char('9').is_some());
        assert!(vk_for_char('0').is_some());
    }

    #[test]
    fn function_key_range() {
        assert_eq!(vk_function(1), Some(0x70));
        assert_eq!(vk_function(13), Some(0x7C));
        assert_eq!(vk_function(24), Some(0x87));
        assert_eq!(vk_function(0), None);
        assert_eq!(vk_function(25), None);
    }

    #[test]
    fn empty_chord_is_error() {
        assert_eq!(send_chord(&[]), Err(InjectError::Empty));
    }
}
