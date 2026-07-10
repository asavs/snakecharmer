//! System-tray icon + right-click menu (Win32 `Shell_NotifyIcon`).
//!
//! All the window-class / message-loop / popup-menu FFI is contained here.
//! The app supplies a menu spec (ids + labels, one level of submenus) and a
//! command callback; [`run`] creates the icon, pumps the message loop, and
//! invokes the callback with the chosen id. It does not return while the tray
//! is alive.

use std::ffi::c_void;

use windows_sys::core::PCWSTR;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW,
    GetCursorPos, GetMessageW, GetWindowLongPtrW, LoadIconW, PostMessageW, PostQuitMessage,
    RegisterClassW, SetForegroundWindow, SetWindowLongPtrW, TrackPopupMenu, TranslateMessage,
    CW_USEDEFAULT, GWLP_USERDATA, HMENU, IDI_APPLICATION, MF_POPUP, MF_SEPARATOR, MF_STRING, MSG,
    TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_COMMAND, WM_DESTROY,
    WM_LBUTTONDBLCLK, WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};

const TRAY_CALLBACK_MSG: u32 = WM_APP + 1;

/// Command id the callback receives when the tray icon is double-clicked.
pub const TRAY_DOUBLE_CLICK: u32 = 0xFFFF_FFFF;

/// One entry in the tray's right-click menu.
pub enum MenuItem {
    /// A clickable item that reports `id` to the callback.
    Action { id: u32, label: String },
    /// A horizontal separator.
    Separator,
    /// A nested submenu.
    Submenu { label: String, items: Vec<MenuItem> },
}

impl MenuItem {
    pub fn action(id: u32, label: impl Into<String>) -> MenuItem {
        MenuItem::Action { id, label: label.into() }
    }
    pub fn submenu(label: impl Into<String>, items: Vec<MenuItem>) -> MenuItem {
        MenuItem::Submenu { label: label.into(), items }
    }
}

/// Boxed state stashed in the window's user-data pointer.
struct TrayState {
    menu: Vec<MenuItem>,
    on_command: Box<dyn Fn(u32)>,
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Run the tray icon and its message loop on the current thread. Does not
/// return while the tray lives (call it on a dedicated thread).
pub fn run(tooltip: &str, menu: Vec<MenuItem>, on_command: impl Fn(u32) + 'static) {
    let state = Box::new(TrayState {
        menu,
        on_command: Box::new(on_command),
    });

    // SAFETY: standard Win32 window-class registration + hidden window creation.
    // All pointers (class name, state box) outlive the calls; the state box is
    // handed to the window via GWLP_USERDATA and freed on WM_DESTROY.
    unsafe {
        let hinstance = GetModuleHandleW(std::ptr::null());
        let class_name = to_wide("AntiSynapseTrayWnd");

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

        let title = to_wide("Anti-Synapse");
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

        // Add the notification-area icon.
        let hicon = LoadIconW(std::ptr::null_mut(), IDI_APPLICATION);
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = TRAY_CALLBACK_MSG;
        nid.hIcon = hicon;
        let tip = to_wide(tooltip);
        let n = tip.len().min(nid.szTip.len());
        nid.szTip[..n].copy_from_slice(&tip[..n]);
        Shell_NotifyIconW(NIM_ADD, &nid);

        // Install the low-level mouse hook on THIS message-pumping thread (a
        // WH_MOUSE_LL hook requires its owning thread to pump messages). It is
        // pass-through until thumb-button remaps are configured.
        crate::mouse_hook::install();

        // Message loop.
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        crate::mouse_hook::uninstall();
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
                WM_LBUTTONDBLCLK => {
                    if let Some(state) = state_ref(hwnd) {
                        (state.on_command)(TRAY_DOUBLE_CLICK);
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
        WM_DESTROY => {
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = hwnd;
            nid.uID = 1;
            Shell_NotifyIconW(NIM_DELETE, &nid);
            // Reclaim and drop the state box.
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayState;
            if !ptr.is_null() {
                drop(Box::from_raw(ptr));
            }
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn show_context_menu(hwnd: HWND) {
    let Some(state) = state_ref(hwnd) else { return };
    let hmenu = build_menu(&state.menu);

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
