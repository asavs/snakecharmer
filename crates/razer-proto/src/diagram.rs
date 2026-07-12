//! Device diagrams as data: a tiny shape DSL each [`crate::DeviceSpec`]
//! carries, describing a top-down schematic of the physical controls.
//!
//! One definition per device drives two renderers, so the art can never fork:
//! * the settings window draws it with GDI+ (the `platform` crate),
//! * [`Diagram::to_svg`] emits the `docs/assets/<device>.svg` shown in
//!   `docs/SUPPORTED-DEVICES.md` (a drift-check test regenerates the SVG and
//!   fails if the committed asset differs).
//!
//! Coordinates are integer design units (rendered 1 unit = 1 px at 96 dpi;
//! the emitter derives the viewBox from the content, so nothing can clip).
//! Authoring a new device's diagram is ~15 lines of data — start from an
//! existing device and move points; no artwork skills or tools required.

/// How an element is styled. Rendering picks stroke/fill color, width and
/// font size from the role, so diagrams stay theme-agnostic data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Primary outline (the device silhouette). Strong stroke.
    Body,
    /// Secondary line work (seams, splits, unlit parts). Thinner stroke.
    Detail,
    /// Leader line from a feature to its caption. Thin, faded.
    Lead,
    /// Main caption text.
    Label,
    /// Small dim text (sub-captions, footnotes).
    Note,
    /// Lighting-zone accent (shapes stroked / text filled in the RGB color).
    RgbZone,
    /// Remappable-button accent (shapes stroked / text filled in the accent).
    Button,
}

impl Role {
    /// Font size (design units) used when this role styles a [`Shape::Text`].
    pub const fn font_size(self) -> i32 {
        match self {
            Role::Label => 13,
            _ => 11,
        }
    }

