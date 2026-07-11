//! Native Win32 settings window (no GUI framework — `windows-sys` common
//! controls only, to protect the footprint gate).
//!
//! The window is generic: the app supplies current values + label lists via
//! [`SettingsInit`] and a callback; the window emits [`SettingsEvent`]s (with
//! indices/values) on live changes and on Apply/Save. Mapping indices back to
//! action strings / effects stays in the app, so this module knows nothing
//! about Razer specifics. [`open`] runs a private message loop and returns when
//! the window closes (call it on a dedicated thread).

use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateSolidBrush, DeleteObject, GetStockObject, SetBkMode, DEFAULT_GUI_FONT, HBRUSH, OPAQUE,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::Dialogs::{ChooseColorW, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW};
use windows_sys::Win32::UI::Controls::{
    InitCommonControlsEx, ICC_BAR_CLASSES, INITCOMMONCONTROLSEX, TBM_SETPOS, TBM_SETRANGE,
    TB_BOTTOM, TB_ENDTRACK, TB_LINEDOWN, TB_LINEUP, TB_PAGEDOWN, TB_PAGEUP, TB_THUMBPOSITION,
    TB_TOP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, LoadCursorW,
    RegisterClassW, SendMessageW, SetWindowTextW, ShowWindow, TranslateMessage, CBS_DROPDOWNLIST,
    CB_ADDSTRING, CB_GETCURSEL, CB_SETCURSEL, CW_USEDEFAULT, IDC_ARROW, MSG,
    SW_SHOW, WM_CLOSE, WM_COMMAND, WM_CTLCOLORSTATIC, WM_DESTROY, WM_HSCROLL, WM_SETFONT,
    WNDCLASSW, WS_CHILD, WS_OVERLAPPED, WS_CAPTION, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

// Style/message constants windows-sys does not surface as named items.
const SS_CENTER: u32 = 0x0000_0001;
const TBM_GETPOS: u32 = 0x0400; // WM_USER + 0
const BS_PUSHBUTTON: u32 = 0x0000_0000;
const CBN_SELCHANGE: u16 = 1;
const BN_CLICKED: u16 = 0;

// Control ids.
const ID_TB_DPI: u16 = 101;
const ID_LBL_DPI: u16 = 102;
const ID_CB_UP: u16 = 103;
const ID_CB_DOWN: u16 = 104;
const ID_CB_EFFECT: u16 = 105;
const ID_CB_THUMB_BACK: u16 = 111;
const ID_CB_THUMB_FWD: u16 = 112;
const ID_CB_POLL: u16 = 113;
const ID_SWATCH: u16 = 106;
const ID_BTN_COLOR: u16 = 107;
const ID_BTN_APPLY: u16 = 108;
const ID_BTN_SAVE: u16 = 109;
const ID_BTN_CLOSE: u16 = 110;

/// Initial values to populate the window.
pub struct SettingsInit {
    pub dpi: u16,
    pub dpi_min: u16,
    pub dpi_max: u16,
    pub polling_labels: Vec<String>,
    pub polling_index: usize,
    pub action_labels: Vec<String>,
    pub up_index: usize,
    pub down_index: usize,
    pub thumb_back_index: usize,
    pub thumb_forward_index: usize,
    pub effect_labels: Vec<String>,
    pub effect_index: usize,
    pub color: (u8, u8, u8),
}

/// Events emitted by the window on live changes / button presses.
#[derive(Debug, Clone, Copy)]
pub enum SettingsEvent {
    Dpi(u16),
    Polling(usize),
    UpAction(usize),
    DownAction(usize),
    ThumbBack(usize),
    ThumbForward(usize),
    Effect(usize),
    Color(u8, u8, u8),
    Apply,
    Save,
}

struct WindowState {
    on_event: Box<dyn Fn(SettingsEvent)>,
    tb: HWND,
    lbl_dpi: HWND,
    swatch: HWND,
    color: (u8, u8, u8),
    swatch_brush: HBRUSH,
    dpi_min: u16,
    dpi_max: u16,
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn colorref(c: (u8, u8, u8)) -> COLORREF {
    // COLORREF is 0x00BBGGRR
    (c.0 as u32) | ((c.1 as u32) << 8) | ((c.2 as u32) << 16)
}

fn loword(v: usize) -> u16 {
    (v & 0xFFFF) as u16
}
fn hiword(v: usize) -> u16 {
    ((v >> 16) & 0xFFFF) as u16
}

/// Open the settings window and run its message loop until closed.
pub fn open(init: SettingsInit, on_event: impl Fn(SettingsEvent) + 'static) {
    // SAFETY: standard Win32 control creation. All wide-string buffers outlive
    // their calls; the boxed WindowState is owned by the window via GWLP_USERDATA
    // (see the tray module for the same pattern) and dropped on WM_DESTROY.
    unsafe {
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_BAR_CLASSES,
        };
        InitCommonControlsEx(&icc);

        let hinstance = GetModuleHandleW(std::ptr::null());
        let class_name = to_wide("SnakecharmerSettings");
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: (15 + 1) as _, // COLOR_BTNFACE + 1
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&wc);

        let title = to_wide("Snakecharmer Settings");
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU;
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            390,
            524,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            return;
        }

        let font = GetStockObject(DEFAULT_GUI_FONT);
        let mk = |class: &str, text: &str, style: u32, id: u16, x, y, w, h| -> HWND {
            let cls = to_wide(class);
            let txt = to_wide(text);
            let child = CreateWindowExW(
                0,
                cls.as_ptr(),
                txt.as_ptr(),
                WS_CHILD | WS_VISIBLE | style,
                x,
                y,
                w,
                h,
                hwnd,
                id as isize as _,
                hinstance,
                std::ptr::null(),
            );
            SendMessageW(child, WM_SETFONT, font as WPARAM, 1);
            child
        };

        // DPI label + trackbar.
        let lbl_dpi = mk("STATIC", &format!("DPI: {}", init.dpi), SS_CENTER, ID_LBL_DPI, 20, 12, 330, 20);
        let tb = mk("msctls_trackbar32", "", WS_TABSTOP, ID_TB_DPI, 20, 34, 330, 30);
        SendMessageW(
            tb,
            TBM_SETRANGE,
            1,
            ((init.dpi_min as i32) | ((init.dpi_max as i32) << 16)) as LPARAM,
        );
        SendMessageW(tb, TBM_SETPOS, 1, init.dpi as LPARAM);

        // Helper to create an action combo populated with the shared labels.
        let combo_style = (CBS_DROPDOWNLIST as u32) | WS_TABSTOP;
        let make_action_combo = |id: u16, y: i32, sel: usize| -> HWND {
            let cb = mk("COMBOBOX", "", combo_style, id, 20, y, 330, 220);
            for label in &init.action_labels {
                let w = to_wide(label);
                SendMessageW(cb, CB_ADDSTRING, 0, w.as_ptr() as LPARAM);
            }
            SendMessageW(cb, CB_SETCURSEL, sel, 0);
            cb
        };

        // Polling rate combo (the device's supported rates; "keep" = don't manage).
        let _ = mk("STATIC", "Polling rate:", 0, 0, 20, 74, 330, 18);
        let cb_poll = mk("COMBOBOX", "", combo_style, ID_CB_POLL, 20, 94, 330, 200);
        for label in &init.polling_labels {
            let w = to_wide(label);
            SendMessageW(cb_poll, CB_ADDSTRING, 0, w.as_ptr() as LPARAM);
        }
        SendMessageW(cb_poll, CB_SETCURSEL, init.polling_index, 0);

        // DPI-button action combos.
        let _ = mk("STATIC", "Front DPI button (dpi_up):", 0, 0, 20, 128, 330, 18);
        let _cb_up = make_action_combo(ID_CB_UP, 148, init.up_index);
        let _ = mk("STATIC", "Rear DPI button (dpi_down):", 0, 0, 20, 182, 330, 18);
        let _cb_down = make_action_combo(ID_CB_DOWN, 202, init.down_index);

        // Thumb-button action combos (XBUTTON1 / XBUTTON2).
        let _ = mk("STATIC", "Back thumb button (XBUTTON1):", 0, 0, 20, 236, 330, 18);
        let _cb_tb = make_action_combo(ID_CB_THUMB_BACK, 256, init.thumb_back_index);
        let _ = mk("STATIC", "Forward thumb button (XBUTTON2):", 0, 0, 20, 290, 330, 18);
        let _cb_tf = make_action_combo(ID_CB_THUMB_FWD, 310, init.thumb_forward_index);

        // Lighting effect combo.
        let _ = mk("STATIC", "Lighting effect:", 0, 0, 20, 344, 330, 18);
        let cb_effect = mk("COMBOBOX", "", combo_style, ID_CB_EFFECT, 20, 364, 330, 200);
        for label in &init.effect_labels {
            let w = to_wide(label);
            SendMessageW(cb_effect, CB_ADDSTRING, 0, w.as_ptr() as LPARAM);
        }
        SendMessageW(cb_effect, CB_SETCURSEL, init.effect_index, 0);

        // Color swatch + picker button.
        let _ = mk("STATIC", "Color:", 0, 0, 20, 406, 40, 20);
        let swatch = mk("STATIC", "", SS_CENTER, ID_SWATCH, 66, 404, 80, 22);
        let _ = mk("BUTTON", "Choose...", BS_PUSHBUTTON | WS_TABSTOP, ID_BTN_COLOR, 156, 402, 90, 26);

        // Action buttons.
        let _ = mk("BUTTON", "Apply", BS_PUSHBUTTON | WS_TABSTOP, ID_BTN_APPLY, 20, 448, 90, 28);
        let _ = mk("BUTTON", "Save", BS_PUSHBUTTON | WS_TABSTOP, ID_BTN_SAVE, 130, 448, 90, 28);
        let _ = mk("BUTTON", "Close", BS_PUSHBUTTON | WS_TABSTOP, ID_BTN_CLOSE, 260, 448, 90, 28);

        let state = Box::new(WindowState {
            on_event: Box::new(on_event),
            tb,
            lbl_dpi,
            swatch,
            color: init.color,
            swatch_brush: CreateSolidBrush(colorref(init.color)),
            dpi_min: init.dpi_min,
            dpi_max: init.dpi_max,
        });
        use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowLongPtrW, GWLP_USERDATA};
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);

        ShowWindow(hwnd, SW_SHOW);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe fn state_mut<'a>(hwnd: HWND) -> Option<&'a mut WindowState> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, GWLP_USERDATA};
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
    ptr.as_mut()
}

