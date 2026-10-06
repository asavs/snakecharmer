//! On-board keymap: the button binds a mouse stores in its own memory.
//!
//! Not from OpenRazer, which has no key remapping. Decoded from USB captures of
//! Synapse 4 on the DeathAdder V3; the evidence and the open questions are in
//! `docs/ONBOARD-KEYMAP.md`. This module only builds the **read**, sent exactly
//! as Synapse sends it. There is deliberately no write constructor until a write
//! path is confirmed and approved.
//!
//! Read: class `0x02`, id `0x8C`, data_size `0x0A`, args `[0x01, button, 0x00]`.
//! The reply carries the stored bind:
//!
//! ```text
//! [8] 0x01  [9] button  [10] ?  [11] category  [12] length  [13..] payload
//!   category 0x00 disabled   length 0
//!   category 0x01 mouse      length 1   [mouse function]
//!   category 0x02 keyboard   length 2   [modifier bitmask, HID usage]
//! ```

use crate::{build_report, ProtoError, REPORT_LEN};

/// Command class of the keymap commands.
pub const CLASS: u8 = 0x02;
/// Read one button's bind (the write is `0x0C`; Razer sets the high bit on reads).
pub const READ_ID: u8 = 0x8C;
const DATA_SIZE: u8 = 0x0A;
/// `[8]`: `0x01` in every captured read and write. Its meaning is unconfirmed.
const ARG0: u8 = 0x01;

/// A mouse whose keymap read is recorded in `docs/ONBOARD-KEYMAP.md`, and the
/// button ids Synapse was seen querying on it. Nothing outside this list is
/// ever sent: probing other ids would be fuzzing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeymapSpec {
    pub product_id: u16,
    pub buttons: &'static [(u8, &'static str)],
}

/// Mice with a recorded keymap read. Kept apart from `DeviceSpec` while this is
/// read-only research, so device ports stay one-file diffs.
pub const SUPPORTED: &[KeymapSpec] = &[KeymapSpec {
    product_id: 0x00B2, // DeathAdder V3
    buttons: &[
        (0x01, "left"),
        (0x02, "right"),
        (0x03, "wheel click"),
        (0x04, "back"),
        (0x05, "forward"),
        (0x09, "wheel up"),
        (0x0A, "wheel down"),
        (0x60, "underside"),
    ],
}];

pub fn spec_for(product_id: u16) -> Option<&'static KeymapSpec> {
    SUPPORTED.iter().find(|s| s.product_id == product_id)
}

/// The read for one button. Synapse numbers its transactions; the device's
/// usual transaction id is used here, as for every other command.
pub fn read_binding_report(transaction_id: u8, button: u8) -> [u8; REPORT_LEN] {
    build_report(transaction_id, CLASS, READ_ID, DATA_SIZE, &[ARG0, button, 0x00])
        .expect("3 args always valid")
}

/// One button's stored bind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Binding {
    Disabled,
    /// A mouse function. On the buttons captured so far the default is the
    /// button's own id: back (`0x04`) defaults to function `0x04`.
    Mouse(u8),
    /// A key: HID modifier bitmask and HID keyboard usage.
    Key { modifiers: u8, usage: u8 },
    /// A category not decoded yet (`0x06` on the V3's underside button).
    Unknown { category: u8, payload: Vec<u8> },
}

impl Binding {
    /// The factory bind for `button`, where a capture established it.
    pub fn default_for(button: u8) -> Option<Binding> {
        matches!(button, 0x01..=0x05 | 0x09 | 0x0A).then_some(Binding::Mouse(button))
    }
}

/// Parse a validated read reply for `button`.
pub fn parse_binding(response: &[u8], button: u8) -> Result<Binding, ProtoError> {
    if response[9] != button {
        return Err(ProtoError::KeymapButtonMismatch { sent: button, got: response[9] });
    }
    let category = response[11];
    let len = response[12] as usize;
    if 13 + len > 88 {
        return Err(ProtoError::BadResponseLen(len));
    }
    let payload = &response[13..13 + len];
    Ok(match (category, payload) {
        (0x00, _) => Binding::Disabled,
        (0x01, [f]) => Binding::Mouse(*f),
        (0x02, [m, u]) => Binding::Key { modifiers: *m, usage: *u },
        _ => Binding::Unknown { category, payload: payload.to_vec() },
    })
}

impl core::fmt::Display for Binding {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Binding::Disabled => write!(f, "disabled"),
            Binding::Mouse(m) => write!(f, "{}", mouse_function_name(*m)),
            Binding::Key { modifiers, usage } => {
                for (bit, name) in MODIFIERS {
                    if modifiers & bit != 0 {
                        write!(f, "{name}+")?;
                    }
                }
                write!(f, "{}", usage_name(*usage))
            }
            Binding::Unknown { category, payload } => {
                let hex: Vec<String> = payload.iter().map(|b| format!("{b:02x}")).collect();
                write!(f, "undecoded (category 0x{category:02x}: {})", hex.join(" "))
            }
        }
    }
}

