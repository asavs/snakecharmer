//! Generic 2-D schematic rendering for the settings window: anti-aliased
//! vector shapes via the GDI+ flat API (`gdiplus.dll`, part of Windows —
//! exposed through the already-pinned `windows-sys`, so no new dependency),
//! text via classic GDI so captions get native ClearType.
//!
//! This module knows nothing about mice or Razer: it draws [`Shape`]s. The
//! app maps its domain diagram data (e.g. `razer_proto::diagram`) into these
//! types. Roles carry the entire visual vocabulary, so diagrams stay data.

use windows_sys::Win32::Foundation::SIZE;
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontW, DeleteObject, GetDC, GetTextExtentPoint32W, ReleaseDC, SelectObject, SetBkMode,
    SetTextAlign, SetTextColor, TextOutW, CLEARTYPE_QUALITY, FW_NORMAL, HDC, HFONT, TA_BASELINE,
    TA_CENTER, TA_LEFT, TA_RIGHT, TRANSPARENT,
};
use windows_sys::Win32::Graphics::GdiPlus::{
    GdipAddPathArc, GdipAddPathBezier, GdipClosePathFigure, GdipCreateFromHDC,
    GdipCreatePath, GdipCreatePen1, GdipDeleteGraphics, GdipDeletePath, GdipDeletePen,
    GdipDrawEllipse, GdipDrawLines, GdipDrawPath, GdipSetSmoothingMode, GdiplusShutdown,
    GdiplusStartup, FillModeAlternate, GdiplusStartupInput, GpPath, GpPen, PointF,
    SmoothingModeAntiAlias, UnitPixel,
};

/// Styling slot for an element; colors and weights come from the [`Palette`]
/// and per-role constants, never from the diagram data itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Primary outline. Strong stroke in the foreground color.
    Body,
    /// Secondary line work. Thinner foreground stroke.
    Detail,
    /// Leader line to a caption. Thin, semi-transparent.
    Lead,
    /// Main caption text (foreground).
    Label,
    /// Small dim text.
    Note,
    /// First accent (shapes stroked / text filled in `Palette::accent_a`).
    AccentA,
    /// Second accent (`Palette::accent_b`).
    AccentB,
}

impl Role {
    fn font_size(self) -> i32 {
        match self {
            Role::Label => 13,
            _ => 11,
        }
    }
    fn stroke_width(self) -> f32 {
        match self {
            Role::Body | Role::AccentA | Role::AccentB => 1.6,
            Role::Detail => 1.1,
            _ => 1.0,
        }
    }
}

/// Horizontal anchoring of a [`Shape::Text`] relative to its position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

/// One cubic-bezier segment: (control 1, control 2, end point).
pub type Curve = ((i32, i32), (i32, i32), (i32, i32));

/// Which app-level control a [`Shape::Callout`] anchors. The window mounts
/// the matching dropdown at the callout's position (see
/// [`callout_combo_rects`]); this module only reserves the space and draws
/// the caption — what the slots mean stays the app's business.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CalloutSlot {
    DpiUp,
    DpiDown,
    ThumbBack,
    ThumbForward,
    /// The scroll wheel / middle click (a disabled, identity-only dropdown —
    /// middle-click remap is scaffolded, not yet implemented).
    Wheel,
}

/// Width of the dropdown a callout reserves space for (design units). Wide
/// enough for the longest self-identifying entry ("Middle click (default)")
/// without clipping.
pub const CALLOUT_COMBO_W: i32 = 145;
/// Closed height of the dropdown a callout reserves space for.
pub const CALLOUT_COMBO_H: i32 = 24;
/// Vertical offset from a captioned callout's label baseline to its
/// dropdown's top (the dropdown stacks beneath the two caption lines).
const CALLOUT_COMBO_DY: i32 = 24;
/// Caption-less callouts (empty `label` and `note`) mount the dropdown *as*
/// the callout: vertically centered on the caption block's midpoint (label
/// baseline + 8), so the diagram's leader lines meet the dropdown itself.
const CALLOUT_COMBO_DY_BARE: i32 = 8 - CALLOUT_COMBO_H / 2;

