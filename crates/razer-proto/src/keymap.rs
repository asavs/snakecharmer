//! On-board keymap: the button binds a mouse stores in its own memory.
//!
//! Not from OpenRazer, which has no key remapping. Decoded from USB captures of
//! Synapse 4 on the DeathAdder V3; the evidence and the open questions are in
//! `docs/ONBOARD-KEYMAP.md`. Both commands are sent as Synapse sends them, and a
//! write only goes to a button whose write was captured (`KeymapSpec::writable`).
//!
//! Read: class `0x02`, id `0x8C`, data_size `0x0A`, args `[0x01, button, 0x00]`.
//! Write: class `0x02`, id `0x0C`, data_size `0x0A`, args in the reply layout
//! with `[10] = 0x00`, as in every captured write. The reply to either carries
//! the stored bind:
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
/// Read one button's bind (Razer sets the high bit on reads).
pub const READ_ID: u8 = 0x8C;
/// Write one button's bind to on-board memory. It survives a power cycle.
pub const WRITE_ID: u8 = 0x0C;
const DATA_SIZE: u8 = 0x0A;
/// `[8]`: `0x01` in every captured read and write. Its meaning is unconfirmed.
const ARG0: u8 = 0x01;

/// A mouse whose keymap is recorded in `docs/ONBOARD-KEYMAP.md`: the button ids
/// Synapse was seen querying on it, and the ones it was seen writing. Nothing
/// outside these lists is ever sent: probing other ids would be fuzzing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeymapSpec {
    pub product_id: u16,
    pub buttons: &'static [(u8, &'static str)],
    /// Buttons a capture recorded Synapse writing. Only these are written.
    pub writable: &'static [u8],
}

/// Mice with a recorded keymap. Kept apart from `DeviceSpec` so device ports
/// stay one-file diffs.
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
    // Sessions 2026-10-06 (back, forward) and 2026-10-16 (the rest). Left and
    // the underside DPI button have no captured write.
    writable: &[0x02, 0x03, 0x04, 0x05, 0x09, 0x0A],
}];

pub fn spec_for(product_id: u16) -> Option<&'static KeymapSpec> {
    SUPPORTED.iter().find(|s| s.product_id == product_id)
}

impl KeymapSpec {
    /// Find a button by name, forgiving case, spaces, `-` and `_`
    /// (`wheel-click`, `WheelClick`, `wheel click`).
    pub fn button(&self, name: &str) -> Option<(u8, &'static str)> {
        let want = squash(name);
        self.buttons.iter().copied().find(|(_, n)| squash(n) == want)
    }

    pub fn is_writable(&self, button: u8) -> bool {
        self.writable.contains(&button)
    }
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| !matches!(c, ' ' | '-' | '_')).flat_map(char::to_lowercase).collect()
}

/// The read for one button. Synapse numbers its transactions; the device's
/// usual transaction id is used here, as for every other command.
pub fn read_binding_report(transaction_id: u8, button: u8) -> [u8; REPORT_LEN] {
    build_report(transaction_id, CLASS, READ_ID, DATA_SIZE, &[ARG0, button, 0x00])
        .expect("3 args always valid")
}

/// The write for one button. Refuses a button whose write wasn't captured, an
/// undecoded category, a mouse function never seen, and a key outside the
/// named list.
pub fn write_binding_report(
    km: &KeymapSpec,
    transaction_id: u8,
    button: u8,
    bind: &Binding,
) -> Result<[u8; REPORT_LEN], ProtoError> {
    if !km.is_writable(button) {
        return Err(ProtoError::KeymapNotWritable(format!(
            "no captured write for button 0x{button:02x}"
        )));
    }
    let tail: Vec<u8> = match bind {
        Binding::Disabled => vec![0x00, 0x00],
        Binding::Mouse(f) if MOUSE_FUNCTIONS.contains(f) => vec![0x01, 0x01, *f],
        Binding::Key { modifiers, usage } if usage_name(*usage).is_some() => {
            vec![0x02, 0x02, *modifiers, *usage]
        }
        other => return Err(ProtoError::KeymapNotWritable(other.to_string())),
    };
    let mut args = vec![ARG0, button, 0x00];
    args.extend_from_slice(&tail);
    Ok(build_report(transaction_id, CLASS, WRITE_ID, DATA_SIZE, &args).expect("at most 7 args"))
}

/// Mouse functions seen in a capture or a read reply.
const MOUSE_FUNCTIONS: [u8; 7] = [0x01, 0x02, 0x03, 0x04, 0x05, 0x09, 0x0A];

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
    /// The default bind for `button`, where a capture established it.
    pub fn default_for(button: u8) -> Option<Binding> {
        matches!(button, 0x01..=0x05 | 0x09 | 0x0A).then_some(Binding::Mouse(button))
    }
}

