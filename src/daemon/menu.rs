//! Tray menu construction and translation, plus application of menu/settings commands.
//! Session-only events (window lifetime and Quit teardown) stay in the daemon loop.

use platform::autostart::Registration;
use razer_hid::Mouse;
use razer_proto::{DeviceSpec, Rgb};

use super::{
    apply_config_candidate, apply_startup_lighting, apply_thumb_hook, report_config_application,
    save_config, toggle_autostart, write_onboard_config, Bindings, MenuAction, SessionError,
};
use crate::config::Config;
use crate::lighting::EffectSpec;
use crate::logger::Logger;

mod menu_id {
    /// DPI menu items encode their value in the id: `DPI_FLAG | dpi`. DPI fits
    /// in a u16, so the encoded range (0x1_0000..0x2_0000) can't collide with
    /// the small fixed ids below (and is checked before `TRAY_CLICK`).
    pub const DPI_FLAG: u32 = 0x1_0000;

    pub const STATIC_RED: u32 = 30;
    pub const STATIC_GREEN: u32 = 31;
    pub const STATIC_BLUE: u32 = 32;
    pub const STATIC_WHITE: u32 = 33;
    pub const STATIC_CYAN: u32 = 34;
    pub const STATIC_YELLOW: u32 = 35;

    pub const BREATHE_RED: u32 = 40;
    pub const BREATHE_GREEN: u32 = 41;
    pub const BREATHE_BLUE: u32 = 42;
    pub const BREATHE_WHITE: u32 = 43;

    pub const SPECTRUM: u32 = 25;
    pub const OFF: u32 = 26;

    pub const SETTINGS: u32 = 80;
    pub const AUTOSTART: u32 = 81;
    pub const RELOAD: u32 = 90;
    pub const QUIT: u32 = 91;
}

/// Preset ladder offered in the DPI submenu, filtered to the connected
/// device's range; the device's own maximum is always appended.
const DPI_LADDER: &[u16] = &[400, 800, 1200, 1600, 1800, 3200, 6400, 12800];

/// DPI presets for a device (or the bare ladder before one is connected).
fn dpi_presets(spec: Option<&DeviceSpec>) -> Vec<u16> {
    let mut v: Vec<u16> = match spec {
        Some(s) => {
            let mut v: Vec<u16> = DPI_LADDER
                .iter()
                .copied()
                .filter(|d| (s.dpi_min..=s.dpi_max).contains(d))
                .collect();
            if !v.contains(&s.dpi_max) {
                v.push(s.dpi_max);
            }
            v
        }
        None => DPI_LADDER.to_vec(),
    };
    v.sort_unstable();
    v
}

/// The startup item shows the registration request, not Windows' separate
/// effective startup policy. A failed read must not look like an unchecked box.
pub(super) fn build_menu_spec(
    spec: Option<&DeviceSpec>,
    autostart: Registration,
) -> Vec<platform::tray::MenuItem> {
    use menu_id::*;
    use platform::tray::MenuItem as M;
    let dpi_items: Vec<M> = dpi_presets(spec)
        .into_iter()
        .map(|d| M::action(DPI_FLAG | d as u32, d.to_string()))
        .collect();
    vec![
        M::submenu("DPI", dpi_items),
        M::submenu(
            "Lighting",
            vec![
                M::submenu(
                    "Static color",
                    vec![
                        M::action(STATIC_RED, "Red"),
                        M::action(STATIC_GREEN, "Green"),
                        M::action(STATIC_BLUE, "Blue"),
                        M::action(STATIC_WHITE, "White"),
                        M::action(STATIC_CYAN, "Cyan"),
                        M::action(STATIC_YELLOW, "Yellow"),
                    ],
                ),
                M::submenu(
                    "Breathing",
                    vec![
                        M::action(BREATHE_RED, "Red"),
                        M::action(BREATHE_GREEN, "Green"),
                        M::action(BREATHE_BLUE, "Blue"),
                        M::action(BREATHE_WHITE, "White"),
                    ],
                ),
                M::action(SPECTRUM, "Spectrum"),
                M::action(OFF, "Off"),
            ],
        ),
        M::Separator,
        M::action(SETTINGS, "Settings..."),
        match autostart {
            Registration::Unknown => M::action(AUTOSTART, "Startup request unavailable"),
            state => M::check(
                AUTOSTART,
                "Request start with Windows",
                state == Registration::Registered,
            ),
        },
        M::action(RELOAD, "Reload config"),
        M::action(QUIT, "Quit"),
    ]
}

