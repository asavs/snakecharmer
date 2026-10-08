//! Read-only check of the button binds stored in the mouse's on-board memory.
//! They apply with no software running, so a non-default one is usually a
//! leftover from Synapse. Shared by `charmctl keymap` and the daemon so the
//! "what counts as a leftover" rule lives in one place.

use std::sync::Mutex;

use razer_hid::Mouse;
use razer_proto::keymap::{Binding, KeymapSpec};

/// The on-board keymap research: what's decoded, for which mice.
pub const KEYMAP_DOC_URL: &str =
    "https://github.com/asavs/snakecharmer/blob/master/docs/ONBOARD-KEYMAP.md";

/// One button's stored bind and whether it differs from the factory one.
/// A button with no known factory bind is never flagged.
pub struct StoredBind {
    pub name: &'static str,
    pub bind: Binding,
    pub is_default: Option<bool>,
}

/// Read every button in `km` (and only those) from the mouse. Read-only; the
/// first failure aborts, since a failed read means later ones likely fail too.
pub fn read_binds(mouse: &Mouse, km: &KeymapSpec) -> Result<Vec<StoredBind>, razer_hid::Error> {
    km.buttons
        .iter()
        .map(|&(button, name)| {
            let bind = mouse.get_button_binding(button)?;
            let is_default = Binding::default_for(button).map(|d| d == bind);
            Ok(StoredBind { name, bind, is_default })
        })
        .collect()
}

/// The binds that are known to differ from the factory default.
pub fn leftovers(binds: &[StoredBind]) -> Vec<&StoredBind> {
    binds.iter().filter(|b| b.is_default == Some(false)).collect()
}

/// "back = 0, wheel click = 8" for the log and the notice.
pub fn describe(leftovers: &[&StoredBind]) -> String {
    leftovers
        .iter()
        .map(|b| format!("{} = {}", b.name, b.bind))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The notice body shown when `describe`d leftovers are found.
pub fn notice_text(device: &str, described: &str) -> String {
    format!(
        "Your {device} has binds stored on the mouse: {described}.\n\n\
         They apply even without Snakecharmer, which is why a button can send \
         something other than what you expect.\n\n\
         Open the page that explains how to reset them?"
    )
}

/// The last set of leftovers a notice was shown for in this daemon run.
static LAST_NOTIFIED: Mutex<Option<String>> = Mutex::new(None);

/// True the first time `described` is seen (or when it changes); false for a
/// repeat, so a reconnect doesn't re-nag about the same leftovers.
pub fn first_time(described: &str) -> bool {
    let mut last = LAST_NOTIFIED.lock().unwrap_or_else(|e| e.into_inner());
    if last.as_deref() == Some(described) {
        return false;
    }
    *last = Some(described.to_string());
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sb(name: &'static str, bind: Binding, is_default: Option<bool>) -> StoredBind {
        StoredBind { name, bind, is_default }
    }

    #[test]
    fn leftovers_skip_defaults_and_unknown_defaults() {
        let binds = vec![
            sb("left", Binding::Mouse(1), Some(true)),
            sb("back", Binding::Key { modifiers: 0, usage: 0x27 }, Some(false)),
            sb("underside", Binding::Disabled, None),
        ];
        let l = leftovers(&binds);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].name, "back");
    }

    #[test]
    fn notice_names_device_and_binds() {
        let binds = vec![
            sb("back", Binding::Disabled, Some(false)),
            sb("wheel click", Binding::Disabled, Some(false)),
        ];
        let d = describe(&leftovers(&binds));
        assert_eq!(d, "back = disabled, wheel click = disabled");
        let t = notice_text("DeathAdder V3", &d);
        assert!(t.starts_with("Your DeathAdder V3 has binds stored on the mouse: back = disabled"));
    }

    #[test]
    fn first_time_dedups_same_set_only() {
        assert!(first_time("test-set-a"));
        assert!(!first_time("test-set-a"));
        assert!(first_time("test-set-b"));
    }
}
