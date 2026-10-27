//! Smoke test for the settings window: opens it with sample values, prints any
//! events, and auto-closes after ~5s (so it can run non-interactively / in CI).
//!
//!   cargo run -p platform --example settings_smoke              # full layout
//!   cargo run -p platform --example settings_smoke -- --minimal # no RGB, no DPI buttons
//!   ... -- --no-diagram                                         # device with no button map yet
//!   ... -- --hold 30                                            # stay open longer
//!
//! Settings values are samples; diagram geometry comes directly from the
//! shipped device specs. Native callout controls identify themselves through
//! their selected entries, while SVG rendering retains the stored captions.

use platform::diagram::Diagram;
use platform::settings::{
    self, ActionCombo, DpiButtonsInit, LightingZoneInit, OnboardButton, SettingsEvent, SettingsInit,
};

// Preview the shipped immutable diagrams rather than maintaining copies of
// their coordinate data. This dependency is confined to the example.
fn diagram_full() -> Diagram {
    razer_proto::DEATHADDER_ELITE
        .diagram
        .expect("Elite diagram")
}

fn diagram_minimal() -> Diagram {
    razer_proto::DEATHADDER_V3.diagram.expect("V3 diagram")
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// A sample per-button dropdown, following the daemon's convention: index 0
/// is the button's identity/default entry, the rest are action presets.
fn combo(identity: &str, sel: usize) -> ActionCombo {
    let mut labels = vec![identity.to_string()];
    labels.extend(strings(&["copy", "paste", "cut", "key:9", "key:0"]));
    ActionCombo { labels, index: sel }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let minimal = args.iter().any(|a| a == "--minimal");
    // A device whose DeviceSpec has `diagram: None` -- a shipped protocol port
    // that nobody has drawn yet. The window falls back to labeled control rows.
    let no_diagram = args.iter().any(|a| a == "--no-diagram");
    let hold_secs: u64 = args
        .iter()
        .position(|a| a == "--hold")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);

    // Auto-close by posting WM_CLOSE to our own window.
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(hold_secs));
        use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};
        let cls: Vec<u16> = "SnakecharmerSettings"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: FindWindowW/PostMessageW with a valid NUL-terminated class name.
        unsafe {
            let hwnd = FindWindowW(cls.as_ptr(), std::ptr::null());
            if !hwnd.is_null() {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
            }
        }
    });

    let init = if minimal {
        // A V3-like device: no lighting, no wheel DPI buttons, 8 kHz polling.
        SettingsInit {
            device_name: "Sample Mouse (minimal)".to_string(),
            input_device: None, // Preview only: no device input is registered.
            diagram: (!no_diagram).then(diagram_minimal),
            dpi: 1800,
            dpi_min: 100,
            dpi_max: 30000,
            polling_labels: strings(&[
                "keep — leave as set",
                "125 Hz",
                "500 Hz",
                "1000 Hz",
                "2000 Hz",
                "4000 Hz",
                "8000 Hz",
            ]),
            polling_index: 3,
            dpi_buttons: None,
            thumb_back: combo("\u{2190} Back (default)", 3),
            thumb_forward: combo("\u{2192} Forward (default)", 0),
            lighting: vec![],
            // V3-like: every writable button is an on-board control at its
            // CalloutSlot::Button, so the thumb dropdowns above aren't mounted.
            onboard: Some(
                [
                    (
                        0x03,
                        "Wheel click",
                        "Middle click (default)",
                        "Copy (Ctrl+C)",
                    ),
                    (0x04, "Back", "Back (default)", "f5"),
                    (0x05, "Forward", "Forward (default)", "Forward (default)"),
                    (
                        0x09,
                        "Wheel up",
                        "Scroll up (default)",
                        "Scroll up (default)",
                    ),
                    (
                        0x0A,
                        "Wheel down",
                        "Scroll down (default)",
                        "Scroll down (default)",
                    ),
                ]
                .into_iter()
                .map(|(id, label, default, text)| OnboardButton {
                    id,
                    label: label.to_string(),
                    presets: strings(&[
                        default,
                        "Middle click",
                        "Copy (Ctrl+C)",
                        "Paste (Ctrl+V)",
                        "Disabled",
                    ]),
                    text: text.to_string(),
                })
                .collect(),
            ),
        }
    } else {
        // An Elite-like device: everything on.
        SettingsInit {
            device_name: "Sample Mouse (full)".to_string(),
            input_device: None,
            diagram: (!no_diagram).then(diagram_full),
            dpi: 1800,
            dpi_min: 100,
            dpi_max: 16000,
            polling_labels: strings(&["keep — leave as set", "125 Hz", "500 Hz", "1000 Hz"]),
            polling_index: 3,
            dpi_buttons: Some(DpiButtonsInit {
                up: combo("Front DPI — unbound", 1),
                down: combo("Rear DPI — unbound", 2),
            }),
            thumb_back: combo("\u{2190} Back (default)", 3),
            thumb_forward: combo("\u{2192} Forward (default)", 0),
            // Two zones (wheel + logo), independently colored like the Elite.
            lighting: vec![
                LightingZoneInit {
                    label: "Wheel".to_string(),
                    effect_labels: strings(&["keep", "static", "breathing", "spectrum", "off"]),
                    effect_index: 1,
                    color: (0x00, 0xC8, 0x40),
                },
                LightingZoneInit {
                    label: "Logo".to_string(),
                    effect_labels: strings(&["keep", "static", "breathing", "spectrum", "off"]),
                    effect_index: 2,
                    color: (0x40, 0x60, 0xE0),
                },
            ],
            onboard: None,
        }
    };

    println!(
        "Opening settings window ({}; auto-closes in ~{hold_secs}s)...",
        match (minimal, no_diagram) {
            (true, true) => "minimal, no diagram",
            (true, false) => "minimal",
            (false, true) => "full, no diagram",
            (false, false) => "full",
        }
    );
    settings::open(init, |ev: SettingsEvent| println!("event: {ev:?}"));
    println!("Settings window closed. Smoke test OK.");
}