    /// Stroke width in hundredths of a unit (1.6 units = 160).
    pub const fn stroke_width_100(self) -> i32 {
        match self {
            Role::Body | Role::RgbZone | Role::Button => 160,
            Role::Detail => 110,
            _ => 100,
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

/// Which interactive control a [`Shape::Callout`] anchors. The settings
/// window mounts the matching dropdown at the callout's position; the SVG
/// emitter (a static picture) renders the caption text only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalloutSlot {
    DpiUp,
    DpiDown,
    ThumbBack,
    ThumbForward,
}

/// A drawing primitive. Everything is const-constructible so diagrams live
/// in the `DeviceSpec` table as plain `static` data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// Cubic-bezier path from `start` through `curves`, optionally closed.
    Path { role: Role, start: (i32, i32), curves: &'static [Curve], closed: bool },
    /// Axis-aligned rectangle with corner radius `r`.
    RoundRect { role: Role, x: i32, y: i32, w: i32, h: i32, r: i32 },
    Circle { role: Role, cx: i32, cy: i32, r: i32 },
    /// Straight segments through `points` (used for leads, seams, splits).
    Polyline { role: Role, points: &'static [(i32, i32)] },
    /// One line of text; `at` is the baseline point (SVG semantics).
    Text { role: Role, at: (i32, i32), anchor: Anchor, text: &'static str },
    /// A remappable control's caption block *and* control anchor: `label` at
    /// baseline `at` (main caption style) with `note` 16 units below (styled
    /// by `note_role`). In the SVG this is just those two text lines; in the
    /// settings window the caption is drawn and the slot's actual dropdown is
    /// mounted directly beneath it — the diagram callout IS the control, so
    /// the fact is never stated twice.
    Callout {
        slot: CalloutSlot,
        at: (i32, i32),
        anchor: Anchor,
        label: &'static str,
        note: &'static str,
        note_role: Role,
    },
}

/// A device diagram: a design-canvas hint plus its shapes, in paint order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Diagram {
    /// Design canvas hint (the emitter computes real bounds from content).
    pub width: i32,
    pub height: i32,
    pub shapes: &'static [Shape],
}

/// Estimated pixel width of `text` at `size` (average advance ≈ 0.62 em —
/// generous for both the SVG's monospace stack and the window's Segoe UI).
pub fn est_text_width(text: &str, size: i32) -> i32 {
    text.chars().count() as i32 * size * 62 / 100
}

/// Horizontal extent `[x0, x1]` of a text run given its anchor.
fn text_extent_x(x: i32, anchor: Anchor, w: i32) -> (i32, i32) {
    match anchor {
        Anchor::Start => (x, x + w),
        Anchor::Middle => (x - w / 2, x + w - w / 2),
        Anchor::End => (x - w, x),
    }
}

impl Diagram {
    /// Content bounding box `(x0, y0, x1, y1)` over every shape, including
    /// estimated text extents — the emitter and the window renderer both size
    /// from this, which is what guarantees captions can't clip.
    pub fn bounds(&self) -> (i32, i32, i32, i32) {
        let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        let mut add = |x: i32, y: i32| {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        };
        for s in self.shapes {
            match *s {
                Shape::Path { start, curves, .. } => {
                    add(start.0, start.1);
                    for &((c1x, c1y), (c2x, c2y), (ex, ey)) in curves {
                        add(c1x, c1y);
                        add(c2x, c2y);
                        add(ex, ey);
                    }
                }
                Shape::RoundRect { x, y, w, h, .. } => {
                    add(x, y);
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
                    let (tx0, tx1) = text_extent_x(at.0, anchor, est_text_width(text, size));
                    add(tx0, at.1 - size); // ascent
                    add(tx1, at.1 + size / 3); // descent
                }
                Shape::Callout { at, anchor, label, note, note_role, .. } => {
                    let (lx0, lx1) =
                        text_extent_x(at.0, anchor, est_text_width(label, Role::Label.font_size()));
                    add(lx0, at.1 - Role::Label.font_size());
                    add(lx1, at.1);
                    let nsize = note_role.font_size();
                    let (nx0, nx1) = text_extent_x(at.0, anchor, est_text_width(note, nsize));
                    add(nx0, at.1 + 16 - nsize);
                    add(nx1, at.1 + 16 + nsize / 3);
                }
            }
        }
        if self.shapes.is_empty() {
            return (0, 0, self.width, self.height);
        }
        (x0, y0, x1, y1)
    }

    /// Emit the diagram as a small standalone SVG (theme-aware via
    /// `prefers-color-scheme`, system monospace stack, no external assets).
    /// The viewBox is derived from [`Diagram::bounds`] plus padding, so text
    /// that outgrows the design canvas enlarges the image instead of clipping.
    pub fn to_svg(&self) -> String {
        const PAD: i32 = 12;
        let (x0, y0, x1, y1) = self.bounds();
        let (vx, vy) = (x0 - PAD, y0 - PAD);
        let (vw, vh) = (x1 - x0 + 2 * PAD, y1 - y0 + 2 * PAD);

        let mut out = String::with_capacity(4096);
        out.push_str(&format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{vx} {vy} {vw} {vh}\" \
             width=\"{vw}\" height=\"{vh}\" \
             font-family=\"ui-monospace,'Cascadia Mono',Consolas,Menlo,monospace\" \
             font-size=\"13\">\n"
        ));
        out.push_str(
            "  <style>\n\
             \x20   svg { --fg: #6e7781; --rgb: #2da44e; --btn: #4a7fd4; }\n\
             \x20   @media (prefers-color-scheme: dark) {\n\
             \x20     svg { --fg: #9da7b3; --rgb: #3fb950; --btn: #6ca0f0; }\n\
             \x20   }\n\
             \x20   .body { stroke: var(--fg); fill: none; stroke-width: 1.6; }\n\
             \x20   .det  { stroke: var(--fg); fill: none; stroke-width: 1.1; }\n\
             \x20   .lead { stroke: var(--fg); fill: none; stroke-width: 1; opacity: .55; }\n\
             \x20   .txt  { fill: var(--fg); }\n\
             \x20   .sub  { fill: var(--fg); opacity: .72; font-size: 11px; }\n\
             \x20   .rgb  { stroke: var(--rgb); fill: none; stroke-width: 1.6; }\n\
             \x20   .rgbt { fill: var(--rgb); font-size: 11px; }\n\
             \x20   .btn  { stroke: var(--btn); fill: none; stroke-width: 1.6; }\n\
             \x20   .btnt { fill: var(--btn); font-size: 11px; }\n\
             \x20 </style>\n",
        );

        for s in self.shapes {
            match *s {
                Shape::Path { role, start, curves, closed } => {
                    let mut d = format!("M{} {}", start.0, start.1);
                    for &((c1x, c1y), (c2x, c2y), (ex, ey)) in curves {
                        d.push_str(&format!(" C{c1x} {c1y} {c2x} {c2y} {ex} {ey}"));
                    }
                    if closed {
                        d.push_str(" Z");
                    }
                    out.push_str(&format!(
                        "  <path class=\"{}\" d=\"{d}\"/>\n",
                        stroke_class(role)
                    ));
                }
                Shape::RoundRect { role, x, y, w, h, r } => {
                    out.push_str(&format!(
                        "  <rect class=\"{}\" x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" rx=\"{r}\"/>\n",
                        stroke_class(role)
                    ));
                }
                Shape::Circle { role, cx, cy, r } => {
                    out.push_str(&format!(
                        "  <circle class=\"{}\" cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\"/>\n",
                        stroke_class(role)
                    ));
                }
                Shape::Polyline { role, points } => {
                    let pts: Vec<String> =
                        points.iter().map(|&(x, y)| format!("{x},{y}")).collect();
                    out.push_str(&format!(
                        "  <polyline class=\"{}\" points=\"{}\"/>\n",
                        stroke_class(role),
                        pts.join(" ")
                    ));
                }
                Shape::Text { role, at, anchor, text } => {
                    out.push_str(&svg_text(text_class(role), at, anchor, text));
                }
                // A static picture has no dropdown to mount, so a callout is
                // simply its caption: label line + note line.
                Shape::Callout { at, anchor, label, note, note_role, .. } => {
                    out.push_str(&svg_text(text_class(Role::Label), at, anchor, label));
                    out.push_str(&svg_text(text_class(note_role), (at.0, at.1 + 16), anchor, note));
                }
            }
        }
        out.push_str("</svg>\n");
        out
    }
}

/// One `<text>` element with the given class, position, and anchor.
fn svg_text(class: &str, at: (i32, i32), anchor: Anchor, text: &str) -> String {
    let anchor_attr = match anchor {
        Anchor::Start => "",
        Anchor::Middle => " text-anchor=\"middle\"",
        Anchor::End => " text-anchor=\"end\"",
    };
    format!(
        "  <text class=\"{class}\" x=\"{}\" y=\"{}\"{anchor_attr}>{}</text>\n",
        at.0,
        at.1,
        escape(text)
    )
}

/// CSS class for a stroked (non-text) shape of this role.
fn stroke_class(role: Role) -> &'static str {
    match role {
        Role::Body => "body",
        Role::Detail => "det",
        Role::Lead => "lead",
        Role::RgbZone => "rgb",
        Role::Button => "btn",
        // Text roles never stroke shapes, but map them sanely anyway.
        Role::Label => "body",
        Role::Note => "det",
    }
}

/// CSS class for a text run of this role.
fn text_class(role: Role) -> &'static str {
    match role {
        Role::Label => "txt",
        Role::Note => "sub",
        Role::RgbZone => "rgbt",
        Role::Button => "btnt",
        // Shape roles as text fall back to the main caption style.
        _ => "txt",
    }
}

