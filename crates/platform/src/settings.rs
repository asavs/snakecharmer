//! Native Win32 settings window (no GUI framework — `windows-sys` common
//! controls only, to protect the footprint gate).
//!
//! The window is generic: the app supplies current values + label lists via
//! [`SettingsInit`] and a callback; the window emits [`SettingsEvent`]s (with
//! indices/values) on live changes and on Apply/Save. Mapping indices back to
//! action strings / effects stays in the app, so this module knows nothing
//! about Razer specifics. Capability gating is generic too: optional groups
//! ([`SettingsInit::dpi_buttons`], [`SettingsInit::lighting`]) simply don't
//! create their controls when `None`, and the layout reflows so no gaps
//! remain.
//!
//! Layout (a portrait window, width < height): centered rows of device-wide
//! knobs at the top — a modest-width DPI slider row, then polling and (when
//! present) lighting rows — the device schematic ([`SettingsInit::diagram`],
//! drawn anti-aliased with GDI+, see [`crate::diagram`]) as the centerpiece
//! in the middle with the per-button dropdowns mounted *on* it at its
//! callout slots, and the apply/save hint plus Apply/Save/Close centered at
//! the bottom.
//!
//! Look and feel: comctl32 v6 visual styles (manifest embedded by the build
//! scripts), Segoe UI 9 pt, and a dark titlebar when the OS theme is dark.
//! [`open`] runs a private message loop and returns when the window closes
//! (call it on a dedicated thread).

use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, EndPaint, GetStockObject,
    GetSysColor, SetBkMode, CLEARTYPE_QUALITY, COLOR_BTNTEXT, COLOR_GRAYTEXT, DEFAULT_GUI_FONT,
    FW_NORMAL, HBRUSH, HFONT, HGDIOBJ, OPAQUE, PAINTSTRUCT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::UI::Controls::Dialogs::{ChooseColorW, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW};
use windows_sys::Win32::UI::Controls::{
    InitCommonControlsEx, ICC_BAR_CLASSES, INITCOMMONCONTROLSEX, TBM_SETPOS, TBM_SETRANGE,
    TB_BOTTOM, TB_ENDTRACK, TB_LINEDOWN, TB_LINEUP, TB_PAGEDOWN, TB_PAGEUP, TB_THUMBPOSITION,
    TB_TOP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRect, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, LoadCursorW, RegisterClassW, SendMessageW, SetWindowTextW, ShowWindow,
    TranslateMessage, CBS_DROPDOWNLIST, CB_ADDSTRING, CB_GETCURSEL, CB_SETCURSEL, CW_USEDEFAULT,
    IDC_ARROW, MSG, SW_SHOW, WM_CLOSE, WM_COMMAND, WM_CTLCOLORSTATIC, WM_DESTROY, WM_HSCROLL,
    WM_PAINT, WM_SETFONT, WNDCLASSW, WS_CHILD, WS_OVERLAPPED, WS_CAPTION, WS_SYSMENU, WS_TABSTOP,
    WS_VISIBLE,
};

use crate::diagram::{self, CalloutSlot, Diagram, Palette};

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
const ID_BTN_DPI_MINUS: u16 = 115;
const ID_BTN_DPI_PLUS: u16 = 116;
const ID_SWATCH: u16 = 106;
const ID_BTN_COLOR: u16 = 107;
const ID_BTN_APPLY: u16 = 108;
const ID_BTN_SAVE: u16 = 109;
const ID_BTN_CLOSE: u16 = 110;

