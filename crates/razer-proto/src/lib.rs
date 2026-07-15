//! Pure Razer HID protocol for supported Razer mice (see [`SUPPORTED`]).
//!
//! This contains **no I/O**: it only builds the 90-byte feature reports and parses the
//! 90-byte responses, so it is fully unit-testable. The `razer-hid` crate layers device
//! access on top.
//!
//! Protocol source: OpenRazer (github.com/openrazer/openrazer), files
//! `driver/razercommon.{c,h}`, `driver/razerchromacommon.c`, `driver/razermouse_driver.c`.
//!
//! The Razer control protocol is a 90-byte HID feature report (report ID 0):
//!
//! ```text
//! offset 0     status            (0x00 on request; response: 0x02=OK, 0x01=busy,
//!                                 0x03=failure, 0x04=timeout, 0x05=not supported)
//! offset 1     transaction_id    (per-device: 0x3F on the Elite, 0x1F on the V3)
//! offset 2-3   remaining_packets (big-endian, 0)
//! offset 4     protocol_type     (0)
//! offset 5     data_size
//! offset 6     command_class
//! offset 7     command_id
//! offset 8-87  arguments (80 bytes)
//! offset 88    crc = XOR of bytes 2..87
//! offset 89    reserved (0)
//! ```

#![forbid(unsafe_code)]

pub mod diagram;

use diagram::{Anchor, CalloutSlot, Diagram, Role, Shape};

/// USB vendor id for Razer.
pub const VENDOR_ID: u16 = 0x1532;

/// The vendor codes a mouse's extra DPI buttons emit in driver mode.
///
/// These arrive as input reports on the auxiliary HID collections (first byte
/// `0x04`, then the active codes) and are invisible to Windows otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DpiButtons {
    /// Code emitted by the DPI-up button (the one closer to the wheel).
    pub up: u8,
    /// Code emitted by the DPI-down button.
    pub down: u8,
}

/// Which OpenRazer polling-rate command family a device speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollingProtocol {
    /// One-byte command, class 0x00, id 0x05 set / 0x85 get
    /// (`razer_chroma_misc_set_polling_rate`). Tops out at 1000 Hz.
    Classic,
    /// Two-byte command, class 0x00, id 0x40 set / 0xC0 get
    /// (`razer_chroma_misc_set_polling_rate2`). Reaches 8000 Hz.
    Extended,
}

/// A device's polling-rate capability: the command family plus the rates (Hz)
/// the hardware accepts, per its cases in OpenRazer's `razermouse_driver.c`
/// (`razer_attr_write_polling_rate` / `razer_attr_read_polling_rate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollingSpec {
    pub protocol: PollingProtocol,
    /// Supported rates in Hz, ascending.
    pub rates: &'static [u16],
}

/// Per-device protocol parameters and hardware feature set.
///
/// The Razer control protocol is shared across the mouse family; a `DeviceSpec`
/// captures everything that varies by model, so the upper layers can build
/// correct reports and skip features a device lacks. Adding a device means
/// adding one of these to [`SUPPORTED`] — see `docs/SUPPORTED-DEVICES.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceSpec {
    /// USB product id.
    pub product_id: u16,
    /// transaction_id byte (offset 1 of every report); varies by model
    /// generation. Find it in the device's cases in OpenRazer's
    /// `razermouse_driver.c`.
    pub transaction_id: u8,
    /// Human-readable model name (for logs and the tray tooltip).
    pub name: &'static str,
    /// The device's addressable lighting zones ([`led`] ids), in the order
    /// effects are applied. Empty for devices with no lighting hardware.
    pub rgb_zones: &'static [u8],
    /// The wheel DPI buttons' driver-mode vendor codes, or `None` if the model
    /// has no such buttons (then driver mode and the vendor-code listeners are
    /// skipped entirely).
    pub dpi_buttons: Option<DpiButtons>,
    /// Lowest DPI the sensor accepts.
    pub dpi_min: u16,
    /// Highest DPI the sensor accepts. Drives report validation and the
    /// settings-window slider range.
    pub dpi_max: u16,
    /// Polling-rate command family and the rates the hardware accepts.
    pub polling: PollingSpec,
    /// Top-down schematic of the physical controls, as shape data (see
    /// [`diagram`]). One definition drives both the settings-window rendering
    /// (GDI+) and the generated `docs/assets/<device>.svg`; a drift-check test
    /// regenerates the SVG and fails if the committed asset differs.
    pub diagram: Diagram,
}

impl DeviceSpec {
    /// Whether the device has any addressable lighting.
    pub fn has_rgb(&self) -> bool {
        !self.rgb_zones.is_empty()
    }
}

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
            Shape::Polyline { role: Role::Lead, points: &[(241, 98), (396, 98)] },
            Shape::Callout { slot: CalloutSlot::Wheel, at: (402, 95), anchor: Anchor::Start,
                label: "scroll wheel — middle click", note: "", note_role: Role::Note },
            // Below the wheel's dropdown-mount rect so the window doesn't cover it.
            Shape::Text { role: Role::RgbZone, at: (402, 128), anchor: Anchor::Start, text: "RGB zone 0x01" },
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
            // footnote, wrapped so it never widens the canvas (the thumb-hook
            // disclaimer lives beside the thumb callouts)
            Shape::Text { role: Role::Note, at: (330, 420), anchor: Anchor::Middle, text: "DPI-button remaps are hook-free:" },
            Shape::Text { role: Role::Note, at: (330, 436), anchor: Anchor::Middle, text: "vendor codes in driver mode, pointer motion untouched" },
        ],
    },
};

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

