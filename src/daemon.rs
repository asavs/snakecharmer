//! The headless daemon (with system tray): enable driver mode, lock DPI, apply
//! lighting, listen for the DPI-button vendor reports on the readable
//! collections (blocking reads in per-collection threads), inject the
//! configured keystrokes, and serve the tray menu. One process, one exe.

use std::collections::HashSet;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use razer_hid::{aux_collection_paths, Listener, Mouse};
use razer_proto::{DeviceMode, DeviceSpec, DpiButtons, Rgb};

use crate::actions::Action;
use crate::config::Config;
use crate::lighting::EffectSpec;
use crate::logger::Logger;

/// Ignore a repeat of the same code within this window (debounce / anti double-fire).
const DEBOUNCE: Duration = Duration::from_millis(200);

/// A menu / settings-window selection turned into a daemon command.
#[derive(Debug, Clone)]
pub enum MenuAction {
    SetDpi(u16),
    /// `Some(hz)` = set + manage; `None` = "keep" (stop managing, leave hardware).
    SetPollingRate(Option<u16>),
    Effect(EffectSpec),
    SetUpAction(String),
    SetDownAction(String),
    SetThumbBack(String),
    SetThumbForward(String),
    SetEffectKind(String),
    SetColor(Rgb),
    Apply,
    Save,
    OpenSettings,
    SettingsClosed,
    ReloadConfig,
    Quit,
}

/// Events feeding the daemon's main loop.
enum Event {
    Button(u8),
    Menu(MenuAction),
    ListenerClosed,
}

/// Action presets offered in the settings-window dropdowns.
const ACTION_PRESETS: &[&str] =
    &["copy", "paste", "cut", "none", "key:9", "key:0", "key:f13", "key:f14"];
/// Lighting options offered in the settings-window effect dropdown.
const EFFECT_PRESETS: &[&str] = &["keep", "static", "breathing", "spectrum", "off"];

/// Build the action dropdown labels (including the current values) and the
/// selected indices for dpi_up / dpi_down.
fn action_options(cfg: &Config) -> (Vec<String>, usize, usize) {
    let mut labels: Vec<String> = ACTION_PRESETS.iter().map(|s| s.to_string()).collect();
    for v in [&cfg.dpi_up, &cfg.dpi_down, &cfg.thumb_back, &cfg.thumb_forward] {
        if !labels.iter().any(|l| l == v) {
            labels.push(v.clone());
        }
    }
    let up = labels.iter().position(|l| l == &cfg.dpi_up).unwrap_or(0);
    let down = labels.iter().position(|l| l == &cfg.dpi_down).unwrap_or(0);
    (labels, up, down)
}

/// Parse a config action into a keystroke chord (`None` = passthrough/disabled).
fn chord_of(spec: &str) -> Option<Vec<u16>> {
    crate::actions::parse(spec)
        .ok()
        .and_then(|a| a.chord().map(|c| c.to_vec()))
}

/// Push the current thumb-button remaps into the low-level mouse hook.
fn apply_thumb_hook(cfg: &Config, log: &Logger) {
    let back = chord_of(&cfg.thumb_back);
    let forward = chord_of(&cfg.thumb_forward);
    let desc = |on: bool, s: &str| if on { s.to_string() } else { "passthrough".to_string() };
    let any = back.is_some() || forward.is_some();
    log.log(&format!(
        "Thumb remap: back={}, forward={} (global mouse hook: {}).",
        desc(back.is_some(), &cfg.thumb_back),
        desc(forward.is_some(), &cfg.thumb_forward),
        if any { "active" } else { "off - zero overhead" }
    ));
    platform::mouse_hook::set_thumb_actions(back, forward);
}

/// Build the thumb-button dropdown indices (reusing the action label list).
fn thumb_indices(cfg: &Config, labels: &[String]) -> (usize, usize) {
    let find = |v: &str| labels.iter().position(|l| l == v).unwrap_or(0);
    (find(&cfg.thumb_back), find(&cfg.thumb_forward))
}

/// Polling dropdown labels ("keep" + the device's supported rates) and the
/// selected index matching the config (`None` -> "keep").
fn polling_options(cfg: &Config, spec: &DeviceSpec) -> (Vec<String>, usize) {
    let mut labels = vec!["keep".to_string()];
    labels.extend(spec.polling.rates.iter().map(|hz| format!("{hz} Hz")));
    let idx = cfg
        .polling_rate
        .and_then(|hz| spec.polling.rates.iter().position(|&r| r == hz).map(|i| i + 1))
        .unwrap_or(0);
    (labels, idx)
}

