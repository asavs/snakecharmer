//! One module per supported device: each file owns that model's complete
//! [`DeviceSpec`](crate::DeviceSpec) — protocol parameters and schematic
//! geometry together. Adding a mouse means adding a file here and listing it
//! in [`SUPPORTED`]; see `docs/SUPPORTED-DEVICES.md`.

mod deathadder_elite;
mod deathadder_v3;

pub use deathadder_elite::DEATHADDER_ELITE;
pub use deathadder_v3::DEATHADDER_V3;

/// Every device Snakecharmer knows how to drive.
pub const SUPPORTED: &[crate::DeviceSpec] = &[DEATHADDER_ELITE, DEATHADDER_V3];
