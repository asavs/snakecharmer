//! The button binds stored in the mouse's on-board memory. They apply with no
//! software running, so a non-default one is usually a leftover from Synapse.
//! Shared by `charmctl keymap` and the daemon so the rules for what counts as a
//! leftover, and what gets written, live in one place.
//!
//! Writes go through [`Mouse::set_button_binding`], which only accepts the
//! buttons a capture recorded Synapse writing, and reads every write back.
//! Nothing is written unless it differs from what the mouse already stores.

use std::collections::BTreeMap;
use std::sync::Mutex;

use razer_hid::Mouse;
use razer_proto::keymap::{parse_bind, Binding, KeymapSpec};

/// The on-board keymap research: what's decoded, for which mice.
pub const KEYMAP_DOC_URL: &str =
    "https://github.com/asavs/snakecharmer/blob/master/docs/ONBOARD-KEYMAP.md";

/// One button's stored bind and whether it differs from the factory one.
/// A button with no known factory bind is never flagged.
pub struct StoredBind {
    pub button: u8,
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
            Ok(StoredBind { button, name, bind, is_default })
        })
        .collect()
}

/// The binds that are known to differ from the factory default, skipping the
/// buttons in `configured` (the user chose those in `[onboard]`).
pub fn leftovers<'a>(binds: &'a [StoredBind], configured: &[u8]) -> Vec<&'a StoredBind> {
    binds
        .iter()
        .filter(|b| b.is_default == Some(false) && !configured.contains(&b.button))
        .collect()
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
         Reset them to the factory binds now?"
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

/// A bind to put on one button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    pub button: u8,
    pub name: &'static str,
    pub bind: Binding,
}

/// Resolve the `[onboard]` config table (button name -> bind) against `km`.
/// Every bad entry is reported, not just the first, so one save fixes them all.
pub fn parse_config(km: &KeymapSpec, table: &BTreeMap<String, String>) -> Result<Vec<Wanted>, String> {
    let mut wanted = Vec::new();
    let mut errors = Vec::new();
    for (key, value) in table {
        let Some((button, name)) = km.button(key) else {
            errors.push(format!("{key:?} isn't a button on this mouse"));
            continue;
        };
        if !km.is_writable(button) {
            errors.push(format!("{name} can't be rebound yet"));
            continue;
        }
        match parse_bind(value, button) {
            Ok(bind) => wanted.push(Wanted { button, name, bind }),
            Err(e) => errors.push(format!("{name} = {value:?}: {e}")),
        }
    }
    if errors.is_empty() { Ok(wanted) } else { Err(errors.join("; ")) }
}

/// The writable buttons in display order, each with the config entry that
/// resolves to it (empty when `[onboard]` leaves it alone). What the settings
/// window shows: only buttons `km` can write are editable.
pub fn editable_rows(km: &KeymapSpec, table: &BTreeMap<String, String>) -> Vec<(&'static str, String)> {
    km.buttons
        .iter()
        .filter(|&&(button, _)| km.is_writable(button))
        .map(|&(button, name)| {
            let text = table
                .iter()
                .find(|(k, _)| km.button(k).map(|(b, _)| b) == Some(button))
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            (name, text)
        })
        .collect()
}

/// Stage one settings-window entry into `table`: blank removes the button's
/// entry (leave the mouse alone), anything else is validated exactly as
/// `parse_config` would and stored under the canonical key (`wheel_click`),
/// replacing any spelling of the same button already there. A bad entry
/// leaves `table` untouched. Returns what to write now, if anything.
pub fn set_entry(
    km: &KeymapSpec,
    table: &mut BTreeMap<String, String>,
    name: &str,
    value: &str,
) -> Result<Option<Wanted>, String> {
    let (button, canon) = km.button(name).ok_or_else(|| format!("{name:?} isn't a button on this mouse"))?;
    let value = value.trim();
    let stale: Vec<String> =
        table.keys().filter(|k| km.button(k).map(|(b, _)| b) == Some(button)).cloned().collect();
    if value.is_empty() {
        for k in stale {
            table.remove(&k);
        }
        return Ok(None);
    }
    let one = BTreeMap::from([(canon.to_string(), value.to_string())]);
    let wanted = parse_config(km, &one)?.pop();
    for k in stale {
        table.remove(&k);
    }
    table.insert(canon.replace(' ', "_"), value.to_string());
    Ok(wanted)
}

/// One write that was made.
pub struct Change {
    pub name: &'static str,
    pub before: Binding,
    pub after: Binding,
}

impl core::fmt::Display for Change {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}: {} -> {}", self.name, self.before, self.after)
    }
}

/// Put `wanted` on the mouse, writing only buttons whose stored bind differs.
pub fn apply(mouse: &Mouse, km: &KeymapSpec, wanted: &[Wanted]) -> Result<Vec<Change>, razer_hid::Error> {
    let mut changes = Vec::new();
    for w in wanted {
        let before = mouse.get_button_binding(w.button)?;
        if before != w.bind {
            let after = mouse.set_button_binding(km, w.button, &w.bind)?;
            changes.push(Change { name: w.name, before, after });
        }
    }
    Ok(changes)
}

/// What a reset did, and the leftovers it couldn't touch (buttons with no
/// captured write, such as left).
pub struct ResetOutcome {
    pub changes: Vec<Change>,
    pub stuck: Vec<String>,
}