/// HID modifier bitmask, in the order people write shortcuts. Only left Ctrl
/// (`0x01`) has been captured; the rest is the standard HID layout.
const MODIFIERS: [(u8, &str); 8] = [
    (0x01, "Ctrl"),
    (0x10, "RCtrl"),
    (0x02, "Shift"),
    (0x20, "RShift"),
    (0x04, "Alt"),
    (0x40, "RAlt"),
    (0x08, "Win"),
    (0x80, "RWin"),
];

fn mouse_function_name(m: u8) -> String {
    match m {
        0x01 => "left click".into(),
        0x02 => "right click".into(),
        0x03 => "middle click".into(),
        0x04 => "back (mouse button 4)".into(),
        0x05 => "forward (mouse button 5)".into(),
        0x09 => "scroll up".into(),
        0x0A => "scroll down".into(),
        other => format!("mouse function 0x{other:02x}"),
    }
}

/// HID keyboard usage (USB HID Usage Tables, page 0x07) to a key name.
fn usage_name(u: u8) -> String {
    match u {
        0x04..=0x1D => ((b'A' + (u - 0x04)) as char).to_string(),
        0x1E..=0x26 => ((b'1' + (u - 0x1E)) as char).to_string(),
        0x27 => "0".into(),
        0x28 => "Enter".into(),
        0x29 => "Esc".into(),
        0x2A => "Backspace".into(),
        0x2B => "Tab".into(),
        0x2C => "Space".into(),
        0x3A..=0x45 => format!("F{}", u - 0x3A + 1),
        0x68..=0x73 => format!("F{}", u - 0x68 + 13),
        other => format!("key 0x{other:02x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{crc, status};

    /// A reply as the V3 sent it: status 0x02, the read's header, these args.
    fn reply(args: &[u8]) -> [u8; REPORT_LEN] {
        let mut r = build_report(0x1F, CLASS, READ_ID, DATA_SIZE, args).unwrap();
        r[0] = status::SUCCESS;
        r
    }

    #[test]
    fn read_matches_capture() {
        // Synapse 4 launch capture: class 0x02 id 0x8C size 10 | 01 04 00 00 00 00 00 00 00 00
        let r = read_binding_report(0x1F, 0x04);
        assert_eq!(&r[5..8], &[0x0A, 0x02, 0x8C]);
        assert_eq!(&r[8..18], &[0x01, 0x04, 0x00, 0, 0, 0, 0, 0, 0, 0]);
        assert!(r[18..88].iter().all(|&b| b == 0));
        assert_eq!(r[88], crc(&r));
    }

    #[test]
    fn parses_captured_replies() {
        // Launch read replies (defaults) and session acks, which share the layout.
        let cases: [(&[u8], u8, Binding, &str); 7] = [
            (&[0x01, 0x04, 0x00, 0x01, 0x01, 0x04], 0x04, Binding::Mouse(4), "back (mouse button 4)"),
            (&[0x01, 0x01, 0x01, 0x01, 0x01, 0x01], 0x01, Binding::Mouse(1), "left click"),
            (&[0x01, 0x04, 0x00, 0x02, 0x02, 0x00, 0x1E], 0x04,
             Binding::Key { modifiers: 0, usage: 0x1E }, "1"),
            (&[0x01, 0x04, 0x00, 0x02, 0x02, 0x01, 0x06], 0x04,
             Binding::Key { modifiers: 0x01, usage: 0x06 }, "Ctrl+C"),
            (&[0x01, 0x04, 0x00, 0x01, 0x01, 0x03], 0x04, Binding::Mouse(3), "middle click"),
            (&[0x01, 0x04, 0x00, 0x00, 0x00], 0x04, Binding::Disabled, "disabled"),
            (&[0x01, 0x60, 0x00, 0x06, 0x01, 0x06], 0x60,
             Binding::Unknown { category: 0x06, payload: vec![0x06] }, "undecoded (category 0x06: 06)"),
        ];
        for (args, button, want, shown) in cases {
            let got = parse_binding(&reply(args), button).unwrap();
            assert_eq!(got, want);
            assert_eq!(got.to_string(), shown);
        }
    }

    #[test]
    fn leftover_binds_are_not_default() {
        // The V3's binds before Synapse 4 reset them: back typed 0, wheel click 8.
        assert_eq!(usage_name(0x27), "0");
        assert_eq!(usage_name(0x25), "8");
        assert_ne!(Some(Binding::Key { modifiers: 0, usage: 0x27 }), Binding::default_for(0x04));
        assert_eq!(Binding::default_for(0x04), Some(Binding::Mouse(4)));
        assert_eq!(Binding::default_for(0x60), None, "underside default not established");
    }

    #[test]
    fn reply_for_another_button_is_rejected() {
        let e = parse_binding(&reply(&[0x01, 0x05, 0x00, 0x01, 0x01, 0x05]), 0x04);
        assert_eq!(e, Err(ProtoError::KeymapButtonMismatch { sent: 0x04, got: 0x05 }));
    }
}
