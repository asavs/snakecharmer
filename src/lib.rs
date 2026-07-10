//! Snakecharmer shared library: config, action parsing, logging, the headless
//! daemon (with tray), shared by the windowless daemon binary (`main.rs`) and
//! the console CLI (`bin/charmctl.rs`).

pub mod actions;
pub mod config;
pub mod daemon;
pub mod lighting;
pub mod logger;
