//! Snakecharmer shared library: config, action parsing, logging, the headless
//! daemon (with tray), shared by the windowless daemon binary (`main.rs`) and
//! the console CLI (`bin/charmctl.rs`).

pub mod actions;
pub mod config;
pub mod daemon;
#[cfg(feature = "pc-vitals")]
pub mod health;
#[cfg(not(feature = "pc-vitals"))]
#[path = "health_disabled.rs"]
pub mod health;
pub mod lighting;
pub mod logger;
pub mod onboard;