// Layout metrics (px at 96 dpi; the running cursor stacks controls with these).
const MARGIN: i32 = 16;
/// Width of a fallback labeled-combo row (diagrams without callout slots).
const COL_W: i32 = 340;
/// Widest the diagram pane may grow; larger diagrams scale down to fit.
const PANE_MAX_W: i32 = 960;
/// Height of a one-line STATIC label.
const LBL_H: i32 = 18;
/// Gap between a label and the control it captions.
const LBL_GAP: i32 = 2;
/// Visible (closed) height of a combo box.
const COMBO_H: i32 = 24;
/// Total combo height at creation time = closed height + dropped list height.
const COMBO_DROP: i32 = 200;
/// Vertical gap after each control group.
const GROUP_GAP: i32 = 12;
/// Top-cluster row 1 (the DPI slider row) height.
const ROW1_H: i32 = 30;
/// Top-cluster dropdown row (polling / lighting) height.
const ROW2_H: i32 = 26;
/// Bottom button row (Apply/Save/Close) height.
const BOTTOM_H: i32 = 28;
/// Width of the DPI trackbar itself (its row is centered, never full-width).
const SLIDER_W: i32 = 240;
/// Minimum portrait aspect: height ≥ width × this. The diagram data is
/// intrinsically wide (a caption column and a combo column flank the mouse),
/// so a full 1:1.618 would be mostly void; this keeps the window clearly
/// portrait while the slack breathes around the diagram.
const PORTRAIT_MIN: f32 = 1.2;
/// Minimum client width (the centered rows must fit without a diagram).
const MIN_CLIENT_W: i32 = 430;

/// One remappable-button dropdown: its list entries and the selected index.
/// Convention: index 0 is the button's *identity/default* entry (e.g.
/// "← Back (default)"), so the dropdown names its own button and no separate
/// caption is needed; the app maps indices back to action values.
pub struct ActionCombo {
    pub labels: Vec<String>,
    pub index: usize,
}

/// Initial values for the two remappable extra-button dropdowns (e.g. the
/// wheel DPI buttons). `None` in [`SettingsInit`] = the device has no such
/// buttons and the dropdowns are not created.
pub struct DpiButtonsInit {
    pub up: ActionCombo,
    pub down: ActionCombo,
}

/// Initial values for the lighting controls (effect dropdown + color swatch +
/// picker). `None` in [`SettingsInit`] = no lighting hardware, no controls.
pub struct LightingInit {
    pub effect_labels: Vec<String>,
    pub effect_index: usize,
    pub color: (u8, u8, u8),
}

