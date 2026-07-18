//! Smoke test for the settings window: opens it with sample values, prints any
//! events, and auto-closes after ~5s (so it can run non-interactively / in CI).
//!
//!   cargo run -p platform --example settings_smoke              # full layout
//!   cargo run -p platform --example settings_smoke -- --minimal # no RGB, no DPI buttons
//!   ... -- --hold 30                                            # stay open longer
//!
//! The sample data is platform-test data, not device truth — the real diagrams
//! and capability gating come from `razer-proto`'s `DeviceSpec` via the daemon
//! (the sample geometry below mirrors the real specs, including the daemon's
//! conversion: callout captions are empty because the dropdowns identify
//! themselves via their index-0 entries — so screenshots are representative).

use platform::diagram::{Anchor, CalloutSlot, Diagram, Role, Shape};
use platform::settings::{
    self, ActionCombo, DpiButtonsInit, LightingZoneInit, SettingsEvent, SettingsInit,
};

/// A caption-less callout, as the daemon emits them for the window (the
/// dropdown mounts at the anchor; its index-0 entry names the button).
fn callout(slot: CalloutSlot, at: (i32, i32), anchor: Anchor) -> Shape {
    Shape::Callout { slot, at, anchor, label: String::new(), note: String::new(), note_role: Role::Note }
}

/// Full sample, mirroring the DeathAdder Elite spec: right-handed ergo shell,
/// RGB wheel/logo zones, DPI-button strip, thumb buttons on the flare.
fn diagram_full() -> Diagram {
    Diagram {
        shapes: vec![
            // body silhouette (top-down, cable at top)
            Shape::Path {
                role: Role::Body,
                start: (228, 30),
                closed: true,
                curves: vec![
                    ((180, 28), (140, 50), (130, 95)),    // left front shoulder
                    ((128, 125), (138, 155), (144, 185)), // left waist scoop
                    ((148, 205), (130, 230), (128, 255)), // left thumb flare/hip
                    ((126, 290), (155, 345), (195, 375)), // lower left toward tail
                    ((212, 388), (244, 388), (261, 375)), // rounded tail
                    ((301, 345), (322, 290), (320, 255)), // lower right/hip
                    ((318, 230), (300, 205), (304, 185)), // right side (pinky waist scoop)
                    ((308, 155), (320, 125), (326, 95)),  // right upper side
                    ((322, 50), (276, 28), (228, 30)),    // right front shoulder
                ],
            },
            // cable and strain relief boot
            Shape::RoundRect { role: Role::Detail, x: 224, y: 12, w: 8, h: 18, r: 1 },
            Shape::Polyline { role: Role::Detail, points: vec![(228, 12), (228, 2)] },
            // button split (broken around the wheel slot) + button/palm seam
            Shape::Polyline { role: Role::Detail, points: vec![(228, 30), (228, 66)] },
            Shape::Polyline { role: Role::Detail, points: vec![(228, 126), (228, 185)] },
            Shape::Path {
                role: Role::Detail,
                start: (144, 185),
                closed: false,
                curves: vec![((190, 205), (268, 200), (304, 185))],
            },
            Shape::Text { role: Role::Note, at: (186, 74), anchor: Anchor::Middle, text: "left".into() },
            Shape::Text { role: Role::Note, at: (270, 74), anchor: Anchor::Middle, text: "right".into() },
            // scroll wheel (long slot) — RGB zone. Keybind rides the LEFT
            // arm, RGB rides the RIGHT — same split as the DPI/thumb
            // keybinds vs. the logo's RGB below, so each side carries 3
            // balanced callouts (mirrors the Elite).
            Shape::RoundRect { role: Role::AccentA, x: 219, y: 68, w: 18, h: 56, r: 9 },
            Shape::Polyline { role: Role::Detail, points: vec![(220, 78), (236, 78)] },
            Shape::Polyline { role: Role::Detail, points: vec![(220, 88), (236, 88)] },
            Shape::Polyline { role: Role::Detail, points: vec![(220, 98), (236, 98)] },
            Shape::Polyline { role: Role::Detail, points: vec![(220, 108), (236, 108)] },
            Shape::Polyline { role: Role::Detail, points: vec![(220, 118), (236, 118)] },
            Shape::Polyline { role: Role::Lead, points: vec![(215, 96), (104, 96)] },
            callout(CalloutSlot::Wheel, (100, 93), Anchor::End),
            Shape::Polyline { role: Role::Lead, points: vec![(241, 96), (396, 96)] },
            callout(CalloutSlot::Lighting(0), (402, 93), Anchor::Start),
            // dpi_up (front, nearer the wheel) and dpi_down (rear) — center strip
            Shape::RoundRect { role: Role::AccentB, x: 218, y: 134, w: 20, h: 15, r: 5 },
            Shape::Polyline { role: Role::Lead, points: vec![(242, 142), (396, 154)] },
            callout(CalloutSlot::DpiUp, (402, 154), Anchor::Start),
            Shape::RoundRect { role: Role::AccentB, x: 218, y: 155, w: 20, h: 15, r: 5 },
            Shape::Polyline { role: Role::Lead, points: vec![(242, 163), (396, 221)] },
            // hook-free note in the gap between the DPI dropdowns — beside
            // the buttons it describes, not a distant footnote.
            Shape::Text { role: Role::Note, at: (402, 193), anchor: Anchor::Start, text: "rebinds always hook-free".into() },
            callout(CalloutSlot::DpiDown, (402, 221), Anchor::Start),
            // thumb buttons on the flare
            Shape::RoundRect { role: Role::AccentB, x: 126, y: 168, w: 18, h: 40, r: 6 },
            Shape::RoundRect { role: Role::AccentB, x: 120, y: 218, w: 18, h: 40, r: 6 },
            Shape::Polyline { role: Role::Lead, points: vec![(122, 188), (104, 188)] },
            Shape::Polyline { role: Role::Lead, points: vec![(116, 238), (104, 238)] },
            callout(CalloutSlot::ThumbForward, (100, 168), Anchor::End),
            callout(CalloutSlot::ThumbBack, (100, 240), Anchor::End),
            // hook cost, right at the point of decision (under the thumb
            // dropdowns) — benefit first, then the cost detail
            Shape::Text { role: Role::Note, at: (100, 274), anchor: Anchor::End, text: "default = no hook, zero cost".into() },
            Shape::Text { role: Role::Note, at: (100, 290), anchor: Anchor::End, text: "remaps use a global mouse hook".into() },
            Shape::Text { role: Role::Note, at: (100, 306), anchor: Anchor::End, text: "(~µs per mouse event)".into() },
            // logo LED on the palm (abstract marker)
            Shape::Circle { role: Role::AccentA, cx: 228, cy: 300, r: 16 },
            Shape::Path {
                role: Role::AccentA,
                start: (236, 290),
                closed: false,
                curves: vec![((220, 290), (238, 300), (222, 310))],
            },
            Shape::Polyline { role: Role::Lead, points: vec![(248, 300), (396, 300)] },
            // Logo zone's lighting cluster, mirroring the wheel's.
            callout(CalloutSlot::Lighting(1), (402, 297), Anchor::Start),
        ],
    }
}