/// Every device Snakecharmer knows how to drive.
pub const SUPPORTED: &[DeviceSpec] = &[DEATHADDER_ELITE, DEATHADDER_V3];

/// Look up the [`DeviceSpec`] for a USB product id, if it's supported.
pub fn spec_for(product_id: u16) -> Option<DeviceSpec> {
    SUPPORTED.iter().copied().find(|s| s.product_id == product_id)
}

/// Length of a Razer control report / response, in bytes.
pub const REPORT_LEN: usize = 90;

/// Response status codes (byte 0 of a response).
pub mod status {
    pub const NEW_COMMAND: u8 = 0x00;
    pub const BUSY: u8 = 0x01;
    pub const SUCCESS: u8 = 0x02;
    pub const FAILURE: u8 = 0x03;
    pub const TIMEOUT: u8 = 0x04;
    pub const NOT_SUPPORTED: u8 = 0x05;
}

/// Device mode values (argument to the set-device-mode command).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DeviceMode {
    /// Factory behavior: the DPI buttons cycle DPI stages internally and send nothing.
    Hardware = 0x00,
    /// Driver mode: the firmware stops consuming the DPI buttons and emits vendor events.
    Driver = 0x03,
}

impl DeviceMode {
    /// Interpret a raw mode byte (as read back from the device).
    pub fn from_byte(b: u8) -> Option<DeviceMode> {
        match b {
            0x00 => Some(DeviceMode::Hardware),
            0x03 => Some(DeviceMode::Driver),
            _ => None,
        }
    }

    pub fn as_byte(self) -> u8 {
        self as u8
    }
}

/// Errors that can arise purely from building/parsing reports (no I/O).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtoError {
    /// More than 80 argument bytes were supplied.
    ArgsTooLong(usize),
    /// A DPI value fell outside the target device's supported range
    /// (`DeviceSpec::dpi_min ..= dpi_max`).
    DpiOutOfRange(u16),
    /// A polling rate the target device's spec (`DeviceSpec::polling`) does not list.
    PollingRateUnsupported { hz: u16, supported: &'static [u16] },
    /// A get-polling-rate response byte outside the known encoding.
    UnknownPollingRate(u8),
    /// A response was not [`REPORT_LEN`] bytes.
    BadResponseLen(usize),
    /// The device reported a non-success status byte.
    DeviceStatus(u8),
    /// The response's command class/id did not echo the request.
    CommandEchoMismatch { sent: (u8, u8), got: (u8, u8) },
    /// A color string was not `#RRGGBB` / `RRGGBB`.
    BadColor(String),
}

impl core::fmt::Display for ProtoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ProtoError::ArgsTooLong(n) => write!(f, "arguments too long: {n} > 80"),
            ProtoError::DpiOutOfRange(v) => write!(f, "DPI {v} out of the device's supported range"),
            ProtoError::PollingRateUnsupported { hz, supported } => {
                let list: Vec<String> = supported.iter().map(|r| r.to_string()).collect();
                write!(f, "polling rate {hz} Hz unsupported (supported: {} Hz)", list.join(", "))
            }
            ProtoError::UnknownPollingRate(b) => write!(f, "unknown polling-rate byte 0x{b:02x}"),
            ProtoError::BadResponseLen(n) => write!(f, "unexpected response length {n} (want {REPORT_LEN})"),
            ProtoError::DeviceStatus(s) => write!(f, "device returned status 0x{s:02x}"),
            ProtoError::CommandEchoMismatch { sent, got } => write!(
                f,
                "response echo mismatch: sent {:02x}/{:02x}, got {:02x}/{:02x}",
                sent.0, sent.1, got.0, got.1
            ),
            ProtoError::BadColor(s) => write!(f, "bad color {s:?} (want #RRGGBB)"),
        }
    }
}

impl std::error::Error for ProtoError {}

/// Compute the Razer CRC: XOR of bytes `2..=87` of the report.
pub fn crc(report: &[u8; REPORT_LEN]) -> u8 {
    report[2..88].iter().fold(0u8, |acc, &b| acc ^ b)
}

/// Build a 90-byte Razer report (with transaction id and CRC filled in).
///
/// Builds the 90-byte control report (per OpenRazer `razercommon.c`).
pub fn build_report(
    transaction_id: u8,
    command_class: u8,
    command_id: u8,
    data_size: u8,
    args: &[u8],
) -> Result<[u8; REPORT_LEN], ProtoError> {
    if args.len() > 80 {
        return Err(ProtoError::ArgsTooLong(args.len()));
    }
    let mut buf = [0u8; REPORT_LEN];
    buf[1] = transaction_id;
    buf[5] = data_size;
    buf[6] = command_class;
    buf[7] = command_id;
    buf[8..8 + args.len()].copy_from_slice(args);
    buf[88] = crc(&buf);
    Ok(buf)
}

// --- Command constructors -------------------------------------------------

/// set device mode: class 0x00, id 0x04, args [mode, 0x00].
pub fn set_device_mode_report(transaction_id: u8, mode: DeviceMode) -> [u8; REPORT_LEN] {
    build_report(transaction_id, 0x00, 0x04, 0x02, &[mode.as_byte(), 0x00]).expect("2 args always valid")
}