/// Put every leftover back to its factory bind, skipping `configured` buttons.
pub fn reset(mouse: &Mouse, km: &KeymapSpec, configured: &[u8]) -> Result<ResetOutcome, razer_hid::Error> {
    let binds = read_binds(mouse, km)?;
    let (wanted, stuck) = reset_plan(km, &leftovers(&binds, configured));
    let changes = apply(mouse, km, &wanted)?;
    Ok(ResetOutcome { changes, stuck })
}

/// Split leftovers into the defaults to write and the ones that can't be.
fn reset_plan(km: &KeymapSpec, leftovers: &[&StoredBind]) -> (Vec<Wanted>, Vec<String>) {
    let mut wanted = Vec::new();
    let mut stuck = Vec::new();
    for b in leftovers {
        match Binding::default_for(b.button) {
            Some(bind) if km.is_writable(b.button) => wanted.push(Wanted { button: b.button, name: b.name, bind }),
            _ => stuck.push(format!("{} = {}", b.name, b.bind)),
        }
    }
    (wanted, stuck)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v3() -> &'static KeymapSpec {
        razer_proto::keymap::spec_for(0x00B2).unwrap()
    }

    fn sb(button: u8, name: &'static str, bind: Binding, is_default: Option<bool>) -> StoredBind {
        StoredBind { button, name, bind, is_default }
    }

    #[test]
    fn leftovers_skip_defaults_unknown_defaults_and_configured() {
        let binds = vec![
            sb(0x01, "left", Binding::Mouse(1), Some(true)),
            sb(0x04, "back", Binding::Key { modifiers: 0, usage: 0x27 }, Some(false)),
            sb(0x05, "forward", Binding::Disabled, Some(false)),
            sb(0x60, "underside", Binding::Disabled, None),
        ];
        let l = leftovers(&binds, &[0x05]);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].name, "back");
    }

    #[test]
    fn notice_names_device_and_binds() {
        let binds = vec![
            sb(0x04, "back", Binding::Disabled, Some(false)),
            sb(0x03, "wheel click", Binding::Disabled, Some(false)),
        ];
        let d = describe(&leftovers(&binds, &[]));
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

    #[test]
    fn config_resolves_names_and_reports_every_error() {
        let table: BTreeMap<String, String> =
            [("back", "ctrl+c"), ("wheel-click", "default")].map(|(k, v)| (k.into(), v.into())).into();
        let w = parse_config(v3(), &table).unwrap();
        assert_eq!(w[0], Wanted { button: 0x04, name: "back", bind: Binding::Key { modifiers: 1, usage: 0x06 } });
        assert_eq!(w[1], Wanted { button: 0x03, name: "wheel click", bind: Binding::Mouse(0x03) });

        let bad: BTreeMap<String, String> =
            [("left", "a"), ("thumb", "a"), ("back", "ctrl+nope")].map(|(k, v)| (k.into(), v.into())).into();
        let e = parse_config(v3(), &bad).unwrap_err();
        assert!(e.contains("left can't be rebound"), "{e}");
        assert!(e.contains("\"thumb\" isn't a button"), "{e}");
        assert!(e.contains("back = \"ctrl+nope\""), "{e}");
    }

    #[test]
    fn editable_rows_are_the_writable_buttons_with_their_config_text() {
        let table: BTreeMap<String, String> =
            [("wheel-click", "default"), ("left", "a")].map(|(k, v)| (k.into(), v.into())).into();
        let rows = editable_rows(v3(), &table);
        let names: Vec<&str> = rows.iter().map(|r| r.0).collect();
        assert_eq!(names, ["right", "wheel click", "back", "forward", "wheel up", "wheel down"]);
        assert_eq!(rows[1].1, "default");
        assert!(rows[0].1.is_empty());
    }

    #[test]
    fn set_entry_validates_stores_canonically_and_clears() {
        let mut t: BTreeMap<String, String> = [("wheel-click".to_string(), "disabled".to_string())].into();
        let w = set_entry(v3(), &mut t, "wheel click", " ctrl+c ").unwrap().unwrap();
        assert_eq!(w.bind, Binding::Key { modifiers: 1, usage: 0x06 });
        assert_eq!(t, BTreeMap::from([("wheel_click".to_string(), "ctrl+c".to_string())]));

        // A bad bind or an unwritable button is rejected and changes nothing.
        let before = t.clone();
        assert!(set_entry(v3(), &mut t, "wheel click", "ctrl+nope").is_err());
        assert!(set_entry(v3(), &mut t, "left", "a").is_err());
        assert!(set_entry(v3(), &mut t, "thumb", "a").is_err());
        assert_eq!(t, before);

        assert!(set_entry(v3(), &mut t, "wheel click", "  ").unwrap().is_none());
        assert!(t.is_empty());
    }

    #[test]
    fn reset_writes_defaults_and_reports_unwritable_leftovers() {
        let binds = vec![
            sb(0x01, "left", Binding::Mouse(0x02), Some(false)),
            sb(0x03, "wheel click", Binding::Key { modifiers: 0, usage: 0x25 }, Some(false)),
        ];
        let (wanted, stuck) = reset_plan(v3(), &leftovers(&binds, &[]));
        assert_eq!(wanted, vec![Wanted { button: 0x03, name: "wheel click", bind: Binding::Mouse(0x03) }]);
        assert_eq!(stuck, vec!["left = right click".to_string()]);
    }
}
