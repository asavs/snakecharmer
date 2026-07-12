//! Device access for supported Razer mice (see [`razer_proto::SUPPORTED`]).
//!
//! This layers real HID I/O (via the `hidapi` crate) on top of the pure
//! [`razer_proto`] protocol: opening the control endpoint, sending feature
//! reports, and the device-mode / DPI wrappers.
//!
//! On Windows the Razer control endpoint is the **interface-0 mouse top-level
//! collection** (`usage_page 0x0001`, `usage 0x0002`) — the only collection
//! that accepts these vendor feature reports; the auxiliary keyboard-style
//! collections reject `HidD_SetFeature` with "Incorrect function".

use std::ffi::CString;
use std::thread::sleep;
use std::time::Duration;

use hidapi::{HidApi, HidDevice};
use razer_proto as proto;
use razer_proto::{DeviceMode, Rgb, REPORT_LEN};

/// Create a fresh [`HidApi`] instance (the library is safe to instantiate more
/// than once). Exposed so callers can share one instance across opens.
pub fn open_api() -> Result<HidApi> {
    Ok(HidApi::new()?)
}

/// Errors from talking to the mouse.
#[derive(Debug)]
pub enum Error {
    /// The `hidapi` library returned an error.
    Hid(hidapi::HidError),
    /// No supported mouse's control interface was found (mouse unplugged?).
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
            Error::DeviceNotFound => {
                let names: Vec<&str> = proto::SUPPORTED.iter().map(|s| s.name).collect();
                write!(
                    f,
                    "no supported Razer mouse found (supported: {}) - is it plugged in?",
                    names.join(", ")
                )
            }
            Error::Proto(e) => write!(f, "protocol error: {e}"),
            Error::Busy(s) => write!(f, "device stayed busy (status 0x{s:02x})"),
            Error::Verify(m) => write!(f, "verification failed: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl Error {
    /// Preserve a native OS error when hidapi exposes one; other variants remain typed
    /// but deliberately do not invent a platform code.
    pub fn native_error_code(&self) -> Option<i64> {
        match self {
            Self::Hid(hidapi::HidError::IoError { error }) => error.raw_os_error().map(i64::from),
            _ => None,
        }
    }
}

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

/// An open handle to a supported mouse's control collection.
pub struct Mouse {
    dev: HidDevice,
    spec: proto::DeviceSpec,
}

impl Mouse {
    /// Open the interface-0 mouse control collection.
    ///
    /// Creates its own [`HidApi`] instance; for repeated opens, prefer
    /// [`Mouse::open_with`] and reuse one `HidApi`.
    pub fn open() -> Result<Mouse> {
        let api = HidApi::new()?;
        Self::open_with(&api)
    }

    /// Open using a caller-provided [`HidApi`] (avoids re-enumerating hidapi).
    ///
    /// Matches the interface-0 mouse control collection of *any* supported
    /// device (see [`proto::SUPPORTED`]); the matched [`DeviceSpec`] is remembered
    /// so every command is built with that model's transaction id and its
    /// features are gated correctly.
    pub fn open_with(api: &HidApi) -> Result<Mouse> {
        let info = api
            .device_list()
            .find(|d| {
                d.vendor_id() == proto::VENDOR_ID
                    && proto::spec_for(d.product_id()).is_some()
                    && d.interface_number() == 0
                    && d.usage_page() == 0x0001
                    && d.usage() == 0x0002
            })
            .ok_or(Error::DeviceNotFound)?;
        let spec = proto::spec_for(info.product_id()).expect("filtered on a supported product id");
        let dev = info.open_device(api)?;
        Ok(Mouse { dev, spec })
    }

    /// The matched device's [`DeviceSpec`] — name, transaction id, and which
    /// features (RGB, DPI buttons) the hardware actually has.
    pub fn spec(&self) -> proto::DeviceSpec {
        self.spec
    }

    /// Send one 90-byte report as feature report 0 and return the 90-byte response.
    ///
    /// Retries on BUSY, mirroring OpenRazer's `razer_send_payload` loop.
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
        let resp = self.send_command(&proto::get_device_mode_report(self.spec.transaction_id))?;
        Ok(proto::parse_device_mode(&resp))
    }

    /// Set the device mode and return the read-back mode for verification.
    pub fn set_device_mode(&self, mode: DeviceMode) -> Result<u8> {
        self.send_command(&proto::set_device_mode_report(self.spec.transaction_id, mode))?;
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
        let resp = self.send_command(&proto::get_dpi_report(self.spec.transaction_id))?;
        Ok(proto::parse_dpi(&resp))
    }