/// get device mode: class 0x00, id 0x84.
pub fn get_device_mode_report(transaction_id: u8) -> [u8; REPORT_LEN] {
    build_report(transaction_id, 0x00, 0x84, 0x02, &[]).expect("no args always valid")
}

/// set DPI (xy): class 0x04, id 0x05, args [0x00 (NOSTORE), x_hi, x_lo, y_hi, y_lo, 0, 0].
///
/// `dpi_min`/`dpi_max` bound the accepted values; pass them from the target
/// device's [`DeviceSpec`] so validation matches the actual hardware.
pub fn set_dpi_report(
    transaction_id: u8,
    dpi_min: u16,
    dpi_max: u16,
    dpi_x: u16,
    dpi_y: u16,
) -> Result<[u8; REPORT_LEN], ProtoError> {
    for v in [dpi_x, dpi_y] {
        if !(dpi_min..=dpi_max).contains(&v) {
            return Err(ProtoError::DpiOutOfRange(v));
        }
    }
    let args = [
        0x00, // no variable storage (NOSTORE), per openrazer for this device
        (dpi_x >> 8) as u8,
        (dpi_x & 0xFF) as u8,
        (dpi_y >> 8) as u8,
        (dpi_y & 0xFF) as u8,
        0x00,
        0x00,
    ];
    build_report(transaction_id, 0x04, 0x05, 0x07, &args)
}

/// get DPI (xy): class 0x04, id 0x85.
pub fn get_dpi_report(transaction_id: u8) -> [u8; REPORT_LEN] {
    build_report(transaction_id, 0x04, 0x85, 0x07, &[]).expect("no args always valid")
}

// --- Polling rate -----------------------------------------------------------
//
// Two command families, selected per device by `DeviceSpec::polling`:
//  * Classic (`razer_chroma_misc_{get,set}_polling_rate`): class 0x00,
//    id 0x05/0x85, one rate byte: 0x01=1000, 0x02=500, 0x08=125 Hz.
//  * Extended (`razer_chroma_misc_{get,set}_polling_rate2`): class 0x00,
//    id 0x40/0xC0, set args [0x00, rate]: 0x01=8000, 0x02=4000, 0x04=2000,
//    0x08=1000, 0x10=500, 0x20=250, 0x40=125 Hz; the get response carries the
//    rate byte in arguments[1].

/// Hz -> on-the-wire rate byte for the given command family.
fn polling_rate_byte(protocol: PollingProtocol, hz: u16) -> Option<u8> {
    match (protocol, hz) {
        (PollingProtocol::Classic, 1000) => Some(0x01),
        (PollingProtocol::Classic, 500) => Some(0x02),
        (PollingProtocol::Classic, 125) => Some(0x08),
        (PollingProtocol::Extended, 8000) => Some(0x01),
        (PollingProtocol::Extended, 4000) => Some(0x02),
        (PollingProtocol::Extended, 2000) => Some(0x04),
        (PollingProtocol::Extended, 1000) => Some(0x08),
        (PollingProtocol::Extended, 500) => Some(0x10),
        (PollingProtocol::Extended, 250) => Some(0x20),
        (PollingProtocol::Extended, 125) => Some(0x40),
        _ => None,
    }
}

/// On-the-wire rate byte -> Hz (inverse of [`polling_rate_byte`]).
fn polling_rate_hz(protocol: PollingProtocol, byte: u8) -> Option<u16> {
    let all: &[u16] = match protocol {
        PollingProtocol::Classic => &[125, 500, 1000],
        PollingProtocol::Extended => &[125, 250, 500, 1000, 2000, 4000, 8000],
    };
    all.iter().copied().find(|&hz| polling_rate_byte(protocol, hz) == Some(byte))
}

/// set polling rate: `razer_chroma_misc_set_polling_rate` (classic) or
/// `..._set_polling_rate2` (extended), per `polling.protocol`. `polling.rates`
/// gates `hz`; pass the target device's [`DeviceSpec::polling`].
pub fn set_polling_rate_report(
    transaction_id: u8,
    polling: PollingSpec,
    hz: u16,
) -> Result<[u8; REPORT_LEN], ProtoError> {
    let unsupported = ProtoError::PollingRateUnsupported { hz, supported: polling.rates };
    if !polling.rates.contains(&hz) {
        return Err(unsupported);
    }
    let b = polling_rate_byte(polling.protocol, hz).ok_or(unsupported)?;
    match polling.protocol {
        PollingProtocol::Classic => build_report(transaction_id, 0x00, 0x05, 0x01, &[b]),
        PollingProtocol::Extended => build_report(transaction_id, 0x00, 0x40, 0x02, &[0x00, b]),
    }
}

/// get polling rate: class 0x00, id 0x85 (classic) / 0xC0 (extended).
pub fn get_polling_rate_report(transaction_id: u8, protocol: PollingProtocol) -> [u8; REPORT_LEN] {
    match protocol {
        PollingProtocol::Classic => build_report(transaction_id, 0x00, 0x85, 0x01, &[]),
        PollingProtocol::Extended => build_report(transaction_id, 0x00, 0xC0, 0x01, &[]),
    }
    .expect("no args always valid")
}

