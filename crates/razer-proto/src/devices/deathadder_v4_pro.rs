//! DeathAdder V4 Pro, wired and wireless: protocol parameters only. No
//! schematic yet (`diagram: None`), so the settings window shows labeled
//! control rows; drawing one is a separate contribution
//! (`docs/DRAWING-MICE-GUIDE.md`).

use crate::{DeviceSpec, PollingProtocol, PollingSpec};

/// DeathAdder V4 Pro over the cable (`PID 0x00BE`): no lighting, no wheel DPI
/// buttons, 45000 DPI. OpenRazer puts it in the same `razermouse_driver.c`
/// cases as the V3 for DPI and polling (transaction_id `0x1F`,
/// `..._polling_rate2`); `DPI_MAX` and `POLL_RATES` are from the daemon's
/// `RazerDeathAdderV4ProWired` class.
pub const DEATHADDER_V4_PRO_WIRED: DeviceSpec = DeviceSpec {
    product_id: 0x00BE,
    transaction_id: 0x1F,
    name: "DeathAdder V4 Pro (wired)",
    rgb_zones: &[],
    dpi_buttons: None,
    dpi_min: 100,
    dpi_max: 45000,
    polling: PollingSpec {
        protocol: PollingProtocol::Extended,
        rates: &[125, 500, 1000, 2000, 4000, 8000],
    },
    diagram: None,
};

/// DeathAdder V4 Pro through its HyperSpeed dongle (`PID 0x00BF`). The dongle
/// enumerates under its own PID but OpenRazer drives it with exactly the
/// wired cases above. OpenRazer waits 31 ms for receiver responses
/// (`RAZER_NEW_MOUSE_RECEIVER_WAIT_US`); `send_command`'s 60 ms already covers it.
pub const DEATHADDER_V4_PRO_WIRELESS: DeviceSpec = DeviceSpec {
    product_id: 0x00BF,
    name: "DeathAdder V4 Pro (wireless)",
    ..DEATHADDER_V4_PRO_WIRED
};
