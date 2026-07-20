//! System-tray icon + right-click menu (Win32 `Shell_NotifyIcon`).
//!
//! All the window-class / message-loop / popup-menu FFI is contained here.
//! The app supplies a menu spec (ids + labels, one level of submenus) and a
//! command callback; [`run`] creates the icon, pumps the message loop, and
//! invokes the callback with the chosen id. It does not return while the tray
//! is alive.

use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};

use windows_sys::core::PCWSTR;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
    DispatchMessageW, GetCursorPos, GetMessageW, GetWindowLongPtrW, LoadIconW, PostMessageW,
    PostQuitMessage, RegisterClassW, SetForegroundWindow, SetWindowLongPtrW, TrackPopupMenu,
    TranslateMessage, CW_USEDEFAULT, GWLP_USERDATA, HICON, HMENU, IDI_APPLICATION, MF_CHECKED,
    MF_POPUP, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, MSG, TPM_LEFTALIGN, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WM_APP, WM_COMMAND,
    WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};

const TRAY_CALLBACK_MSG: u32 = WM_APP + 1;
/// Posted (from any thread) to make the tray thread run `mouse_hook::sync()` —
/// the hook can only be (un)installed on this message-pumping thread.
const HOOK_SYNC_MSG: u32 = WM_APP + 2;

/// Command id the callback receives when the tray icon is activated by a plain
/// left-click or a double-click (both open settings; right-click still shows
/// the menu).
pub const TRAY_CLICK: u32 = 0xFFFF_FFFF;

/// The tray window handle (0 = tray not running), for cross-thread posts.
static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);

/// Ask the tray thread to reconcile the mouse hook with the desired state
/// (see [`crate::mouse_hook::sync`]). Safe from any thread; a no-op if the
/// tray isn't up yet — [`run`] syncs once at startup to cover that window.
pub fn request_hook_sync() {
    let hwnd = TRAY_HWND.load(Ordering::SeqCst);
    if hwnd != 0 {
        // SAFETY: PostMessageW is thread-safe; a stale/invalid HWND fails harmlessly.
        unsafe {
            PostMessageW(hwnd as HWND, HOOK_SYNC_MSG, 0, 0);
        }
    }
}

/// One entry in the tray's right-click menu.
#[derive(Clone)]
pub enum MenuItem {
    /// A clickable item that reports `id` to the callback.
    Action { id: u32, label: String },
    /// A checkable item. Reports `id` like an `Action`; `checked` is a *render*
    /// of state the app owns — the tray never toggles it itself.
    Check { id: u32, label: String, checked: bool },
    /// A horizontal separator.
    Separator,
    /// A nested submenu.
    Submenu { label: String, items: Vec<MenuItem> },
}

impl MenuItem {
    pub fn action(id: u32, label: impl Into<String>) -> MenuItem {
        MenuItem::Action { id, label: label.into() }
    }
    pub fn check(id: u32, label: impl Into<String>, checked: bool) -> MenuItem {
        MenuItem::Check { id, label: label.into(), checked }
    }
    pub fn submenu(label: impl Into<String>, items: Vec<MenuItem>) -> MenuItem {
        MenuItem::Submenu { label: label.into(), items }
    }
}

/// Boxed state stashed in the window's user-data pointer.
struct TrayState {
    on_command: Box<dyn Fn(u32)>,
    /// The runtime-drawn app icon (freed with `DestroyIcon` on `WM_DESTROY`);
    /// null if creation fell back to the stock icon.
    hicon: HICON,
}

/// The live menu spec. The popup HMENU is rebuilt from this on every
/// right-click, so replacing it via [`set_menu`] takes effect immediately —
/// same pattern as `mouse_hook::set_thumb_actions`.
static MENU: OnceLock<Mutex<Vec<MenuItem>>> = OnceLock::new();

fn menu_cell() -> &'static Mutex<Vec<MenuItem>> {
    MENU.get_or_init(|| Mutex::new(Vec::new()))
}

