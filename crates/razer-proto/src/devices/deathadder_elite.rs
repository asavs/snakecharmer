//! DeathAdder Elite: full [`DeviceSpec`] — protocol parameters and the
//! top-down schematic geometry that drives the settings window and the
//! generated `docs/assets/deathadder-elite.svg`.

use crate::diagram::{Anchor, CalloutSlot, Diagram, Role, Shape};
use crate::{led, DeviceSpec, DpiButtons, PollingProtocol, PollingSpec};

/// DeathAdder Elite (`PID 0x005C`): two RGB zones, wheel DPI buttons, 16000 DPI.
pub const DEATHADDER_ELITE: DeviceSpec = DeviceSpec {
    product_id: 0x005C,
    transaction_id: 0x3F,
    name: "DeathAdder Elite",
    rgb_zones: &[led::SCROLL_WHEEL, led::LOGO],
    dpi_buttons: Some(DpiButtons { up: 0x20, down: 0x21 }),
    dpi_min: 100,
    dpi_max: 16000,
    polling: PollingSpec { protocol: PollingProtocol::Classic, rates: &[125, 500, 1000] },
    // Positions per Razer's official schematic (dl.razerzone.com/src/aag/2043-2-en-v2.png,
    // reference only): right-handed ergo shell — left side flares out for the thumb
    // rest, right side tucks toward a leaning tail; long wheel slot; two-button DPI
    // strip behind the wheel; side buttons on the thumb flare. Original line work.
    diagram: Diagram {
        width: 780,
        height: 440,
        shapes: &[
            // body silhouette (top-down, cable at top). The center cutout is a
            // true slot: from the pointed tips at the cable boot's base the
            // outline dives down both sides of the wheel and closes around its
            // bottom, meeting the center split to the DPI strip.
            Shape::Path { role: Role::Body, start: (214, 30), closed: true, curves: &[
                ((174, 39), (141, 58), (126, 73)),     // left front shoulder & top edge
                ((130, 90), (134, 108), (137, 129)),   // left upper wall
                ((141, 152), (143, 178), (140, 205)),  // left waist scoop
                ((137, 225), (133, 249), (133, 274)),  // left hip
                ((133, 312), (144, 342), (164, 361)),  // left rear flare
                ((181, 377), (202, 386), (225, 389)),  // left tail half
                ((256, 389), (283, 379), (302, 361)),  // right tail half
                ((332, 341), (332, 310), (332, 273)),  // right rear flare
                ((326, 248), (332, 224), (323, 204)),  // right hip
                ((317, 179), (316, 154), (318, 130)),  // right waist scoop
                ((319, 111), (323, 91), (326, 73)),    // right upper wall
                ((311, 58), (278, 39), (242, 30)),     // right top edge & front shoulder
                ((242, 62), (242, 92), (242, 122)),    // right slot wall, straight past the boot
                ((242, 134), (214, 134), (214, 122)),  // one clean U around the wheel bottom
                ((214, 92), (214, 62), (214, 30)),     // left slot wall back to the tip (closes)
            ]},
            // cable and strain relief boot (sits inside the cutout)
            Shape::Path { role: Role::Detail, start: (217, -10), closed: false, curves: &[
                ((220, -15), (224, -14), (228, -11)),
                ((232, -7), (236, -7), (240, -11)),
            ]},
            Shape::Path { role: Role::Detail, start: (217, -4), closed: false, curves: &[
                ((220, -9), (224, -8), (228, -5)),
                ((232, -1), (236, -1), (240, -5)),
            ]},
            Shape::Polyline { role: Role::Detail, points: &[(228, -4), (228, 12)] },
            Shape::RoundRect { role: Role::Detail, x: 220, y: 12, w: 16, h: 18, r: 1 },
            Shape::Polyline { role: Role::Detail, points: &[(222, 16), (234, 16)] },
            Shape::Polyline { role: Role::Detail, points: &[(222, 20), (234, 20)] },
            Shape::Polyline { role: Role::Detail, points: &[(222, 24), (234, 24)] },
            Shape::Polyline { role: Role::Detail, points: &[(222, 28), (234, 28)] },
            // boot channel seams: the slot walls already draw the sides, so
            // only two cross-curves remain — the shell/boot joint on top and
            // the channel's inner end below, one merged adjuster each
            Shape::Path { role: Role::Detail, start: (214, 40), closed: false, curves: &[
                ((228, 38), (228, 38), (242, 40)),     // shell meets the boot
            ]},
            Shape::Path { role: Role::Detail, start: (214, 69), closed: false, curves: &[
                ((214, 65), (217, 62), (221, 62)),     // left fillet off the slot wall
                ((226, 62), (230, 62), (235, 62)),     // flat crown, echoing the wheel top
                ((239, 62), (242, 65), (242, 69)),     // right fillet into the wall
            ]},
            // center spine: wheel-to-dpi_up stem, then the tail split snug
            // against dpi_down's bottom edge
            Shape::Polyline { role: Role::Detail, points: &[(228, 131), (228, 140)] },
            Shape::Polyline { role: Role::Detail, points: &[(228, 201), (228, 223)] },
            // interior side grips & channel detail
            Shape::Path { role: Role::Detail, start: (129, 76), closed: false, curves: &[
                ((134, 94), (138, 107), (142, 121)),
                ((147, 135), (151, 152), (153, 170)),
                ((155, 189), (155, 209), (151, 225)),
                ((148, 236), (145, 245), (142, 253)),
            ]},
            Shape::Path { role: Role::Detail, start: (326, 73), closed: true, curves: &[
                ((323, 97), (321, 118), (319, 141)),
                ((318, 167), (321, 192), (325, 215)),
                ((328, 233), (331, 251), (331, 273)),
                ((327, 264), (324, 251), (322, 237)),
                ((319, 213), (317, 189), (317, 165)),
                ((317, 132), (320, 99), (326, 73)),
            ]},
            Shape::Text { role: Role::Note, at: (186, 74), anchor: Anchor::Middle, text: "left" },
            Shape::Text { role: Role::Note, at: (270, 74), anchor: Anchor::Middle, text: "right" },
            // scroll wheel (long slot) — RGB zone 0x01
            Shape::RoundRect { role: Role::RgbZone, x: 214, y: 65, w: 27, h: 66, r: 10 },
            Shape::RoundRect { role: Role::Detail, x: 218, y: 67, w: 20, h: 63, r: 8 },
            Shape::Polyline { role: Role::Detail, points: &[(219, 75), (237, 75)] },
            Shape::Polyline { role: Role::Detail, points: &[(219, 85), (237, 85)] },
            Shape::Polyline { role: Role::Detail, points: &[(219, 95), (237, 95)] },
            Shape::Polyline { role: Role::Detail, points: &[(219, 105), (237, 105)] },
            Shape::Polyline { role: Role::Detail, points: &[(219, 115), (237, 115)] },
            Shape::Polyline { role: Role::Detail, points: &[(219, 125), (237, 125)] },
            // middle click rides the LEFT arm (empty space above the thumb
            // callouts); the wheel's right-arm lead belongs to its RGB zone
            Shape::Polyline { role: Role::Lead, points: &[(212, 98), (104, 98)] },
            Shape::Callout { slot: CalloutSlot::Wheel, at: (100, 95), anchor: Anchor::End,
                label: "scroll wheel — middle click", note: "", note_role: Role::Note },
            // Wheel zone lighting at the end of its own lead: the caption
            // names the zone, the bare Lighting slot mounts the effect +
            // color cluster in the settings window.
            Shape::Polyline { role: Role::Lead, points: &[(241, 98), (396, 98)] },
            Shape::Text { role: Role::RgbZone, at: (402, 95), anchor: Anchor::Start, text: "RGB zone 0x01" },
            Shape::Callout { slot: CalloutSlot::Lighting(0), at: (402, 110), anchor: Anchor::Start,
                label: "", note: "", note_role: Role::Note },
            // dpi_up (front, nearer the wheel) and dpi_down (rear) — center strip
            Shape::RoundRect { role: Role::Button, x: 222, y: 140, w: 13, h: 30, r: 4 },
            Shape::Polyline { role: Role::Lead, points: &[(235, 155), (396, 155)] },
            Shape::Callout { slot: CalloutSlot::DpiUp, at: (402, 151), anchor: Anchor::Start,
                label: "dpi_up", note: "code 0x20 · front, nearer the wheel", note_role: Role::Button },
            Shape::RoundRect { role: Role::Button, x: 222, y: 170, w: 13, h: 31, r: 4 },
            Shape::Polyline { role: Role::Lead, points: &[(235, 185), (340, 185), (372, 218), (396, 218)] },
            Shape::Callout { slot: CalloutSlot::DpiDown, at: (402, 218), anchor: Anchor::Start,
                label: "dpi_down", note: "code 0x21 · rear button", note_role: Role::Button },
            // thumb buttons (drawn as clean crescents following the waist)
            Shape::Path { role: Role::Button, start: (143, 140), closed: true, curves: &[
                ((143, 155), (143, 170), (142, 185)),  // left side hugs the waist
                ((142, 185), (152, 185), (152, 185)),  // inner edge, meets "back"
                ((151, 170), (149, 155), (147, 140)),  // right side inside the grip line
                ((147, 140), (143, 140), (143, 140)),  // outer edge (closes)
            ]},
            Shape::Path { role: Role::Button, start: (152, 187), closed: true, curves: &[
                ((152, 187), (142, 187), (142, 187)),  // inner edge, meets "forward"
                ((141, 205), (140, 222), (138, 238)),  // left side follows the hip
                ((138, 238), (143, 238), (143, 238)),  // outer edge
                ((148, 222), (151, 205), (152, 187)),  // right side inside the grip line (closes)
            ]},
            Shape::Polyline { role: Role::Lead, points: &[(143, 170), (104, 170)] },
            Shape::Polyline { role: Role::Lead, points: &[(144, 204), (104, 204)] },
            Shape::Callout { slot: CalloutSlot::ThumbForward, at: (100, 165), anchor: Anchor::End,
                label: "\"forward\"", note: "XBUTTON2", note_role: Role::Note },
            Shape::Callout { slot: CalloutSlot::ThumbBack, at: (100, 222), anchor: Anchor::End,
                label: "\"back\"", note: "XBUTTON1", note_role: Role::Note },
            // hook cost, right at the point of decision (under the thumb
            // callouts) — benefit first, then the cost detail
            Shape::Text { role: Role::Note, at: (100, 252), anchor: Anchor::End, text: "default = no hook, zero cost" },
            Shape::Text { role: Role::Note, at: (100, 268), anchor: Anchor::End, text: "remaps use a global mouse hook" },
            Shape::Text { role: Role::Note, at: (100, 284), anchor: Anchor::End, text: "(~µs per mouse event)" },
            // logo LED on the palm — RGB zone 0x04 (abstract marker; never Razer's logo)
            Shape::Circle { role: Role::RgbZone, cx: 228, cy: 300, r: 24 },
            Shape::Path { role: Role::RgbZone, start: (228, 284), closed: false, curves: &[
                ((212, 292), (244, 308), (228, 316)),
            ]},
            Shape::Polyline { role: Role::Lead, points: &[(252, 300), (396, 300)] },
            Shape::Text { role: Role::Label, at: (402, 297), anchor: Anchor::Start, text: "logo" },
            Shape::Text { role: Role::RgbZone, at: (402, 313), anchor: Anchor::Start, text: "RGB zone 0x04" },
            // Logo zone lighting cluster, mirroring the wheel's.
            Shape::Callout { slot: CalloutSlot::Lighting(1), at: (402, 325), anchor: Anchor::Start,
                label: "", note: "", note_role: Role::Note },
            // footnote, wrapped so it never widens the canvas (the thumb-hook
            // disclaimer lives beside the thumb callouts)
            Shape::Text { role: Role::Note, at: (330, 420), anchor: Anchor::Middle, text: "DPI-button remaps are hook-free:" },
            Shape::Text { role: Role::Note, at: (330, 436), anchor: Anchor::Middle, text: "vendor codes in driver mode, pointer motion untouched" },
        ],
    },
};