/// Minimal XML escaping for text content.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// File-name slug for a device's generated SVG asset
/// (`docs/assets/<slug>.svg`): lowercased, runs of non-alphanumerics
/// collapsed to `-`.
pub fn asset_slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: Diagram = Diagram {
        width: 100,
        height: 50,
        shapes: &[
            Shape::Circle { role: Role::RgbZone, cx: 20, cy: 20, r: 10 },
            Shape::Text { role: Role::Label, at: (30, 40), anchor: Anchor::Start, text: "a<b&c" },
        ],
    };

    #[test]
    fn bounds_cover_shapes_and_text() {
        let (x0, y0, x1, y1) = D.bounds();
        assert_eq!((x0, y0), (10, 10), "circle drives the min corner");
        assert!(x1 >= 30 + est_text_width("a<b&c", 13), "text width extends the max corner");
        assert!(y1 >= 40, "text descent extends below the baseline");
    }

    #[test]
    fn svg_is_escaped_and_padded() {
        let svg = D.to_svg();
        assert!(svg.contains("a&lt;b&amp;c"), "text content must be XML-escaped");
        assert!(svg.contains("viewBox=\"-2 -2"), "viewBox = bounds min - 12 padding");
        assert!(svg.contains("prefers-color-scheme: dark"), "dark-mode aware");
        assert!(!svg.contains("http://") || svg.matches("http://").count() == 1,
            "no external references beyond the xmlns");
    }

    #[test]
    fn empty_diagram_falls_back_to_canvas() {
        let d = Diagram { width: 10, height: 20, shapes: &[] };
        assert_eq!(d.bounds(), (0, 0, 10, 20));
    }

    #[test]
    fn slugs() {
        assert_eq!(asset_slug("DeathAdder Elite"), "deathadder-elite");
        assert_eq!(asset_slug("DeathAdder V3"), "deathadder-v3");
        assert_eq!(asset_slug("Naga (Pro) 2023"), "naga-pro-2023");
    }
}
