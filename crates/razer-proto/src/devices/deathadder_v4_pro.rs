//! DeathAdder V4 Pro (wired and wireless): full [`DeviceSpec`] — protocol
//! parameters only. The button-map diagram is a separate contribution; see
//! `docs/DRAWING-MICE-GUIDE.md`.

use crate::{DeviceSpec, PollingProtocol, PollingSpec};

/// DeathAdder V4 Pro (`PID 0x00BE` wired, `0x00BF` wireless): no lighting, no
/// wheel DPI buttons, 45000 DPI, 8000 Hz.
///
/// Protocol per openrazer `razermouse_driver.c` / `razermouse_driver.h`:
/// transaction_id `0x1F`, and the two PIDs sit in the same case arm of every
/// mouse command — including `razer_attr_write_dpi`, where the V3's PID
/// (`0x00B2`) takes the `0x3F` arm instead. So the attach method changes the
/// USB id and nothing else, which is why both live in one spec.
///
/// The wired PID is the primary `product_id` because OpenRazer's
/// `razer_attr_read_device_type` case names the wired model first, so the name
/// and id the log shows are the same however the mouse is attached; see
/// [`DeviceSpec`].
pub const DEATHADDER_V4_PRO: DeviceSpec = DeviceSpec {
    product_ids: &[0x00BE, 0x00BF],
    product_id: 0x00BE,
    transaction_id: 0x1F,
    name: "DeathAdder V4 Pro",
    // `razermouse_probe` creates the attr_poll_rate/dpi/dpi_stages/charge/
    // idle_time files and no LED file at all: the V4 Pro has no lighting
    // hardware and no Chroma effects in OpenRazer. Lighting commands are clean
    // no-ops (see `apply_startup_lighting`). Same shape as the V3.
    rgb_zones: &[],
    // No `attr_dpi_buttons` file either, same as the V3 — the wheel-DPI vendor
    // codes are not a V4 Pro feature. Driver mode and the vendor-code listeners
    // are skipped entirely on this model.
    dpi_buttons: None,
    dpi_min: 100,
    dpi_max: 45000,
    // `razer_chroma_misc_set_polling_rate2(polling_rate, 0x00)`, then OpenRazer
    // repeats the identical command with argument `0x01` ("For certain devices,
    // Razer sends each request once with 0x00 and once with 0x01"). Sending it
    // twice is OpenRazer working around a Linux device file write that can miss
    // the first request; we hold the handle and check the read-back, so one
    // send is enough and that is what `set_polling_rate_report` builds. The six
    // rates are `POLL_RATES` in the daemon's `RazerDeathAdderV4ProWired` — same
    // family and same rates as the V3.
    polling: PollingSpec {
        protocol: PollingProtocol::Extended,
        rates: &[125, 500, 1000, 2000, 4000, 8000],
    },
    diagram: None,
};
