//! The headless daemon (with system tray): enable driver mode, lock DPI, apply
//! lighting, listen for the DPI-button vendor reports on the readable
//! collections (blocking reads in per-collection threads), inject the
//! configured keystrokes, and serve the tray menu. One process, one exe.

mod config_application;
mod menu;

use config_application::{apply_config_candidate, report_config_application, save_config, SessionError};
use menu::{build_menu_spec, handle_menu, menu_action_for};

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use razer_hid::{aux_collection_paths, Listener, Mouse};
use razer_proto::{DeviceMode, DeviceSpec, DpiButtons, Rgb};
use platform::autostart::Registration;

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
    /// Set one lighting zone's effect kind (zone = a `razer_proto::led` id).
    SetZoneEffectKind(u8, String),
    /// Set one lighting zone's color (zone = a `razer_proto::led` id).
    SetZoneColor(u8, Rgb),
    /// Stage one on-board bind from the settings window (keymap button id,
    /// bind text; blank = drop the entry) and write it to the mouse if valid.
    SetOnboard(u8, String),
    Save,
    OpenSettings,
    SettingsClosed,
    ReloadConfig,
    /// Flip the "Start with Windows" registration. Carries no payload: the
    /// handler reads the current registration at click time and inverts that,
    /// so a stale tray tick self-corrects instead of writing the wrong value.
    ToggleAutostart,
    /// The user clicked Retry on the no-device notice: probe for the mouse
    /// now instead of waiting out the rest of the reconnect backoff.
    RetryProbe,
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