/// Initial values to populate the window.
pub struct SettingsInit {
    /// Shown in the title bar: `Snakecharmer Settings — <device_name>`
    /// (or just `Snakecharmer Settings` when empty).
    pub device_name: String,
    /// Device schematic drawn anti-aliased between the top and bottom bars
    /// (see [`crate::diagram`]); `None` = no pane.
    pub diagram: Option<Diagram>,
    pub dpi: u16,
    pub dpi_min: u16,
    pub dpi_max: u16,
    pub polling_labels: Vec<String>,
    pub polling_index: usize,
    pub dpi_buttons: Option<DpiButtonsInit>,
    pub thumb_back: ActionCombo,
    pub thumb_forward: ActionCombo,
    pub lighting: Option<LightingInit>,
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

/// Everything WM_PAINT needs to draw the diagram pane.
struct DiagramPane {
    diagram: Diagram,
    origin: (i32, i32),
    scale: f32,
    palette: Palette,
}

struct WindowState {
    on_event: Box<dyn Fn(SettingsEvent)>,
    tb: HWND,
    lbl_dpi: HWND,
    /// Null when the device has no lighting (no swatch control exists).
    swatch: HWND,
    color: (u8, u8, u8),
    swatch_brush: HBRUSH,
    /// Owned UI font (Segoe UI 9 pt) shared by all controls.
    ui_font: HFONT,
    pane: Option<DiagramPane>,
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

/// Whether Windows itself is set to dark mode (the "apps" theme toggle).
///
/// SAFETY: plain registry read with NUL-terminated strings and a sized buffer.
unsafe fn os_dark_mode() -> bool {
    let sub = to_wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let val = to_wide("AppsUseLightTheme");
    let mut data: u32 = 1;
    let mut size: u32 = 4;
    let ok = RegGetValueW(
        HKEY_CURRENT_USER,
        sub.as_ptr(),
        val.as_ptr(),
        RRF_RT_REG_DWORD,
        std::ptr::null_mut(),
        &mut data as *mut u32 as _,
        &mut size,
    );
    ok == 0 && data == 0
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
        // GDI+ backs the diagram pane; started per window, stopped after the loop.
        let gdiplus = diagram::startup();

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

        // --- Measure the diagram (if any). It renders 1:1 when it fits
        // (scaled down past PANE_MAX_W); the window is then sized around it
        // toward a golden-rectangle aspect.
        let mut dia_w = 0i32;
        let mut dia_h = 0i32;
        // (scale, palette, bounds-min): bounds-min is the design point that
        // lands at the pane origin — callout combos are placed through the
        // exact same mapping the painter uses.
        let mut pane_meta: Option<(f32, Palette, (i32, i32))> = None;
        if let Some(d) = &init.diagram {
            let (x0, y0, x1, y1) = diagram::measure(d);
            let (w, h) = (x1 - x0, y1 - y0);
            if w > 0 && h > 0 {
                let scale = (PANE_MAX_W as f32 / w as f32).min(1.0);
                dia_w = (w as f32 * scale).ceil() as i32;
                dia_h = (h as f32 * scale).ceil() as i32;
                let palette = Palette {
                    fg: GetSysColor(COLOR_BTNTEXT),
                    dim: GetSysColor(COLOR_GRAYTEXT),
                    accent_a: colorref((0x2D, 0xA4, 0x4E)), // lighting zones (green)
                    accent_b: colorref((0x2F, 0x6F, 0xD0)), // remappable buttons (blue)
                };
                pane_meta = Some((scale, palette, (x0, y0)));
            }
        }
        let has_diagram = pane_meta.is_some();

        // Callout-hosted dropdowns: when the diagram carries anchor slots,
        // the DPI-button / thumb-button combos live *on the diagram* — each
        // dropdown's index-0 entry names its own button (ActionCombo's
        // convention), so no fact is stated twice. Diagrams without the
        // slots (or no diagram) fall back to labeled rows under the top bar.
        let callouts: Vec<(CalloutSlot, (i32, i32, i32, i32))> = init
            .diagram
            .as_ref()
            .filter(|_| has_diagram)
            .map(diagram::callout_combo_rects)
            .unwrap_or_default();
        let slot_rect =
            |slot: CalloutSlot| callouts.iter().find(|(s, _)| *s == slot).map(|(_, r)| *r);
        let dpi_in_diagram = init.dpi_buttons.is_some()
            && slot_rect(CalloutSlot::DpiUp).is_some()
            && slot_rect(CalloutSlot::DpiDown).is_some();
        let thumb_in_diagram = slot_rect(CalloutSlot::ThumbBack).is_some()
            && slot_rect(CalloutSlot::ThumbForward).is_some();

        // --- Vertical plan (portrait): centered top cluster (slider row,
        // polling row, lighting row, any fallback rows), diagram in the
        // middle, bottom bar (hint + buttons). The width comes from the
        // diagram; any stretch toward PORTRAIT_MIN opens up as breathing
        // room around the diagram.
        let fb_row = LBL_H + LBL_GAP + COMBO_H + GROUP_GAP;
        let mut fb_h = 0i32;
        if init.dpi_buttons.is_some() && !dpi_in_diagram {
            fb_h += 2 * fb_row; // up + down fallback rows
        }
        if !thumb_in_diagram {
            fb_h += 2 * fb_row + 4 + 16; // thumb rows + hook note tucked under
        }
        if fb_h > 0 {
            fb_h += GROUP_GAP;
        }
        let mut top_h = ROW1_H + GROUP_GAP + ROW2_H; // DPI slider + polling
        if init.lighting.is_some() {
            top_h += GROUP_GAP + ROW2_H; // lighting row
        }
        top_h += fb_h;
        let bottom_h = 16 + 6 + BOTTOM_H; // hint line above the button row
        let natural_h = MARGIN
            + top_h
            + GROUP_GAP
            + if has_diagram { dia_h + GROUP_GAP } else { 0 }
            + bottom_h
            + MARGIN;
        let client_w = (dia_w + 2 * MARGIN).max(MIN_CLIENT_W);
        let client_h = natural_h.max((client_w as f32 * PORTRAIT_MIN).round() as i32);
        // Portrait slack: half above the diagram (below the top cluster),
        // half below it (above the bottom bar) — diagram stays the centered
        // centerpiece, the clusters stay pinned to their edges.
        let slack = client_h - natural_h;

        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU;
        let mut rect = RECT { left: 0, top: 0, right: client_w, bottom: client_h };
        AdjustWindowRect(&mut rect, style, 0);

        let title = if init.device_name.is_empty() {
            "Snakecharmer Settings".to_string()
        } else {
            format!("Snakecharmer Settings — {}", init.device_name)
        };
        let title_w = to_wide(&title);
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            title_w.as_ptr(),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            rect.right - rect.left,
            rect.bottom - rect.top,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            diagram::shutdown(gdiplus);
            return;
        }

