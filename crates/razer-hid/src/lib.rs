//! Device access for the Razer DeathAdder Elite control interface.
//!
//! This layers real HID I/O (via the `hidapi` crate) on top of the pure
//! [`razer_proto`] protocol. It is the direct port of the I/O half of
//! `reference/razer_common.py` (`open_control`, `send_command`, and the
//! device-mode / DPI wrappers).
//!
//! On Windows the Razer control endpoint is the **interface-0 mouse top-level
//! collection** (`usage_page 0x0001`, `usage 0x0002`) — the only collection
//! that accepts these vendor feature reports; the auxiliary keyboard-style
//! collections reject `HidD_SetFeature` with "Incorrect function".

use std::thread::sleep;
use std::time::Duration;

use hidapi::{HidApi, HidDevice};
use razer_proto as proto;
use razer_proto::{DeviceMode, REPORT_LEN};

/// Errors from talking to the mouse.
#[derive(Debug)]
pub enum Error {
    /// The `hidapi` library returned an error.
    Hid(hidapi::HidError),
    /// The DeathAdder Elite control interface was not found (mouse unplugged?).
    DeviceNotFound,
    /// A protocol-level error (bad response, device status, etc.).
    Proto(proto::ProtoError),
    /// The device stayed BUSY across all retries.
    Busy(u8),
    /// A requested mode/DPI did not match on read-back.
    Verify(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Hid(e) => write!(f, "hid error: {e}"),
            Error::DeviceNotFound => write!(
                f,
                "DeathAdder Elite (1532:005C) control interface not found - is the mouse plugged in?"
            ),
            Error::Proto(e) => write!(f, "protocol error: {e}"),
            Error::Busy(s) => write!(f, "device stayed busy (status 0x{s:02x})"),
            Error::Verify(m) => write!(f, "verification failed: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<hidapi::HidError> for Error {
    fn from(e: hidapi::HidError) -> Self {
        Error::Hid(e)
    }
}
impl From<proto::ProtoError> for Error {
    fn from(e: proto::ProtoError) -> Self {
        Error::Proto(e)
    }
}

type Result<T> = std::result::Result<T, Error>;

/// An open handle to the DeathAdder Elite's control collection.
pub struct DeathAdder {
    dev: HidDevice,
}

impl DeathAdder {
    /// Open the interface-0 mouse control collection.
    ///
    /// Creates its own [`HidApi`] instance; for repeated opens, prefer
    /// [`DeathAdder::open_with`] and reuse one `HidApi`.
    pub fn open() -> Result<DeathAdder> {
        let api = HidApi::new()?;
        Self::open_with(&api)
    }

    /// Open using a caller-provided [`HidApi`] (avoids re-enumerating hidapi).
    pub fn open_with(api: &HidApi) -> Result<DeathAdder> {
        let info = api
            .device_list()
            .find(|d| {
                d.vendor_id() == proto::VENDOR_ID
                    && d.product_id() == proto::PRODUCT_ID
                    && d.interface_number() == 0
                    && d.usage_page() == 0x0001
                    && d.usage() == 0x0002
            })
            .ok_or(Error::DeviceNotFound)?;
        let dev = info.open_device(api)?;
        Ok(DeathAdder { dev })
    }

    /// Send one 90-byte report as feature report 0 and return the 90-byte response.
    ///
    /// Retries on BUSY, mirroring `send_command` in the Python prototype and
    /// openrazer's `razer_send_payload` loop.
    pub fn send_command(&self, request: &[u8; REPORT_LEN]) -> Result<[u8; REPORT_LEN]> {
        // hidapi wants a leading report-id byte (0x00) -> 91-byte write buffer.
        let mut out = [0u8; REPORT_LEN + 1];
        out[1..].copy_from_slice(request);

        let mut last_status = 0u8;
        for _ in 0..5 {
            self.dev.send_feature_report(&out)?;
            sleep(Duration::from_millis(60));

            let mut inbuf = [0u8; REPORT_LEN + 1];
            inbuf[0] = 0x00; // report id
            let n = self.dev.get_feature_report(&mut inbuf)?;

            // Strip the leading report-id byte if present.
            let resp: &[u8] = if n == REPORT_LEN + 1 {
                &inbuf[1..]
            } else {
                &inbuf[..n]
            };
            if resp.len() != REPORT_LEN {
                return Err(Error::Proto(proto::ProtoError::BadResponseLen(resp.len())));
            }
            last_status = resp[0];
            if last_status == proto::status::BUSY {
                sleep(Duration::from_millis(100));
                continue;
            }
            let validated = proto::validate_response(request, resp)?;
            let mut owned = [0u8; REPORT_LEN];
            owned.copy_from_slice(validated);
            return Ok(owned);
        }
        Err(Error::Busy(last_status))
    }

    /// Read the current device mode byte.
    pub fn get_device_mode(&self) -> Result<u8> {
        let resp = self.send_command(&proto::get_device_mode_report())?;
        Ok(proto::parse_device_mode(&resp))
    }

    /// Set the device mode and return the read-back mode for verification.
    pub fn set_device_mode(&self, mode: DeviceMode) -> Result<u8> {
        self.send_command(&proto::set_device_mode_report(mode))?;
        sleep(Duration::from_millis(50));
        let read = self.get_device_mode()?;
        if read != mode.as_byte() {
            return Err(Error::Verify(format!(
                "requested mode 0x{:02x}, device reports 0x{:02x}",
                mode.as_byte(),
                read
            )));
        }
        Ok(read)
    }

    /// Read the current (x, y) DPI.
    pub fn get_dpi(&self) -> Result<(u16, u16)> {
        let resp = self.send_command(&proto::get_dpi_report())?;
        Ok(proto::parse_dpi(&resp))
    }

    /// Set the (x, y) DPI and return the read-back value for verification.
    pub fn set_dpi(&self, dpi_x: u16, dpi_y: u16) -> Result<(u16, u16)> {
        self.send_command(&proto::set_dpi_report(dpi_x, dpi_y)?)?;
        sleep(Duration::from_millis(50));
        let read = self.get_dpi()?;
        if read != (dpi_x, dpi_y) {
            return Err(Error::Verify(format!(
                "requested DPI {dpi_x}x{dpi_y}, device reports {}x{}",
                read.0, read.1
            )));
        }
        Ok(read)
    }
}
