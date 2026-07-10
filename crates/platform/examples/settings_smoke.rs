//! Smoke test for the settings window: opens it with sample values, prints any
//! events, and auto-closes after ~5s (so it can run non-interactively / in CI).
//!
//!   cargo run -p platform --example settings_smoke

use platform::settings::{self, SettingsEvent, SettingsInit};

fn main() {
    // Auto-close after a few seconds by posting WM_CLOSE to our own window.
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(5));
        use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};
        let cls: Vec<u16> = "AntiSynapseSettings".encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: FindWindowW/PostMessageW with a valid NUL-terminated class name.
        unsafe {
            let hwnd = FindWindowW(cls.as_ptr(), std::ptr::null());
            if !hwnd.is_null() {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
            }
        }
    });

    let init = SettingsInit {
        dpi: 1800,
        dpi_min: 100,
        dpi_max: 3200,
        action_labels: ["copy", "paste", "cut", "none", "key:9", "key:0"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        up_index: 0,
        down_index: 1,
        thumb_back_index: 3,
        thumb_forward_index: 0,
        effect_labels: ["keep", "static", "breathing", "spectrum", "off"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        effect_index: 1,
        color: (0x00, 0xC8, 0x40),
    };

    println!("Opening settings window (auto-closes in ~5s)...");
    settings::open(init, |ev: SettingsEvent| println!("event: {ev:?}"));
    println!("Settings window closed. Smoke test OK.");
}