/// A drawing primitive in integer design units (1 unit = 1 px before scaling).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    Path { role: Role, start: (i32, i32), curves: Vec<Curve>, closed: bool },
    RoundRect { role: Role, x: i32, y: i32, w: i32, h: i32, r: i32 },
    Circle { role: Role, cx: i32, cy: i32, r: i32 },
    Polyline { role: Role, points: Vec<(i32, i32)> },
    /// One line of text; `at` is the baseline point.
    Text { role: Role, at: (i32, i32), anchor: Anchor, text: String },
    /// A caption block (label + note) that also anchors an interactive
    /// control: the renderer draws the two caption lines, [`measure`]
    /// reserves room for a dropdown beneath them, and the window places the
    /// slot's real combo box in that reserved rect. When both `label` and
    /// `note` are empty, no caption is drawn and the dropdown mounts centered
    /// where the caption block would be — the control identifies itself (e.g.
    /// via its list entries), so no fact is stated twice.
    Callout {
        slot: CalloutSlot,
        at: (i32, i32),
        anchor: Anchor,
        label: String,
        note: String,
        note_role: Role,
    },
}

/// The rect (design units: x, y, w, h) a callout reserves for its dropdown.
/// `has_caption` selects the stacked (below the caption lines) or bare
/// (centered on the caption block) mount — see the two `CALLOUT_COMBO_DY*`.
fn callout_combo_rect(at: (i32, i32), anchor: Anchor, has_caption: bool) -> (i32, i32, i32, i32) {
    let x = match anchor {
        Anchor::Start => at.0,
        Anchor::Middle => at.0 - CALLOUT_COMBO_W / 2,
        Anchor::End => at.0 - CALLOUT_COMBO_W,
    };
    let dy = if has_caption { CALLOUT_COMBO_DY } else { CALLOUT_COMBO_DY_BARE };
    (x, at.1 + dy, CALLOUT_COMBO_W, CALLOUT_COMBO_H)
}

/// Whether a callout draws any caption text at all.
fn callout_has_caption(label: &str, note: &str) -> bool {
    !label.is_empty() || !note.is_empty()
}

/// Every callout's slot and reserved dropdown rect (design units), in shape
/// order. The window maps these through the same origin/scale as [`render`].
pub fn callout_combo_rects(diagram: &Diagram) -> Vec<(CalloutSlot, (i32, i32, i32, i32))> {
    diagram
        .shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Callout { slot, at, anchor, label, note, .. } => {
                Some((*slot, callout_combo_rect(*at, *anchor, callout_has_caption(label, note))))
            }
            _ => None,
        })
        .collect()
}

/// A schematic: shapes in paint order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagram {
    pub shapes: Vec<Shape>,
}

/// Colors the renderer draws with (COLORREF, 0x00BBGGRR). Pick them from the
/// window palette so the diagram themes with the surface it sits on.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Outlines and main captions.
    pub fg: u32,
    /// Dim text and leader lines.
    pub dim: u32,
    /// First accent (e.g. lighting zones).
    pub accent_a: u32,
    /// Second accent (e.g. remappable buttons).
    pub accent_b: u32,
}

fn colorref_to_argb(c: u32, alpha: u8) -> u32 {
    let (r, g, b) = (c & 0xFF, (c >> 8) & 0xFF, (c >> 16) & 0xFF);
    ((alpha as u32) << 24) | (r << 16) | (g << 8) | b
}

/// (stroke ARGB, text COLORREF) for a role under a palette.
fn role_colors(role: Role, p: &Palette) -> (u32, u32) {
    match role {
        Role::Body | Role::Detail | Role::Label => (colorref_to_argb(p.fg, 0xFF), p.fg),
        Role::Lead => (colorref_to_argb(p.dim, 0x8C), p.dim),
        Role::Note => (colorref_to_argb(p.dim, 0xFF), p.dim),
        Role::AccentA => (colorref_to_argb(p.accent_a, 0xFF), p.accent_a),
        Role::AccentB => (colorref_to_argb(p.accent_b, 0xFF), p.accent_b),
    }
}