/// Parse a validated read or write reply for `button`.
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

/// Parse a bind as people write it, for `button`:
///
/// * `default`, `disabled`
/// * a mouse function: `left click`, `right click`, `middle click`, `back`,
///   `forward`, `scroll up`, `scroll down`
/// * a key or chord: `a`, `1`, `f5`, `enter`, `ctrl+c`, `ctrl+shift+t`, `key:a`,
///   and the daemon's `copy` / `paste` / `cut`
///
/// What `Display` prints parses back to the same bind.
pub fn parse_bind(spec: &str, button: u8) -> Result<Binding, String> {
    let lower = spec.trim().to_ascii_lowercase();
    let s = lower.strip_prefix("key:").unwrap_or(&lower);
    let mouse = |f| Ok(Binding::Mouse(f));
    let ctrl = |usage| Ok(Binding::Key { modifiers: 0x01, usage });
    match s {
        "default" => {
            return Binding::default_for(button).ok_or_else(|| "this button has no known default".into())
        }
        "disabled" | "off" => return Ok(Binding::Disabled),
        "left click" => return mouse(0x01),
        "right click" => return mouse(0x02),
        "middle click" | "wheel click" => return mouse(0x03),
        "back" | "back (mouse button 4)" => return mouse(0x04),
        "forward" | "forward (mouse button 5)" => return mouse(0x05),
        "scroll up" => return mouse(0x09),
        "scroll down" => return mouse(0x0A),
        "copy" => return ctrl(0x06),
        "paste" => return ctrl(0x19),
        "cut" => return ctrl(0x1B),
        _ => {}
    }
    // A lone "+" or "-" is a key, not a chord separator.
    let parts: Vec<&str> = if s.len() > 1 { s.split('+').map(str::trim).collect() } else { vec![s] };
    let (key, mods) = parts.split_last().ok_or("empty bind")?;
    let mut modifiers = 0u8;
    for m in mods {
        modifiers |= match *m {
            "ctrl" | "control" => 0x01,
            "shift" => 0x02,
            "alt" => 0x04,
            "win" => 0x08,
            other => return Err(format!("unknown modifier {other:?}")),
        };
    }
    let usage = usage_for(key).ok_or_else(|| format!("unknown key {key:?}"))?;
    Ok(Binding::Key { modifiers, usage })
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
                match usage_name(*usage) {
                    Some(n) => write!(f, "{n}"),
                    None => write!(f, "key 0x{usage:02x}"),
                }
            }
            Binding::Unknown { category, payload } => {
                let hex: Vec<String> = payload.iter().map(|b| format!("{b:02x}")).collect();
                write!(f, "undecoded (category 0x{category:02x}: {})", hex.join(" "))
            }
        }
    }
}

/// HID modifier bitmask, in the order people write shortcuts. Only left Ctrl
/// (`0x01`) has been captured; the rest is the standard HID layout. The parser
/// only produces the left-hand ones.
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

/// HID keyboard usage (USB HID Usage Tables, page 0x07) to a key name: every
/// key on a standard US keyboard. Only keys named here can be written. The
/// names are what the settings window's key recorder produces
/// (`platform::key_name`), so none contains a `+`.
fn usage_name(u: u8) -> Option<String> {
    Some(match u {
        0x04..=0x1D => ((b'A' + (u - 0x04)) as char).to_string(),
        0x1E..=0x26 => ((b'1' + (u - 0x1E)) as char).to_string(),
        0x27 => "0".into(),
        0x3A..=0x45 => format!("F{}", u - 0x3A + 1),
        0x59..=0x61 => format!("Num{}", u - 0x59 + 1),
        0x62 => "Num0".into(),
        0x68..=0x73 => format!("F{}", u - 0x68 + 13),
        _ => NAMED_KEYS.iter().find(|(c, _)| *c == u)?.1.into(),
    })
}