/// Minimal sample, mirroring the DeathAdder V3 spec: modern shell with a thumb
/// scoop, plain wheel, no zones, no wheel DPI buttons.
fn diagram_minimal() -> Diagram {
    Diagram {
        shapes: vec![
            // body silhouette (top-down, cable at top)
            Shape::Path {
                role: Role::Body,
                start: (213, 31),
                closed: true,
                curves: vec![
                    ((189, 34), (167, 41), (148, 50)),    // left front shoulder
                    ((143, 55), (139, 60), (138, 69)),    // left upper wall
                    ((140, 95), (142, 120), (145, 140)),  // left waist scoop upper
                    ((147, 156), (147, 176), (143, 200)), // left waist scoop lower
                    ((138, 227), (132, 260), (133, 294)), // left hip flare
                    ((134, 331), (149, 359), (175, 377)), // lower left toward tail
                    ((191, 388), (209, 393), (230, 393)), // rounded tail left half
                    ((253, 393), (272, 387), (288, 373)), // rounded tail right half
                    ((311, 353), (323, 324), (325, 291)), // lower right hip
                    ((325, 262), (320, 234), (316, 209)), // right side pinky scoop lower
                    ((312, 185), (312, 164), (314, 141)), // right side pinky scoop upper
                    ((317, 113), (318, 89), (318, 68)),   // right upper wall
                    ((318, 59), (314, 54), (309, 50)),    // right shoulder upper
                    ((289, 40), (267, 34), (243, 30)),    // right front shoulder to cutout corner
                    ((237, 31), (232, 33), (228, 36)),    // cutout inside left wall curve
                    ((224, 33), (219, 31), (213, 31)),    // cutout inside right wall curve
                ],
            },
            // cable and strain relief boot
            Shape::Path {
                role: Role::Detail,
                start: (216, -24),
                closed: false,
                curves: vec![
                    ((221, -30), (225, -28), (229, -25)),
                    ((233, -22), (237, -22), (241, -25)),
                ],
            },
            Shape::Path {
                role: Role::Detail,
                start: (216, -18),
                closed: false,
                curves: vec![
                    ((221, -23), (225, -22), (229, -19)),
                    ((233, -15), (237, -15), (241, -19)),
                ],
            },
            Shape::Polyline { role: Role::Detail, points: vec![(228, -18), (228, -8)] },
            Shape::Polyline {
                role: Role::Detail,
                points: vec![
                    (220, -8), (237, -8), (237, -2), (234, -2), (234, 3), (238, 3), (238, 8), (234, 8),
                    (234, 13), (238, 13), (238, 19), (219, 19), (219, 13), (222, 13), (222, 8), (219, 8),
                    (219, 3), (222, 3), (222, -2), (220, -2), (220, -8)
                ],
            },
            Shape::Polyline { role: Role::Detail, points: vec![(222, -1), (234, -1)] },
            Shape::Polyline { role: Role::Detail, points: vec![(222, 4), (234, 4)] },
            Shape::Polyline { role: Role::Detail, points: vec![(222, 10), (234, 10)] },
            Shape::Polyline { role: Role::Detail, points: vec![(222, 15), (234, 15)] },
            // button split (broken around the wheel slot)
            Shape::Polyline { role: Role::Detail, points: vec![(228, 132), (228, 209)] },
            // button plate seams
            Shape::Path {
                role: Role::Detail,
                start: (138, 69),
                closed: false,
                curves: vec![
                    ((140, 94), (143, 116), (145, 139)),
                    ((148, 159), (147, 178), (143, 200)),
                    ((140, 218), (137, 237), (135, 257)),
                ],
            },
            Shape::Path {
                role: Role::Detail,
                start: (318, 68),
                closed: false,
                curves: vec![
                    ((316, 94), (315, 118), (313, 141)),
                    ((311, 164), (312, 186), (316, 209)),
                    ((318, 228), (321, 246), (323, 265)),
                ],
            },
            // button splits inside
            Shape::Polyline { role: Role::Detail, points: vec![(213, 63), (213, 132)] },
            Shape::Polyline { role: Role::Detail, points: vec![(243, 63), (243, 132)] },
            Shape::Polyline { role: Role::Detail, points: vec![(220, 59), (237, 59)] },
            Shape::Polyline { role: Role::Detail, points: vec![(220, 133), (237, 133)] },
            // tail highlighting seams
            Shape::Path {
                role: Role::Detail,
                start: (133, 295),
                closed: false,
                curves: vec![
                    ((136, 326), (148, 351), (171, 369)),
                    ((187, 381), (207, 388), (229, 388)),
                ],
            },
            Shape::Path {
                role: Role::Detail,
                start: (229, 388),
                closed: false,
                curves: vec![
                    ((253, 388), (272, 381), (288, 368)),
                    ((309, 350), (320, 323), (323, 293)),
                ],
            },
            Shape::Text { role: Role::Note, at: (180, 78), anchor: Anchor::Middle, text: "left".into() },
            Shape::Text { role: Role::Note, at: (275, 78), anchor: Anchor::Middle, text: "right".into() },
            // scroll wheel
            Shape::RoundRect { role: Role::Detail, x: 213, y: 63, w: 30, h: 70, r: 10 },
            Shape::RoundRect { role: Role::Detail, x: 217, y: 68, w: 22, h: 62, r: 8 },
            Shape::Polyline { role: Role::Detail, points: vec![(219, 72), (236, 72)] },
            Shape::Polyline { role: Role::Detail, points: vec![(218, 80), (237, 80)] },
            Shape::Polyline { role: Role::Detail, points: vec![(218, 89), (237, 89)] },
            Shape::Polyline { role: Role::Detail, points: vec![(218, 97), (237, 97)] },
            Shape::Polyline { role: Role::Detail, points: vec![(218, 106), (237, 106)] },
            Shape::Polyline { role: Role::Detail, points: vec![(218, 114), (237, 114)] },
            Shape::Polyline { role: Role::Detail, points: vec![(219, 122), (236, 122)] },
            Shape::Polyline { role: Role::Lead, points: vec![(243, 98), (332, 98)] },
            callout(CalloutSlot::Wheel, (348, 95), Anchor::Start),
            // thumb buttons (drawn as bumps sticking out of the left outline)
            Shape::Path {
                role: Role::AccentB,
                start: (141, 139),
                closed: true,
                curves: vec![
                    ((144, 138), (147, 140), (148, 144)),
                    ((150, 183), (150, 183), (150, 183)),
                    ((150, 189), (147, 192), (143, 193)),
                    ((141, 193), (138, 191), (138, 187)),
                    ((138, 146), (138, 146), (138, 146)),
                    ((138, 142), (139, 139), (141, 139)),
                ],
            },
            Shape::Path {
                role: Role::AccentB,
                start: (143, 197),
                closed: true,
                curves: vec![
                    ((146, 197), (148, 199), (148, 204)),
                    ((148, 244), (148, 244), (148, 244)),
                    ((148, 249), (145, 253), (141, 253)),
                    ((138, 252), (136, 249), (136, 245)),
                    ((137, 204), (137, 204), (137, 204)),
                    ((137, 200), (139, 197), (143, 197)),
                ],
            },
            Shape::Polyline { role: Role::Detail, points: vec![(140, 146), (147, 145)] },
            Shape::Polyline { role: Role::Detail, points: vec![(138, 204), (147, 202)] },
            Shape::Polyline { role: Role::Lead, points: vec![(138, 166), (126, 166)] },
            Shape::Polyline { role: Role::Lead, points: vec![(136, 225), (126, 225)] },
            callout(CalloutSlot::ThumbForward, (122, 162), Anchor::End),
            callout(CalloutSlot::ThumbBack, (122, 221), Anchor::End),
            // hook cost, right at the point of decision (under the thumb
            // dropdowns) — benefit first, then the cost detail
            Shape::Text { role: Role::Note, at: (122, 255), anchor: Anchor::End, text: "default = no hook, zero cost".into() },
            Shape::Text { role: Role::Note, at: (122, 271), anchor: Anchor::End, text: "remaps use a global mouse hook".into() },
            Shape::Text { role: Role::Note, at: (122, 287), anchor: Anchor::End, text: "(~µs per mouse event)".into() },
        ],
    }
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// A sample per-button dropdown, following the daemon's convention: index 0
/// is the button's identity/default entry, the rest are action presets.
fn combo(identity: &str, sel: usize) -> ActionCombo {
    let mut labels = vec![identity.to_string()];
    labels.extend(strings(&["copy", "paste", "cut", "key:9", "key:0"]));
    ActionCombo { labels, index: sel }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let minimal = args.iter().any(|a| a == "--minimal");
    let hold_secs: u64 = args
        .iter()
        .position(|a| a == "--hold")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);

    // Auto-close by posting WM_CLOSE to our own window.
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(hold_secs));
        use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};
        let cls: Vec<u16> = "SnakecharmerSettings".encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: FindWindowW/PostMessageW with a valid NUL-terminated class name.
        unsafe {
            let hwnd = FindWindowW(cls.as_ptr(), std::ptr::null());
            if !hwnd.is_null() {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
            }
        }
    });

    let init = if minimal {
        // A V3-like device: no lighting, no wheel DPI buttons, 8 kHz polling.
        SettingsInit {
            device_name: "Sample Mouse (minimal)".to_string(),
            diagram: Some(diagram_minimal()),
            dpi: 1800,
            dpi_min: 100,
            dpi_max: 30000,
            polling_labels: strings(&[
                "keep — leave as set", "125 Hz", "500 Hz", "1000 Hz", "2000 Hz", "4000 Hz",
                "8000 Hz",
            ]),
            polling_index: 3,
            dpi_buttons: None,
            thumb_back: combo("\u{2190} Back (default)", 3),
            thumb_forward: combo("\u{2192} Forward (default)", 0),
            wheel: ActionCombo { labels: strings(&["Middle click (default)"]), index: 0 },
            lighting: vec![],
        }
    } else {
        // An Elite-like device: everything on.
        SettingsInit {
            device_name: "Sample Mouse (full)".to_string(),
            diagram: Some(diagram_full()),
            dpi: 1800,
            dpi_min: 100,
            dpi_max: 16000,
            polling_labels: strings(&["keep — leave as set", "125 Hz", "500 Hz", "1000 Hz"]),
            polling_index: 3,
            dpi_buttons: Some(DpiButtonsInit {
                up: combo("Front DPI — unbound", 1),
                down: combo("Rear DPI — unbound", 2),
            }),
            thumb_back: combo("\u{2190} Back (default)", 3),
            thumb_forward: combo("\u{2192} Forward (default)", 0),
            wheel: ActionCombo { labels: strings(&["Middle click (default)"]), index: 0 },
            // Two zones (wheel + logo), independently colored like the Elite.
            lighting: vec![
                LightingZoneInit {
                    label: "Wheel".to_string(),
                    effect_labels: strings(&["keep", "static", "breathing", "spectrum", "off"]),
                    effect_index: 1,
                    color: (0x00, 0xC8, 0x40),
                },
                LightingZoneInit {
                    label: "Logo".to_string(),
                    effect_labels: strings(&["keep", "static", "breathing", "spectrum", "off"]),
                    effect_index: 2,
                    color: (0x40, 0x60, 0xE0),
                },
            ],
        }
    };

    println!(
        "Opening settings window ({}; auto-closes in ~{hold_secs}s)...",
        if minimal { "minimal" } else { "full" }
    );
    settings::open(init, |ev: SettingsEvent| println!("event: {ev:?}"));
    println!("Settings window closed. Smoke test OK.");
}
