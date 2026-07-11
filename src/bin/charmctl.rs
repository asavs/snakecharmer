//! `charmctl` — the Snakecharmer console CLI. A separate console-subsystem binary
//! so the daemon can stay windowless while command-line control still prints to
//! the terminal cleanly.
//!
//! Usage:
//!   charmctl status                         device mode + DPI + polling rate (read-only)
//!   charmctl set-dpi X [Y]                  set DPI
//!   charmctl set-poll <hz>                  set polling rate (Hz)
//!   charmctl set-mode driver|hardware       set device mode
//!   charmctl set-color <#RRGGBB>            static color (both zones)
//!   charmctl set-effect static [#RRGGBB]    static color effect
//!   charmctl set-effect breathing [#RRGGBB] breathing effect
//!   charmctl set-effect spectrum            spectrum cycling
//!   charmctl set-effect off                 lighting off
//!   charmctl self-test                      exercise keystroke injection (F13)
//!   charmctl where                          print config/log paths

use snakecharmer::config::Config;
use snakecharmer::lighting::EffectSpec;
use razer_hid::Mouse;
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
        "set-poll" => set_poll(rest),
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
        "charmctl — Snakecharmer control CLI (supported Razer mice)\n\n\
         USAGE:\n\
         \x20 charmctl status                          device mode + DPI + polling rate (read-only)\n\
         \x20 charmctl set-dpi X [Y]                   set DPI\n\
         \x20 charmctl set-poll <hz>                   set polling rate (Hz)\n\
         \x20 charmctl set-mode driver|hardware        set device mode\n\
         \x20 charmctl set-color <#RRGGBB>             static color (both zones)\n\
         \x20 charmctl set-effect static [#RRGGBB]     static color effect\n\
         \x20 charmctl set-effect breathing [#RRGGBB]  breathing effect\n\
         \x20 charmctl set-effect spectrum             spectrum cycling\n\
         \x20 charmctl set-effect off                  lighting off\n\
         \x20 charmctl self-test                       test keystroke injection (F13)\n\
         \x20 charmctl where                           print config/log paths\n"
    );
}

fn status() -> Result<(), Box<dyn std::error::Error>> {
    let mouse = Mouse::open()?;
    let spec = mouse.spec();
    let mode = mouse.get_device_mode()?;
    let (x, y) = mouse.get_dpi()?;
    // Degrade gracefully: a device whose firmware rejects the get command
    // still gets the rest of the status line.
    let poll = match mouse.get_polling_rate() {
        Ok(hz) => format!("{hz} Hz"),
        Err(e) => format!("unavailable ({e})"),
    };
    println!(
        "device = {} (PID 0x{:04X}), mode = {}, dpi = {x} x {y}, polling = {poll}",
        spec.name,
        spec.product_id,
        mode_name(mode)
    );
    Ok(())
}

fn set_poll(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let hz: u16 = args.first().ok_or("usage: set-poll <hz>")?.parse()?;
    let mouse = Mouse::open()?;
    let read = mouse.set_polling_rate(hz)?;
    println!("Polling rate set to {read} Hz.");
    Ok(())
}

fn set_dpi(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let x: u16 = args.first().ok_or("usage: set-dpi X [Y]")?.parse()?;
    let y: u16 = args.get(1).map(|s| s.parse()).transpose()?.unwrap_or(x);
    let mouse = Mouse::open()?;
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
    let mouse = Mouse::open()?;
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
    let mouse = Mouse::open()?;
    if !mouse.spec().has_rgb() {
        println!("{} has no lighting hardware; nothing to set.", mouse.spec().name);
        return Ok(());
    }
    mouse.set_color(rgb)?;
    println!("Sent static #{:02x}{:02x}{:02x} to all zones (device ack 0x02).", rgb.r, rgb.g, rgb.b);
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
    let mouse = Mouse::open()?;
    if !mouse.spec().has_rgb() {
        println!("{} has no lighting hardware; nothing to set.", mouse.spec().name);
        return Ok(());
    }
    spec.apply(&mouse)?;
    println!("Sent {} to all zones (device ack 0x02).", spec.describe());
    Ok(())
}

fn self_test() -> Result<(), Box<dyn std::error::Error>> {
    let (cfg, _) = Config::load_or_create(&Config::config_path());
    println!("Config: dpi={:?} up={:?} down={:?} lighting={:?}", cfg.dpi_xy(), cfg.dpi_up, cfg.dpi_down, cfg.lighting);
    for (label, spec) in [("dpi_up", &cfg.dpi_up), ("dpi_down", &cfg.dpi_down)] {
        match snakecharmer::actions::parse(spec) {
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
