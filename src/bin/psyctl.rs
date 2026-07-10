//! `psyctl` — the Psylli console CLI. A separate console-subsystem binary
//! so the daemon can stay windowless while command-line control still prints to
//! the terminal cleanly.
//!
//! Usage:
//!   psyctl status                         device mode + DPI (read-only)
//!   psyctl set-dpi X [Y]                  set DPI
//!   psyctl set-mode driver|hardware       set device mode
//!   psyctl set-color <#RRGGBB>            static color (both zones)
//!   psyctl set-effect static [#RRGGBB]    static color effect
//!   psyctl set-effect breathing [#RRGGBB] breathing effect
//!   psyctl set-effect spectrum            spectrum cycling
//!   psyctl set-effect off                 lighting off
//!   psyctl self-test                      exercise keystroke injection (F13)
//!   psyctl where                          print config/log paths

use psylli::config::Config;
use psylli::lighting::EffectSpec;
use razer_hid::DeathAdder;
use razer_proto::{DeviceMode, Rgb};

fn mode_name(b: u8) -> String {
    match DeviceMode::from_byte(b) {
        Some(DeviceMode::Hardware) => "hardware (0x00)".into(),
        Some(DeviceMode::Driver) => "driver (0x03)".into(),
        None => format!("unknown (0x{b:02x})"),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };

    let result: Result<(), Box<dyn std::error::Error>> = match cmd {
        "status" => status(),
        "set-dpi" => set_dpi(rest),
        "set-mode" => set_mode(rest),
        "set-color" => set_color(rest),
        "set-effect" => set_effect(rest),
        "self-test" | "--self-test" => self_test(),
        "where" => where_paths(),
        "" | "--help" | "-h" | "help" => {
            print_help();
            Ok(())
        }
        other => {
            eprintln!("unknown command {other:?}\n");
            print_help();
            std::process::exit(2);
        }
    };

    if let Err(e) = result {
        eprintln!("ERROR: {e}");
        std::process::exit(1);
    }
}

fn print_help() {
    println!(
        "psyctl — Psylli control CLI (DeathAdder Elite)\n\n\
         USAGE:\n\
         \x20 psyctl status                          device mode + DPI (read-only)\n\
         \x20 psyctl set-dpi X [Y]                   set DPI\n\
         \x20 psyctl set-mode driver|hardware        set device mode\n\
         \x20 psyctl set-color <#RRGGBB>             static color (both zones)\n\
         \x20 psyctl set-effect static [#RRGGBB]     static color effect\n\
         \x20 psyctl set-effect breathing [#RRGGBB]  breathing effect\n\
         \x20 psyctl set-effect spectrum             spectrum cycling\n\
         \x20 psyctl set-effect off                  lighting off\n\
         \x20 psyctl self-test                       test keystroke injection (F13)\n\
         \x20 psyctl where                           print config/log paths\n"
    );
}

fn status() -> Result<(), Box<dyn std::error::Error>> {
    let mouse = DeathAdder::open()?;
    let mode = mouse.get_device_mode()?;
    let (x, y) = mouse.get_dpi()?;
    println!("mode = {}, dpi = {x} x {y}", mode_name(mode));
    Ok(())
}

fn set_dpi(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let x: u16 = args.first().ok_or("usage: set-dpi X [Y]")?.parse()?;
    let y: u16 = args.get(1).map(|s| s.parse()).transpose()?.unwrap_or(x);
    let mouse = DeathAdder::open()?;
    let (rx, ry) = mouse.set_dpi(x, y)?;
    println!("DPI set to {rx} x {ry}.");
    Ok(())
}

fn set_mode(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mode = match args.first().map(|s| s.as_str()) {
        Some("driver") => DeviceMode::Driver,
        Some("hardware") => DeviceMode::Hardware,
        _ => return Err("usage: set-mode driver|hardware".into()),
    };
    let mouse = DeathAdder::open()?;
    let m = mouse.set_device_mode(mode)?;
    println!("mode = {}", mode_name(m));
    Ok(())
}

fn parse_color(arg: Option<&String>) -> Result<Rgb, Box<dyn std::error::Error>> {
    let s = arg.ok_or("expected a color like #00ff00")?;
    Ok(Rgb::parse_hex(s)?)
}

fn set_color(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let rgb = parse_color(args.first())?;
    let mouse = DeathAdder::open()?;
    mouse.set_color(rgb)?;
    println!("Sent static #{:02x}{:02x}{:02x} to both zones (device ack 0x02).", rgb.r, rgb.g, rgb.b);
    Ok(())
}

fn set_effect(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let kind = args.first().map(|s| s.as_str()).ok_or(
        "usage: set-effect static|breathing|spectrum|off [#RRGGBB]",
    )?;
    let default_color = Rgb::new(0x00, 0xFF, 0x00);
    let spec = match kind {
        "static" => EffectSpec::Static(args.get(1).map(|s| Rgb::parse_hex(s)).transpose()?.unwrap_or(default_color)),
        "breathing" => EffectSpec::Breathing(args.get(1).map(|s| Rgb::parse_hex(s)).transpose()?.unwrap_or(default_color)),
        "spectrum" => EffectSpec::Spectrum,
        "off" | "none" => EffectSpec::Off,
        other => return Err(format!("unknown effect {other:?}").into()),
    };
    let mouse = DeathAdder::open()?;
    spec.apply(&mouse)?;
    println!("Sent {} to both zones (device ack 0x02).", spec.describe());
    Ok(())
}

fn self_test() -> Result<(), Box<dyn std::error::Error>> {
    let (cfg, _) = Config::load_or_create(&Config::config_path());
    println!("Config: dpi={:?} up={:?} down={:?} lighting={:?}", cfg.dpi_xy(), cfg.dpi_up, cfg.dpi_down, cfg.lighting);
    for (label, spec) in [("dpi_up", &cfg.dpi_up), ("dpi_down", &cfg.dpi_down)] {
        match psylli::actions::parse(spec) {
            Ok(a) => println!("  {label} = {spec:?} -> {:?}", a.chord()),
            Err(e) => println!("  {label} = {spec:?} -> PARSE ERROR: {e}"),
        }
    }
    let f13 = platform::vk_function(13).expect("F13");
    println!("Injecting F13 (harmless) to verify SendInput...");
    platform::send_chord(&[f13])?;
    println!("SendInput OK. Self-test passed.");
    Ok(())
}

fn where_paths() -> Result<(), Box<dyn std::error::Error>> {
    println!("config: {}", Config::config_path().display());
    println!("log:    {}", Config::log_path().display());
    Ok(())
}