fn to_wide_no_nul(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Create a Segoe UI font at `px` pixels (caller frees with `DeleteObject`).
///
/// SAFETY: plain `CreateFontW` with a NUL-terminated face name.
unsafe fn make_font(px: i32) -> HFONT {
    let face: Vec<u16> = "Segoe UI".encode_utf16().chain(std::iter::once(0)).collect();
    CreateFontW(
        -px,
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
    )
}

/// Draw one baseline-anchored ClearType text run.
///
/// # Safety
/// `hdc` must be a valid device context and `font` a valid font handle.
unsafe fn draw_gdi_text(hdc: HDC, font: HFONT, color: u32, x: i32, y: i32, anchor: Anchor, text: &str) {
    let old = SelectObject(hdc, font as _);
    SetTextColor(hdc, color);
    let align = match anchor {
        Anchor::Start => TA_LEFT,
        Anchor::Middle => TA_CENTER,
        Anchor::End => TA_RIGHT,
    };
    SetTextAlign(hdc, TA_BASELINE | align);
    let w = to_wide_no_nul(text);
    TextOutW(hdc, x, y, w.as_ptr(), w.len() as i32);
    SelectObject(hdc, old);
}

/// Measure a text run's width at design size with the actual UI font.
unsafe fn text_width(hdc: HDC, font: HFONT, text: &str) -> i32 {
    let old = SelectObject(hdc, font as _);
    let w = to_wide_no_nul(text);
    let mut size = SIZE { cx: 0, cy: 0 };
    if !w.is_empty() {
        GetTextExtentPoint32W(hdc, w.as_ptr(), w.len() as i32, &mut size);
    }
    SelectObject(hdc, old);
    size.cx
}

/// Content bounding box `(x0, y0, x1, y1)` in design units, with text runs
/// measured exactly (screen DC + the fonts the renderer will use), so the
/// pane the caller allocates can never clip a caption.
pub fn measure(diagram: &Diagram) -> (i32, i32, i32, i32) {
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    let mut add = |x: i32, y: i32| {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    };
    // SAFETY: screen DC for measuring only; fonts freed before returning.
    unsafe {
        let hdc = GetDC(std::ptr::null_mut());
        let f_label = make_font(Role::Label.font_size());
        let f_note = make_font(Role::Note.font_size());
        for s in &diagram.shapes {
            match s {
                Shape::Path { start, curves, .. } => {
                    add(start.0, start.1);
                    for &((c1x, c1y), (c2x, c2y), (ex, ey)) in curves {
                        add(c1x, c1y);
                        add(c2x, c2y);
                        add(ex, ey);
                    }
                }
                Shape::RoundRect { x, y, w, h, .. } => {
                    add(*x, *y);
                    add(x + w, y + h);
                }
                Shape::Circle { cx, cy, r, .. } => {
                    add(cx - r, cy - r);
                    add(cx + r, cy + r);
                }
                Shape::Polyline { points, .. } => {
                    for &(x, y) in points {
                        add(x, y);
                    }
                }
                Shape::Text { role, at, anchor, text } => {
                    let size = role.font_size();
                    let font = if size >= 13 { f_label } else { f_note };
                    let w = text_width(hdc, font, text);
                    let (tx0, tx1) = match anchor {
                        Anchor::Start => (at.0, at.0 + w),
                        Anchor::Middle => (at.0 - w / 2, at.0 + w - w / 2),
                        Anchor::End => (at.0 - w, at.0),
                    };
                    add(tx0, at.1 - size);
                    add(tx1, at.1 + size / 3);
                }
                Shape::Callout { at, anchor, label, note, note_role, .. } => {
                    // Caption lines (skipped when empty — caption-less
                    // callouts are just their dropdown)...
                    let has_caption = callout_has_caption(label, note);
                    if has_caption {
                        let lw = text_width(hdc, f_label, label);
                        let nfont = if note_role.font_size() >= 13 { f_label } else { f_note };
                        let nw = text_width(hdc, nfont, note);
                        for (w, base, size) in [
                            (lw, at.1, Role::Label.font_size()),
                            (nw, at.1 + 16, note_role.font_size()),
                        ] {
                            let (tx0, tx1) = match anchor {
                                Anchor::Start => (at.0, at.0 + w),
                                Anchor::Middle => (at.0 - w / 2, at.0 + w - w / 2),
                                Anchor::End => (at.0 - w, at.0),
                            };
                            add(tx0, base - size);
                            add(tx1, base + size / 3);
                        }
                    }
                    // ...plus the reserved dropdown rect.
                    let (cx, cy, cw, ch) = callout_combo_rect(*at, *anchor, has_caption);
                    add(cx, cy);
                    add(cx + cw, cy + ch);
                }
            }
        }
        DeleteObject(f_label as _);
        DeleteObject(f_note as _);
        ReleaseDC(std::ptr::null_mut(), hdc);
    }
    if diagram.shapes.is_empty() {
        return (0, 0, 0, 0);
    }
    (x0, y0, x1, y1)
}

/// Horizontal extent `(x0, x1)` in design units of the diagram's primary
/// outline ([`Role::Body`] shapes), or `None` if it has none. The settings
/// window uses this to center the *mouse body* on the window centerline
/// (rather than the full content bounds, which lean toward the wider caption
/// column). Generic: it keys only on the role, no per-device knowledge.
pub fn body_x_bounds(diagram: &Diagram) -> Option<(i32, i32)> {
    let (mut x0, mut x1) = (i32::MAX, i32::MIN);
    let mut add = |x: i32| {
        x0 = x0.min(x);
        x1 = x1.max(x);
    };
    for s in &diagram.shapes {
        match s {
            Shape::Path { role: Role::Body, start, curves, .. } => {
                add(start.0);
                for &((c1x, _), (c2x, _), (ex, _)) in curves {
                    add(c1x);
                    add(c2x);
                    add(ex);
                }
            }
            Shape::RoundRect { role: Role::Body, x, w, .. } => {
                add(*x);
                add(x + w);
            }
            Shape::Circle { role: Role::Body, cx, r, .. } => {
                add(cx - r);
                add(cx + r);
            }
            Shape::Polyline { role: Role::Body, points } => {
                for &(x, _) in points {
                    add(x);
                }
            }
            _ => {}
        }
    }
    (x0 <= x1).then_some((x0, x1))
}

/// Initialize GDI+ for this process; returns the token for [`shutdown`].
///
/// # Safety
/// Standard `GdiplusStartup` call; the returned token must be passed to
/// [`shutdown`] exactly once, after all rendering is done.
pub unsafe fn startup() -> usize {
    let input = GdiplusStartupInput {
        GdiplusVersion: 1,
        DebugEventCallback: 0,
        SuppressBackgroundThread: 0,
        SuppressExternalCodecs: 0,
    };
    let mut token = 0usize;
    GdiplusStartup(&mut token, &input, std::ptr::null_mut());
    token
}

/// Tear down GDI+ for this process.
///
/// # Safety
/// `token` must come from [`startup`] and be passed here exactly once; no
/// GDI+ calls may follow.
pub unsafe fn shutdown(token: usize) {
    GdiplusShutdown(token);
}

/// Draw the diagram onto `hdc`: design coordinates are mapped so that the
/// content bounds' min corner lands at `origin`, scaled by `scale`.
/// Shapes are GDI+ anti-aliased; text is GDI ClearType at the baseline.
///
/// # Safety
/// `hdc` must be a valid device context for the duration of the call, and
/// GDI+ must have been initialized via [`startup`].
pub unsafe fn render(hdc: HDC, diagram: &Diagram, origin: (i32, i32), scale: f32, palette: &Palette) {
    let (min_x, min_y, _, _) = measure(diagram);
    let tx = |x: i32| origin.0 as f32 + (x - min_x) as f32 * scale;
    let ty = |y: i32| origin.1 as f32 + (y - min_y) as f32 * scale;

    let mut graphics = std::ptr::null_mut();
    if GdipCreateFromHDC(hdc, &mut graphics) != 0 {
        return;
    }
    GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);

    // One pen per shape (paints are rare; simplicity over caching).
    let make_pen = |role: Role| -> *mut GpPen {
        let (argb, _) = role_colors(role, palette);
        let mut pen = std::ptr::null_mut();
        GdipCreatePen1(argb, role.stroke_width() * scale, UnitPixel, &mut pen);
        pen
    };

    // Fonts for the two text sizes, at scaled pixel sizes.
    let px = |size: i32| ((size as f32 * scale).round() as i32).max(8);
    let f_label = make_font(px(Role::Label.font_size()));
    let f_note = make_font(px(Role::Note.font_size()));
    SetBkMode(hdc, TRANSPARENT as i32);

    for s in &diagram.shapes {
        match s {
            Shape::Path { role, start, curves, closed } => {
                let mut path: *mut GpPath = std::ptr::null_mut();
                GdipCreatePath(FillModeAlternate, &mut path);
                let (mut cx, mut cy) = (tx(start.0), ty(start.1));
                for &((c1x, c1y), (c2x, c2y), (ex, ey)) in curves {
                    GdipAddPathBezier(
                        path,
                        cx,
                        cy,
                        tx(c1x),
                        ty(c1y),
                        tx(c2x),
                        ty(c2y),
                        tx(ex),
                        ty(ey),
                    );
                    (cx, cy) = (tx(ex), ty(ey));
                }
                if *closed {
                    GdipClosePathFigure(path);
                }
                let pen = make_pen(*role);
                GdipDrawPath(graphics, pen, path);
                GdipDeletePen(pen);
                GdipDeletePath(path);
            }
            Shape::RoundRect { role, x, y, w, h, r } => {
                let (rx, ry, rw, rh) = (tx(*x), ty(*y), *w as f32 * scale, *h as f32 * scale);
                let d = (2.0 * *r as f32 * scale).min(rw).min(rh);
                let mut path: *mut GpPath = std::ptr::null_mut();
                GdipCreatePath(FillModeAlternate, &mut path);
                GdipAddPathArc(path, rx, ry, d, d, 180.0, 90.0);
                GdipAddPathArc(path, rx + rw - d, ry, d, d, 270.0, 90.0);
                GdipAddPathArc(path, rx + rw - d, ry + rh - d, d, d, 0.0, 90.0);
                GdipAddPathArc(path, rx, ry + rh - d, d, d, 90.0, 90.0);
                GdipClosePathFigure(path);
                let pen = make_pen(*role);
                GdipDrawPath(graphics, pen, path);
                GdipDeletePen(pen);
                GdipDeletePath(path);
            }
            Shape::Circle { role, cx, cy, r } => {
                let d = 2.0 * *r as f32 * scale;
                let pen = make_pen(*role);
                GdipDrawEllipse(graphics, pen, tx(cx - r), ty(cy - r), d, d);
                GdipDeletePen(pen);
            }
            Shape::Polyline { role, points } => {
                if points.len() < 2 {
                    continue;
                }
                let pts: Vec<PointF> =
                    points.iter().map(|&(x, y)| PointF { X: tx(x), Y: ty(y) }).collect();
                let pen = make_pen(*role);
                GdipDrawLines(graphics, pen, pts.as_ptr(), pts.len() as i32);
                GdipDeletePen(pen);
            }
            Shape::Text { role, at, anchor, text } => {
                let font = if role.font_size() >= 13 { f_label } else { f_note };
                draw_gdi_text(hdc, font, role_colors(*role, palette).1, tx(at.0) as i32, ty(at.1) as i32, *anchor, text);
            }
            Shape::Callout { at, anchor, label, note, note_role, .. } => {
                // Caption only — the window mounts the actual dropdown in the
                // reserved rect (callout_combo_rects). Empty caption lines
                // draw nothing: the dropdown is the callout.
                if !label.is_empty() {
                    draw_gdi_text(hdc, f_label, role_colors(Role::Label, palette).1, tx(at.0) as i32, ty(at.1) as i32, *anchor, label);
                }
                if !note.is_empty() {
                    let nfont = if note_role.font_size() >= 13 { f_label } else { f_note };
                    draw_gdi_text(hdc, nfont, role_colors(*note_role, palette).1, tx(at.0) as i32, ty(at.1 + 16) as i32, *anchor, note);
                }
            }
        }
    }

    DeleteObject(f_label as _);
    DeleteObject(f_note as _);
    GdipDeleteGraphics(graphics);
}
