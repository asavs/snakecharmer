//! Minimal example of driving the mouse straight through `razer-hid`.
//! Read-only: prints the current device mode and DPI. Safe to run any time.
//!
//!   cargo run -p razer-hid --example m1

use razer_hid::Mouse;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mouse = Mouse::open()?;
    let mode = mouse.get_device_mode()?;
    let (x, y) = mouse.get_dpi()?;
    println!("device: {}", mouse.spec().name);
    println!("device mode byte: 0x{mode:02x}");
    println!("dpi: {x} x {y}");
    Ok(())
}