const NAMED_KEYS: [(u8, &str); 41] = [
    (0x28, "Enter"),
    (0x29, "Esc"),
    (0x2A, "Backspace"),
    (0x2B, "Tab"),
    (0x2C, "Space"),
    (0x2D, "-"),
    (0x2E, "="),
    (0x2F, "["),
    (0x30, "]"),
    (0x31, "\\"),
    (0x33, ";"),
    (0x34, "'"),
    (0x35, "`"),
    (0x36, ","),
    (0x37, "."),
    (0x38, "/"),
    (0x39, "CapsLock"),
    (0x46, "PrintScreen"),
    (0x47, "ScrollLock"),
    (0x48, "Pause"),
    (0x49, "Insert"),
    (0x4A, "Home"),
    (0x4B, "PageUp"),
    (0x4C, "Delete"),
    (0x4D, "End"),
    (0x4E, "PageDown"),
    (0x4F, "Right"),
    (0x50, "Left"),
    (0x51, "Down"),
    (0x52, "Up"),
    (0x53, "NumLock"),
    (0x54, "NumDivide"),
    (0x55, "NumMultiply"),
    (0x56, "NumSubtract"),
    (0x57, "NumAdd"),
    (0x58, "NumEnter"),
    (0x63, "NumDecimal"),
    (0x65, "Menu"),
    // Synonyms people type; the first spelling above is what's displayed.
    (0x29, "Escape"),
    (0x4C, "Del"),
    (0x46, "PrtSc"),
];