pub(super) fn menu_action_for(id: u32) -> Option<MenuAction> {
    use menu_id::*;
    // Value-encoded DPI ids first (an exact range check, so TRAY_CLICK
    // â€” which also has the flag bit set â€” falls through to the match below).
    if (DPI_FLAG..DPI_FLAG + 0x1_0000).contains(&id) {
        return Some(MenuAction::SetDpi((id - DPI_FLAG) as u16));
    }
    let red = Rgb::new(0xFF, 0, 0);
    let green = Rgb::new(0, 0xFF, 0);
    let blue = Rgb::new(0, 0, 0xFF);
    let white = Rgb::new(0xFF, 0xFF, 0xFF);
    let cyan = Rgb::new(0, 0xFF, 0xFF);
    let yellow = Rgb::new(0xFF, 0xFF, 0);
    Some(match id {
        STATIC_RED => MenuAction::Effect(EffectSpec::Static(red)),
        STATIC_GREEN => MenuAction::Effect(EffectSpec::Static(green)),
        STATIC_BLUE => MenuAction::Effect(EffectSpec::Static(blue)),
        STATIC_WHITE => MenuAction::Effect(EffectSpec::Static(white)),
        STATIC_CYAN => MenuAction::Effect(EffectSpec::Static(cyan)),
        STATIC_YELLOW => MenuAction::Effect(EffectSpec::Static(yellow)),
        BREATHE_RED => MenuAction::Effect(EffectSpec::Breathing(red)),
        BREATHE_GREEN => MenuAction::Effect(EffectSpec::Breathing(green)),
        BREATHE_BLUE => MenuAction::Effect(EffectSpec::Breathing(blue)),
        BREATHE_WHITE => MenuAction::Effect(EffectSpec::Breathing(white)),
        SPECTRUM => MenuAction::Effect(EffectSpec::Spectrum),
        OFF => MenuAction::Effect(EffectSpec::Off),
        SETTINGS => MenuAction::OpenSettings,
        AUTOSTART => MenuAction::ToggleAutostart,
        RELOAD => MenuAction::ReloadConfig,
        QUIT => MenuAction::Quit,
        _ if id == platform::tray::TRAY_CLICK => MenuAction::OpenSettings,
        _ => return None,
    })
}