fn snap50(v: i32) -> u16 {
    (((v + 25) / 50) * 50).clamp(0, 65535) as u16
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_HSCROLL => {
            if let Some(st) = state_mut(hwnd) {
                let pos = SendMessageW(st.tb, TBM_GETPOS, 0, 0) as i32;
                let dpi = snap50(pos).clamp(st.dpi_min, st.dpi_max);
                let txt = to_wide(&format!("DPI: {dpi}"));
                SetWindowTextW(st.lbl_dpi, txt.as_ptr());
                // Fire the device change only on discrete steps / release, not
                // during continuous thumb-drag.
                let code = loword(wparam) as u32;
                let fire = matches!(
                    code,
                    TB_ENDTRACK | TB_THUMBPOSITION | TB_LINEUP | TB_LINEDOWN | TB_PAGEUP
                        | TB_PAGEDOWN | TB_TOP | TB_BOTTOM
                );
                if fire {
                    (st.on_event)(SettingsEvent::Dpi(dpi));
                }
            }
            0
        }
        WM_COMMAND => {
            let id = loword(wparam);
            let notify = hiword(wparam);
            if let Some(st) = state_mut(hwnd) {
                match (id, notify) {
                    (ID_CB_UP, CBN_SELCHANGE) => {
                        let i = SendMessageW(lparam as HWND, CB_GETCURSEL, 0, 0);
                        if i >= 0 {
                            (st.on_event)(SettingsEvent::UpAction(i as usize));
                        }
                    }
                    (ID_CB_DOWN, CBN_SELCHANGE) => {
                        let i = SendMessageW(lparam as HWND, CB_GETCURSEL, 0, 0);
                        if i >= 0 {
                            (st.on_event)(SettingsEvent::DownAction(i as usize));
                        }
                    }
                    (ID_CB_THUMB_BACK, CBN_SELCHANGE) => {
                        let i = SendMessageW(lparam as HWND, CB_GETCURSEL, 0, 0);
                        if i >= 0 {
                            (st.on_event)(SettingsEvent::ThumbBack(i as usize));
                        }
                    }
                    (ID_CB_THUMB_FWD, CBN_SELCHANGE) => {
                        let i = SendMessageW(lparam as HWND, CB_GETCURSEL, 0, 0);
                        if i >= 0 {
                            (st.on_event)(SettingsEvent::ThumbForward(i as usize));
                        }
                    }
                    (ID_CB_EFFECT, CBN_SELCHANGE) => {
                        let i = SendMessageW(lparam as HWND, CB_GETCURSEL, 0, 0);
                        if i >= 0 {
                            (st.on_event)(SettingsEvent::Effect(i as usize));
                        }
                    }
                    (ID_CB_POLL, CBN_SELCHANGE) => {
                        let i = SendMessageW(lparam as HWND, CB_GETCURSEL, 0, 0);
                        if i >= 0 {
                            (st.on_event)(SettingsEvent::Polling(i as usize));
                        }
                    }
                    (ID_BTN_COLOR, BN_CLICKED) => {
                        if let Some(c) = choose_color(hwnd, st.color) {
                            st.color = c;
                            // Rebuild the swatch brush.
                            DeleteObject(st.swatch_brush);
                            st.swatch_brush = CreateSolidBrush(colorref(c));
                            use windows_sys::Win32::Graphics::Gdi::InvalidateRect;
                            InvalidateRect(st.swatch, std::ptr::null(), 1);
                            (st.on_event)(SettingsEvent::Color(c.0, c.1, c.2));
                        }
                    }
                    (ID_BTN_APPLY, BN_CLICKED) => (st.on_event)(SettingsEvent::Apply),
                    (ID_BTN_SAVE, BN_CLICKED) => (st.on_event)(SettingsEvent::Save),
                    (ID_BTN_CLOSE, BN_CLICKED) => {
                        DestroyWindow(hwnd);
                    }
                    _ => {}
                }
            }
            0
        }
        WM_CTLCOLORSTATIC => {
            // Paint the swatch static with the current color.
            if let Some(st) = state_mut(hwnd) {
                use windows_sys::Win32::UI::WindowsAndMessaging::GetDlgCtrlID;
                if GetDlgCtrlID(lparam as HWND) == ID_SWATCH as i32 {
                    let hdc = wparam as windows_sys::Win32::Graphics::Gdi::HDC;
                    SetBkMode(hdc, OPAQUE as i32);
                    use windows_sys::Win32::Graphics::Gdi::SetBkColor;
                    SetBkColor(hdc, colorref(st.color));
                    return st.swatch_brush as LRESULT;
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GetWindowLongPtrW, PostQuitMessage, GWLP_USERDATA,
            };
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if !ptr.is_null() {
                let st = Box::from_raw(ptr);
                DeleteObject(st.swatch_brush);
                drop(st);
            }
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Show the native ChooseColor dialog; returns the picked (r,g,b) or None.
unsafe fn choose_color(owner: HWND, initial: (u8, u8, u8)) -> Option<(u8, u8, u8)> {
    let mut custom = [0x00FF_FFFFu32; 16];
    let mut cc: CHOOSECOLORW = std::mem::zeroed();
    cc.lStructSize = std::mem::size_of::<CHOOSECOLORW>() as u32;
    cc.hwndOwner = owner;
    cc.rgbResult = colorref(initial);
    cc.lpCustColors = custom.as_mut_ptr();
    cc.Flags = CC_RGBINIT | CC_FULLOPEN;
    if ChooseColorW(&mut cc) != 0 {
        let v = cc.rgbResult;
        Some(((v & 0xFF) as u8, ((v >> 8) & 0xFF) as u8, ((v >> 16) & 0xFF) as u8))
    } else {
        None
    }
}
