//! Anti-Synapse — native Windows replacement for Razer Synapse (DeathAdder Elite).
//!
//! Phase 2: the headless daemon. Also keeps the Phase-1 utility subcommands.
//!
//! Usage:
//!   anti-synapse                 run the daemon (default)
//!   anti-synapse --self-test     exercise keystroke injection (harmless F13) and exit
//!   anti-synapse status          print current device mode + DPI (read-only)
//!   anti-synapse set-dpi X [Y]   set DPI and exit
//!   anti-synapse set-mode driver|hardware   set device mode and exit
//!   anti-synapse where           print config/log paths
//!   anti-synapse --help

mod actions;
mod config;
mod daemon;
mod logger;

use config::Config;
use logger::Logger;
use razer_hid::DeathAdder;
use razer_proto::DeviceMode;

const SINGLE_INSTANCE_NAME: &str = "Local\\AntiSynapseDaemon";

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

    let result: Result<(), Box<dyn std::error::Error>> = match cmd {
        "" | "run" | "daemon" => run_daemon(),
        "--self-test" | "self-test" => self_test(),
        "status" => status(),
        "set-dpi" => set_dpi(&args[1..]),
        "set-mode" => set_mode(&args[1..]),
        "where" => where_paths(),
        "--help" | "-h" | "help" => {
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
        "anti-synapse — DeathAdder Elite daemon (no Synapse)\n\n\
         USAGE:\n\
         \x20 anti-synapse                 run the daemon (default)\n\
         \x20 anti-synapse --self-test     test keystroke injection (F13) and exit\n\
         \x20 anti-synapse status          print device mode + DPI (read-only)\n\
         \x20 anti-synapse set-dpi X [Y]   set DPI and exit\n\
         \x20 anti-synapse set-mode M      M = driver | hardware\n\
         \x20 anti-synapse where           print config/log paths\n"
    );
}

fn run_daemon() -> Result<(), Box<dyn std::error::Error>> {
    let log = Logger::new(Config::log_path());
    if !platform::acquire_single_instance(SINGLE_INSTANCE_NAME) {
        log.log("Another instance is already running; exiting.");
        return Ok(());
    }
    let (cfg, note) = Config::load_or_create(&Config::config_path());
    if let Some(note) = note {
        log.log(&note);
    }
    daemon::run(cfg, log); // never returns
}

fn self_test() -> Result<(), Box<dyn std::error::Error>> {
    let (cfg, _) = Config::load_or_create(&Config::config_path());
    println!("Config: dpi={:?} up={:?} down={:?}", cfg.dpi_xy(), cfg.dpi_up, cfg.dpi_down);

    // Show the chords the buttons would fire (parse path exercised)...
    for (label, spec) in [("dpi_up", &cfg.dpi_up), ("dpi_down", &cfg.dpi_down)] {
        match actions::parse(spec) {
            Ok(a) => println!("  {label} = {spec:?} -> {:?}", a.chord()),
            Err(e) => println!("  {label} = {spec:?} -> PARSE ERROR: {e}"),
        }
    }

    // ...then actually inject a harmless keystroke (F13) to prove SendInput works.
    let f13 = platform::vk_function(13).expect("F13");
    println!("Injecting F13 (harmless) to verify SendInput...");
    platform::send_chord(&[f13])?;
    println!("SendInput OK. Self-test passed.");
    Ok(())
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

fn where_paths() -> Result<(), Box<dyn std::error::Error>> {
    println!("config: {}", Config::config_path().display());
    println!("log:    {}", Config::log_path().display());
    Ok(())
}