/// Build one remappable-button dropdown for the settings window: display
/// labels, the parallel config values, and the selected index for `current`.
///
/// Index 0 is the button's *identity* entry â€” the dropdown names its own
/// button (there is no separate caption in the window) â€” and maps to the
/// config value `"none"`. The wording carries the none-semantics: a thumb
/// button's identity reads "(default)" because config `none` means no hook
/// and the native Back/Forward behavior, while a DPI button's reads
/// "unbound" because in driver mode a none'd DPI button does nothing.
fn combo_options(current: &str, identity: &str) -> (Vec<String>, Vec<String>, usize) {
    let mut labels = vec![identity.to_string()];
    let mut values = vec!["none".to_string()];
    for p in ACTION_PRESETS.iter().filter(|p| **p != "none") {
        labels.push(p.to_string());
        values.push(p.to_string());
    }
    if !values.iter().any(|v| v == current) {
        labels.push(current.to_string());
        values.push(current.to_string());
    }
    let index = values.iter().position(|v| v == current).unwrap_or(0);
    (labels, values, index)
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

/// Polling dropdown labels ("keep" + the device's supported rates) and the
/// selected index matching the config (`None` -> "keep").
fn polling_options(cfg: &Config, spec: &DeviceSpec) -> (Vec<String>, usize) {
    let mut labels = vec!["keep â€” leave as set".to_string()];
    labels.extend(spec.polling.rates.iter().map(|hz| format!("{hz} Hz")));
    let idx = cfg
        .polling_rate
        .and_then(|hz| spec.polling.rates.iter().position(|&r| r == hz).map(|i| i + 1))
        .unwrap_or(0);
    (labels, idx)
}

/// Effect dropdown labels and the selected index for one zone's effective
/// lighting kind (its config override, else the device-wide setting).
fn effect_options(kind: &str) -> (Vec<String>, usize) {
    let labels: Vec<String> = EFFECT_PRESETS.iter().map(|s| s.to_string()).collect();
    let idx = labels.iter().position(|l| l.eq_ignore_ascii_case(kind)).unwrap_or(0);
    (labels, idx)
}

/// Where a user with an unsupported mouse is sent: the list of every Razer
/// mouse whose protocol OpenRazer already documents, with that device's USB id
/// and transaction id already filled in.
const WISHLIST_URL: &str =
    "https://github.com/asavs/snakecharmer/blob/master/docs/DEVICE-WISHLIST.md";

/// Describe the Razer device that *is* plugged in but isn't supported, if there
/// is one â€” "Razer DeathAdder Chroma (1532:0043)", or just the id when the
/// device reports no product string.
///
/// Enumeration only; the device is never opened or written to. Returns `None`
/// when no Razer hardware is present at all, which is a different message: one
/// case is "plug something in", the other is "this could work, here's how".
fn found_but_unsupported() -> Option<String> {
    let api = razer_hid::open_api().ok()?;
    let found = razer_hid::detected_mice(&api).into_iter().find(|m| !m.supported)?;
    Some(found.to_string())
}

/// "wheel click" -> "Wheel click", the caption of an on-board bind field.
fn onboard_label(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// Spawn the settings window on its own thread (message loop lives there).
/// `spec` supplies the plugged-in device's identity and capabilities: the
/// slider covers the whole DPI range of *that* mouse (16000 on the Elite,
/// 30000 on the V3), its schematic fills the side pane, and the DPI-button
/// / lighting control groups only exist when the hardware does.
fn open_settings_window(
    cfg: &Config,
    spec: DeviceSpec,
    product_id: u16,
    stored: &[crate::onboard::StoredBind],
    tx: &Sender<Event>,
) {
    use platform::settings::ActionCombo;
    // Each dropdown's index-0 entry is its button's identity (mapping to
    // config "none"); the thumb entries say "(default)" â€” no hook, native
    // Back/Forward â€” while the DPI entries say "unbound" â€” in driver mode a
    // none'd DPI button does nothing. Human names only; no XBUTTON jargon.
    let (up_labels, up_values, up_index) = combo_options(&cfg.dpi_up, "Front DPI â€” unbound");
    let (down_labels, down_values, down_index) = combo_options(&cfg.dpi_down, "Rear DPI â€” unbound");
    let (back_labels, back_values, back_index) =
        combo_options(&cfg.thumb_back, "\u{2190} Back (default)");
    let (fwd_labels, fwd_values, fwd_index) =
        combo_options(&cfg.thumb_forward, "\u{2192} Forward (default)");
    let (polling_labels, polling_index) = polling_options(cfg, &spec);
    // One lighting row per zone (wheel, logo, ...), each seeded with that
    // zone's effective effect + color (override, else the device-wide value).
    let lighting: Vec<platform::settings::LightingZoneInit> = spec
        .rgb_zones
        .iter()
        .map(|&zone| {
            let name = razer_proto::led::name(zone);
            let (effect_labels, effect_index) = effect_options(cfg.zone_lighting(name));
            let color = Rgb::parse_hex(cfg.zone_color(name)).unwrap_or(Rgb::new(0, 0xFF, 0));
            platform::settings::LightingZoneInit {
                label: razer_proto::led::label(zone).to_string(),
                effect_labels,
                effect_index,
                color: (color.r, color.g, color.b),
            }
        })
        .collect();
    // On-board bind controls exist only for mice with a recorded keymap: one
    // per writable button, mounted at its CalloutSlot::Button on the diagram
    // and opening on its [onboard] entry. They replace the thumb dropdowns.
    let onboard = razer_proto::keymap::spec_for(spec.product_id).map(|km| {
        crate::onboard::controls(km, &cfg.onboard, stored)
            .into_iter()
            .map(|c| platform::settings::OnboardButton {
                id: c.button,
                label: onboard_label(c.name),
                presets: c.presets,
                text: c.text,
            })
            .collect()
    });
    let init = platform::settings::SettingsInit {
        device_name: spec.name.to_string(),
        diagram: spec.diagram,
        dpi: cfg.dpi,
        dpi_min: spec.dpi_min,
        dpi_max: spec.dpi_max,
        polling_labels,
        polling_index,
        dpi_buttons: spec.dpi_buttons.map(|_| platform::settings::DpiButtonsInit {
            up: ActionCombo { labels: up_labels, index: up_index },
            down: ActionCombo { labels: down_labels, index: down_index },
        }),
        thumb_back: ActionCombo { labels: back_labels, index: back_index },
        thumb_forward: ActionCombo { labels: fwd_labels, index: fwd_index },
        lighting,
        onboard,
        // Use the bus identity, including its current attach-method alias.
        input_device: Some(platform::settings::UsbDeviceId {
            vendor_id: razer_proto::VENDOR_ID,
            product_id,
        }),
    };
    let tx = tx.clone();
    thread::spawn(move || {
        use platform::settings::SettingsEvent as SE;
        // Clear the daemon's `settings_open` latch when this thread ends, on a
        // normal return *or* a panic during window construction. Without this,
        // any failure to complete `open` would leave the latch stuck `true`
        // and silently swallow every later "open settings" click.
        struct CloseGuard(Sender<Event>);
        impl Drop for CloseGuard {
            fn drop(&mut self) {
                let _ = self.0.send(Event::Menu(MenuAction::SettingsClosed));
            }
        }
        let _close_guard = CloseGuard(tx.clone());
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
                // Dropdown indices map through the parallel *values* lists
                // (index 0 = the identity entry = config "none").
                SE::UpAction(i) => up_values.get(i).map(|s| MenuAction::SetUpAction(s.clone())),
                SE::DownAction(i) => {
                    down_values.get(i).map(|s| MenuAction::SetDownAction(s.clone()))
                }
                SE::ThumbBack(i) => {
                    back_values.get(i).map(|s| MenuAction::SetThumbBack(s.clone()))
                }
                SE::ThumbForward(i) => {
                    fwd_values.get(i).map(|s| MenuAction::SetThumbForward(s.clone()))
                }
                // Lighting events carry the zone's row index = its position
                // in the spec's rgb_zones (the rows were built in that order).
                SE::Effect(zi, i) => spec.rgb_zones.get(zi).and_then(|&zone| {
                    EFFECT_PRESETS
                        .get(i)
                        .map(|kind| MenuAction::SetZoneEffectKind(zone, kind.to_string()))
                }),
                SE::Color(zi, r, g, b) => spec
                    .rgb_zones
                    .get(zi)
                    .map(|&zone| MenuAction::SetZoneColor(zone, Rgb::new(r, g, b))),
                SE::Onboard(button, text) => Some(MenuAction::SetOnboard(
                    button,
                    crate::onboard::choice_to_bind(button, &text),
                )),
                SE::Save => Some(MenuAction::Save),
            };
            if let Some(c) = cmd {
                let _ = tx_ev.send(Event::Menu(c));
            }
        });
        // `_close_guard` sends SettingsClosed as this thread unwinds (see above).
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

/// Flip the "Start with Windows" registration and re-render the tray tick.
///
/// Shared by the in-session and no-device paths (the action is
/// device-independent; `spec` only shapes the rebuilt menu). Reads the current
/// registration at click time. An unreadable value is left alone, and a
/// failed mutation re-reads registration rather than assuming success.
fn toggle_autostart(spec: Option<&DeviceSpec>, log: &Logger) {
    let want = match platform::autostart::registration() {
        Registration::Registered => Registration::Absent,
        Registration::Absent => Registration::Registered,
        Registration::Unknown => {
            log.log("WARN startup registration is unreadable; leaving it unchanged.");
            platform::tray::set_menu(build_menu_spec(spec, Registration::Unknown));
            return;
        }
    };
    let attempt =
        if want == Registration::Registered { platform::autostart::register() } else { platform::autostart::unregister() };
    let actual = match attempt {
        Ok(state) => state,
        Err(e) => {
            log.log(&format!("WARN could not update startup registration: {e}"));
            platform::autostart::registration()
        }
    };
    let describe = |state| match state {
        Registration::Registered => "registered",
        Registration::Absent => "absent",
        Registration::Unknown => "unavailable",
    };
    if actual == want {
        log.log(&format!("Menu: startup request -> {}. Windows startup settings can override the request.", describe(actual)));
    } else {
        log.log(&format!(
            "WARN startup request: wanted {}, registration read-back is {}.",
            describe(want),
            describe(actual)
        ));
    }
    platform::tray::set_menu(build_menu_spec(spec, actual));
}

/// What the startup autostart pass should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutostartBoot {
    Register,
    Refresh,
    Nothing,
}

/// never bootstrapped -> Register (on by default, exactly once)
/// bootstrapped + registered -> Refresh (repair a stale executable path)
/// bootstrapped + absent -> Nothing (respect the removed registration)
/// unreadable -> Nothing (do not overwrite a registration we cannot inspect)
fn autostart_boot_plan(bootstrapped: bool, registration: Registration) -> AutostartBoot {
    match (bootstrapped, registration) {
        (_, Registration::Unknown) => AutostartBoot::Nothing,
        (false, _) => AutostartBoot::Register,
        (true, Registration::Registered) => AutostartBoot::Refresh,
        (true, Registration::Absent) => AutostartBoot::Nothing,
    }
}

/// First-run "Start with Windows" registration, and exe-path self-heal on
/// later launches. Skipped entirely in debug builds so `cargo run` never
/// registers (or refreshes to) a `target\debug` exe in the user's login.
///
/// The latch is set whether or not the write succeeded â€” we get one shot; a
/// machine where the write always fails must not retry forever.
fn bootstrap_autostart(cfg: &mut Config, log: &Logger) {
    if cfg!(debug_assertions) {
        return;
    }
    let registration = platform::autostart::registration();
    match autostart_boot_plan(cfg.autostart_bootstrapped, registration) {
        AutostartBoot::Register => {
            match platform::autostart::register() {
                Ok(Registration::Registered) => log.log("Start with Windows requested (first run); Windows startup settings can override it."),
                Ok(_) => log.log("WARN startup registration did not read back as registered."),
                Err(e) => log.log(&format!("WARN could not register Start with Windows: {e}")),
            }
            cfg.autostart_bootstrapped = true;
            save_config(cfg, log);
        }
        AutostartBoot::Refresh => {
            if let Err(e) = platform::autostart::refresh() {
                log.log(&format!("WARN could not refresh Start with Windows entry: {e}"));
            }
        }
        AutostartBoot::Nothing => {
            if registration == Registration::Unknown {
                log.log("WARN startup registration is unreadable; skipping registration refresh.");
            }
        }
    }
}

// --- Daemon ---------------------------------------------------------------

/// Wait for a supported mouse, then run until Quit or a terminal device failure.
pub fn run(mut cfg: Config, log: Logger) -> ! {
    let mut health = crate::health::HealthReporter::new();
    if let Err(error) = health.starting(cfg.polling_rate) {
        log.log(&format!("WARN could not publish PC Vitals capsule: {error}"));
    }
    log.log(&format!(
        "Daemon starting. dpi={:?} up={:?} down={:?} lighting={:?} reassert={}s",
        cfg.dpi_xy(),
        cfg.dpi_up,
        cfg.dpi_down,
        cfg.lighting,
        cfg.reassert_interval_secs
    ));

    // Register/refresh "Start with Windows" before the tray spawns, so the
    // initial menu reflects the registration read-back.
    bootstrap_autostart(&mut cfg, &log);

    let (tx, rx) = mpsc::channel::<Event>();

    // Spawn the tray icon + menu once, on its own thread.
    {
        let tx = tx.clone();
        thread::spawn(move || {
            platform::tray::run(
                "Snakecharmer",
                build_menu_spec(None, platform::autostart::registration()),
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
    let popup_open = Arc::new(AtomicBool::new(false));
    let mut prev_delay = Duration::ZERO; // no retry slept yet
    loop {
        let started = Instant::now();
        let result = run_session(
            &mut cfg, &mut bindings, &log, &tx, &rx, &mut settings_open, &mut health,
        );
        let delay = next_retry_delay(prev_delay, started.elapsed());
        match result {
            Ok(()) => log.log("Session ended cleanly; restarting."),
            Err(SessionError::Config(e)) => {
                let message = format!("Configuration rejected: {e}\n\nThe mouse settings were not applied. Fix config.toml and restart Snakecharmer.");
                log.log(&message);
                if let Err(error) = health.stopped() {
                    log.log(&format!("WARN could not publish PC Vitals capsule: {error}"));
                }
                platform::mouse_hook::uninstall();
                platform::alert("Snakecharmer: configuration error", &message);
                std::process::exit(1);
            }
            Err(SessionError::Device(e)) => {
                if e.requires_stop() {
                    stop_for_safety(&e, &log, &mut health);
                }
                log.log(&format!(
                    "Session ended: {e}. Retrying in {}s (mouse unplugged/asleep?).",
                    delay.as_secs()
                ));
                if let Err(error) = health.session_failed(&e, delay.as_secs() as u32) {
                    log.log(&format!("WARN could not publish PC Vitals capsule: {error}"));
                }
            }
        }
        service_retry_gap(delay, &tx, &rx, &log, &mut health, &mut settings_open, &popup_open);
        prev_delay = delay;
    }
}

/// Service tray/menu events for `delay` while no device is connected, instead
/// of sleeping deaf through the session-retry gap. With a plain sleep here,
/// "Open settings" and "Quit" clicks queued unhandled â€” Quit could not exit
/// the app â€” and everything replayed in a burst once a mouse reconnected.
///
/// No-device semantics: Quit exits cleanly (same path as in-session), Open
/// settings gets a notice box â€” or focuses a settings window still open from
/// the previous session â€” Start with Windows toggles as normal (it is
/// device-independent), and device-bound commands are dropped with a log
/// line. The notice box runs on a throwaway thread so a dialog left open never
/// stalls the reconnect clock; `popup_open` collapses repeat clicks while one
/// is already up. Never shows UI on its own â€” only in response to a click â€”
/// so the retry cycle itself stays silent.
///
/// The notice's Retry button ("I plugged it in") sends [`MenuAction::RetryProbe`],
/// which ends the gap early so the next probe happens immediately, and re-queues
/// the settings request it stands for â€” a successful probe opens the window the
/// user originally asked for. Probing is gated on that explicit click â€” a user
/// without the mouse can't trigger re-enumeration by simply clicking the tray.
fn service_retry_gap(
    delay: Duration,
    tx: &Sender<Event>,
    rx: &Receiver<Event>,
    log: &Logger,
    health: &mut crate::health::HealthReporter,
    settings_open: &mut bool,
    popup_open: &Arc<AtomicBool>,
) {
    let deadline = Instant::now() + delay;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return;
        }
        match rx.recv_timeout(remaining) {
            Ok(Event::Menu(MenuAction::Quit)) => shutdown(log, health, None),
            Ok(Event::Menu(MenuAction::OpenSettings)) => {
                if *settings_open {
                    // A window from the previous session is still up; surface
                    // it (it may be buried) instead of showing the notice.
                    platform::settings::focus();
                    log.log("Settings already open; brought to foreground (no device).");
                } else if popup_open
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    log.log("Settings requested with no device; showing notice.");
                    let popup_open = Arc::clone(popup_open);
                    let tx = tx.clone();
                    thread::spawn(move || {
                        let retry = match found_but_unsupported() {
                            // A Razer mouse *is* plugged in, just not one
                            // we drive. This is the moment a user most wants
                            // this to work, so name the device and point at
                            // the page that says adding it is small - its
                            // protocol is very likely already documented.
                            // Retry would only find the same unsupported
                            // device again, so the choice offered here is
                            // the one that can actually help.
                            Some(found) => {
                                if platform::alert_yes_no(
                                    "Snakecharmer",
                                    &format!(
                                        "Found {found}, which isn't supported yet.\n\n\
                                         Its protocol is very likely already documented, so adding it \
                                         is a small job. You may only need to report a couple of \
                                         numbers from the device.\n\n\
                                         Open the page that lists what's already known about it?"
                                    ),
                                ) {
                                    platform::open_url(WISHLIST_URL);
                                }
                                false
                            }
                            None => {
                                let supported: Vec<&str> = razer_proto::devices::SUPPORTED
                                    .iter()
                                    .map(|s| s.name)
                                    .collect();
                                platform::alert_retry(
                                    "Snakecharmer",
                                    &format!(
                                        "No supported Razer mouse found.\n\n\
                                         Supported: {}.\n\n\
                                         If you've just plugged one in, click Retry to check now.",
                                        supported.join(", ")
                                    ),
                                )
                            }
                        };
                        popup_open.store(false, Ordering::SeqCst);
                        if retry {
                            let _ = tx.send(Event::Menu(MenuAction::RetryProbe));
                        }
                    });
                }
            }
            Ok(Event::Menu(MenuAction::SettingsClosed)) => {
                *settings_open = false;
                log.log("Settings window closed.");
            }
            Ok(Event::Menu(MenuAction::RetryProbe)) => {
                // Returning ends the gap; the outer loop re-attempts the
                // session (one probe) right away. RetryProbe only ever comes
                // from the settings-triggered notice above, so the user is
                // still waiting on that window: re-queue the request for the
                // next session's loop to pick up once the mouse opens. If the
                // probe fails instead, the event lands back here and re-shows
                // the notice â€” also right.
                log.log("User confirmed mouse plugged in; probing now.");
                let _ = tx.send(Event::Menu(MenuAction::OpenSettings));
                return;
            }
            // Device-independent, so it must not fall into the catch-all
            // below and go silently dead while the mouse is unplugged.
            Ok(Event::Menu(MenuAction::ToggleAutostart)) => toggle_autostart(None, log),
            Ok(Event::Menu(action)) => {
                // DPI presets, lighting picks, settings edits: nothing to
                // apply them to. Drop them now rather than queue-and-burst
                // on reconnect.
                log.log(&format!("Menu: {action:?} ignored (no device connected)."));
            }
            // Stragglers from the session that just ended.
            Ok(Event::Button(_)) | Ok(Event::ListenerClosed) => {}
            Err(RecvTimeoutError::Timeout) => return,
            // Every sender gone (tray thread died) â€” nothing left to service;
            // wait out the rest of the gap the old-fashioned way.
            Err(RecvTimeoutError::Disconnected) => {
                thread::sleep(remaining);
                return;
            }
        }
    }
}

/// Report a terminal device failure without a recovery write or another session.
fn stop_for_safety(error: &razer_hid::Error, log: &Logger, health: &mut crate::health::HealthReporter) -> ! {
    razer_hid::stop_commands(error);
    let message = format!("Device communication stopped: {error}.\n\nNo further commands will be sent. Check that the mouse still moves and clicks before restarting Snakecharmer.");
    log.log(&format!("SAFETY STOP: {message}"));
    if let Err(error) = health.session_failed(error, 0) {
        log.log(&format!("WARN could not publish PC Vitals capsule: {error}"));
    }
    platform::mouse_hook::uninstall();
    platform::alert("Snakecharmer ? device communication stopped", &message);
    std::process::exit(1);
}

/// Clean daemon exit, shared by the in-session and no-device Quit paths:
/// put the mouse back in hardware mode (`driver`: the session switched it to
/// driver mode, so without this its DPI buttons go dead until a replug),
/// publish the stopped capsule, remove the global mouse hook, end the process.
fn shutdown(log: &Logger, health: &mut crate::health::HealthReporter, driver: Option<&Mouse>) -> ! {
    log.log("Quit selected; removing mouse hook and exiting.");
    if let Some(ctrl) = driver {
        match ctrl.set_device_mode(DeviceMode::Hardware) {
            Ok(_) => log.log("Hardware mode restored."),
            Err(e) => stop_for_safety(&e, log, health),
        }
    }
    if let Err(error) = health.stopped() {
        log.log(&format!("WARN could not publish PC Vitals capsule: {error}"));
    }
    platform::mouse_hook::uninstall();
    std::process::exit(0);
}

/// Shortest / longest session-retry delays. Each retry re-enumerates HID, so a
/// device that keeps flapping gets exponentially fewer pokes (3s -> 6s -> 12s
/// -> 24s -> 30s cap); a session that held for a while resets the ladder.
const RETRY_MIN: Duration = Duration::from_secs(3);
const RETRY_MAX: Duration = Duration::from_secs(30);
/// A session that survived this long counts as a healthy device.
const RETRY_HEALTHY: Duration = Duration::from_secs(60);

/// The delay to sleep before the next session attempt, given the previously
/// slept delay (`Duration::ZERO` = none yet) and how long the session that
/// just ended had lasted.
fn next_retry_delay(prev: Duration, session_lasted: Duration) -> Duration {
    if session_lasted >= RETRY_HEALTHY || prev.is_zero() {
        RETRY_MIN
    } else {
        (prev * 2).min(RETRY_MAX)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MaintenanceTask {
    Reassert,
    Liveness,
}

/// Event traffic never advances these timestamps. Only successful maintenance
/// schedules its next deadline; a full reassert also satisfies the liveness read.
struct Maintenance {
    reasserted: Instant,
    contacted: Instant,
}

impl Maintenance {
    fn new(now: Instant) -> Self {
        Self { reasserted: now, contacted: now }
    }

    fn wait(&self, now: Instant, interval: Duration, has_listeners: bool) -> Duration {
        let reassert = interval.saturating_sub(now.duration_since(self.reasserted));
        if has_listeners {
            reassert
        } else {
            reassert.min(Duration::from_secs(5).saturating_sub(now.duration_since(self.contacted)))
        }
    }

    fn service_due<E>(
        &mut self,
        now: Instant,
        interval: Duration,
        has_listeners: bool,
        perform: impl FnOnce(MaintenanceTask) -> Result<Instant, E>,
    ) -> Result<(), E> {
        let task = if now.duration_since(self.reasserted) >= interval {
            MaintenanceTask::Reassert
        } else if !has_listeners && now.duration_since(self.contacted) >= Duration::from_secs(5) {
            MaintenanceTask::Liveness
        } else {
            return Ok(());
        };
        // Completion time avoids catch-up bursts after a slow command. An error
        // leaves the schedule alone and propagates out of the session at once.
        let completed = perform(task)?;
        self.contacted = completed;
        if task == MaintenanceTask::Reassert {
            self.reasserted = completed;
        }
        Ok(())
    }
}

fn run_session(
    cfg: &mut Config,
    bindings: &mut Bindings,
    log: &Logger,
    tx: &Sender<Event>,
    rx: &Receiver<Event>,
    settings_open: &mut bool,
    health: &mut crate::health::HealthReporter,
) -> Result<(), SessionError> {
    let api = razer_hid::open_api()?;
    let ctrl = Mouse::open_with(&api)?;
    let spec = ctrl.spec();
    // The id the device actually enumerated as â€” a spec may cover more than one
    // (the DeathAdder V4 Pro is wired 0x00BE, wireless 0x00BF). Report and
    // enumerate by this one; `spec.product_id` is only the canonical id.
    let pid = ctrl.product_id();
    if let Err(error) = health.connected(spec.name, 0x1532, pid) {
        log.log(&format!("WARN could not publish PC Vitals capsule: {error}"));
    }
    log.log(&format!(
        "Opened {} (PID 0x{:04X}, txn 0x{:02X}; rgb={}, dpi_buttons={}).",
        spec.name,
        pid,
        spec.transaction_id,
        spec.has_rgb(),
        spec.dpi_buttons.is_some()
    ));
    // Rebuild the tray menu around this device (DPI presets over its range).
    platform::tray::set_menu(build_menu_spec(Some(&spec), platform::autostart::registration()));

    let outcome = apply_config_candidate(cfg, cfg.clone(), spec, |candidate| {
        // Each setter confirms its read form before writing. A device failure
        // still ends the session; a local config error is caught before here.
        let (dx, dy) = candidate.dpi_xy();
        let (rx2, ry) = ctrl.set_dpi(dx, dy)?;
        log.log(&format!("DPI locked to {rx2} x {ry}."));
        if let Some(hz) = candidate.polling_rate {
            let r = ctrl.set_polling_rate(hz)?;
            log.log(&format!("Polling rate locked to {r} Hz."));
        }
        apply_startup_lighting(&ctrl, candidate, log)?;
        check_onboard_binds(&ctrl, candidate, log)
    })?;
    report_config_application(outcome, cfg, log);
    *bindings = Bindings::from_config(cfg, log);
    apply_thumb_hook(cfg, log);

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
        for path in aux_collection_paths(&api, ctrl.product_id()) {
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
            return Err(razer_hid::Error::DeviceNotFound.into());
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

    // With listeners, an unplug fails a blocking read almost instantly. Without
    // them (a device with no DPI buttons), nothing notices until we next talk to
    // the device â€” so maintain a separate five-second liveness deadline.
    let has_listeners = spec.dpi_buttons.is_some();
    let mut maintenance = Maintenance::new(Instant::now());
    let mut last_fire: Option<(u8, Instant)> = None;

    loop {
        // A reset on the notice thread may have tripped the shared latch.
        razer_hid::ensure_active()?;
        let interval = Duration::from_secs(cfg.reassert_interval_secs.max(5));
        let wait = maintenance.wait(Instant::now(), interval, has_listeners)
            .min(health.refresh_due_in());
        match rx.recv_timeout(wait) {
            Ok(Event::Button(code)) => {
                handle_code(code, spec.dpi_buttons, bindings, log, &mut last_fire)
            }
            Ok(Event::Menu(MenuAction::OpenSettings)) => {
                if *settings_open {
                    // The window already exists; surface it (it may be buried
                    // behind other windows) instead of doing nothing.
                    platform::settings::focus();
                    log.log("Settings already open; brought to foreground.");
                } else {
                    *settings_open = true;
                    // The on-board controls open on what the mouse stores, so
                    // a leftover bind shows as itself. A failed read stops the session.
                    let stored = razer_proto::keymap::spec_for(spec.product_id)
                        .map(|km| crate::onboard::read_binds(&ctrl, km))
                        .transpose()?
                        .unwrap_or_default();
                    open_settings_window(cfg, spec, pid, &stored, tx);
                    log.log("Opened settings window.");
                }
            }
            Ok(Event::Menu(MenuAction::SettingsClosed)) => {
                *settings_open = false;
                log.log("Settings window closed.");
            }
            Ok(Event::Menu(action)) => {
                if handle_menu(action, &ctrl, cfg, bindings, log, health)? {
                    shutdown(log, health, spec.dpi_buttons.is_some().then_some(&ctrl));
                }
            }
            Ok(Event::ListenerClosed) => {
                // Losing an input collection is a stop condition, not a reason
                // to send another probe or restart writes on a new handle.
                return Err(razer_hid::Error::Verify("device input collection closed".into()).into());
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err(razer_hid::Error::Verify("event channel closed".into()).into())
            }
        }
        // Service deadlines after events as well as timeouts. In particular,
        // button presses and health refreshes cannot postpone device checks.
        // A reload may have changed the interval while handling this event.
        let interval = Duration::from_secs(cfg.reassert_interval_secs.max(5));
        maintenance.service_due(Instant::now(), interval, has_listeners, |task| {
            match task {
                MaintenanceTask::Reassert => reassert(&ctrl, cfg, log)?,
                MaintenanceTask::Liveness => { ctrl.get_dpi()?; }
            }
            Ok::<_, razer_hid::Error>(Instant::now())
        })?;
        if let Err(error) = health.refresh_if_due() {
            log.log(&format!("WARN could not refresh PC Vitals capsule: {error}"));
        }
    }
}

/// Write the `[onboard]` binds the config asks for (only the ones that differ)
/// and return the buttons it configures. A bad config configures nothing;
/// a failed device command stops the session.
fn write_onboard_config(
    ctrl: &Mouse,
    km: &'static razer_proto::keymap::KeymapSpec,
    cfg: &Config,
    log: &Logger,
) -> Result<Vec<u8>, razer_hid::Error> {
    use crate::onboard;
    let wanted = match onboard::parse_config(km, &cfg.onboard) {
        Ok(w) => w,
        Err(e) => {
            log.log(&format!("WARN [onboard] not applied: {e}"));
            Vec::new()
        }
    };
    for c in onboard::apply(ctrl, km, &wanted)? {
        log.log(&format!("On-board bind written: {c}"));
    }
    Ok(wanted.iter().map(|w| w.button).collect())
}

/// Once per attach: write the `[onboard]` binds the config asks for (only the
/// ones that differ), then read the rest and, if any are non-default leftovers,
/// log them and offer to reset them. Device errors end the session. The notice
/// runs on a throwaway thread and is shown once per distinct set of leftovers.
fn check_onboard_binds(ctrl: &Mouse, cfg: &Config, log: &Logger) -> Result<(), razer_hid::Error> {
    use crate::onboard;
    let spec = ctrl.spec();
    let Some(km) = razer_proto::keymap::spec_for(spec.product_id) else {
        if !cfg.onboard.is_empty() {
            log.log(&format!("WARN [onboard] ignored: on-board binds aren't supported on the {}", spec.name));
        }
        return Ok(());
    };
    let configured = write_onboard_config(ctrl, km, cfg, log)?;
    let binds = onboard::read_binds(ctrl, km)?;
    let left = onboard::leftovers(&binds, &configured);
    if left.is_empty() {
        log.log("On-board binds match the config and the defaults.");
        return Ok(());
    }
    let described = onboard::describe(&left);
    log.log(&format!("WARN non-default binds stored on the mouse: {described}."));
    if onboard::first_time(&described) {
        let text = onboard::notice_text(spec.name, &described);
        let log = log.clone();
        thread::spawn(move || {
            if platform::alert_yes_no("Snakecharmer", &text) {
                reset_leftovers(km, &configured, &log);
            }
        });
    }
    Ok(())
}

/// The "reset them now?" answer. Runs on the notice's thread with its own
/// handle; a clash with the session's periodic reassert fails a read-back,
/// which is reported rather than retried.
fn reset_leftovers(km: &'static razer_proto::keymap::KeymapSpec, configured: &[u8], log: &Logger) {
    use crate::onboard;
    let result = Mouse::open()
        .and_then(|m| onboard::reset(&m, km, configured));
    let message = match result {
        Ok(out) => {
            for c in &out.changes {
                log.log(&format!("On-board bind reset: {c}"));
            }
            let mut m = format!("Reset {} button(s) to the default bind.", out.changes.len());
            if !out.stuck.is_empty() {
                m += &format!(
                    "\n\nCan't reset {} yet: no recorded write for that button.",
                    out.stuck.join(", ")
                );
            }
            m
        }
        Err(e) => {
            log.log(&format!("WARN on-board reset failed: {e}"));
            format!("The reset didn't complete: {e}\n\nDevice communication has stopped. Check that the mouse still moves and clicks before restarting Snakecharmer.")
        }
    };
    platform::alert("Snakecharmer", &message);
}

fn apply_startup_lighting(ctrl: &Mouse, cfg: &Config, log: &Logger) -> Result<(), razer_hid::Error> {
    if !ctrl.spec().has_rgb() {
        if !cfg.lighting.eq_ignore_ascii_case("keep") {
            log.log(&format!(
                "{} has no lighting hardware; ignoring lighting={:?}.",
                ctrl.spec().name,
                cfg.lighting
            ));
        }
        return Ok(());
    }
    // Each zone resolves its own effect + color (override, else device-wide),
    // so the wheel and logo can carry different lighting.
    for &zone in ctrl.spec().rgb_zones {
        let name = razer_proto::led::name(zone);
        match EffectSpec::from_config(cfg.zone_lighting(name), cfg.zone_color(name)) {
            Ok(Some(effect)) => {
                effect.apply_zone(ctrl, zone)?;
                log.log(&format!("Lighting ({name}) set to {}.", effect.describe()));
            }
            Ok(None) => {} // "keep": leave this zone untouched
            Err(e) => log.log(&format!("WARN invalid {name} lighting config: {e}")),
        }
    }
    Ok(())
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
                razer_hid::stop_commands(&e);
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
    fn continuous_button_events_do_not_postpone_reassertion() {
        let started = Instant::now();
        let mut maintenance = Maintenance::new(started);
        let interval = Duration::from_secs(60);
        let (tx, rx) = mpsc::channel();
        let mut commands = Vec::new();
        // Every receive returns a button event, never Timeout. Maintenance
        // still reaches its deadlines while the queue is continuously active.
        for second in 1..=120 {
            let now = started + Duration::from_secs(second);
            tx.send(Event::Button(0x20)).unwrap();
            assert!(matches!(rx.recv_timeout(maintenance.wait(now, interval, true)),
                Ok(Event::Button(0x20))));
            maintenance.service_due(now, interval, true, |task| {
                commands.push((second, task));
                Ok::<_, ()>(now)
            }).unwrap();
        }
        assert_eq!(commands, vec![(60, MaintenanceTask::Reassert), (120, MaintenanceTask::Reassert)]);
    }

    #[test]
    fn liveness_runs_during_traffic_and_reassertion_replaces_the_same_tick_read() {
        let started = Instant::now();
        let mut maintenance = Maintenance::new(started);
        let interval = Duration::from_secs(60);
        let mut commands = Vec::new();
        for second in 1..=60 {
            let now = started + Duration::from_secs(second);
            maintenance.service_due(now, interval, false, |task| {
                commands.push((second, task));
                Ok::<_, ()>(now)
            }).unwrap();
        }
        let mut expected: Vec<_> = (5..60).step_by(5)
            .map(|second| (second, MaintenanceTask::Liveness)).collect();
        expected.push((60, MaintenanceTask::Reassert));
        assert_eq!(commands, expected);
        assert_eq!(maintenance.wait(started + Duration::from_secs(60), interval, false),
            Duration::from_secs(5));
    }

    #[test]
    fn earlier_health_wakeup_does_not_advance_the_device_deadline() {
        let started = Instant::now();
        let mut maintenance = Maintenance::new(started);
        let interval = Duration::from_secs(60);
        let now = started + Duration::from_secs(4);
        assert_eq!(maintenance.wait(now, interval, false), Duration::from_secs(1));
        let health_due_in = Duration::ZERO;
        assert_eq!(maintenance.wait(now, interval, false).min(health_due_in), Duration::ZERO);
        maintenance.service_due(now, interval, false, |_| -> Result<Instant, ()> {
            panic!("a health-only wakeup must not send a device command")
        }).unwrap();
        assert_eq!(maintenance.wait(now, interval, false), Duration::from_secs(1));
        let due = started + Duration::from_secs(5);
        let mut commands = Vec::new();
        maintenance.service_due(due, interval, false, |task| {
            commands.push(task);
            Ok::<_, ()>(due)
        }).unwrap();
        assert_eq!(commands, vec![MaintenanceTask::Liveness]);
    }

    #[test]
    fn reloaded_interval_takes_effect_without_restarting_the_session() {
        let started = Instant::now();
        let mut maintenance = Maintenance::new(started);
        let now = started + Duration::from_secs(10);
        assert_eq!(maintenance.wait(now, Duration::from_secs(60), true), Duration::from_secs(50));
        let mut commands = Vec::new();
        maintenance.service_due(now, Duration::from_secs(5), true, |task| {
            commands.push(task);
            Ok::<_, ()>(now)
        }).unwrap();
        assert_eq!(commands, vec![MaintenanceTask::Reassert]);
        assert_eq!(maintenance.wait(now, Duration::from_secs(120), true), Duration::from_secs(120));
    }

    #[test]
    fn slow_maintenance_uses_completion_time_without_catch_up_commands() {
        let started = Instant::now();
        let mut maintenance = Maintenance::new(started);
        let interval = Duration::from_secs(60);
        let mut commands = Vec::new();
        // Simulate resuming long after the deadline, followed by a slow read.
        let now = started + Duration::from_secs(600);
        let completed = now + Duration::from_secs(15);
        maintenance.service_due(now, interval, false, |task| {
            commands.push(task);
            Ok::<_, ()>(completed)
        }).unwrap();
        maintenance.service_due(completed, interval, false, |_| -> Result<Instant, ()> {
            panic!("one delayed reassert must not cause a catch-up burst")
        }).unwrap();
        assert_eq!(commands, vec![MaintenanceTask::Reassert]);
        assert_eq!(maintenance.wait(completed, interval, false), Duration::from_secs(5));
        assert_eq!(maintenance.wait(completed, interval, true), interval);
    }

    #[test]
    fn maintenance_failure_propagates_without_marking_the_command_successful() {
        let started = Instant::now();
        let mut maintenance = Maintenance::new(started);
        let interval = Duration::from_secs(60);
        let now = started + interval;
        let mut commands = Vec::new();
        let result = maintenance.service_due(now, interval, false, |task| {
            commands.push(task);
            Err("device failure")
        });
        assert_eq!(result, Err("device failure"));
        assert_eq!(commands, vec![MaintenanceTask::Reassert]);
        assert_eq!(maintenance.wait(now, interval, false), Duration::ZERO);
    }

    #[test]
    fn onboard_labels_capitalize_the_button_name() {
        assert_eq!(onboard_label("wheel click"), "Wheel click");
        assert_eq!(onboard_label(""), "");
    }

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
    fn combo_options_identity_maps_to_none() {
        // Index 0 is always the identity entry, and it maps to config "none";
        // "none" itself never appears as a visible entry.
        let (labels, values, idx) = combo_options("none", "\u{2190} Back (default)");
        assert_eq!(labels[0], "\u{2190} Back (default)");
        assert_eq!(values[0], "none");
        assert_eq!(idx, 0, "config none selects the identity entry");
        assert!(!labels.iter().any(|l| l == "none"), "no raw none entry on screen");
        assert_eq!(labels.len(), values.len(), "labels and values stay parallel");

        // A preset value selects its own (label == value) row.
        let cfg = Config::default(); // dpi_up=copy, dpi_down=paste
        let (labels, values, up) = combo_options(&cfg.dpi_up, "Front DPI â€” unbound");
        assert_eq!(values[up], "copy");
        assert_eq!(labels[up], "copy");

        // A custom value not in the presets must be appended and selected.
        let (labels, values, idx) = combo_options("ctrl+shift+v", "Rear DPI â€” unbound");
        assert_eq!(values[idx], "ctrl+shift+v");
        assert_eq!(labels[idx], "ctrl+shift+v");
        assert!(values.iter().filter(|v| *v == "ctrl+shift+v").count() == 1);
    }

    #[test]
    fn retry_delay_backs_off_and_resets() {
        let s = Duration::from_secs;
        // First-ever retry, and any retry after a healthy session: minimum.
        assert_eq!(next_retry_delay(Duration::ZERO, s(0)), RETRY_MIN);
        assert_eq!(next_retry_delay(s(30), s(3600)), RETRY_MIN);
        assert_eq!(next_retry_delay(s(24), RETRY_HEALTHY), RETRY_MIN);
        // Fast failures escalate: 3 -> 6 -> 12 -> 24 -> 30 (capped).
        assert_eq!(next_retry_delay(s(3), s(1)), s(6));
        assert_eq!(next_retry_delay(s(6), s(1)), s(12));
        assert_eq!(next_retry_delay(s(12), s(1)), s(24));
        assert_eq!(next_retry_delay(s(24), s(1)), s(30));
        assert_eq!(next_retry_delay(s(30), s(1)), s(30));
    }

    #[test]
    fn retry_probe_ends_gap_and_requeues_settings() {
        // Retry only ever comes from the settings-triggered notice, so the
        // gap must both end early AND re-queue OpenSettings for the session
        // it hands off to â€” otherwise the reconnect "silently succeeds" and
        // the window the user asked for never appears.
        let (tx, rx) = mpsc::channel();
        tx.send(Event::Menu(MenuAction::RetryProbe)).unwrap();
        let dir = std::env::temp_dir().join("snakecharmer-test-retry-gap");
        let log = Logger::new(dir.join("daemon.log"));
        let mut health = crate::health::HealthReporter::with_paths(
            dir.join("capsule.json"),
            dir.join("provider-salt"),
        );
        let mut settings_open = false;
        let popup_open = Arc::new(AtomicBool::new(false));
        let start = Instant::now();
        service_retry_gap(
            Duration::from_secs(30),
            &tx,
            &rx,
            &log,
            &mut health,
            &mut settings_open,
            &popup_open,
        );
        assert!(start.elapsed() < Duration::from_secs(5), "RetryProbe must end the gap early");
        assert!(
            matches!(rx.try_recv(), Ok(Event::Menu(MenuAction::OpenSettings))),
            "the settings request must be re-queued for the next session"
        );
    }

    #[test]
    fn polling_options_track_config_and_spec() {
        let v3 = razer_proto::DEATHADDER_V3;
        let mut cfg = Config::default(); // polling_rate = None
        let (labels, idx) = polling_options(&cfg, &v3);
        assert_eq!(labels[0], "keep â€” leave as set");
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
        let (labels, idx) = effect_options(cfg.zone_lighting("wheel"));
        assert_eq!(labels[idx], "keep");
        cfg.lighting = "spectrum".into();
        let (labels, idx) = effect_options(cfg.zone_lighting("wheel"));
        assert_eq!(labels[idx], "spectrum");
        // A zone override wins over the device-wide kind.
        cfg.zones.insert(
            "logo".into(),
            crate::config::ZoneConfig { lighting: Some("breathing".into()), color: None },
        );
        let (labels, idx) = effect_options(cfg.zone_lighting("logo"));
        assert_eq!(labels[idx], "breathing");
        let (labels, idx) = effect_options(cfg.zone_lighting("wheel"));
        assert_eq!(labels[idx], "spectrum", "un-overridden zone follows device-wide");
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
    fn thumb_combo_options_track_config() {
        let mut cfg = Config {
            thumb_back: "cut".into(),
            ..Config::default()
        };
        let (labels, values, back) = combo_options(&cfg.thumb_back, "\u{2190} Back (default)");
        assert_eq!(values[back], "cut");
        assert_eq!(labels[back], "cut");
        // forward defaulted to none -> the identity entry is selected.
        let (labels, _, fwd) = combo_options(&cfg.thumb_forward, "\u{2192} Forward (default)");
        assert_eq!(fwd, 0);
        assert_eq!(labels[fwd], "\u{2192} Forward (default)");

        // A custom thumb chord must be present and selected.
        cfg.thumb_forward = "ctrl+shift+t".into();
        let (_, values, fwd) = combo_options(&cfg.thumb_forward, "\u{2192} Forward (default)");
        assert_eq!(values[fwd], "ctrl+shift+t");
    }

    #[test]
    fn diagram_arm_balance_within_slack() {
        // A heads-up, not a hard requirement â€” see platform::settings::
        // CENTER_SLACK's docs. Hardware dictates which callouts land on
        // which side (an RGB zone's controls sit beside that zone; a device
        // with no lighting has nothing to balance a wheel callout against),
        // so some imbalance is often unavoidable and the settings window
        // already absorbs up to CENTER_SLACK of it as body drift before any
        // margin skew shows. This just tells a new device's author how much
        // residual skew to expect. Computed at scale=1 (true for every
        // diagram today â€” all comfortably under platform's PANE_MAX_W).
        let budget = 2.0 * platform::settings::CENTER_SLACK;
        for spec in razer_proto::SUPPORTED {
            let Some(diagram) = spec.diagram else { continue };
            let Some((left, right)) = platform::diagram::arm_balance(&diagram) else { continue };
            let imbalance = (left - right).abs();
            if imbalance > budget {
                eprintln!(
                    "note: {}'s diagram has a {imbalance:.0}px left/right arm imbalance \
                     (left={left:.0} right={right:.0}) â€” CENTER_SLACK absorbs {budget:.0}px \
                     before margins go visibly lopsided; balance callouts across both arms \
                     if the hardware allows it",
                    spec.name,
                );
            }
        }
    }

    #[test]
    fn autostart_boot_plan_respects_absent_or_unknown_registration() {
        assert_eq!(autostart_boot_plan(false, Registration::Absent), AutostartBoot::Register);
        assert_eq!(autostart_boot_plan(false, Registration::Registered), AutostartBoot::Register);
        assert_eq!(autostart_boot_plan(true, Registration::Registered), AutostartBoot::Refresh);
        // A removed registration must not be recreated after bootstrap.
        assert_eq!(autostart_boot_plan(true, Registration::Absent), AutostartBoot::Nothing);
        // Failure to inspect an existing request must never authorize a write,
        // including before first-run bootstrap has been recorded.
        for bootstrapped in [false, true] {
            assert_eq!(autostart_boot_plan(bootstrapped, Registration::Unknown), AutostartBoot::Nothing);
        }
    }

}
