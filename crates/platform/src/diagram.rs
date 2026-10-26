//! Generic 2-D schematic rendering for the settings window: anti-aliased
//! vector shapes via the GDI+ flat API (`gdiplus.dll`, part of Windows —
//! exposed through the already-pinned `windows-sys`, so no new dependency),
//! text via classic GDI so captions get native ClearType.
//!
//! The schema is shared with the SVG emitter in `device-diagram`. Native
//! callouts reserve space for self-identifying controls rather than painting
//! the static captions that the documentation SVG displays.

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

pub use device_diagram::{Anchor, CalloutSlot, Curve, Diagram, Role, Shape};

/// Width of the dropdown a callout reserves space for (design units), sized
/// for the self-identifying entries and action presets.
pub const CALLOUT_COMBO_W: i32 = 145;
/// A [`CalloutSlot::Lighting`] cluster's parts, left to right: effect
/// dropdown, then the color swatch (itself the picker trigger — see
/// `settings::ID_BTN_COLOR_BASE`), with [`LIGHT_GAP`] between them.
pub const LIGHT_COMBO_W: i32 = 110;
pub const LIGHT_SWATCH_W: i32 = 40;
pub const LIGHT_GAP: i32 = 4;
/// Full width a Lighting callout reserves (one row of the two controls).
pub const LIGHT_ROW_W: i32 = LIGHT_COMBO_W + LIGHT_GAP + LIGHT_SWATCH_W;
/// Closed height of the dropdown a callout reserves space for.
pub const CALLOUT_COMBO_H: i32 = 24;
/// The control is centered on its callout anchor's caption-block midpoint.
const CALLOUT_COMBO_DY_BARE: i32 = 8 - CALLOUT_COMBO_H / 2;

/// The rect (design units: x, y, w, h) a callout reserves for its dropdown.
/// `has_caption` selects the stacked (below the caption lines) or bare
/// (centered on the caption block) mount — see the two `CALLOUT_COMBO_DY*`.
fn callout_combo_rect(
    slot: CalloutSlot,
    at: (i32, i32),
    anchor: Anchor,
) -> (i32, i32, i32, i32) {
    let w = match slot {
        CalloutSlot::Lighting(_) => LIGHT_ROW_W,
        _ => CALLOUT_COMBO_W,
    };
    let x = match anchor {
        Anchor::Start => at.0,
        Anchor::Middle => at.0 - w / 2,
        Anchor::End => at.0 - w,
    };
    (x, at.1 + CALLOUT_COMBO_DY_BARE, w, CALLOUT_COMBO_H)
}

/// Every callout's slot and reserved dropdown rect (design units), in shape
/// order. The window maps these through the same origin/scale as [`render`].
pub fn callout_combo_rects(diagram: &Diagram) -> Vec<(CalloutSlot, (i32, i32, i32, i32))> {
    diagram
        .shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Callout { slot, at, anchor, .. } => Some((
                *slot,
                callout_combo_rect(*slot, *at, *anchor),
            )),
            _ => None,
        })
        .collect()
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
        Role::RgbZone => (colorref_to_argb(p.accent_a, 0xFF), p.accent_a),
        Role::Button => (colorref_to_argb(p.accent_b, 0xFF), p.accent_b),
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
        for s in diagram.shapes {
            match s {
                Shape::Path { start, curves, .. } => {
                    add(start.0, start.1);
                    for &((c1x, c1y), (c2x, c2y), (ex, ey)) in curves.iter() {
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
                    for &(x, y) in points.iter() {
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
                Shape::Callout { slot, at, anchor, .. } => {
                    let (cx, cy, cw, ch) = callout_combo_rect(*slot, *at, *anchor);
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
    for s in diagram.shapes {
        match s {
            Shape::Path { role: Role::Body, start, curves, .. } => {
                add(start.0);
                for &((c1x, _), (c2x, _), (ex, _)) in curves.iter() {
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
                for &(x, _) in points.iter() {
                    add(x);
                }
            }
            _ => {}
        }
    }
    (x0 <= x1).then_some((x0, x1))
}

/// How far the diagram's content reaches left and right of the mouse body's
/// centerline (design units, unscaled) — `(left_arm, right_arm)`. `None`
/// when there's no body path or the content has zero width.
///
/// The two arms are almost never equal: which callouts land on which side is
/// dictated by the hardware (an RGB zone's controls sit beside that zone; a
/// device with no lighting has nothing to balance a wheel callout against),
/// so every diagram — present or future — carries some imbalance. The
/// settings window (see its `CENTER_SLACK` clamp) uses this to bound how much
/// of that imbalance becomes an asymmetric margin versus the body drifting
/// off the window's centerline.
pub fn arm_balance(diagram: &Diagram) -> Option<(f32, f32)> {
    let (x0, _, x1, _) = measure(diagram);
    let (bx0, bx1) = body_x_bounds(diagram)?;
    let w = x1 - x0;
    if w <= 0 {
        return None;
    }
    let body_off = ((bx0 + bx1) / 2 - x0) as f32;
    Some((body_off, w as f32 - body_off))
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
        GdipCreatePen1(argb, role.stroke_width_100() as f32 / 100.0 * scale, UnitPixel, &mut pen);
        pen
    };

    // Fonts for the two text sizes, at scaled pixel sizes.
    let px = |size: i32| ((size as f32 * scale).round() as i32).max(8);
    let f_label = make_font(px(Role::Label.font_size()));
    let f_note = make_font(px(Role::Note.font_size()));
    SetBkMode(hdc, TRANSPARENT as i32);

    for s in diagram.shapes {
        match s {
            Shape::Path { role, start, curves, closed } => {
                let mut path: *mut GpPath = std::ptr::null_mut();
                GdipCreatePath(FillModeAlternate, &mut path);
                let (mut cx, mut cy) = (tx(start.0), ty(start.1));
                for &((c1x, c1y), (c2x, c2y), (ex, ey)) in curves.iter() {
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
            // The native control itself supplies the callout's identity.
            Shape::Callout { .. } => {}
        }
    }

    DeleteObject(f_label as _);
    DeleteObject(f_note as _);
    GdipDeleteGraphics(graphics);
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPTIONED: Diagram = Diagram {
        width: 100,
        height: 100,
        shapes: &[
            Shape::Callout {
                slot: CalloutSlot::Button(0x03),
                at: (100, 93),
                anchor: Anchor::End,
                label: "Middle click",
                note: "SVG caption only",
                note_role: Role::Note,
            },
            Shape::Callout {
                slot: CalloutSlot::Lighting(0),
                at: (402, 93),
                anchor: Anchor::Start,
                label: "Wheel lighting",
                note: "SVG note only",
                note_role: Role::RgbZone,
            },
            Shape::Callout {
                slot: CalloutSlot::DpiUp,
                at: (228, 154),
                anchor: Anchor::Middle,
                label: "Front DPI",
                note: "",
                note_role: Role::Button,
            },
        ],
    };

    #[test]
    fn native_controls_keep_the_captionless_anchor_rects() {
        // These are the old native mapper's rectangles after it stripped SVG
        // captions. Sharing the original captioned shapes must not move them.
        assert_eq!(callout_combo_rects(&CAPTIONED), vec![
            (CalloutSlot::Button(0x03), (-45, 89, 145, 24)),
            (CalloutSlot::Lighting(0), (402, 89, 154, 24)),
            (CalloutSlot::DpiUp, (156, 150, 145, 24)),
        ]);
        let svg = CAPTIONED.to_svg();
        assert!(svg.contains("Middle click"));
        assert!(svg.contains("SVG caption only"));
        assert!(svg.contains("Wheel lighting"));
    }
}
