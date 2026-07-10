//! The headless daemon: enable driver mode, lock DPI, listen for the DPI-button
//! vendor reports on the readable collections (blocking reads in per-collection
//! threads), and inject the configured keystrokes. Parity with
//! `reference/dpi_button_daemon.py`.

use std::collections::HashSet;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use razer_hid::{aux_collection_paths, DeathAdder, Listener};
use razer_proto::DeviceMode;

use crate::actions::Action;
use crate::config::Config;
use crate::logger::Logger;

/// Vendor codes emitted by the DPI buttons in driver mode.
const CODE_DPI_UP: u8 = 0x20; // front button (closer to the wheel)
const CODE_DPI_DOWN: u8 = 0x21; // rear button

/// Ignore a repeat of the same code within this window (debounce / anti double-fire).
const DEBOUNCE: Duration = Duration::from_millis(200);

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

/// Run the daemon forever, restarting the device session on unplug/sleep errors.
pub fn run(cfg: Config, log: Logger) -> ! {
    log.log(&format!(
        "Daemon starting. dpi={:?} up={:?} down={:?} reassert={}s",
        cfg.dpi_xy(),
        cfg.dpi_up,
        cfg.dpi_down,
        cfg.reassert_interval_secs
    ));
    let bindings = Bindings::from_config(&cfg, &log);
    loop {
        match run_session(&cfg, &bindings, &log) {
            Ok(()) => log.log("Session ended cleanly; restarting."),
            Err(e) => log.log(&format!(
                "Session ended: {e}. Retrying in 3s (mouse unplugged/asleep?)."
            )),
        }
        thread::sleep(Duration::from_secs(3));
    }
}

fn run_session(cfg: &Config, bindings: &Bindings, log: &Logger) -> Result<(), razer_hid::Error> {
    let api = razer_hid::open_api()?;
    let ctrl = DeathAdder::open_with(&api)?;

    // Lock DPI (non-fatal if it fails — the buttons are the primary feature).
    let (dx, dy) = cfg.dpi_xy();
    match ctrl.set_dpi(dx, dy) {
        Ok((rx, ry)) => log.log(&format!("DPI locked to {rx} x {ry}.")),
        Err(e) => log.log(&format!("WARN could not set DPI: {e}")),
    }

    // Enable driver mode (fatal if it fails — no point listening otherwise).
    let mode = ctrl.set_device_mode(DeviceMode::Driver)?;
    log.log(&format!("Driver mode enabled (read back 0x{mode:02x})."));

    // Probe which auxiliary collections are readable (mirrors the Python daemon's
    // "dropped N unreadable collections" behavior), then keep the survivors.
    let mut listeners: Vec<Listener> = Vec::new();
    let mut dropped = 0usize;
    for path in aux_collection_paths(&api) {
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
                // Readable: switch to true blocking for the listener thread.
                listener.set_blocking(true)?;
                listeners.push(listener);
            }
            Err(_) => dropped += 1, // e.g. keyboard TLCs: "Incorrect function"
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

    // One blocking-read thread per collection -> channel. CPU stays ~0 when idle.
    let (tx, rx) = mpsc::channel::<u8>();
    for listener in listeners {
        let tx = tx.clone();
        let log = log.clone();
        thread::spawn(move || reader_thread(listener, tx, log));
    }
    drop(tx); // when every reader exits, rx disconnects and we restart the session

    let interval = Duration::from_secs(cfg.reassert_interval_secs.max(5));
    let mut last_fire: Option<(u8, Instant)> = None;

    loop {
        match rx.recv_timeout(interval) {
            Ok(code) => handle_code(code, bindings, log, &mut last_fire),
            Err(RecvTimeoutError::Timeout) => reassert(&ctrl, cfg, log)?,
            Err(RecvTimeoutError::Disconnected) => {
                return Err(razer_hid::Error::Verify(
                    "all listener collections closed".to_string(),
                ))
            }
        }
    }
}

/// Blocking read loop for one collection. Emits rising-edge vendor codes.
fn reader_thread(listener: Listener, tx: mpsc::Sender<u8>, log: Logger) {
    let label = listener.label();
    let mut prev: HashSet<u8> = HashSet::new();
    let mut buf = [0u8; 64];
    loop {
        match listener.read(&mut buf) {
            Ok(n) => {
                let data = &buf[..n];
                if data.first() != Some(&0x04) {
                    continue; // not a DPI-button vendor report
                }
                let cur: HashSet<u8> = data[1..].iter().copied().filter(|&b| b != 0).collect();
                for &code in cur.difference(&prev) {
                    if tx.send(code).is_err() {
                        return; // main loop gone
                    }
                }
                prev = cur;
            }
            Err(e) => {
                log.log(&format!("Listener {label} closed: {e}"));
                return;
            }
        }
    }
}

fn handle_code(code: u8, bindings: &Bindings, log: &Logger, last_fire: &mut Option<(u8, Instant)>) {
    // Debounce: swallow the same code if it recurs within DEBOUNCE.
    if let Some((c, t)) = last_fire {
        if *c == code && t.elapsed() < DEBOUNCE {
            return;
        }
    }
    let (name, action) = match code {
        CODE_DPI_UP => ("dpi_up", &bindings.up),
        CODE_DPI_DOWN => ("dpi_down", &bindings.down),
        other => {
            log.log(&format!("Unmapped vendor code 0x{other:02x}"));
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
fn reassert(ctrl: &DeathAdder, cfg: &Config, log: &Logger) -> Result<(), razer_hid::Error> {
    let mode = ctrl.get_device_mode()?;
    if mode != DeviceMode::Driver.as_byte() {
        let m = ctrl.set_device_mode(DeviceMode::Driver)?;
        log.log(&format!("Re-asserted driver mode (was 0x{mode:02x}, now 0x{m:02x})."));
    }
    let (want_x, want_y) = cfg.dpi_xy();
    let (cx, cy) = ctrl.get_dpi()?;
    if (cx, cy) != (want_x, want_y) {
        let (rx, ry) = ctrl.set_dpi(want_x, want_y)?;
        log.log(&format!("Re-locked DPI (was {cx}x{cy}, now {rx}x{ry})."));
    }
    Ok(())
}