fn effect_options(cfg: &Config) -> (Vec<String>, usize) {
    let labels: Vec<String> = EFFECT_PRESETS.iter().map(|s| s.to_string()).collect();
    let idx = labels
        .iter()
        .position(|l| l.eq_ignore_ascii_case(&cfg.lighting))
        .unwrap_or(0);
    (labels, idx)
}

/// Spawn the settings window on its own thread (message loop lives there).
/// `spec` supplies the plugged-in device's DPI range so the slider covers the
/// whole capable range of *that* mouse (16000 on the Elite, 30000 on the V3).
fn open_settings_window(cfg: &Config, spec: DeviceSpec, tx: &Sender<Event>) {
    let (action_labels, up_index, down_index) = action_options(cfg);
    let (thumb_back_index, thumb_forward_index) = thumb_indices(cfg, &action_labels);
    let (effect_labels, effect_index) = effect_options(cfg);
    let (polling_labels, polling_index) = polling_options(cfg, &spec);
    let color = Rgb::parse_hex(&cfg.color).unwrap_or(Rgb::new(0, 0xFF, 0));
    let init = platform::settings::SettingsInit {
        dpi: cfg.dpi,
        dpi_min: spec.dpi_min,
        dpi_max: spec.dpi_max,
        polling_labels,
        polling_index,
        action_labels: action_labels.clone(),
        up_index,
        down_index,
        thumb_back_index,
        thumb_forward_index,
        effect_labels: effect_labels.clone(),
        effect_index,
        color: (color.r, color.g, color.b),
    };
    let tx = tx.clone();
    thread::spawn(move || {
        use platform::settings::SettingsEvent as SE;
        let tx_ev = tx.clone();
        platform::settings::open(init, move |ev| {
            let cmd = match ev {
                SE::Dpi(v) => Some(MenuAction::SetDpi(v)),
                // Index 0 = "keep" (stop managing); 1.. = the spec's rates.
                SE::Polling(0) => Some(MenuAction::SetPollingRate(None)),
                SE::Polling(i) => spec
                    .polling
                    .rates
                    .get(i - 1)
                    .map(|&hz| MenuAction::SetPollingRate(Some(hz))),
                SE::UpAction(i) => action_labels.get(i).map(|s| MenuAction::SetUpAction(s.clone())),
                SE::DownAction(i) => {
                    action_labels.get(i).map(|s| MenuAction::SetDownAction(s.clone()))
                }
                SE::ThumbBack(i) => {
                    action_labels.get(i).map(|s| MenuAction::SetThumbBack(s.clone()))
                }
                SE::ThumbForward(i) => {
                    action_labels.get(i).map(|s| MenuAction::SetThumbForward(s.clone()))
                }
                SE::Effect(i) => effect_labels.get(i).map(|s| MenuAction::SetEffectKind(s.clone())),
                SE::Color(r, g, b) => Some(MenuAction::SetColor(Rgb::new(r, g, b))),
                SE::Apply => Some(MenuAction::Apply),
                SE::Save => Some(MenuAction::Save),
            };
            if let Some(c) = cmd {
                let _ = tx_ev.send(Event::Menu(c));
            }
        });
        // Window closed.
        let _ = tx.send(Event::Menu(MenuAction::SettingsClosed));
    });
}

/// Pre-parsed button actions.
struct Bindings {
    up: Action,
    down: Action,
}

impl Bindings {
    fn from_config(cfg: &Config, log: &Logger) -> Bindings {
        let up = crate::actions::parse(&cfg.dpi_up).unwrap_or_else(|e| {
            log.log(&format!("WARN dpi_up action invalid ({e}); disabling that button"));
            Action::None
        });
        let down = crate::actions::parse(&cfg.dpi_down).unwrap_or_else(|e| {
            log.log(&format!("WARN dpi_down action invalid ({e}); disabling that button"));
            Action::None
        });
        Bindings { up, down }
    }
}

// --- Tray menu ------------------------------------------------------------