        // Dark titlebar when the OS theme is dark (silently a no-op on builds
        // without the attribute; the client area keeps native system colors).
        if os_dark_mode() {
            let dark: i32 = 1;
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
                &dark as *const i32 as _,
                std::mem::size_of::<i32>() as u32,
            );
        }

        // Segoe UI 9 pt for every control (falls back to the stock GUI font
        // if creation fails; DEFAULT_GUI_FONT is the legacy bitmap-era face).
        let face = to_wide("Segoe UI");
        let ui_font: HFONT = CreateFontW(
            -12, // 9 pt at 96 dpi
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            1, // DEFAULT_CHARSET
            0,
            0,
            CLEARTYPE_QUALITY as u32,
            0, // DEFAULT_PITCH | FF_DONTCARE
            face.as_ptr(),
        );
        let font: HGDIOBJ =
            if ui_font.is_null() { GetStockObject(DEFAULT_GUI_FONT) } else { ui_font as _ };
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

        // Diagram pane: drawn anti-aliased in WM_PAINT (no child control);
        // centered horizontally, and vertically within the portrait slack.
        let pane_origin = ((client_w - dia_w) / 2, MARGIN + top_h + GROUP_GAP + slack / 2);
        let pane = pane_meta.map(|(scale, palette, _)| DiagramPane {
            diagram: init.diagram.clone().expect("pane_meta implies a diagram"),
            origin: pane_origin,
            scale,
            palette,
        });

        // --- Top cluster, every row centered. Creation order = tab order:
        // DPI slider row, polling, lighting, per-button dropdowns, then
        // Apply/Save/Close.
        let mut y = MARGIN;
        let combo_style = (CBS_DROPDOWNLIST as u32) | WS_TABSTOP;

        // Row 1: value label, − stepper, a modest-width trackbar, + stepper
        // (steppers step by 50, same event path as the slider; arrow keys
        // still work when it has focus). Never full-window width.
        let row1_w = 84 + 4 + 26 + 6 + SLIDER_W + 6 + 26;
        let mut x = (client_w - row1_w) / 2;
        let lbl_dpi =
            mk("STATIC", &format!("DPI: {}", init.dpi), 0, ID_LBL_DPI, x, y + 7, 84, LBL_H);
        x += 84 + 4;
        let _ = mk("BUTTON", "\u{2212}", BS_PUSHBUTTON | WS_TABSTOP, ID_BTN_DPI_MINUS, x, y + 2, 26, 26);
        x += 26 + 6;
        let tb = mk("msctls_trackbar32", "", WS_TABSTOP, ID_TB_DPI, x, y, SLIDER_W, ROW1_H);
        x += SLIDER_W + 6;
        let _ = mk("BUTTON", "+", BS_PUSHBUTTON | WS_TABSTOP, ID_BTN_DPI_PLUS, x, y + 2, 26, 26);
        y += ROW1_H + GROUP_GAP;
        SendMessageW(
            tb,
            TBM_SETRANGE,
            1,
            ((init.dpi_min as i32) | ((init.dpi_max as i32) << 16)) as LPARAM,
        );
        SendMessageW(tb, TBM_SETPOS, 1, init.dpi as LPARAM);

        // Helper: a pre-populated dropdown at an arbitrary position.
        let make_combo = |id: u16, x: i32, cy: i32, w: i32, items: &[String], sel: usize| -> HWND {
            let cb = mk("COMBOBOX", "", combo_style, id, x, cy, w, COMBO_H + COMBO_DROP);
            for item in items {
                let wtxt = to_wide(item);
                SendMessageW(cb, CB_ADDSTRING, 0, wtxt.as_ptr() as LPARAM);
            }
            SendMessageW(cb, CB_SETCURSEL, sel, 0);
            cb
        };