/// Replace the tray menu. Safe to call from any thread at any time; the next
/// right-click shows the new menu.
pub fn set_menu(items: Vec<MenuItem>) {
    if let Ok(mut m) = menu_cell().lock() {
        *m = items;
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Run the tray icon and its message loop on the current thread. Does not
/// return while the tray lives (call it on a dedicated thread).
pub fn run(tooltip: &str, menu: Vec<MenuItem>, on_command: impl Fn(u32) + 'static) {
    set_menu(menu);

    // Draw the app icon with GDI+ (kept up for the tray's lifetime; the HICON
    // is independent of GDI+ once created, but starting/stopping around it is
    // simplest here). Falls back to the stock application icon on failure.
    // SAFETY: matched startup/shutdown; the icon is drawn into an owned bitmap.
    let (gdiplus, hicon) = unsafe {
        let token = crate::diagram::startup();
        (token, crate::icon::create_app_icon())
    };

    let state = Box::new(TrayState {
        on_command: Box::new(on_command),
        hicon,
    });

    // SAFETY: standard Win32 window-class registration + hidden window creation.
    // All pointers (class name, state box) outlive the calls; the state box is
    // handed to the window via GWLP_USERDATA and freed on WM_DESTROY.
    unsafe {
        let hinstance = GetModuleHandleW(std::ptr::null());
        let class_name = to_wide("SnakecharmerTrayWnd");

        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&wc);

        let title = to_wide("Snakecharmer");
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null(),
        );
        // Hand ownership of the state to the window (freed in WM_DESTROY).
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
        TRAY_HWND.store(hwnd as isize, Ordering::SeqCst);

        // Add the notification-area icon (our drawn mark, or the stock icon if
        // drawing failed).
        let tray_icon =
            if hicon.is_null() { LoadIconW(std::ptr::null_mut(), IDI_APPLICATION) } else { hicon };
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = TRAY_CALLBACK_MSG;
        nid.hIcon = tray_icon;
        let tip = to_wide(tooltip);
        let n = tip.len().min(nid.szTip.len());
        nid.szTip[..n].copy_from_slice(&tip[..n]);
        Shell_NotifyIconW(NIM_ADD, &nid);

        // Reconcile the low-level mouse hook on THIS message-pumping thread (a
        // WH_MOUSE_LL hook requires its owning thread to pump messages). With
        // no thumb remaps configured this installs nothing; later changes
        // arrive as HOOK_SYNC_MSG posts from set_thumb_actions.
        crate::mouse_hook::sync();

        // Message loop.
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        TRAY_HWND.store(0, Ordering::SeqCst);
        crate::mouse_hook::uninstall();
        crate::diagram::shutdown(gdiplus);
    }
}

unsafe fn state_ref<'a>(hwnd: HWND) -> Option<&'a TrayState> {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const TrayState;
    ptr.as_ref()
}

/// Build a popup HMENU from the spec (one level of submenus supported).
unsafe fn build_menu(items: &[MenuItem]) -> HMENU {
    let hmenu = CreatePopupMenu();
    for item in items {
        match item {
            MenuItem::Separator => {
                AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());
            }
            MenuItem::Action { id, label } => {
                let w = to_wide(label);
                AppendMenuW(hmenu, MF_STRING, *id as usize, w.as_ptr());
            }
            MenuItem::Check { id, label, checked } => {
                let w = to_wide(label);
                let flags = MF_STRING | if *checked { MF_CHECKED } else { MF_UNCHECKED };
                AppendMenuW(hmenu, flags, *id as usize, w.as_ptr());
            }
            MenuItem::Submenu { label, items } => {
                let sub = build_menu(items);
                let w = to_wide(label);
                AppendMenuW(hmenu, MF_POPUP, sub as usize, w.as_ptr());
            }
        }
    }
    hmenu
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        TRAY_CALLBACK_MSG => {
            // Low word of lParam is the mouse event on the icon.
            match lparam as u32 {
                WM_RBUTTONUP => show_context_menu(hwnd),
                // A single left-click or a double-click opens settings.
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                    if let Some(state) = state_ref(hwnd) {
                        (state.on_command)(TRAY_CLICK);
                    }
                }
                _ => {}
            }
            0
        }
        WM_COMMAND => {
            // Menu selection via TrackPopupMenu(TPM_RETURNCMD) is handled inline
            // in show_context_menu; this path covers accelerator/child cases.
            0
        }
        HOOK_SYNC_MSG => {
            crate::mouse_hook::sync();
            0
        }
        WM_DESTROY => {
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = hwnd;
            nid.uID = 1;
            Shell_NotifyIconW(NIM_DELETE, &nid);
            // Reclaim and drop the state box, freeing the drawn icon.
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayState;
            if !ptr.is_null() {
                let state = Box::from_raw(ptr);
                if !state.hicon.is_null() {
                    DestroyIcon(state.hicon);
                }
                drop(state);
            }
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn show_context_menu(hwnd: HWND) {
    let Some(state) = state_ref(hwnd) else { return };
    // Clone the spec out so the lock is not held across TrackPopupMenu (which
    // blocks until the user dismisses the menu).
    let items: Vec<MenuItem> = match menu_cell().lock() {
        Ok(m) => m.clone(),
        Err(_) => return,
    };
    let hmenu = build_menu(&items);

    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    // Required so the menu dismisses correctly when clicking elsewhere.
    SetForegroundWindow(hwnd);

    let cmd = TrackPopupMenu(
        hmenu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_LEFTALIGN,
        pt.x,
        pt.y,
        0,
        hwnd,
        std::ptr::null(),
    );
    PostMessageW(hwnd, 0x0000, 0, 0); // WM_NULL, per MSDN TrackPopupMenu note
    DestroyMenu(hmenu);

    if cmd != 0 {
        (state.on_command)(cmd as u32);
    }
}

// --- Console attach (for the CLI binary launched from a terminal) ---------

/// Attach to the parent process's console if there is one, so a
/// `windows`-subsystem build can still print to the launching terminal.
/// No-op (and harmless) when there is no parent console.
pub fn attach_parent_console() {
    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    // SAFETY: AttachConsole takes only a pid constant; failure is fine.
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

// Silence unused import warnings for PCWSTR / c_void when features shift.
#[allow(dead_code)]
fn _type_anchors(_: PCWSTR, _: *const c_void) {}