mod menu_id {
    /// DPI menu items encode their value in the id: `DPI_FLAG | dpi`. DPI fits
    /// in a u16, so the encoded range (0x1_0000..0x2_0000) can't collide with
    /// the small fixed ids below (and is checked before `TRAY_DOUBLE_CLICK`).
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
            let mut v: Vec<u16> =
                DPI_LADDER.iter().copied().filter(|d| (s.dpi_min..=s.dpi_max).contains(d)).collect();
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

fn build_menu_spec(spec: Option<&DeviceSpec>) -> Vec<platform::tray::MenuItem> {
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
        M::action(RELOAD, "Reload config"),
        M::action(QUIT, "Quit"),
    ]
}

fn menu_action_for(id: u32) -> Option<MenuAction> {
    use menu_id::*;
    // Value-encoded DPI ids first (an exact range check, so TRAY_DOUBLE_CLICK
    // — which also has the flag bit set — falls through to the match below).
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
        RELOAD => MenuAction::ReloadConfig,
        QUIT => MenuAction::Quit,
        _ if id == platform::tray::TRAY_DOUBLE_CLICK => MenuAction::OpenSettings,
        _ => return None,
    })
}

// --- Daemon ---------------------------------------------------------------

/// Run the daemon forever, restarting the device session on unplug/sleep errors.
pub fn run(mut cfg: Config, log: Logger) -> ! {
    log.log(&format!(
        "Daemon starting. dpi={:?} up={:?} down={:?} lighting={:?} reassert={}s",
        cfg.dpi_xy(),
        cfg.dpi_up,
        cfg.dpi_down,
        cfg.lighting,
        cfg.reassert_interval_secs
    ));

    let (tx, rx) = mpsc::channel::<Event>();

    // Spawn the tray icon + menu once, on its own thread.
    {
        let tx = tx.clone();
        thread::spawn(move || {
            platform::tray::run(
                "Snakecharmer",
                build_menu_spec(None),
                move |id| {
                    if let Some(a) = menu_action_for(id) {
                        let _ = tx.send(Event::Menu(a));
                    }
                },
            );
        });
    }

    let mut bindings = Bindings::from_config(&cfg, &log);
    apply_thumb_hook(&cfg, &log);
    let mut settings_open = false;
    loop {
        match run_session(&mut cfg, &mut bindings, &log, &tx, &rx, &mut settings_open) {
            Ok(()) => log.log("Session ended cleanly; restarting."),
            Err(e) => log.log(&format!(
                "Session ended: {e}. Retrying in 3s (mouse unplugged/asleep?)."
            )),
        }
        thread::sleep(Duration::from_secs(3));
    }
}

