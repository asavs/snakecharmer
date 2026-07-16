//! DeathAdder V3 (wired): full [`DeviceSpec`] — protocol parameters and the
//! top-down schematic geometry that drives the settings window and the
//! generated `docs/assets/deathadder-v3.svg`.

use crate::diagram::{Anchor, CalloutSlot, Diagram, Role, Shape};
use crate::{DeviceSpec, PollingProtocol, PollingSpec};

/// DeathAdder V3 wired (`PID 0x00B2`): no lighting, no wheel DPI buttons, 30000 DPI.
/// Protocol per openrazer `razermouse_driver.c` (transaction_id `0x1F`).
pub const DEATHADDER_V3: DeviceSpec = DeviceSpec {
    product_id: 0x00B2,
    transaction_id: 0x1F,
    name: "DeathAdder V3",
    rgb_zones: &[],
    dpi_buttons: None,
    dpi_min: 100,
    dpi_max: 30000,
    polling: PollingSpec {
        protocol: PollingProtocol::Extended,
        rates: &[125, 500, 1000, 2000, 4000, 8000],
    },
    // Positions per Razer's official schematic (dl.razerzone.com/src2/6128/6128-2-en-v1.png,
    // reference only): lighter modern shell — concave thumb scoop mid-left, deep
    // sculpted button plates, larger wheel set further back, side buttons above the
    // scoop. No lighting, no logo LED. Original line work.
    diagram: Diagram {
        width: 780,
        height: 430,
        shapes: &[
            // body silhouette (top-down, cable at top)
            Shape::Path { role: Role::Body, start: (213, 31), closed: true, curves: &[
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
            ]},
            // cable and strain relief boot
            Shape::Path { role: Role::Detail, start: (216, -24), closed: false, curves: &[
                ((221, -30), (225, -28), (229, -25)),
                ((233, -22), (237, -22), (241, -25)),
            ]},
            Shape::Path { role: Role::Detail, start: (216, -18), closed: false, curves: &[
                ((221, -23), (225, -22), (229, -19)),
                ((233, -15), (237, -15), (241, -19)),
            ]},
            Shape::Polyline { role: Role::Detail, points: &[(228, -18), (228, -8)] },
            Shape::Polyline { role: Role::Detail, points: &[(220, -8), (237, -8), (237, -2), (234, -2), (234, 3), (238, 3), (238, 8), (234, 8), (234, 13), (238, 13), (238, 19), (219, 19), (219, 13), (222, 13), (222, 8), (219, 8), (219, 3), (222, 3), (222, -2), (220, -2), (220, -8)] },
            Shape::Polyline { role: Role::Detail, points: &[(222, -1), (234, -1)] },
            Shape::Polyline { role: Role::Detail, points: &[(222, 4), (234, 4)] },
            Shape::Polyline { role: Role::Detail, points: &[(222, 10), (234, 10)] },
            Shape::Polyline { role: Role::Detail, points: &[(222, 15), (234, 15)] },
            // button split (broken around the wheel slot)
            Shape::Polyline { role: Role::Detail, points: &[(228, 132), (228, 209)] },
            // button plate seams
            Shape::Path { role: Role::Detail, start: (138, 69), closed: false, curves: &[
                ((140, 94), (143, 116), (145, 139)),
                ((148, 159), (147, 178), (143, 200)),
                ((140, 218), (137, 237), (135, 257)),
            ]},
            Shape::Path { role: Role::Detail, start: (318, 68), closed: false, curves: &[
                ((316, 94), (315, 118), (313, 141)),
                ((311, 164), (312, 186), (316, 209)),
                ((318, 228), (321, 246), (323, 265)),
            ]},
            // button splits inside
            Shape::Polyline { role: Role::Detail, points: &[(213, 63), (213, 132)] },
            Shape::Polyline { role: Role::Detail, points: &[(243, 63), (243, 132)] },
            Shape::Polyline { role: Role::Detail, points: &[(220, 59), (237, 59)] },
            Shape::Polyline { role: Role::Detail, points: &[(220, 133), (237, 133)] },
            // tail highlighting seams
            Shape::Path { role: Role::Detail, start: (133, 295), closed: false, curves: &[
                ((136, 326), (148, 351), (171, 369)),
                ((187, 381), (207, 388), (229, 388)),
            ]},
            Shape::Path { role: Role::Detail, start: (229, 388), closed: false, curves: &[
                ((253, 388), (272, 381), (288, 368)),
                ((309, 350), (320, 323), (323, 293)),
            ]},
            Shape::Text { role: Role::Note, at: (180, 78), anchor: Anchor::Middle, text: "left" },
            Shape::Text { role: Role::Note, at: (275, 78), anchor: Anchor::Middle, text: "right" },
            // scroll wheel
            Shape::RoundRect { role: Role::Detail, x: 213, y: 63, w: 30, h: 70, r: 10 },
            Shape::RoundRect { role: Role::Detail, x: 217, y: 68, w: 22, h: 62, r: 8 },
            Shape::Polyline { role: Role::Detail, points: &[(219, 72), (236, 72)] },
            Shape::Polyline { role: Role::Detail, points: &[(218, 80), (237, 80)] },
            Shape::Polyline { role: Role::Detail, points: &[(218, 89), (237, 89)] },
            Shape::Polyline { role: Role::Detail, points: &[(218, 97), (237, 97)] },
            Shape::Polyline { role: Role::Detail, points: &[(218, 106), (237, 106)] },
            Shape::Polyline { role: Role::Detail, points: &[(218, 114), (237, 114)] },
            Shape::Polyline { role: Role::Detail, points: &[(219, 122), (236, 122)] },
            Shape::Polyline { role: Role::Lead, points: &[(243, 98), (332, 98)] },
            Shape::Callout { slot: CalloutSlot::Wheel, at: (348, 95), anchor: Anchor::Start,
                label: "scroll wheel — middle click", note: "", note_role: Role::Note },
            // thumb buttons (drawn as bumps sticking out of the left outline)
            Shape::Path { role: Role::Button, start: (141, 139), closed: true, curves: &[
                ((144, 138), (147, 140), (148, 144)),
                ((150, 183), (150, 183), (150, 183)),
                ((150, 189), (147, 192), (143, 193)),
                ((141, 193), (138, 191), (138, 187)),
                ((138, 146), (138, 146), (138, 146)),
                ((138, 142), (139, 139), (141, 139)),
            ]},
            Shape::Path { role: Role::Button, start: (143, 197), closed: true, curves: &[
                ((146, 197), (148, 199), (148, 204)),
                ((148, 244), (148, 244), (148, 244)),
                ((148, 249), (145, 253), (141, 253)),
                ((138, 252), (136, 249), (136, 245)),
                ((137, 204), (137, 204), (137, 204)),
                ((137, 200), (139, 197), (143, 197)),
            ]},
            Shape::Polyline { role: Role::Detail, points: &[(140, 146), (147, 145)] },
            Shape::Polyline { role: Role::Detail, points: &[(138, 204), (147, 202)] },
            Shape::Polyline { role: Role::Lead, points: &[(138, 166), (126, 166)] },
            Shape::Polyline { role: Role::Lead, points: &[(136, 225), (126, 225)] },
            Shape::Callout { slot: CalloutSlot::ThumbForward, at: (122, 162), anchor: Anchor::End,
                label: "\"forward\"", note: "XBUTTON2", note_role: Role::Note },
            Shape::Callout { slot: CalloutSlot::ThumbBack, at: (122, 221), anchor: Anchor::End,
                label: "\"back\"", note: "XBUTTON1", note_role: Role::Note },
            // hook cost, right at the point of decision (under the thumb
            // callouts) — benefit first, then the cost detail
            Shape::Text { role: Role::Note, at: (122, 255), anchor: Anchor::End, text: "default = no hook, zero cost" },
            Shape::Text { role: Role::Note, at: (122, 271), anchor: Anchor::End, text: "remaps use a global mouse hook" },
            Shape::Text { role: Role::Note, at: (122, 287), anchor: Anchor::End, text: "(~µs per mouse event)" },
        ],
    },
};