/// Parse Hz from a validated get-polling-rate response (classic: byte 8, i.e.
/// arguments[0]; extended: byte 9, i.e. arguments[1]).
pub fn parse_polling_rate(protocol: PollingProtocol, response: &[u8]) -> Result<u16, ProtoError> {
    let b = match protocol {
        PollingProtocol::Classic => response[8],
        PollingProtocol::Extended => response[9],
    };
    polling_rate_hz(protocol, b).ok_or(ProtoError::UnknownPollingRate(b))
}

// --- Chroma / RGB lighting ------------------------------------------------
//
// Supported mice use OpenRazer's *extended matrix effect* family
// (`razer_chroma_extended_matrix_effect_*` in `razerchromacommon.c`; see each
// device's cases in `razermouse_driver.c`). NOTE: these are command_class 0x0F /
// command_id 0x02 — *not* the class-0x03 "standard" LED commands. The base
// builder (`razer_chroma_extended_matrix_effect_base`) lays out:
//   arg[0] = variable_storage (VARSTORE = 0x01)
//   arg[1] = led_id           (SCROLL_WHEEL_LED 0x01 / LOGO_LED 0x04)
//   arg[2] = effect_id
// with a per-effect data_size and trailing arguments.

/// Variable-storage selector (persist to the device's own store).
pub const VARSTORE: u8 = 0x01;
/// Non-persistent storage selector.
pub const NOSTORE: u8 = 0x00;

/// Lighting-zone (`led_id`) values, per OpenRazer. A device's actual zones are
/// listed in its [`DeviceSpec::rgb_zones`].
pub mod led {
    pub const SCROLL_WHEEL: u8 = 0x01;
    pub const LOGO: u8 = 0x04;

    /// Stable config key for a zone (`[zones.<name>]` in config.toml).
    pub const fn name(led_id: u8) -> &'static str {
        match led_id {
            SCROLL_WHEEL => "wheel",
            LOGO => "logo",
            _ => "zone",
        }
    }

    /// Human-readable zone label for UI captions.
    pub const fn label(led_id: u8) -> &'static str {
        match led_id {
            SCROLL_WHEEL => "Wheel",
            LOGO => "Logo",
            _ => "Zone",
        }
    }
}

/// A 24-bit RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }

    /// Parse `#RRGGBB` or `RRGGBB` (case-insensitive).
    pub fn parse_hex(s: &str) -> Result<Rgb, ProtoError> {
        let h = s.trim().trim_start_matches('#');
        if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ProtoError::BadColor(s.to_string()));
        }
        let v = u32::from_str_radix(h, 16).map_err(|_| ProtoError::BadColor(s.to_string()))?;
        Ok(Rgb::new((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }
}

/// The `razer_chroma_extended_matrix_effect_base`: class 0x0F, id 0x02.
fn extended_matrix_effect_base(
    transaction_id: u8,
    arg_size: u8,
    variable_storage: u8,
    led_id: u8,
    effect_id: u8,
) -> [u8; REPORT_LEN] {
    build_report(transaction_id, 0x0F, 0x02, arg_size, &[variable_storage, led_id, effect_id])
        .expect("3 args always valid")
}

/// "None" (off) effect for a zone. `razer_chroma_extended_matrix_effect_none`.
pub fn effect_none_report(transaction_id: u8, led_id: u8) -> [u8; REPORT_LEN] {
    extended_matrix_effect_base(transaction_id, 0x06, VARSTORE, led_id, 0x00)
}

/// "Spectrum" cycling effect for a zone. `..._effect_spectrum`.
pub fn effect_spectrum_report(transaction_id: u8, led_id: u8) -> [u8; REPORT_LEN] {
    extended_matrix_effect_base(transaction_id, 0x06, VARSTORE, led_id, 0x03)
}

/// "Static" single-color effect for a zone. `..._effect_static`.
pub fn effect_static_report(transaction_id: u8, led_id: u8, rgb: Rgb) -> [u8; REPORT_LEN] {
    let mut r = extended_matrix_effect_base(transaction_id, 0x09, VARSTORE, led_id, 0x01);
    // arguments[5]=0x01, arguments[6..9]=RGB  (arguments start at byte offset 8)
    r[8 + 5] = 0x01;
    r[8 + 6] = rgb.r;
    r[8 + 7] = rgb.g;
    r[8 + 8] = rgb.b;
    r[88] = crc(&r);
    r
}

/// "Breathing" (single-color) effect for a zone. `..._effect_breathing_single`.
pub fn effect_breathing_report(transaction_id: u8, led_id: u8, rgb: Rgb) -> [u8; REPORT_LEN] {
    let mut r = extended_matrix_effect_base(transaction_id, 0x09, VARSTORE, led_id, 0x02);
    // arguments[3]=0x01, arguments[5]=0x01, arguments[6..9]=RGB
    r[8 + 3] = 0x01;
    r[8 + 5] = 0x01;
    r[8 + 6] = rgb.r;
    r[8 + 7] = rgb.g;
    r[8 + 8] = rgb.b;
    r[88] = crc(&r);
    r
}

// --- Response validation & parsing ---------------------------------------

/// Validate a raw 90-byte response against the request that produced it.
///
/// Validates the response (per OpenRazer's report handling): correct length,
/// success status, and matching command-class/id echo. Returns the response
/// slice on success so callers can parse arguments from it.
pub fn validate_response<'a>(
    request: &[u8; REPORT_LEN],
    response: &'a [u8],
) -> Result<&'a [u8], ProtoError> {
    if response.len() != REPORT_LEN {
        return Err(ProtoError::BadResponseLen(response.len()));
    }
    let st = response[0];
    if st != status::SUCCESS {
        return Err(ProtoError::DeviceStatus(st));
    }
    if response[6] != request[6] || response[7] != request[7] {
        return Err(ProtoError::CommandEchoMismatch {
            sent: (request[6], request[7]),
            got: (response[6], response[7]),
        });
    }
    Ok(response)
}