    /// Set the (x, y) DPI and return the read-back value for verification.
    pub fn set_dpi(&self, dpi_x: u16, dpi_y: u16) -> Result<(u16, u16)> {
        self.send_command(&proto::set_dpi_report(
            self.spec.transaction_id,
            self.spec.dpi_min,
            self.spec.dpi_max,
            dpi_x,
            dpi_y,
        )?)?;
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

    /// Read the current polling rate in Hz.
    pub fn get_polling_rate(&self) -> Result<u16> {
        let resp = self.send_command(&proto::get_polling_rate_report(
            self.spec.transaction_id,
            self.spec.polling.protocol,
        ))?;
        Ok(proto::parse_polling_rate(self.spec.polling.protocol, &resp)?)
    }

    /// Set the polling rate (Hz) and return the read-back value for
    /// verification. Rates the spec's `polling.rates` doesn't list are
    /// rejected up front (the error names the supported rates).
    pub fn set_polling_rate(&self, hz: u16) -> Result<u16> {
        self.send_command(&proto::set_polling_rate_report(
            self.spec.transaction_id,
            self.spec.polling,
            hz,
        )?)?;
        sleep(Duration::from_millis(50));
        let read = self.get_polling_rate()?;
        if read != hz {
            return Err(Error::Verify(format!(
                "requested polling rate {hz} Hz, device reports {read} Hz"
            )));
        }
        Ok(read)
    }

    // --- Chroma / RGB lighting ---------------------------------------------
    //
    // Each effect is applied to every zone in the spec's `rgb_zones`, in order.
    // The device acknowledges each command with status 0x02 (verified by
    // `send_command`); there is no color read-back in this protocol, so
    // success == the device accepted the report.
    //
    // On a device with no lighting hardware (`rgb_zones` empty) these are
    // no-ops that return `Ok(())` — so callers don't have to guard every
    // lighting path, and a lighting command never turns into a spurious
    // "not supported" error that would restart the session.

    fn apply_zones<F>(&self, mut make: F) -> Result<()>
    where
        F: FnMut(u8) -> [u8; REPORT_LEN],
    {
        for &zone in self.spec.rgb_zones {
            self.send_command(&make(zone))?;
        }
        Ok(())
    }

    /// Static single color on all lit zones.
    pub fn set_color(&self, rgb: Rgb) -> Result<()> {
        let txn = self.spec.transaction_id;
        self.apply_zones(|zone| proto::effect_static_report(txn, zone, rgb))
    }

    /// Breathing (single color) on all lit zones.
    pub fn set_breathing(&self, rgb: Rgb) -> Result<()> {
        let txn = self.spec.transaction_id;
        self.apply_zones(|zone| proto::effect_breathing_report(txn, zone, rgb))
    }

    /// Spectrum cycling on all lit zones.
    pub fn set_spectrum(&self) -> Result<()> {
        let txn = self.spec.transaction_id;
        self.apply_zones(|zone| proto::effect_spectrum_report(txn, zone))
    }

    /// Lighting off (none) on all lit zones.
    pub fn set_lighting_off(&self) -> Result<()> {
        let txn = self.spec.transaction_id;
        self.apply_zones(|zone| proto::effect_none_report(txn, zone))
    }
}

/// Paths of every auxiliary (non-control) HID collection of the given device
/// (by `product_id`) — i.e. every interface other than interface-0 mouse
/// control. These are the candidate collections for the DPI-button vendor input
/// reports; the caller probes which ones are actually readable. Pass the
/// `product_id` from the opened [`Mouse`]'s [`Mouse::spec`].
pub fn aux_collection_paths(api: &HidApi, product_id: u16) -> Vec<CString> {
    api.device_list()
        .filter(|d| {
            d.vendor_id() == proto::VENDOR_ID
                && d.product_id() == product_id
                && d.interface_number() != 0
        })
        .map(|d| d.path().to_owned())
        .collect()
}

/// A readable auxiliary HID collection, used to listen for DPI-button input
/// reports. `HidDevice` is `Send`, so a `Listener` can be moved into its own
/// thread for a truly blocking read (no poll loop — CPU stays at ~0 when idle).
pub struct Listener {
    dev: HidDevice,
    path: String,
}

impl Listener {
    /// Open an auxiliary collection by path.
    pub fn open(api: &HidApi, path: &CString) -> Result<Listener> {
        let dev = api.open_path(path)?;
        Ok(Listener {
            dev,
            path: path.to_string_lossy().into_owned(),
        })
    }

    /// A short, human-readable tail of the device path (for logs).
    pub fn label(&self) -> String {
        let p = &self.path;
        let tail = if p.len() > 34 { &p[p.len() - 34..] } else { p };
        tail.to_string()
    }

    /// Toggle blocking mode. Blocking (`true`) makes [`Listener::read`] wait for
    /// a report; non-blocking (`false`) returns `Ok(0)` immediately when idle.
    pub fn set_blocking(&self, blocking: bool) -> Result<()> {
        self.dev.set_blocking_mode(blocking)?;
        Ok(())
    }

    /// Read one input report into `buf`, returning the number of bytes read.
    /// In blocking mode this parks the thread until a report arrives.
    pub fn read(&self, buf: &mut [u8]) -> Result<usize> {
        Ok(self.dev.read(buf)?)
    }

    /// Read with an explicit timeout in milliseconds (`-1` = block forever).
    pub fn read_timeout(&self, buf: &mut [u8], timeout_ms: i32) -> Result<usize> {
        Ok(self.dev.read_timeout(buf, timeout_ms)?)
    }
}