        // Polling row, centered (lighting gets its own centered row below —
        // both are device-wide like DPI, so they live in the top cluster;
        // the diagram hosts only the per-button controls).
        let poll_w = 208 + 4 + 110;
        x = (client_w - poll_w) / 2;
        let _ = mk("STATIC", "Polling rate (\"keep\" = leave as set):", 0, 0, x, y + 5, 208, LBL_H);
        let _ = make_combo(ID_CB_POLL, x + 212, y + 1, 110, &init.polling_labels, init.polling_index);
        y += ROW2_H;
        let mut swatch: HWND = std::ptr::null_mut();
        let mut color = (0u8, 0u8, 0u8);
        if let Some(l) = &init.lighting {
            color = l.color;
            y += GROUP_GAP;
            let light_w = 52 + 4 + 110 + 16 + 38 + 2 + 60 + 6 + 90;
            x = (client_w - light_w) / 2;
            let _ = mk("STATIC", "Lighting:", 0, 0, x, y + 5, 52, LBL_H);
            x += 52 + 4;
            let _ = make_combo(ID_CB_EFFECT, x, y + 1, 110, &l.effect_labels, l.effect_index);
            x += 110 + 16;
            let _ = mk("STATIC", "Color:", 0, 0, x, y + 5, 38, LBL_H);
            x += 38 + 2;
            swatch = mk("STATIC", "", SS_CENTER, ID_SWATCH, x, y + 2, 60, 22);
            x += 60 + 6;
            let _ = mk("BUTTON", "Choose...", BS_PUSHBUTTON | WS_TABSTOP, ID_BTN_COLOR, x, y, 90, ROW2_H);
            y += ROW2_H;
        }

        // Fallback rows, only when the diagram doesn't host the per-button
        // dropdowns (no diagram, or no callout slots in it): centered
        // labeled rows.
        let fx = (client_w - COL_W) / 2;
        let labeled_combo = |label: &str, id: u16, items: &[String], sel: usize, y: &mut i32| -> HWND {
            let _ = mk("STATIC", label, 0, 0, fx, *y, COL_W, LBL_H);
            *y += LBL_H + LBL_GAP;
            let cb = make_combo(id, fx, *y, COL_W, items, sel);
            *y += COMBO_H + GROUP_GAP;
            cb
        };
        if fb_h > 0 {
            y += GROUP_GAP;
            if !dpi_in_diagram {
                if let Some(b) = &init.dpi_buttons {
                    let _ = labeled_combo(
                        "Front DPI button — nearer the scroll wheel:",
                        ID_CB_UP,
                        &b.up.labels,
                        b.up.index,
                        &mut y,
                    );
                    let _ = labeled_combo(
                        "Rear DPI button:",
                        ID_CB_DOWN,
                        &b.down.labels,
                        b.down.index,
                        &mut y,
                    );
                }
            }
            if !thumb_in_diagram {
                let _ = labeled_combo(
                    "Back thumb button:",
                    ID_CB_THUMB_BACK,
                    &init.thumb_back.labels,
                    init.thumb_back.index,
                    &mut y,
                );
                let _ = labeled_combo(
                    "Forward thumb button:",
                    ID_CB_THUMB_FWD,
                    &init.thumb_forward.labels,
                    init.thumb_forward.index,
                    &mut y,
                );
                y += -GROUP_GAP + 4; // tuck the note right under its combos
                // (With a diagram, this cost note is part of the schematic,
                // right beside the thumb dropdowns — stated exactly once.)
                let _ = mk(
                    "STATIC",
                    "Remaps use a global hook (~\u{00B5}s per mouse event); the default entry = no hook.",
                    SS_CENTER,
                    0,
                    MARGIN,
                    y,
                    client_w - 2 * MARGIN,
                    16,
                );
                y += 16;
            }
        }
        let _ = y; // cursor now == MARGIN + top_h; the pane starts below it

