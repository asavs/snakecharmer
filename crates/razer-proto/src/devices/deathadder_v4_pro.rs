//! DeathAdder V4 Pro (wired and wireless): full [`DeviceSpec`] — protocol
//! parameters only. The button-map diagram is a separate contribution; see
//! `docs/DRAWING-MICE-GUIDE.md`.

use crate::{DeviceSpec, PollingProtocol, PollingSpec};

/// DeathAdder V4 Pro (`PID 0x00BE` wired, `0x00BF` wireless): no lighting, no
/// wheel DPI buttons, 45000 DPI, 8000 Hz.
///
/// Protocol per openrazer `razermouse_driver.c` / `razermouse_driver.h`:
/// transaction_id `0x1F`, and the two PIDs sit in the same case arm of every
/// command Snakecharmer sends (DPI, polling, device mode, serial, firmware). So
/// the attach method changes the USB id and nothing else Snakecharmer uses,
/// which is why both live in one spec. OpenRazer does split them elsewhere:
/// `read_device_type` names each one, and the dongle gets an indicator-LED
/// file that Snakecharmer doesn't drive.
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
    // idle_time files and no lighting file for the mouse: the V4 Pro has no
    // lighting hardware and no Chroma effects in OpenRazer. (The wireless
    // dongle has its own indicator LEDs, which are out of scope.) Lighting
    // commands are clean no-ops (see `apply_startup_lighting`). Same shape as
    // the V3.
    rgb_zones: &[],
    // No `attr_dpi_buttons` file either, same as the V3 — the wheel-DPI vendor
    // codes are not a V4 Pro feature. Driver mode and the vendor-code listeners
    // are skipped entirely on this model.
    dpi_buttons: None,
    dpi_min: 100,
    dpi_max: 45000,
    // `razer_chroma_misc_set_polling_rate2(polling_rate, 0x00)`, then OpenRazer
    // repeats the command with argument `0x01` ("For certain devices, Razer
    // sends each request once with 0x00 and once with 0x01"), mirroring what
    // Synapse sends. What `0x01` means is unknown (OpenRazer guesses
    // varstore). Snakecharmer sends only the `0x00` form, as it does for the
    // V3; the read-back confirms the live rate, not that it persists. If
    // wireless or persisted polling doesn't hold, the second send goes in as a
    // shared-layer change covering exactly the PIDs OpenRazer lists. The six
    // rates are `POLL_RATES` in the daemon's `RazerDeathAdderV4ProWired`, same
    // family and same rates as the V3.
    //
    // OpenRazer waits `RAZER_NEW_MOUSE_RECEIVER_WAIT_US` (31 ms) for this
    // device's responses; `send_command` already sleeps 60 ms.
    polling: PollingSpec {
        protocol: PollingProtocol::Extended,
        rates: &[125, 500, 1000, 2000, 4000, 8000],
    },
    diagram: None,
};

// Per-device tests: each device file checks its own spec values. See
// deathadder_elite.rs for the annotated model to copy.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{set_dpi_report, spec_for, ProtoError, DEATHADDER_V3};

    #[test]
    fn spec_values() {
        let d = DEATHADDER_V4_PRO;
        assert_eq!(d.product_ids, &[0x00BE, 0x00BF]);
        assert_eq!(d.product_id, 0x00BE);
        assert_eq!(d.transaction_id, 0x1F);
        assert_eq!(d.name, "DeathAdder V4 Pro");
        assert!(!d.has_rgb(), "V4 Pro has no lighting hardware");
        assert!(d.dpi_buttons.is_none(), "V4 Pro has no wheel DPI buttons");
        assert_eq!((d.dpi_min, d.dpi_max), (100, 45000));
        assert_eq!(
            d.polling,
            PollingSpec {
                protocol: PollingProtocol::Extended,
                rates: &[125, 500, 1000, 2000, 4000, 8000],
            }
        );
    }

    /// Both PIDs, wired `0x00BE` and the wireless dongle `0x00BF`, resolve to
    /// this one spec, and the neighbouring ids don't.
    #[test]
    fn is_registered_in_supported() {
        assert_eq!(spec_for(0x00BE), Some(DEATHADDER_V4_PRO), "wired PID");
        assert_eq!(spec_for(0x00BF), Some(DEATHADDER_V4_PRO), "wireless PID");
        assert_eq!(spec_for(0x00B2), Some(DEATHADDER_V3), "V3 unchanged");
        assert_eq!(spec_for(0x00BD), None, "one below the wired PID");
        assert_eq!(spec_for(0x00C0), None, "one above the wireless PID");
    }

    /// Same set-DPI layout as every Razer mouse, stamped with this device's
    /// transaction id. 1600 = 0x0640.
    #[test]
    fn set_dpi_report_bytes() {
        let d = DEATHADDER_V4_PRO;
        let r = set_dpi_report(d.transaction_id, d.dpi_min, d.dpi_max, 1600, 1600).unwrap();
        assert_eq!(&r[0..8], &[0x00, 0x1F, 0x00, 0x00, 0x00, 0x07, 0x04, 0x05]);
        assert_eq!(&r[8..15], &[0x00, 0x06, 0x40, 0x06, 0x40, 0x00, 0x00]);
    }

    /// The Focus Pro 45K sensor's ceiling.
    #[test]
    fn dpi_range_is_enforced() {
        let (t, lo, hi) = (DEATHADDER_V4_PRO.transaction_id, 100, 45000);
        assert!(set_dpi_report(t, lo, hi, 45000, 45000).is_ok());
        assert_eq!(set_dpi_report(t, lo, hi, 45001, 45001), Err(ProtoError::DpiOutOfRange(45001)));
        assert_eq!(set_dpi_report(t, lo, hi, 50, 50), Err(ProtoError::DpiOutOfRange(50)));
    }
}
