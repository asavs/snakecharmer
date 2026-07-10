//! Snakecharmer daemon (windowless): DPI-button remap + DPI lock + lighting +
//! system tray, all in one process. No console window (see the `windows`
//! subsystem attribute below). Command-line control lives in the separate
//! console binary `charmctl.exe`.

#![windows_subsystem = "windows"]

use snakecharmer::config::Config;
use snakecharmer::daemon;
use snakecharmer::logger::Logger;

const SINGLE_INSTANCE_NAME: &str = "Local\\SnakecharmerDaemon";

fn main() {
    let log = Logger::new(Config::log_path());
    if !platform::acquire_single_instance(SINGLE_INSTANCE_NAME) {
        log.log("Another instance is already running; exiting.");
        return;
    }
    let (cfg, note) = Config::load_or_create(&Config::config_path());
    if let Some(note) = note {
        log.log(&note);
    }
    daemon::run(cfg, log); // never returns
}