fn run_session(
    cfg: &mut Config,
    bindings: &mut Bindings,
    log: &Logger,
    tx: &Sender<Event>,
    rx: &Receiver<Event>,
    settings_open: &mut bool,
) -> Result<(), razer_hid::Error> {
    let api = razer_hid::open_api()?;
    let ctrl = Mouse::open_with(&api)?;
    let spec = ctrl.spec();
    log.log(&format!(
        "Opened {} (PID 0x{:04X}, txn 0x{:02X}; rgb={}, dpi_buttons={}).",
        spec.name,
        spec.product_id,
        spec.transaction_id,
        spec.has_rgb(),
        spec.dpi_buttons.is_some()
    ));
    // Rebuild the tray menu around this device (DPI presets over its range).
    platform::tray::set_menu(build_menu_spec(Some(&spec)));

    // Lock DPI (non-fatal if it fails). Every supported device has DPI.
    let (dx, dy) = cfg.dpi_xy();
    match ctrl.set_dpi(dx, dy) {
        Ok((rx2, ry)) => log.log(&format!("DPI locked to {rx2} x {ry}.")),
        Err(e) => log.log(&format!("WARN could not set DPI: {e}")),
    }

    // Lock the polling rate if configured (non-fatal, like DPI); absent = leave as-is.
    if let Some(hz) = cfg.polling_rate {
        match ctrl.set_polling_rate(hz) {
            Ok(r) => log.log(&format!("Polling rate locked to {r} Hz.")),
            Err(e) => log.log(&format!("WARN could not set polling rate: {e}")),
        }
    }

    // Apply configured startup lighting (unless "keep"; a logged no-op on
    // devices without lighting hardware — see `apply_startup_lighting`).
    apply_startup_lighting(&ctrl, cfg, log);

    // Driver mode and the DPI-button vendor-code listeners only apply to devices
    // that actually have the wheel DPI buttons. On a device without them there
    // is nothing to switch into driver mode for and nothing to listen for, so we
    // skip both and just run the DPI/lighting re-assert loop.
    if spec.dpi_buttons.is_some() {
        // Enable driver mode (fatal if it fails) so the buttons emit 0x20/0x21.
        let mode = ctrl.set_device_mode(DeviceMode::Driver)?;
        log.log(&format!("Driver mode enabled (read back 0x{mode:02x})."));

        // Probe readable auxiliary collections; keep the survivors.
        let mut listeners: Vec<Listener> = Vec::new();
        let mut dropped = 0usize;
        for path in aux_collection_paths(&api, spec.product_id) {
            let listener = match Listener::open(&api, &path) {
                Ok(l) => l,
                Err(_) => {
                    dropped += 1;
                    continue;
                }
            };
            listener.set_blocking(false)?;
            let mut probe = [0u8; 64];
            match listener.read(&mut probe) {
                Ok(_) => {
                    listener.set_blocking(true)?;
                    listeners.push(listener);
                }
                Err(_) => dropped += 1,
            }
        }
        if listeners.is_empty() {
            return Err(razer_hid::Error::DeviceNotFound);
        }
        log.log(&format!(
            "Listening on {} collection(s); dropped {} unreadable.",
            listeners.len(),
            dropped
        ));

        // One blocking-read thread per collection -> channel. CPU stays ~0 idle.
        for listener in listeners {
            let tx = tx.clone();
            let log = log.clone();
            thread::spawn(move || reader_thread(listener, tx, log));
        }
    } else {
        log.log(&format!(
            "{} has no wheel DPI buttons; skipping driver mode and vendor-code listeners.",
            spec.name
        ));
    }

    let reassert_interval = Duration::from_secs(cfg.reassert_interval_secs.max(5));
    // With listeners, an unplug fails a blocking read almost instantly. Without
    // them (a device with no DPI buttons), nothing notices until we next talk to
    // the device — so poll liveness on a short tick instead of waiting out the
    // whole re-assert interval with a dead handle.
    let tick = if spec.dpi_buttons.is_some() {
        reassert_interval
    } else {
        reassert_interval.min(Duration::from_secs(5))
    };
    let mut last_reassert = Instant::now();
    let mut last_fire: Option<(u8, Instant)> = None;

    loop {
        match rx.recv_timeout(tick) {
            Ok(Event::Button(code)) => {
                handle_code(code, spec.dpi_buttons, bindings, log, &mut last_fire)
            }
            Ok(Event::Menu(MenuAction::OpenSettings)) => {
                if *settings_open {
                    log.log("Settings window already open.");
                } else {
                    *settings_open = true;
                    open_settings_window(cfg, spec, tx);
                    log.log("Opened settings window.");
                }
            }
            Ok(Event::Menu(MenuAction::SettingsClosed)) => {
                *settings_open = false;
                log.log("Settings window closed.");
            }
            Ok(Event::Menu(action)) => {
                if handle_menu(action, &ctrl, cfg, bindings, log)? {
                    log.log("Quit selected; removing mouse hook and exiting.");
                    platform::mouse_hook::uninstall();
                    std::process::exit(0);
                }
            }
            Ok(Event::ListenerClosed) => {
                // Verify the device is still alive; if not, restart the session.
                if ctrl.get_device_mode().is_err() {
                    return Err(razer_hid::Error::Verify("device lost".into()));
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if last_reassert.elapsed() >= reassert_interval {
                    reassert(&ctrl, cfg, log)?;
                    last_reassert = Instant::now();
                } else {
                    // Liveness only: a cheap read that errors out (ending the
                    // session, triggering the 3s reopen loop) if the device is gone.
                    ctrl.get_dpi()?;
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(razer_hid::Error::Verify("event channel closed".into()))
            }
        }
    }
}

fn apply_startup_lighting(ctrl: &Mouse, cfg: &Config, log: &Logger) {
    if !ctrl.spec().has_rgb() {
        if !cfg.lighting.eq_ignore_ascii_case("keep") {
            log.log(&format!(
                "{} has no lighting hardware; ignoring lighting={:?}.",
                ctrl.spec().name,
                cfg.lighting
            ));
        }
        return;
    }
    match EffectSpec::from_config(&cfg.lighting, &cfg.color) {
        Ok(Some(effect)) => match effect.apply(ctrl) {
            Ok(()) => log.log(&format!("Lighting set to {}.", effect.describe())),
            Err(e) => log.log(&format!("WARN could not set lighting: {e}")),
        },
        Ok(None) => {} // "keep": leave lighting untouched
        Err(e) => log.log(&format!("WARN invalid lighting config: {e}")),
    }
}

/// Handle a menu command. Returns Ok(true) if Quit was selected.
fn handle_menu(
    action: MenuAction,
    ctrl: &Mouse,
    cfg: &mut Config,
    bindings: &mut Bindings,
    log: &Logger,
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
            log.log(&format!("Settings: polling rate set to {r} Hz."));
        }
        MenuAction::SetPollingRate(None) => {
            // "keep": stop managing; leave whatever the hardware is set to.
            cfg.polling_rate = None;
            save_config(cfg, log);
            log.log("Settings: polling rate -> keep (unmanaged).");
        }
        MenuAction::Effect(spec) => {
            spec.apply(ctrl)?;
            let (lighting, color) = spec.to_config();
            cfg.lighting = lighting;
            if let Some(c) = color {
                cfg.color = c;
            }
            save_config(cfg, log);
            log.log(&format!("Menu: lighting -> {}.", spec.describe()));
        }
        MenuAction::SetUpAction(s) => {
            match crate::actions::parse(&s) {
                Ok(_) => {
                    cfg.dpi_up = s.clone();
                    *bindings = Bindings::from_config(cfg, log);
                    save_config(cfg, log);
                    log.log(&format!("Settings: dpi_up -> {s:?}."));
                }
                Err(e) => log.log(&format!("Settings: invalid dpi_up {s:?}: {e}")),
            }
        }
        MenuAction::SetDownAction(s) => {
            match crate::actions::parse(&s) {
                Ok(_) => {
                    cfg.dpi_down = s.clone();
                    *bindings = Bindings::from_config(cfg, log);
                    save_config(cfg, log);
                    log.log(&format!("Settings: dpi_down -> {s:?}."));
                }
                Err(e) => log.log(&format!("Settings: invalid dpi_down {s:?}: {e}")),
            }
        }
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
        MenuAction::SetEffectKind(kind) => match EffectSpec::from_config(&kind, &cfg.color) {
            Ok(Some(spec)) => {
                spec.apply(ctrl)?;
                let (lighting, color) = spec.to_config();
                cfg.lighting = lighting;
                if let Some(c) = color {
                    cfg.color = c;
                }
                save_config(cfg, log);
                log.log(&format!("Settings: lighting -> {}.", spec.describe()));
            }
            Ok(None) => {
                cfg.lighting = "keep".into();
                save_config(cfg, log);
                log.log("Settings: lighting -> keep.");
            }
            Err(e) => log.log(&format!("Settings: invalid effect {kind:?}: {e}")),
        },
        MenuAction::SetColor(rgb) => {
            cfg.color = format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b);
            // Re-apply live if the current effect is color-based.
            if let Ok(Some(spec)) = EffectSpec::from_config(&cfg.lighting, &cfg.color) {
                if matches!(spec, EffectSpec::Static(_) | EffectSpec::Breathing(_)) {
                    spec.apply(ctrl)?;
                }
            }
            save_config(cfg, log);
            log.log(&format!("Settings: color -> {}.", cfg.color));
        }
        MenuAction::Apply => {
            let (dx, dy) = cfg.dpi_xy();
            let _ = ctrl.set_dpi(dx, dy);
            if let Some(hz) = cfg.polling_rate {
                let _ = ctrl.set_polling_rate(hz);
            }
            apply_startup_lighting(ctrl, cfg, log);
            log.log("Settings: applied current config to device.");
        }
        MenuAction::Save => {
            save_config(cfg, log);
            log.log("Settings: config saved.");
        }
        MenuAction::OpenSettings | MenuAction::SettingsClosed => {
            // Handled in the run_session loop (need tx + the open flag).
        }
        MenuAction::ReloadConfig => {
            let (new_cfg, note) = Config::load_or_create(&Config::config_path());
            if let Some(n) = note {
                log.log(&n);
            }
            *cfg = new_cfg;
            *bindings = Bindings::from_config(cfg, log);
            let (dx, dy) = cfg.dpi_xy();
            let _ = ctrl.set_dpi(dx, dy);
            if let Some(hz) = cfg.polling_rate {
                let _ = ctrl.set_polling_rate(hz);
            }
            apply_startup_lighting(ctrl, cfg, log);
            apply_thumb_hook(cfg, log);
            log.log("Menu: config reloaded and reapplied.");
        }
        MenuAction::Quit => return Ok(true),
    }
    Ok(false)
}

