//! Anti-Synapse — Phase 1 milestone (M1) binary.
//!
//! Reproduces the Python prototype end-to-end on the real DeathAdder Elite:
//! get current mode -> set driver mode -> set DPI -> read both back and print them.
//!
//! Usage:
//!   anti-synapse            # default: driver mode + 1600 DPI
//!   anti-synapse 1600       # driver mode + given DPI
//!   anti-synapse 1600 800   # driver mode + given X/Y DPI
//!   anti-synapse hardware   # restore factory (hardware) mode, leave DPI as-is

use razer_hid::DeathAdder;
use razer_proto::{DeviceMode, PRODUCT_ID, VENDOR_ID};

fn mode_name(b: u8) -> String {
    match DeviceMode::from_byte(b) {
        Some(DeviceMode::Hardware) => "hardware (0x00)".into(),
        Some(DeviceMode::Driver) => "driver (0x03)".into(),
        None => format!("unknown (0x{b:02x})"),
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    println!(
        "Anti-Synapse M1 — Razer DeathAdder Elite ({:04x}:{:04x})",
        VENDOR_ID, PRODUCT_ID
    );

    let mouse = DeathAdder::open()?;

    // 1. current state
    let mode0 = mouse.get_device_mode()?;
    let (dx0, dy0) = mouse.get_dpi()?;
    println!("Before: mode = {}, dpi = {dx0} x {dy0}", mode_name(mode0));

    // "hardware" argument => restore factory mode and stop.
    if args.first().map(|s| s.as_str()) == Some("hardware") {
        let m = mouse.set_device_mode(DeviceMode::Hardware)?;
        println!("Restored: mode = {}", mode_name(m));
        return Ok(());
    }

    // parse optional DPI args (default 1600)
    let dpi_x: u16 = args.first().map(|s| s.parse()).transpose()?.unwrap_or(1600);
    let dpi_y: u16 = args.get(1).map(|s| s.parse()).transpose()?.unwrap_or(dpi_x);

    // 2. set driver mode
    let m = mouse.set_device_mode(DeviceMode::Driver)?;
    println!("Set driver mode -> read back {}", mode_name(m));

    // 3. set DPI
    let (rx, ry) = mouse.set_dpi(dpi_x, dpi_y)?;
    println!("Set DPI {dpi_x} x {dpi_y} -> read back {rx} x {ry}");

    // 4. final read-back
    let modef = mouse.get_device_mode()?;
    let (dxf, dyf) = mouse.get_dpi()?;
    println!("After:  mode = {}, dpi = {dxf} x {dyf}", mode_name(modef));
    println!("M1 OK.");
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("ERROR: {e}");
        std::process::exit(1);
    }
}
