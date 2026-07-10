//! Parse config action strings into keystroke chords (virtual-key sequences).
//!
//! Supported:
//!   * `"copy"`  -> Ctrl+C
//!   * `"paste"` -> Ctrl+V
//!   * `"cut"`   -> Ctrl+X
//!   * `"key:c"` -> a single character key (any printable char)
//!   * `"key:9"`, `"key:0"` -> digit keys
//!   * `"key:f13"` .. `"key:f24"` -> function keys
//!   * `"ctrl+c"`, `"ctrl+shift+v"` -> explicit modifier chords
//!   * `"none"`  -> do nothing
//!
//! A chord is a `Vec<u16>` of VKs pressed in order then released in reverse.

use platform::{vk_for_char, vk_function, VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN};

/// A parsed action: either a keystroke chord or an explicit no-op.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Chord(Vec<u16>),
    None,
}

impl Action {
    pub fn chord(&self) -> Option<&[u16]> {
        match self {
            Action::Chord(v) => Some(v),
            Action::None => None,
        }
    }
}

fn vk_for_token(tok: &str) -> Option<u16> {
    let t = tok.trim().to_ascii_lowercase();
    match t.as_str() {
        "ctrl" | "control" => Some(VK_CONTROL),
        "shift" => Some(VK_SHIFT),
        "alt" => Some(VK_MENU),
        "win" | "super" | "meta" => Some(VK_LWIN),
        _ => {
            // f1..f24
            if let Some(rest) = t.strip_prefix('f') {
                if let Ok(n) = rest.parse::<u8>() {
                    return vk_function(n);
                }
            }
            // single character
            let mut chars = t.chars();
            let c = chars.next()?;
            if chars.next().is_none() {
                return vk_for_char(c);
            }
            None
        }
    }
}

/// Parse an action string into an [`Action`]. Returns `Err` with a message on
/// an unrecognized spec.
pub fn parse(spec: &str) -> Result<Action, String> {
    let s = spec.trim();
    match s.to_ascii_lowercase().as_str() {
        "" | "none" | "off" | "disabled" => return Ok(Action::None),
        "copy" => return Ok(Action::Chord(vec![VK_CONTROL, req_char('c')?])),
        "paste" => return Ok(Action::Chord(vec![VK_CONTROL, req_char('v')?])),
        "cut" => return Ok(Action::Chord(vec![VK_CONTROL, req_char('x')?])),
        _ => {}
    }

    // "key:<token>"
    if let Some(rest) = s.strip_prefix("key:").or_else(|| s.strip_prefix("KEY:")) {
        let vk = vk_for_token(rest).ok_or_else(|| format!("unknown key {rest:?}"))?;
        return Ok(Action::Chord(vec![vk]));
    }

    // "mod+mod+key"
    if s.contains('+') {
        let mut vks = Vec::new();
        for part in s.split('+') {
            let vk = vk_for_token(part).ok_or_else(|| format!("unknown key {part:?} in {s:?}"))?;
            vks.push(vk);
        }
        if vks.is_empty() {
            return Err(format!("empty chord {s:?}"));
        }
        return Ok(Action::Chord(vks));
    }

    Err(format!("unrecognized action {spec:?}"))
}

fn req_char(c: char) -> Result<u16, String> {
    vk_for_char(c).ok_or_else(|| format!("no virtual-key for {c:?} on this layout"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_paste_are_ctrl_chords() {
        let copy = parse("copy").unwrap();
        assert_eq!(copy, Action::Chord(vec![VK_CONTROL, vk_for_char('c').unwrap()]));
        let paste = parse("paste").unwrap();
        assert_eq!(paste, Action::Chord(vec![VK_CONTROL, vk_for_char('v').unwrap()]));
    }

    #[test]
    fn key_digits_and_function_keys() {
        assert_eq!(parse("key:9").unwrap(), Action::Chord(vec![vk_for_char('9').unwrap()]));
        assert_eq!(parse("key:0").unwrap(), Action::Chord(vec![vk_for_char('0').unwrap()]));
        assert_eq!(parse("key:f13").unwrap(), Action::Chord(vec![0x7C]));
        assert_eq!(parse("key:f24").unwrap(), Action::Chord(vec![0x87]));
    }

    #[test]
    fn explicit_modifier_chords() {
        assert_eq!(
            parse("ctrl+shift+v").unwrap(),
            Action::Chord(vec![VK_CONTROL, VK_SHIFT, vk_for_char('v').unwrap()])
        );
    }

    #[test]
    fn none_and_empty() {
        assert_eq!(parse("none").unwrap(), Action::None);
        assert_eq!(parse("").unwrap(), Action::None);
        assert!(parse("none").unwrap().chord().is_none());
    }

    #[test]
    fn unknown_is_error() {
        assert!(parse("frobnicate").is_err());
        assert!(parse("key:").is_err());
    }
}