fn save_config(cfg: &Config, log: &Logger) {
    if let Err(e) = cfg.save(&Config::config_path()) {
        log.log(&format!("WARN could not save config: {e}"));
    }
}

/// Blocking read loop for one collection. Emits rising-edge vendor codes.
fn reader_thread(listener: Listener, tx: Sender<Event>, log: Logger) {
    let label = listener.label();
    let mut prev: HashSet<u8> = HashSet::new();
    let mut buf = [0u8; 64];
    loop {
        match listener.read(&mut buf) {
            Ok(n) => {
                let data = &buf[..n];
                if data.first() != Some(&0x04) {
                    continue;
                }
                let cur: HashSet<u8> = data[1..].iter().copied().filter(|&b| b != 0).collect();
                for &code in cur.difference(&prev) {
                    if tx.send(Event::Button(code)).is_err() {
                        return;
                    }
                }
                prev = cur;
            }
            Err(e) => {
                log.log(&format!("Listener {label} closed: {e}"));
                let _ = tx.send(Event::ListenerClosed);
                return;
            }
        }
    }
}

fn handle_code(
    code: u8,
    buttons: Option<DpiButtons>,
    bindings: &Bindings,
    log: &Logger,
    last_fire: &mut Option<(u8, Instant)>,
) {
    if let Some((c, t)) = last_fire {
        if *c == code && t.elapsed() < DEBOUNCE {
            return;
        }
    }
    let (name, action) = match buttons {
        Some(b) if code == b.up => ("dpi_up", &bindings.up),
        Some(b) if code == b.down => ("dpi_down", &bindings.down),
        _ => {
            log.log(&format!("Unmapped vendor code 0x{code:02x}"));
            return;
        }
    };
    *last_fire = Some((code, Instant::now()));
    match action.chord() {
        Some(vks) => match platform::send_chord(vks) {
            Ok(()) => log.log(&format!("{name} (0x{code:02x}) -> injected {vks:02x?}")),
            Err(e) => log.log(&format!("{name} (0x{code:02x}) -> inject FAILED: {e}")),
        },
        None => log.log(&format!("{name} (0x{code:02x}) -> action=none")),
    }
}

