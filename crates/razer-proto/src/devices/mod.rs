//! One module per supported device: each file owns that model's complete
//! [`DeviceSpec`](crate::DeviceSpec) — protocol parameters and schematic
//! geometry together. Adding a mouse means adding a file here and listing it
//! in [`SUPPORTED`]; see `docs/SUPPORTED-DEVICES.md`.

mod deathadder_elite;
mod deathadder_v3;
mod deathadder_v4_pro;

pub use deathadder_elite::DEATHADDER_ELITE;
pub use deathadder_v3::DEATHADDER_V3;
pub use deathadder_v4_pro::DEATHADDER_V4_PRO;

/// Every device Snakecharmer knows how to drive.
pub const SUPPORTED: &[crate::DeviceSpec] =
    &[DEATHADDER_ELITE, DEATHADDER_V3, DEATHADDER_V4_PRO];

#[cfg(test)]
mod tests {
    use super::SUPPORTED;

    /// `spec_for` returns the first spec whose `product_ids` contains the id,
    /// so an id claimed twice would silently shadow one of the specs. Two mice
    /// never share a PID, and an alias like the V4 Pro's wireless id must be
    /// the same model — this catches both mistakes.
    #[test]
    fn product_ids_are_disjoint_across_devices() {
        for (i, a) in SUPPORTED.iter().enumerate() {
            assert!(
                a.covers(a.product_id),
                "{} lists `product_id` 0x{:04X} outside its own `product_ids`",
                a.name,
                a.product_id
            );
            for b in &SUPPORTED[i + 1..] {
                for id in b.product_ids {
                    assert!(
                        !a.covers(*id),
                        "0x{id:04X} is claimed by both {} and {}",
                        a.name,
                        b.name
                    );
                }
            }
        }
    }
}