fn usage_for(name: &str) -> Option<u8> {
    NAMED_KEYS
        .iter()
        .find(|(_, n)| n.eq_ignore_ascii_case(name))
        .map(|&(u, _)| u)
        .or_else(|| (0x04..=0x73).find(|&u| usage_name(u).is_some_and(|k| k.eq_ignore_ascii_case(name))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{crc, status};

    fn v3() -> &'static KeymapSpec {
        spec_for(0x00B2).unwrap()
    }

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
        assert_eq!(usage_name(0x27).as_deref(), Some("0"));
        assert_eq!(usage_name(0x25).as_deref(), Some("8"));
        assert_ne!(Some(Binding::Key { modifiers: 0, usage: 0x27 }), Binding::default_for(0x04));
        assert_eq!(Binding::default_for(0x04), Some(Binding::Mouse(4)));
        assert_eq!(Binding::default_for(0x60), None, "underside default not established");
    }

    #[test]
    fn reply_for_another_button_is_rejected() {
        let e = parse_binding(&reply(&[0x01, 0x05, 0x00, 0x01, 0x01, 0x05]), 0x04);
        assert_eq!(e, Err(ProtoError::KeymapButtonMismatch { sent: 0x04, got: 0x05 }));
    }

    /// Every write Synapse sent to the V3, rebuilt from its bind:
    /// `docs/captures/V3-20261006-135244-report.md` (back, forward) and
    /// `docs/captures/V3-20261016-additional-buttons/` (the rest).
    #[test]
    fn write_matches_capture() {
        let one = Binding::Key { modifiers: 0, usage: 0x1E };
        let cases: [(u8, Binding, [u8; 7]); 16] = [
            (0x04, one.clone(), [0x01, 0x04, 0x00, 0x02, 0x02, 0x00, 0x1E]),
            (0x04, Binding::Key { modifiers: 0, usage: 0x1F }, [0x01, 0x04, 0x00, 0x02, 0x02, 0x00, 0x1F]),
            (0x04, Binding::Key { modifiers: 0, usage: 0x04 }, [0x01, 0x04, 0x00, 0x02, 0x02, 0x00, 0x04]),
            (0x04, Binding::Key { modifiers: 0x01, usage: 0x06 }, [0x01, 0x04, 0x00, 0x02, 0x02, 0x01, 0x06]),
            (0x04, Binding::Mouse(0x03), [0x01, 0x04, 0x00, 0x01, 0x01, 0x03, 0x00]),
            (0x04, Binding::Disabled, [0x01, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00]),
            (0x05, one.clone(), [0x01, 0x05, 0x00, 0x02, 0x02, 0x00, 0x1E]),
            (0x05, Binding::Mouse(0x05), [0x01, 0x05, 0x00, 0x01, 0x01, 0x05, 0x00]),
            (0x04, Binding::Mouse(0x04), [0x01, 0x04, 0x00, 0x01, 0x01, 0x04, 0x00]),
            (0x03, one.clone(), [0x01, 0x03, 0x00, 0x02, 0x02, 0x00, 0x1E]),
            (0x03, Binding::Mouse(0x03), [0x01, 0x03, 0x00, 0x01, 0x01, 0x03, 0x00]),
            (0x02, one.clone(), [0x01, 0x02, 0x00, 0x02, 0x02, 0x00, 0x1E]),
            (0x02, Binding::Mouse(0x02), [0x01, 0x02, 0x00, 0x01, 0x01, 0x02, 0x00]),
            (0x09, one.clone(), [0x01, 0x09, 0x00, 0x02, 0x02, 0x00, 0x1E]),
            (0x09, Binding::Mouse(0x09), [0x01, 0x09, 0x00, 0x01, 0x01, 0x09, 0x00]),
            (0x0A, one, [0x01, 0x0A, 0x00, 0x02, 0x02, 0x00, 0x1E]),
        ];
        for (button, bind, args) in cases {
            let r = write_binding_report(v3(), 0x1F, button, &bind).unwrap();
            assert_eq!(&r[5..8], &[0x0A, 0x02, 0x0C]);
            assert_eq!(&r[8..15], &args, "button 0x{button:02x} = {bind}");
            assert!(r[15..88].iter().all(|&b| b == 0));
            assert_eq!(r[88], crc(&r));
        }
        // And the wheel-down default, the last of the 2026-10-16 writes.
        let r = write_binding_report(v3(), 0x1F, 0x0A, &Binding::Mouse(0x0A)).unwrap();
        assert_eq!(&r[8..14], &[0x01, 0x0A, 0x00, 0x01, 0x01, 0x0A]);
    }

    #[test]
    fn refuses_uncaptured_or_undecoded_writes() {
        let w = |button, bind: Binding| write_binding_report(v3(), 0x1F, button, &bind);
        assert!(w(0x01, Binding::Mouse(0x01)).is_err(), "left: no captured write");
        assert!(w(0x60, Binding::Mouse(0x01)).is_err(), "underside: no captured write");
        assert!(w(0x04, Binding::Unknown { category: 0x06, payload: vec![0x06] }).is_err());
        assert!(w(0x04, Binding::Mouse(0x42)).is_err(), "unseen mouse function");
        assert!(w(0x04, Binding::Key { modifiers: 0, usage: 0xE0 }).is_err(), "unnamed key");
    }

    #[test]
    fn parses_binds() {
        let key = |modifiers, usage| Ok(Binding::Key { modifiers, usage });
        assert_eq!(parse_bind("default", 0x04), Ok(Binding::Mouse(0x04)));
        assert_eq!(parse_bind("Disabled", 0x04), Ok(Binding::Disabled));
        assert_eq!(parse_bind("middle click", 0x04), Ok(Binding::Mouse(0x03)));
        assert_eq!(parse_bind("1", 0x04), key(0, 0x1E));
        assert_eq!(parse_bind("key:a", 0x04), key(0, 0x04));
        assert_eq!(parse_bind("Ctrl+C", 0x04), key(0x01, 0x06));
        assert_eq!(parse_bind("copy", 0x04), key(0x01, 0x06));
        assert_eq!(parse_bind("ctrl+shift+t", 0x04), key(0x03, 0x17));
        assert_eq!(parse_bind("f13", 0x04), key(0, 0x68));
        assert_eq!(parse_bind("pageup", 0x04), key(0, 0x4B));
        assert_eq!(parse_bind("-", 0x04), key(0, 0x2D));
        assert!(parse_bind("hyper+c", 0x04).is_err());
        assert!(parse_bind("ctrl+nope", 0x04).is_err());
        assert!(parse_bind("default", 0x60).is_err());
        for b in [
            Binding::Key { modifiers: 0x03, usage: 0x17 },
            Binding::Mouse(0x03),
            Binding::Mouse(0x04),
            Binding::Disabled,
        ] {
            assert_eq!(parse_bind(&b.to_string(), 0x04), Ok(b.clone()), "{b} round-trips");
        }
    }

    #[test]
    fn every_named_key_round_trips() {
        // Each usage with a name parses back to itself, alone and in a chord.
        for u in 0x04..=0x73u8 {
            let Some(name) = usage_name(u) else { continue };
            assert!(!name.contains('+'), "{name} would split as a chord");
            assert_eq!(parse_bind(&name, 0x04), Ok(Binding::Key { modifiers: 0, usage: u }), "{name}");
            let chord = format!("Ctrl+Shift+{name}");
            assert_eq!(parse_bind(&chord, 0x04), Ok(Binding::Key { modifiers: 0x03, usage: u }), "{chord}");
        }
        assert_eq!(parse_bind("escape", 0x04), parse_bind("esc", 0x04));
        assert_eq!(parse_bind("num7", 0x04), Ok(Binding::Key { modifiers: 0, usage: 0x5F }));
        assert_eq!(parse_bind("/", 0x04), Ok(Binding::Key { modifiers: 0, usage: 0x38 }));
        assert_eq!(Binding::Key { modifiers: 0, usage: 0x29 }.to_string(), "Esc", "first spelling displays");
    }

    #[test]
    fn finds_buttons_by_name() {
        assert_eq!(v3().button("Wheel-Click"), Some((0x03, "wheel click")));
        assert_eq!(v3().button("back"), Some((0x04, "back")));
        assert_eq!(v3().button("thumb"), None);
        assert!(!v3().is_writable(0x01) && !v3().is_writable(0x60));
    }
}