/// Re-assert driver mode and DPI; log only when something actually changed.
fn reassert(ctrl: &Mouse, cfg: &Config, log: &Logger) -> Result<(), razer_hid::Error> {
    // Driver mode only matters on devices with the DPI buttons; elsewhere the
    // get/set-device-mode round-trip is pointless (and touches a command the
    // hardware has no reason to support), so re-lock DPI only.
    if ctrl.spec().dpi_buttons.is_some() {
        let mode = ctrl.get_device_mode()?;
        if mode != DeviceMode::Driver.as_byte() {
            let m = ctrl.set_device_mode(DeviceMode::Driver)?;
            log.log(&format!("Re-asserted driver mode (was 0x{mode:02x}, now 0x{m:02x})."));
        }
    }
    let (want_x, want_y) = cfg.dpi_xy();
    let (cx, cy) = ctrl.get_dpi()?;
    if (cx, cy) != (want_x, want_y) {
        let (rx, ry) = ctrl.set_dpi(want_x, want_y)?;
        log.log(&format!("Re-locked DPI (was {cx}x{cy}, now {rx}x{ry})."));
    }
    if let Some(want_hz) = cfg.polling_rate {
        let cur = ctrl.get_polling_rate()?;
        if cur != want_hz {
            let hz = ctrl.set_polling_rate(want_hz)?;
            log.log(&format!("Re-locked polling rate (was {cur} Hz, now {hz} Hz)."));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_action_presets_parse() {
        for p in ACTION_PRESETS {
            assert!(crate::actions::parse(p).is_ok(), "preset {p:?} must parse");
        }
    }

    #[test]
    fn effect_presets_map_to_specs() {
        // "keep" -> None; the rest -> a concrete spec.
        assert_eq!(EffectSpec::from_config("keep", "#00ff00").unwrap(), None);
        for p in &EFFECT_PRESETS[1..] {
            assert!(EffectSpec::from_config(p, "#00ff00").unwrap().is_some(), "{p}");
        }
    }

    #[test]
    fn action_options_indices_track_config() {
        let mut cfg = Config::default(); // dpi_up=copy, dpi_down=paste
        let (labels, up, down) = action_options(&cfg);
        assert_eq!(labels[up], "copy");
        assert_eq!(labels[down], "paste");

        // A custom value not in the presets must be appended and selected.
        cfg.dpi_up = "ctrl+shift+v".to_string();
        let (labels, up, _) = action_options(&cfg);
        assert_eq!(labels[up], "ctrl+shift+v");
        assert!(labels.iter().filter(|l| *l == "ctrl+shift+v").count() == 1);
    }

    #[test]
    fn polling_options_track_config_and_spec() {
        let v3 = razer_proto::DEATHADDER_V3;
        let mut cfg = Config::default(); // polling_rate = None
        let (labels, idx) = polling_options(&cfg, &v3);
        assert_eq!(labels[0], "keep");
        assert_eq!(idx, 0, "unmanaged config must select keep");
        assert_eq!(labels.len(), v3.polling.rates.len() + 1);

        cfg.polling_rate = Some(4000);
        let (labels, idx) = polling_options(&cfg, &v3);
        assert_eq!(labels[idx], "4000 Hz");

        // A configured rate this device doesn't support falls back to "keep".
        cfg.polling_rate = Some(9999);
        let (_, idx) = polling_options(&cfg, &v3);
        assert_eq!(idx, 0);
    }

    #[test]
    fn effect_options_index_tracks_lighting() {
        let mut cfg = Config::default(); // lighting = keep
        let (labels, idx) = effect_options(&cfg);
        assert_eq!(labels[idx], "keep");
        cfg.lighting = "spectrum".into();
        let (labels, idx) = effect_options(&cfg);
        assert_eq!(labels[idx], "spectrum");
    }

    #[test]
    fn chord_of_disables_none_and_invalid() {
        // Real actions produce a chord; none/invalid produce passthrough (None).
        assert!(chord_of("copy").is_some());
        assert!(chord_of("key:f13").is_some());
        assert!(chord_of("none").is_none());
        assert!(chord_of("frobnicate").is_none());
    }

    #[test]
    fn thumb_indices_track_config() {
        let mut cfg = Config {
            thumb_back: "cut".into(),
            ..Config::default()
        };
        let (labels, _, _) = action_options(&cfg);
        let (back, fwd) = thumb_indices(&cfg, &labels);
        assert_eq!(labels[back], "cut");
        assert_eq!(labels[fwd], "none"); // forward defaulted to none, which is a preset

        // A custom thumb chord must be present in the labels (via action_options).
        cfg.thumb_forward = "ctrl+shift+t".into();
        let (labels, _, _) = action_options(&cfg);
        let (_, fwd) = thumb_indices(&cfg, &labels);
        assert_eq!(labels[fwd], "ctrl+shift+t");
    }

    #[test]
    fn menu_action_for_covers_double_click_and_settings() {
        assert!(matches!(menu_action_for(menu_id::SETTINGS), Some(MenuAction::OpenSettings)));
        assert!(matches!(
            menu_action_for(platform::tray::TRAY_DOUBLE_CLICK),
            Some(MenuAction::OpenSettings)
        ));
        assert!(menu_action_for(9999).is_none());
    }

    #[test]
    fn dpi_menu_ids_roundtrip_values() {
        for dpi in [100u16, 800, 16000, 30000] {
            let id = menu_id::DPI_FLAG | dpi as u32;
            assert!(matches!(menu_action_for(id), Some(MenuAction::SetDpi(v)) if v == dpi));
        }
        // TRAY_DOUBLE_CLICK has the flag bit set but is outside the value range.
        assert!(!matches!(
            menu_action_for(platform::tray::TRAY_DOUBLE_CLICK),
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