/// Handle a menu command. Returns Ok(true) if Quit was selected.
pub(super) fn handle_menu(
    action: MenuAction,
    ctrl: &Mouse,
    cfg: &mut Config,
    bindings: &mut Bindings,
    log: &Logger,
    health: &mut crate::health::HealthReporter,
) -> Result<bool, razer_hid::Error> {
    match action {
        MenuAction::SetDpi(v) => {
            let (rx, ry) = ctrl.set_dpi(v, v)?;
            cfg.dpi = v;
            cfg.dpi_y = None;
            save_config(cfg, log);
            log.log(&format!("Menu: DPI set to {rx} x {ry}."));
        }
        MenuAction::SetPollingRate(Some(hz)) => {
            let r = ctrl.set_polling_rate(hz)?;
            cfg.polling_rate = Some(hz);
            save_config(cfg, log);
            if let Err(error) = health.polling_rate_changed(Some(hz)) {
                log.log(&format!(
                    "WARN could not publish PC Vitals capsule: {error}"
                ));
            }
            log.log(&format!("Settings: polling rate set to {r} Hz."));
        }
        MenuAction::SetPollingRate(None) => {
            // "keep": stop managing; leave whatever the hardware is set to.
            cfg.polling_rate = None;
            save_config(cfg, log);
            if let Err(error) = health.polling_rate_changed(None) {
                log.log(&format!(
                    "WARN could not publish PC Vitals capsule: {error}"
                ));
            }
            log.log("Settings: polling rate -> keep (unmanaged).");
        }
        MenuAction::Effect(spec) => {
            // Tray presets are device-wide: apply to every zone and drop any
            // per-zone overrides so the pick actually shows everywhere.
            spec.apply(ctrl)?;
            let (lighting, color) = spec.to_config();
            cfg.lighting = lighting;
            if let Some(c) = color {
                cfg.color = c;
            }
            cfg.zones.clear();
            save_config(cfg, log);
            log.log(&format!(
                "Menu: lighting -> {} (all zones).",
                spec.describe()
            ));
        }
        MenuAction::SetUpAction(s) => match crate::actions::parse(&s) {
            Ok(_) => {
                cfg.dpi_up = s.clone();
                *bindings = Bindings::from_config(cfg, log);
                save_config(cfg, log);
                log.log(&format!("Settings: dpi_up -> {s:?}."));
            }
            Err(e) => log.log(&format!("Settings: invalid dpi_up {s:?}: {e}")),
        },
        MenuAction::SetDownAction(s) => match crate::actions::parse(&s) {
            Ok(_) => {
                cfg.dpi_down = s.clone();
                *bindings = Bindings::from_config(cfg, log);
                save_config(cfg, log);
                log.log(&format!("Settings: dpi_down -> {s:?}."));
            }
            Err(e) => log.log(&format!("Settings: invalid dpi_down {s:?}: {e}")),
        },
        MenuAction::SetThumbBack(s) => match crate::actions::parse(&s) {
            Ok(_) => {
                cfg.thumb_back = s.clone();
                save_config(cfg, log);
                apply_thumb_hook(cfg, log);
                log.log(&format!("Settings: thumb_back -> {s:?}."));
            }
            Err(e) => log.log(&format!("Settings: invalid thumb_back {s:?}: {e}")),
        },
        MenuAction::SetThumbForward(s) => match crate::actions::parse(&s) {
            Ok(_) => {
                cfg.thumb_forward = s.clone();
                save_config(cfg, log);
                apply_thumb_hook(cfg, log);
                log.log(&format!("Settings: thumb_forward -> {s:?}."));
            }
            Err(e) => log.log(&format!("Settings: invalid thumb_forward {s:?}: {e}")),
        },
        MenuAction::SetZoneEffectKind(zone, kind) => {
            let name = razer_proto::led::name(zone);
            match EffectSpec::from_config(&kind, cfg.zone_color(name)) {
                Ok(parsed) => {
                    if let Some(spec) = parsed {
                        spec.apply_zone(ctrl, zone)?;
                    }
                    // Record the kind as this zone's override ("keep" included:
                    // it means "don't touch this zone at startup").
                    cfg.zones.entry(name.to_string()).or_default().lighting =
                        Some(kind.to_lowercase());
                    save_config(cfg, log);
                    match parsed {
                        Some(spec) => log.log(&format!(
                            "Settings: {name} lighting -> {}.",
                            spec.describe()
                        )),
                        None => log.log(&format!("Settings: {name} lighting -> keep.")),
                    }
                }
                Err(e) => log.log(&format!("Settings: invalid {name} effect {kind:?}: {e}")),
            }
        }
        MenuAction::SetZoneColor(zone, rgb) => {
            let name = razer_proto::led::name(zone);
            let hex = format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b);
            cfg.zones.entry(name.to_string()).or_default().color = Some(hex.clone());
            // Re-apply live if this zone's effective effect is color-based.
            if let Ok(Some(spec)) = EffectSpec::from_config(cfg.zone_lighting(name), &hex) {
                if matches!(spec, EffectSpec::Static(_) | EffectSpec::Breathing(_)) {
                    spec.apply_zone(ctrl, zone)?;
                }
            }
            save_config(cfg, log);
            log.log(&format!("Settings: {name} color -> {hex}."));
        }
        MenuAction::SetOnboard(button, value) => {
            // A rejected entry is reported and never stored; a failed write
            // keeps the valid entry, but device communication stops.
            let spec = ctrl.spec();
            let Some(km) = razer_proto::keymap::spec_for(spec.product_id) else {
                return Ok(false);
            };
            let Some(&(_, name)) = km.buttons.iter().find(|(b, _)| *b == button) else {
                return Ok(false);
            };
            // The window shows the outcome on its status line and, after a
            // write, waits for the button press that proves it.
            use platform::settings::Expect;
            let result = match crate::onboard::set_entry(km, &mut cfg.onboard, name, &value) {
                Ok(None) => {
                    log.log(&format!("Settings: on-board {name} left alone."));
                    Ok(Expect::Unmanaged)
                }
                Ok(Some(w)) => match crate::onboard::apply(ctrl, km, std::slice::from_ref(&w)) {
                    Ok(changes) => {
                        for c in changes {
                            log.log(&format!("On-board bind written: {c}"));
                        }
                        Ok(crate::onboard::expect_for(&w.bind))
                    }
                    Err(e) => {
                        platform::settings::onboard_result(
                            button,
                            Err(format!("the mouse didn't take it ({e})")),
                        );
                        return Err(e);
                    }
                },
                Err(e) => Err(e),
            };
            if let Err(e) = &result {
                log.log(&format!("WARN on-board {name} not saved: {e}"));
            }
            platform::settings::onboard_result(button, result);
        }
        MenuAction::Save => {
            save_config(cfg, log);
            log.log("Settings: config saved.");
        }
        MenuAction::OpenSettings | MenuAction::SettingsClosed => {
            // Handled in the run_session loop (need tx + the open flag).
        }
        MenuAction::RetryProbe => {
            // Retry clicked just as the mouse connected on its own; the
            // probe already succeeded, nothing to do.
        }
        MenuAction::ToggleAutostart => {
            let spec = ctrl.spec();
            toggle_autostart(Some(&spec), log);
        }
        MenuAction::ReloadConfig => {
            let (new_cfg, note) = match Config::load_or_create(&Config::config_path()) {
                Ok(loaded) => loaded,
                Err(e) => {
                    log.log(&format!(
                        "Config reload rejected: {e}. Keeping the active settings."
                    ));
                    return Ok(false);
                }
            };
            if let Some(n) = note {
                log.log(&n);
            }
            let outcome = match apply_config_candidate(cfg, new_cfg, ctrl.spec(), |candidate| {
                let (dx, dy) = candidate.dpi_xy();
                ctrl.set_dpi(dx, dy)?;
                if let Some(hz) = candidate.polling_rate {
                    ctrl.set_polling_rate(hz)?;
                }
                apply_startup_lighting(ctrl, candidate, log)?;
                if let Some(km) = razer_proto::keymap::spec_for(ctrl.spec().product_id) {
                    write_onboard_config(ctrl, km, candidate, log)?;
                }
                Ok(())
            }) {
                Ok(outcome) => outcome,
                Err(SessionError::Config(e)) => {
                    log.log(&format!(
                        "Config reload rejected: {e}. Keeping the active settings."
                    ));
                    return Ok(false);
                }
                Err(SessionError::Device(e)) => return Err(e),
            };
            report_config_application(outcome, cfg, log);
            *bindings = Bindings::from_config(cfg, log);
            apply_thumb_hook(cfg, log);
            if let Err(error) = health.polling_rate_changed(cfg.polling_rate) {
                log.log(&format!(
                    "WARN could not publish PC Vitals capsule: {error}"
                ));
            }
            log.log("Menu: config reloaded and reapplied.");
        }
        MenuAction::Quit => return Ok(true),
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_action_for_covers_double_click_and_settings() {
        assert!(matches!(
            menu_action_for(menu_id::SETTINGS),
            Some(MenuAction::OpenSettings)
        ));
        assert!(matches!(
            menu_action_for(platform::tray::TRAY_CLICK),
            Some(MenuAction::OpenSettings)
        ));
        assert!(menu_action_for(9999).is_none());
    }

    #[test]
    fn autostart_menu_id_is_distinct() {
        assert!(matches!(
            menu_action_for(menu_id::AUTOSTART),
            Some(MenuAction::ToggleAutostart)
        ));
        // Outside the value-encoded DPI range and distinct from the click id.
        // Both are consts, so this holds at compile time â€” in a `const` block
        // it stays a real check instead of a runtime assert clippy rejects as
        // constant under `-D warnings`.
        const { assert!(menu_id::AUTOSTART < menu_id::DPI_FLAG) };
        assert_ne!(menu_id::AUTOSTART, platform::tray::TRAY_CLICK);
    }

    #[test]
    fn menu_spec_renders_autostart_tick() {
        for state in [Registration::Registered, Registration::Absent] {
            let items = build_menu_spec(None, state);
            let tick = items.iter().find_map(|i| match i {
                platform::tray::MenuItem::Check { id, checked, .. }
                    if *id == menu_id::AUTOSTART =>
                {
                    Some(*checked)
                }
                _ => None,
            });
            assert_eq!(tick, Some(state == Registration::Registered));
        }
        let items = build_menu_spec(None, Registration::Unknown);
        assert!(items.iter().any(|item| matches!(item,
            platform::tray::MenuItem::Action { id, label }
                if *id == menu_id::AUTOSTART && label == "Startup request unavailable")));
        assert!(!items.iter().any(|item| matches!(item,
            platform::tray::MenuItem::Check { id, .. } if *id == menu_id::AUTOSTART)));
    }

    #[test]
    fn dpi_menu_ids_roundtrip_values() {
        for dpi in [100u16, 800, 16000, 30000] {
            let id = menu_id::DPI_FLAG | dpi as u32;
            assert!(matches!(menu_action_for(id), Some(MenuAction::SetDpi(v)) if v == dpi));
        }
        // TRAY_CLICK has the flag bit set but is outside the value range.
        assert!(!matches!(
            menu_action_for(platform::tray::TRAY_CLICK),
            Some(MenuAction::SetDpi(_))
        ));
    }

    #[test]
    fn dpi_presets_track_device_range() {
        // Elite: ladder within 100..=16000, max appended.
        let elite = dpi_presets(Some(&razer_proto::DEATHADDER_ELITE));
        assert!(elite.contains(&800) && elite.contains(&12800));
        assert_eq!(*elite.last().unwrap(), 16000);
        // A narrower device: ladder filtered to its range, ceiling appended.
        let narrow = razer_proto::DeviceSpec {
            dpi_min: 800,
            dpi_max: 6000,
            ..razer_proto::DEATHADDER_ELITE
        };
        let presets = dpi_presets(Some(&narrow));
        assert!(!presets.contains(&400), "below dpi_min must be filtered");
        assert_eq!(*presets.last().unwrap(), 6000);
        // No device yet: the bare ladder.
        assert_eq!(dpi_presets(None), DPI_LADDER.to_vec());
        // Sorted, no duplicates (a device whose max is already a ladder step).
        for w in elite.windows(2) {
            assert!(w[0] < w[1]);
        }
    }
}