        // In-diagram dropdowns, mounted in the callouts' reserved rects and
        // mapped through the painter's origin/scale. Each dropdown's index-0
        // entry is its button's identity, so it carries no caption at all —
        // the diagram's leader line runs from the button to the dropdown.
        if let Some((scale, _, (bx0, by0))) = pane_meta {
            let place = |id: u16, slot: CalloutSlot, c: &ActionCombo| {
                if let Some((cx, cy, cw, _)) = slot_rect(slot) {
                    let px = pane_origin.0 + (((cx - bx0) as f32) * scale).round() as i32;
                    let py = pane_origin.1 + (((cy - by0) as f32) * scale).round() as i32;
                    let pw = ((cw as f32) * scale).round() as i32;
                    let _ = make_combo(id, px, py, pw, &c.labels, c.index);
                }
            };
            if dpi_in_diagram {
                if let Some(b) = &init.dpi_buttons {
                    place(ID_CB_UP, CalloutSlot::DpiUp, &b.up);
                    place(ID_CB_DOWN, CalloutSlot::DpiDown, &b.down);
                }
            }
            if thumb_in_diagram {
                place(ID_CB_THUMB_BACK, CalloutSlot::ThumbBack, &init.thumb_back);
                place(ID_CB_THUMB_FWD, CalloutSlot::ThumbForward, &init.thumb_forward);
            }
        }

        // --- Bottom bar, centered: the apply/save hint line directly above
        // the Apply (re-push config) / Save / Close row.
        let by = client_h - MARGIN - BOTTOM_H;
        let _ = mk(
            "STATIC",
            "Changes apply immediately; Save writes config.toml.",
            SS_CENTER,
            0,
            MARGIN,
            by - 6 - 16,
            client_w - 2 * MARGIN,
            16,
        );
        let btn_row_w = 3 * 90 + 2 * 8;
        let bx = (client_w - btn_row_w) / 2;
        let _ = mk("BUTTON", "Apply", BS_PUSHBUTTON | WS_TABSTOP, ID_BTN_APPLY, bx, by, 90, BOTTOM_H);
        let _ = mk("BUTTON", "Save", BS_PUSHBUTTON | WS_TABSTOP, ID_BTN_SAVE, bx + 98, by, 90, BOTTOM_H);
        let _ = mk("BUTTON", "Close", BS_PUSHBUTTON | WS_TABSTOP, ID_BTN_CLOSE, bx + 196, by, 90, BOTTOM_H);

        let state = Box::new(WindowState {
            on_event: Box::new(on_event),
            tb,
            lbl_dpi,
            swatch,
            color,
            swatch_brush: if swatch.is_null() {
                std::ptr::null_mut()
            } else {
                CreateSolidBrush(colorref(color))
            },
            ui_font,
            pane,
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
        diagram::shutdown(gdiplus);
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
        WM_PAINT => {
            if let Some(st) = state_mut(hwnd) {
                if let Some(p) = &st.pane {
                    let mut ps: PAINTSTRUCT = std::mem::zeroed();
                    let hdc = BeginPaint(hwnd, &mut ps);
                    diagram::render(hdc, &p.diagram, p.origin, p.scale, &p.palette);
                    EndPaint(hwnd, &ps);
                    return 0;
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
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
                    (ID_BTN_DPI_MINUS | ID_BTN_DPI_PLUS, BN_CLICKED) => {
                        // Step the slider by one snap increment and fire the
                        // same live-DPI path as a slider move.
                        let cur = snap50(SendMessageW(st.tb, TBM_GETPOS, 0, 0) as i32) as i32;
                        let delta = if id == ID_BTN_DPI_PLUS { 50 } else { -50 };
                        let dpi =
                            (cur + delta).clamp(st.dpi_min as i32, st.dpi_max as i32) as u16;
                        SendMessageW(st.tb, TBM_SETPOS, 1, dpi as LPARAM);
                        let txt = to_wide(&format!("DPI: {dpi}"));
                        SetWindowTextW(st.lbl_dpi, txt.as_ptr());
                        (st.on_event)(SettingsEvent::Dpi(dpi));
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
                if !st.swatch.is_null() && GetDlgCtrlID(lparam as HWND) == ID_SWATCH as i32 {
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
                if !st.swatch_brush.is_null() {
                    DeleteObject(st.swatch_brush);
                }
                if !st.ui_font.is_null() {
                    DeleteObject(st.ui_font as _);
                }
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