/// Parse the device mode from a validated get-device-mode response (byte 8).
pub fn parse_device_mode(response: &[u8]) -> u8 {
    response[8]
}

/// Parse (dpi_x, dpi_y) from a validated get-DPI response (bytes 9..=12).
pub fn parse_dpi(response: &[u8]) -> (u16, u16) {
    let x = ((response[9] as u16) << 8) | response[10] as u16;
    let y = ((response[11] as u16) << 8) | response[12] as u16;
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The DeathAdder Elite's transaction id, used across the byte-exact tests
    /// below (these assertions predate multi-device support and pin the Elite's
    /// on-the-wire bytes; threading its txn keeps them a regression guard).
    const T: u8 = DEATHADDER_ELITE.transaction_id;

    // The SPEC table: driver-mode report is `... 02 00 04 03 00 ... 05 00`.
    #[test]
    fn driver_mode_report_matches_spec() {
        let r = set_device_mode_report(T, DeviceMode::Driver);
        assert_eq!(&r[0..10], &[0x00, 0x3F, 0x00, 0x00, 0x00, 0x02, 0x00, 0x04, 0x03, 0x00]);
        assert_eq!(r[88], 0x05, "driver-mode CRC");
        assert_eq!(r[89], 0x00, "reserved");
        // bytes 10..88 must be zero
        assert!(r[10..88].iter().all(|&b| b == 0));
    }

    #[test]
    fn hardware_mode_report_matches_spec() {
        let r = set_device_mode_report(T, DeviceMode::Hardware);
        assert_eq!(&r[0..10], &[0x00, 0x3F, 0x00, 0x00, 0x00, 0x02, 0x00, 0x04, 0x00, 0x00]);
        assert_eq!(r[88], 0x06, "hardware-mode CRC");
    }

    #[test]
    fn get_mode_report_matches_spec() {
        let r = get_device_mode_report(T);
        assert_eq!(&r[0..10], &[0x00, 0x3F, 0x00, 0x00, 0x00, 0x02, 0x00, 0x84, 0x00, 0x00]);
        assert_eq!(r[88], 0x86, "get-mode CRC");
    }

    /// A synthetic second device for exercising per-device behavior without
    /// depending on any real spec beyond the Elite.
    const TEST_MOUSE: DeviceSpec = DeviceSpec {
        product_id: 0xFFFE,
        transaction_id: 0x1F,
        name: "Test Mouse",
        rgb_zones: &[],
        dpi_buttons: None,
        dpi_min: 100,
        dpi_max: 30000,
        polling: PollingSpec { protocol: PollingProtocol::Classic, rates: &[125, 500, 1000] },
        diagram: Diagram { width: 0, height: 0, shapes: &[] },
    };

    /// The transaction id lives at byte 1, which is *outside* the CRC range
    /// (bytes 2..=87), so the same command on two devices differs in exactly
    /// one byte and shares a CRC.
    #[test]
    fn transaction_id_is_the_only_per_device_difference() {
        let e = DEATHADDER_ELITE;
        let t = TEST_MOUSE;
        let elite = set_dpi_report(e.transaction_id, e.dpi_min, e.dpi_max, 1600, 1600).unwrap();
        let other = set_dpi_report(t.transaction_id, t.dpi_min, t.dpi_max, 1600, 1600).unwrap();
        assert_eq!(elite[1], 0x3F);
        assert_eq!(other[1], 0x1F);
        assert_eq!(&elite[2..], &other[2..], "only byte 1 (txn) may differ");
        assert_eq!(elite[88], other[88], "txn is outside the CRC, so CRC is identical");
    }

    #[test]
    #[allow(clippy::needless_range_loop)] // mirror openrazer's `for(i=2;i<88;i++)` verbatim
    fn crc_is_xor_of_bytes_2_to_87() {
        // Manual reference computation for the driver-mode report.
        let r = set_device_mode_report(T, DeviceMode::Driver);
        let mut expected = 0u8;
        for i in 2..88 {
            expected ^= r[i];
        }
        assert_eq!(crc(&r), expected);
        assert_eq!(r[88], expected);
    }

    #[test]
    fn set_dpi_report_bytes() {
        // 1600 = 0x0640
        let r = set_dpi_report(T, 100, 16000, 1600, 1600).unwrap();
        assert_eq!(&r[0..8], &[0x00, 0x3F, 0x00, 0x00, 0x00, 0x07, 0x04, 0x05]);
        // args: NOSTORE, x_hi, x_lo, y_hi, y_lo, 0, 0
        assert_eq!(&r[8..15], &[0x00, 0x06, 0x40, 0x06, 0x40, 0x00, 0x00]);
        // CRC self-consistency
        assert_eq!(r[88], crc(&r));
    }

    #[test]
    fn set_dpi_asymmetric() {
        let r = set_dpi_report(T, 100, 16000, 1600, 800).unwrap();
        assert_eq!(&r[9..13], &[0x06, 0x40, 0x03, 0x20]); // 0x0640, 0x0320
    }

    #[test]
    fn get_dpi_report_bytes() {
        let r = get_dpi_report(T);
        assert_eq!(&r[0..8], &[0x00, 0x3F, 0x00, 0x00, 0x00, 0x07, 0x04, 0x85]);
        assert_eq!(r[88], crc(&r));
    }

    #[test]
    fn dpi_range_is_enforced() {
        // Elite range (100..=16000): reject below/above, accept the endpoints.
        let (lo, hi) = (DEATHADDER_ELITE.dpi_min, DEATHADDER_ELITE.dpi_max);
        assert_eq!(set_dpi_report(T, lo, hi, 50, 50), Err(ProtoError::DpiOutOfRange(50)));
        assert_eq!(set_dpi_report(T, lo, hi, 20000, 1600), Err(ProtoError::DpiOutOfRange(20000)));
        assert!(set_dpi_report(T, lo, hi, 100, 100).is_ok());
        assert!(set_dpi_report(T, lo, hi, 16000, 16000).is_ok());
    }

    /// DeathAdder V3: transaction id 0x1F, and the Focus Pro sensor's 30000
    /// ceiling permits what the Elite's range rejects.
    #[test]
    fn deathadder_v3_spec() {
        let v = DEATHADDER_V3;
        let r = set_dpi_report(v.transaction_id, v.dpi_min, v.dpi_max, 1600, 1600).unwrap();
        assert_eq!(r[1], 0x1F, "V3 transaction id");
        assert!(set_dpi_report(v.transaction_id, v.dpi_min, v.dpi_max, 30000, 30000).is_ok());
        assert_eq!(
            set_dpi_report(v.transaction_id, v.dpi_min, v.dpi_max, 30001, 30001),
            Err(ProtoError::DpiOutOfRange(30001))
        );
        assert!(!v.has_rgb(), "V3 has no lighting hardware");
        assert!(v.dpi_buttons.is_none(), "V3 has no wheel DPI buttons");
    }

    #[test]
    fn args_too_long_rejected() {
        let big = [0u8; 81];
        assert_eq!(build_report(T, 0x00, 0x00, 0x00, &big), Err(ProtoError::ArgsTooLong(81)));
    }

    #[test]
    fn validate_and_parse_device_mode() {
        let req = get_device_mode_report(T);
        let mut resp = [0u8; REPORT_LEN];
        resp[0] = status::SUCCESS;
        resp[6] = 0x00;
        resp[7] = 0x84;
        resp[8] = 0x03; // driver
        let v = validate_response(&req, &resp).unwrap();
        assert_eq!(parse_device_mode(v), 0x03);
        assert_eq!(DeviceMode::from_byte(parse_device_mode(v)), Some(DeviceMode::Driver));
    }

    #[test]
    fn validate_and_parse_dpi() {
        let req = get_dpi_report(T);
        let mut resp = [0u8; REPORT_LEN];
        resp[0] = status::SUCCESS;
        resp[6] = 0x04;
        resp[7] = 0x85;
        resp[9] = 0x06;
        resp[10] = 0x40; // x = 1600
        resp[11] = 0x03;
        resp[12] = 0x20; // y = 800
        let v = validate_response(&req, &resp).unwrap();
        assert_eq!(parse_dpi(v), (1600, 800));
    }

    // --- Polling rate: byte-exact against OpenRazer razerchromacommon.c -----

    #[test]
    fn set_polling_rate_classic_bytes() {
        // razer_chroma_misc_set_polling_rate: class 0x00, id 0x05, args [rate].
        let p = DEATHADDER_ELITE.polling;
        let r = set_polling_rate_report(T, p, 500).unwrap();
        assert_eq!(&r[0..9], &[0x00, 0x3F, 0x00, 0x00, 0x00, 0x01, 0x00, 0x05, 0x02]);
        assert!(r[9..88].iter().all(|&b| b == 0));
        assert_eq!(r[88], crc(&r));
        // Full classic map: 0x01=1000, 0x02=500, 0x08=125.
        assert_eq!(set_polling_rate_report(T, p, 1000).unwrap()[8], 0x01);
        assert_eq!(set_polling_rate_report(T, p, 125).unwrap()[8], 0x08);
    }

    #[test]
    fn get_polling_rate_classic_bytes() {
        // razer_chroma_misc_get_polling_rate: class 0x00, id 0x85, data_size 0x01.
        let r = get_polling_rate_report(T, PollingProtocol::Classic);
        assert_eq!(&r[0..8], &[0x00, 0x3F, 0x00, 0x00, 0x00, 0x01, 0x00, 0x85]);
        assert_eq!(r[88], crc(&r));
    }

    #[test]
    fn set_polling_rate_extended_bytes() {
        // razer_chroma_misc_set_polling_rate2: class 0x00, id 0x40, args
        // [argument=0x00, rate] (the V3's case passes argument 0x00).
        let v = DEATHADDER_V3;
        let r = set_polling_rate_report(v.transaction_id, v.polling, 1000).unwrap();
        assert_eq!(&r[0..10], &[0x00, 0x1F, 0x00, 0x00, 0x00, 0x02, 0x00, 0x40, 0x00, 0x08]);
        assert!(r[10..88].iter().all(|&b| b == 0));
        assert_eq!(r[88], crc(&r));
        // Full extended map.
        for (hz, b) in [(8000u16, 0x01u8), (4000, 0x02), (2000, 0x04), (500, 0x10), (125, 0x40)] {
            assert_eq!(set_polling_rate_report(v.transaction_id, v.polling, hz).unwrap()[9], b);
        }
    }

    #[test]
    fn get_polling_rate_extended_bytes() {
        // razer_chroma_misc_get_polling_rate2: class 0x00, id 0xC0, data_size 0x01.
        let r = get_polling_rate_report(DEATHADDER_V3.transaction_id, PollingProtocol::Extended);
        assert_eq!(&r[0..8], &[0x00, 0x1F, 0x00, 0x00, 0x00, 0x01, 0x00, 0xC0]);
        assert_eq!(r[88], crc(&r));
    }

    #[test]
    fn unsupported_polling_rates_rejected() {
        let e = DEATHADDER_ELITE;
        assert_eq!(
            set_polling_rate_report(e.transaction_id, e.polling, 2000),
            Err(ProtoError::PollingRateUnsupported { hz: 2000, supported: e.polling.rates })
        );
        let v = DEATHADDER_V3;
        assert!(matches!(
            set_polling_rate_report(v.transaction_id, v.polling, 750),
            Err(ProtoError::PollingRateUnsupported { hz: 750, .. })
        ));
        // 250 Hz encodes on the wire (0x20) but neither device lists it.
        assert!(set_polling_rate_report(v.transaction_id, v.polling, 250).is_err());
    }

    #[test]
    fn validate_and_parse_polling_rate() {
        // Classic: rate byte at arguments[0] (byte 8).
        let req = get_polling_rate_report(T, PollingProtocol::Classic);
        let mut resp = [0u8; REPORT_LEN];
        resp[0] = status::SUCCESS;
        resp[6] = 0x00;
        resp[7] = 0x85;
        resp[8] = 0x01; // 1000 Hz
        let v = validate_response(&req, &resp).unwrap();
        assert_eq!(parse_polling_rate(PollingProtocol::Classic, v), Ok(1000));

        // Extended: rate byte at arguments[1] (byte 9).
        let req = get_polling_rate_report(0x1F, PollingProtocol::Extended);
        let mut resp = [0u8; REPORT_LEN];
        resp[0] = status::SUCCESS;
        resp[6] = 0x00;
        resp[7] = 0xC0;
        resp[9] = 0x01; // 8000 Hz
        let v = validate_response(&req, &resp).unwrap();
        assert_eq!(parse_polling_rate(PollingProtocol::Extended, v), Ok(8000));

        // A byte outside either encoding is an error, not a guess.
        assert_eq!(
            parse_polling_rate(PollingProtocol::Classic, &[0u8; REPORT_LEN]),
            Err(ProtoError::UnknownPollingRate(0x00))
        );
    }

    /// Pin each device's polling capability to its OpenRazer cases
    /// (`razer_attr_write_polling_rate` uses the classic command on the Elite,
    /// `..._polling_rate2` on the V3).
    #[test]
    fn polling_specs_match_openrazer() {
        assert_eq!(
            DEATHADDER_ELITE.polling,
            PollingSpec { protocol: PollingProtocol::Classic, rates: &[125, 500, 1000] }
        );
        assert_eq!(
            DEATHADDER_V3.polling,
            PollingSpec {
                protocol: PollingProtocol::Extended,
                rates: &[125, 500, 1000, 2000, 4000, 8000],
            }
        );
    }

    // --- Chroma tests: assert exact bytes against OpenRazer doc examples ----

    #[test]
    fn chroma_common_header() {
        // All effects: class 0x0F, id 0x02, txn 0x3F, arg[0]=VARSTORE.
        let r = effect_spectrum_report(T, led::SCROLL_WHEEL);
        assert_eq!(r[0], 0x00); // status
        assert_eq!(r[1], 0x3F); // transaction id
        assert_eq!(r[6], 0x0F); // command class
        assert_eq!(r[7], 0x02); // command id
        assert_eq!(r[8], VARSTORE);
    }

    #[test]
    fn chroma_spectrum_matches_openrazer() {
        // Doc: data_size 06, args 01 <led> 03 00 00 00
        let r = effect_spectrum_report(T, led::LOGO);
        assert_eq!(r[5], 0x06); // data_size
        assert_eq!(&r[8..14], &[VARSTORE, led::LOGO, 0x03, 0x00, 0x00, 0x00]);
        assert_eq!(r[88], crc(&r));
    }

    #[test]
    fn chroma_none_matches_openrazer() {
        // Doc: 010500000000 (data_size 06)
        let r = effect_none_report(T, led::SCROLL_WHEEL);
        assert_eq!(r[5], 0x06);
        assert_eq!(&r[8..14], &[VARSTORE, led::SCROLL_WHEEL, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(r[88], crc(&r));
    }

    #[test]
    fn chroma_static_red_matches_openrazer() {
        // Doc pattern: data_size 09, args 01 <led> 01 00 00 01 RR GG BB
        let r = effect_static_report(T, led::SCROLL_WHEEL, Rgb::new(0xFF, 0x00, 0x00));
        assert_eq!(r[5], 0x09);
        assert_eq!(
            &r[8..17],
            &[VARSTORE, led::SCROLL_WHEEL, 0x01, 0x00, 0x00, 0x01, 0xFF, 0x00, 0x00]
        );
        assert_eq!(r[88], crc(&r));
    }

    #[test]
    fn chroma_breathing_green_matches_openrazer() {
        // Doc: 01 05 02 01 00 01 00 ff 00 (data_size 09)
        let r = effect_breathing_report(T, led::LOGO, Rgb::new(0x00, 0xFF, 0x00));
        assert_eq!(r[5], 0x09);
        assert_eq!(
            &r[8..17],
            &[VARSTORE, led::LOGO, 0x02, 0x01, 0x00, 0x01, 0x00, 0xFF, 0x00]
        );
        assert_eq!(r[88], crc(&r));
    }

    #[test]
    fn rgb_hex_parsing() {
        assert_eq!(Rgb::parse_hex("#FF8800"), Ok(Rgb::new(0xFF, 0x88, 0x00)));
        assert_eq!(Rgb::parse_hex("00ff00"), Ok(Rgb::new(0, 255, 0)));
        assert!(Rgb::parse_hex("#12345").is_err());
        assert!(Rgb::parse_hex("gggggg").is_err());
    }

    /// Not a test: regenerates `docs/assets/<device>.svg` from each spec's
    /// diagram data. Run it after changing a diagram:
    ///   cargo test -p razer-proto -- --ignored regenerate_diagram_svgs
    #[test]
    #[ignore = "writes docs/assets; run explicitly after editing a diagram"]
    fn regenerate_diagram_svgs() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/assets");
        std::fs::create_dir_all(dir).expect("create docs/assets");
        for spec in SUPPORTED {
            let path = format!("{dir}/{}.svg", diagram::asset_slug(spec.name));
            std::fs::write(&path, spec.diagram.to_svg()).expect("write SVG asset");
            println!("wrote {path}");
        }
    }

    /// docs/SUPPORTED-DEVICES.md is the human-readable twin of [`SUPPORTED`];
    /// this keeps the two from drifting. If it fails, update the doc table.
    #[test]
    fn supported_devices_doc_matches_table() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/SUPPORTED-DEVICES.md");
        let doc = std::fs::read_to_string(path)
            .expect("docs/SUPPORTED-DEVICES.md must exist")
            .replace("\r\n", "\n"); // normalize CRLF checkouts before verbatim matching

        for spec in SUPPORTED {
            let pid = format!("1532:{:04X}", spec.product_id);
            let row = doc
                .lines()
                .find(|l| l.starts_with('|') && l.contains(&pid))
                .unwrap_or_else(|| {
                    panic!("docs/SUPPORTED-DEVICES.md has no row for {} (`{pid}`) — add one", spec.name)
                });
            for (what, needle) in [
                ("model name", spec.name.to_string()),
                ("transaction id", format!("0x{:02X}", spec.transaction_id)),
                ("DPI range", format!("{}–{}", spec.dpi_min, spec.dpi_max)),
                (
                    "polling rates",
                    spec.polling
                        .rates
                        .iter()
                        .map(|r| r.to_string())
                        .collect::<Vec<_>>()
                        .join("/"),
                ),
            ] {
                assert!(
                    row.contains(&needle),
                    "doc row for {pid} is missing the {what} {needle:?}:\n  {row}"
                );
            }

            // The button-map diagram is spec data (rendered in the settings
            // window). The doc embeds its generated SVG; the file must match
            // the emitter's output byte for byte so doc and UI can't drift.
            assert!(
                !spec.diagram.shapes.is_empty(),
                "{} has no diagram — every supported device ships one",
                spec.name
            );
            let slug = diagram::asset_slug(spec.name);
            assert!(
                doc.contains(&format!("assets/{slug}.svg")),
                "docs/SUPPORTED-DEVICES.md 'Button maps' section must embed assets/{slug}.svg for {}",
                spec.name
            );
            let asset_path = format!(
                "{}/../../docs/assets/{slug}.svg",
                env!("CARGO_MANIFEST_DIR")
            );
            let on_disk = std::fs::read_to_string(&asset_path)
                .unwrap_or_default()
                .replace("\r\n", "\n");
            assert!(
                on_disk == spec.diagram.to_svg(),
                "docs/assets/{slug}.svg is stale or missing — regenerate it with:\n  \
                 cargo test -p razer-proto -- --ignored regenerate_diagram_svgs"
            );
        }

        // No stale rows either: every device row must correspond to a spec.
        let rows = doc.lines().filter(|l| l.starts_with('|') && l.contains("1532:")).count();
        assert_eq!(
            rows,
            SUPPORTED.len(),
            "docs/SUPPORTED-DEVICES.md has {rows} device rows but SUPPORTED has {} — remove or add the difference",
            SUPPORTED.len()
        );
    }

    #[test]
    fn validate_rejects_bad_status_and_echo() {
        let req = get_device_mode_report(T);
        let mut resp = [0u8; REPORT_LEN];
        resp[0] = status::BUSY;
        assert_eq!(validate_response(&req, &resp), Err(ProtoError::DeviceStatus(status::BUSY)));

        resp[0] = status::SUCCESS;
        resp[6] = 0x04; // wrong class
        resp[7] = 0x84;
        assert!(matches!(
            validate_response(&req, &resp),
            Err(ProtoError::CommandEchoMismatch { .. })
        ));

        assert_eq!(validate_response(&req, &resp[..10]), Err(ProtoError::BadResponseLen(10)));
    }
}
