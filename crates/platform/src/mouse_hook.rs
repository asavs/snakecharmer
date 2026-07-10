//! Low-level mouse hook (`WH_MOUSE_LL`) for remapping the two thumb buttons
//! (XBUTTON1 = Back, XBUTTON2 = Forward) to keystrokes, suppressing the native
//! Back/Forward action.
//!
//! # Contract & safety (see docs/P5-HANDOFF.md)
//! * The hook is installed by [`install`], which MUST be called on a thread
//!   that pumps messages (the tray GUI thread) — Windows silently drops a
//!   `WH_MOUSE_LL` hook whose thread doesn't pump. [`uninstall`] removes it.
//! * The hook proc must be fast (Windows enforces `LowLevelHooksTimeout`, ~300 ms)
//!   and must never block: it uses `try_lock` and falls back to **pass-through**
//!   on the (near-impossible) contended case, so it can never freeze input.
//! * **Default is pass-through.** Only a thumb button with a configured remap is
//!   suppressed; every other event (left/right/middle/move/scroll, and thumb
//!   buttons with no remap) goes straight to `CallNextHookEx`.
//! * Injected events (`LLMHF_INJECTED`) are ignored. Injecting *keystrokes* does
//!   not re-enter this *mouse* hook.
//! * UIPI limitation: a non-elevated hook cannot intercept input over an
//!   elevated (higher-integrity) foreground window. Documented, not fought.

use std::sync::atomic::{AtomicIsize, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, MSLLHOOKSTRUCT, WH_MOUSE_LL,
};

// Locally-defined constants (kept explicit to avoid feature/type drift).
const HC_ACTION: i32 = 0;
const LLMHF_INJECTED: u32 = 0x0000_0001;
const WM_XBUTTONDOWN: u32 = 0x020B;
const WM_XBUTTONUP: u32 = 0x020C;
const XBUTTON1: u16 = 0x0001; // Back
const XBUTTON2: u16 = 0x0002; // Forward

/// Configured thumb-button chords (VK sequences). `None` = pass through.
struct HookState {
    back: Option<Vec<u16>>,    // XBUTTON1
    forward: Option<Vec<u16>>, // XBUTTON2
}

static STATE: OnceLock<Mutex<HookState>> = OnceLock::new();
/// The installed HHOOK, as an isize (0 = not installed).
static HOOK: AtomicIsize = AtomicIsize::new(0);
/// Which thumb buttons had a suppressed DOWN and thus need a suppressed UP.
/// bit0 = XBUTTON1, bit1 = XBUTTON2.
static SUPPRESSED: AtomicU8 = AtomicU8::new(0);

fn state() -> &'static Mutex<HookState> {
    STATE.get_or_init(|| {
        Mutex::new(HookState {
            back: None,
            forward: None,
        })
    })
}

/// Set (or clear) the thumb-button remaps. Safe to call from any thread at any
/// time; the hook proc picks up the change on the next event.
pub fn set_thumb_actions(back: Option<Vec<u16>>, forward: Option<Vec<u16>>) {
    if let Ok(mut s) = state().lock() {
        s.back = back;
        s.forward = forward;
    }
}

/// Install the `WH_MOUSE_LL` hook. Must run on a message-pumping thread.
/// Returns true on success. Idempotent-ish: re-installing leaks the old hook,
/// so call once per thread lifetime.
pub fn install() -> bool {
    // SAFETY: standard SetWindowsHookExW with our static hook proc and this
    // module's HINSTANCE. dwThreadId 0 = global low-level hook.
    unsafe {
        let hinst = GetModuleHandleW(std::ptr::null());
        let h = SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), hinst, 0);
        if h.is_null() {
            return false;
        }
        HOOK.store(h as isize, Ordering::SeqCst);
        true
    }
}

/// Remove the hook (call on Quit / tray teardown so Back/Forward work again).
pub fn uninstall() {
    let h = HOOK.swap(0, Ordering::SeqCst);
    if h != 0 {
        // SAFETY: `h` was a valid HHOOK from install(); removing once.
        unsafe {
            UnhookWindowsHookEx(h as HHOOK);
        }
    }
}

/// True if the hook is currently installed.
pub fn is_installed() -> bool {
    HOOK.load(Ordering::SeqCst) != 0
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION {
        let msg = wparam as u32;
        if msg == WM_XBUTTONDOWN || msg == WM_XBUTTONUP {
            let info = &*(lparam as *const MSLLHOOKSTRUCT);
            if info.flags & LLMHF_INJECTED == 0 {
                let xbtn = ((info.mouseData >> 16) & 0xFFFF) as u16; // HIWORD(mouseData)
                let bit = match xbtn {
                    XBUTTON1 => 0b01u8,
                    XBUTTON2 => 0b10u8,
                    _ => 0,
                };
                if bit != 0 {
                    if msg == WM_XBUTTONDOWN {
                        // Grab the chord (cloned) under a non-blocking lock.
                        let chord = state().try_lock().ok().and_then(|s| match bit {
                            0b01 => s.back.clone(),
                            _ => s.forward.clone(),
                        });
                        if let Some(vks) = chord {
                            SUPPRESSED.fetch_or(bit, Ordering::SeqCst);
                            let _ = crate::send_chord(&vks);
                            return 1; // suppress native Back/Forward DOWN
                        }
                    } else {
                        // Suppress the UP iff we suppressed its DOWN.
                        if SUPPRESSED.load(Ordering::SeqCst) & bit != 0 {
                            SUPPRESSED.fetch_and(!bit, Ordering::SeqCst);
                            return 1;
                        }
                    }
                }
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}
