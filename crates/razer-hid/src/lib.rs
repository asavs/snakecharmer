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
use std::sync::{Arc, Mutex, Weak};
use std::thread::sleep;
use std::time::Duration;

use hidapi::{HidApi, HidDevice};
use razer_proto as proto;
use razer_proto::{DeviceMode, Rgb, REPORT_LEN};

#[cfg(test)]
mod safety_tests;

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
    /// Another process owns control access, or the OS could not establish ownership.
    ControlOwnership(platform::process_lease::Error),
    /// A protocol-level error (bad response, device status, etc.).
    Proto(proto::ProtoError),
    /// A prior command failed; no more device commands are permitted this run.
    SafetyStop(String),
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
            Error::ControlOwnership(platform::process_lease::Error::Busy) => write!(f,
                "another Snakecharmer process owns device control; quit the daemon from its tray menu before using charmctl device commands"),
            Error::ControlOwnership(e) => write!(f, "device control access denied: {e}"),
            Error::SafetyStop(reason) => write!(f, "device commands stopped: {reason}"),
            Error::Verify(m) => write!(f, "verification failed: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl Error {
    /// Device failures are terminal. Invalid local settings never reached the device.
    pub fn requires_stop(&self) -> bool {
        matches!(
            self,
            Self::Hid(_)
                | Self::SafetyStop(_)
                | Self::Verify(_)
                | Self::Proto(
                    proto::ProtoError::DeviceStatus(_)
                        | proto::ProtoError::BadResponseLen(_)
                        | proto::ProtoError::CommandEchoMismatch { .. }
                        | proto::ProtoError::UnknownPollingRate(_)
                        | proto::ProtoError::KeymapButtonMismatch { .. }
                )
        )
    }

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

// All handles, including the on-board reset thread, share one latch. Hold this
// lock through read/write/read-back so their feature reports cannot interleave.
static COMMANDS: Mutex<SafetyGate> = Mutex::new(SafetyGate { stopped: None });

// All supported mice share one owner: the application supports one active
// mouse. Global scope also excludes a second Windows login session. Weak
// ownership lets notice-thread handles share the lease without retaining it
// after the last control handle closes.
const CONTROL_LEASE_NAME: &str = "Global\\SnakecharmerDeviceControl";
static CONTROL_LEASE: Mutex<Weak<platform::process_lease::ProcessLease>> = Mutex::new(Weak::new());

fn control_lease() -> Result<Arc<platform::process_lease::ProcessLease>> {
    let mut slot = CONTROL_LEASE
        .lock()
        .map_err(|_| Error::SafetyStop("control ownership lock poisoned".into()))?;
    if let Some(lease) = slot.upgrade() {
        return Ok(lease);
    }
    let lease = Arc::new(
        platform::process_lease::ProcessLease::acquire(CONTROL_LEASE_NAME)
            .map_err(Error::ControlOwnership)?,
    );
    *slot = Arc::downgrade(&lease);
    Ok(lease)
}

struct SafetyGate {
    stopped: Option<String>,
}

impl SafetyGate {
    fn stop(&mut self, error: &Error) {
        if self.stopped.is_none() {
            self.stopped = Some(error.to_string());
        }
    }

    fn check(&self) -> Result<()> {
        match &self.stopped {
            Some(reason) => Err(Error::SafetyStop(reason.clone())),
            None => Ok(()),
        }
    }

    fn run<T>(&mut self, operation: impl FnOnce() -> Result<T>) -> Result<T> {
        self.check()?;
        let result = operation();
        if let Err(error) = &result {
            if error.requires_stop() {
                self.stop(error);
            }
        }
        result
    }
}

/// Check the process-wide latch without sending a report or opening a device.
pub fn ensure_active() -> Result<()> {
    COMMANDS
        .lock()
        .map_err(|_| Error::SafetyStop("command lock poisoned".into()))?
        .check()
}

/// Stop commands after a failure detected outside the control transport too.
pub fn stop_commands(error: &Error) {
    if let Ok(mut commands) = COMMANDS.lock() {
        commands.stop(error);
    }
    // A poisoned lock already prevents all I/O.
}

trait CommandIo {
    fn exchange(&mut self, request: &[u8; REPORT_LEN]) -> Result<[u8; REPORT_LEN]>;
}

// The simulated variant exists only in unit-test builds. Production keeps a
// single HID variant, with no dynamic transport dispatch or public test API.
enum ControlDevice {
    Hid(HidDevice),
    #[cfg(test)]
    Simulated(Mutex<safety_tests::Fake>),
}

enum HidChannel<'a> {
    Hid(&'a HidDevice),
    #[cfg(test)]
    Simulated(&'a mut safety_tests::Fake),
}

trait FeatureIo {
    fn send(&mut self, report: &[u8; REPORT_LEN + 1]) -> Result<()>;
    fn receive(&mut self, report: &mut [u8; REPORT_LEN + 1]) -> Result<usize>;
}

impl FeatureIo for HidChannel<'_> {
    fn send(&mut self, report: &[u8; REPORT_LEN + 1]) -> Result<()> {
        match self {
            Self::Hid(dev) => Ok(dev.send_feature_report(report)?),
            #[cfg(test)]
            Self::Simulated(io) => io.send(report),
        }
    }

    fn receive(&mut self, report: &mut [u8; REPORT_LEN + 1]) -> Result<usize> {
        match self {
            Self::Hid(dev) => Ok(dev.get_feature_report(report)?),
            #[cfg(test)]
            Self::Simulated(io) => io.receive(report),
        }
    }
}

impl<T: FeatureIo> CommandIo for T {
    fn exchange(&mut self, request: &[u8; REPORT_LEN]) -> Result<[u8; REPORT_LEN]> {
        let mut out = [0u8; REPORT_LEN + 1];
        out[1..].copy_from_slice(request);
        self.send(&out)?;
        sleep(Duration::from_millis(60));
        let mut input = [0u8; REPORT_LEN + 1];
        let n = self.receive(&mut input)?;
        let response = if n == REPORT_LEN + 1 {
            &input[1..]
        } else {
            &input[..n]
        };
        // BUSY is a non-success status too: AGENTS.md requires stopping, not retrying.
        let validated = proto::validate_response(request, response)?;
        let mut owned = [0u8; REPORT_LEN];
        owned.copy_from_slice(validated);
        Ok(owned)
    }
}

fn read_dpi(io: &mut impl CommandIo, txn: u8) -> Result<(u16, u16)> {
    Ok(proto::parse_dpi(&io.exchange(&proto::get_dpi_report(txn))?))
}

fn read_mode(io: &mut impl CommandIo, txn: u8) -> Result<u8> {
    Ok(proto::parse_device_mode(
        &io.exchange(&proto::get_device_mode_report(txn))?,
    ))
}

fn read_polling(io: &mut impl CommandIo, spec: proto::DeviceSpec) -> Result<u16> {
    let response = io.exchange(&proto::get_polling_rate_report(
        spec.transaction_id,
        spec.polling.protocol,
    ))?;
    Ok(proto::parse_polling_rate(spec.polling.protocol, &response)?)
}

fn read_binding(io: &mut impl CommandIo, txn: u8, button: u8) -> Result<proto::keymap::Binding> {
    let response = io.exchange(&proto::keymap::read_binding_report(txn, button))?;
    Ok(proto::keymap::parse_binding(&response, button)?)
}

fn write_verified<I: CommandIo, T: PartialEq + std::fmt::Debug>(
    io: &mut I,
    report: &[u8; REPORT_LEN],
    expected: T,
    mut read: impl FnMut(&mut I) -> Result<T>,
) -> Result<T> {
    read(io)?; // Confirm this command's read form before its write.
    io.exchange(report)?;
    sleep(Duration::from_millis(50));
    let actual = read(io)?;
    if actual != expected {
        return Err(Error::Verify(format!(
            "requested {expected:?}, device reports {actual:?}"
        )));
    }
    Ok(actual)
}

/// An open handle to a supported mouse's control collection.
pub struct Mouse {
    dev: ControlDevice,
    spec: proto::DeviceSpec,
    /// The USB product id the device actually enumerated as. Distinct from
    /// [`DeviceSpec::product_id`], which is the spec's canonical id — a device
    /// with aliases (the DeathAdder V4 Pro wired vs. wireless) reports one of
    /// several. Anything that re-enumerates the bus *by id* must use this one.
    product_id: u16,
    // Fields drop in declaration order: close the HID handle before releasing
    // ownership, even if the last handle is dropped on a different thread.
    _control_lease: Arc<platform::process_lease::ProcessLease>,
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
    ///
    /// A model with more than one id can enumerate under both at once (a V4 Pro
    /// charging on its cable with the dongle plugged in). Then the id listed
    /// first in [`proto::DeviceSpec::product_ids`] wins, which is the wired one: the
    /// dongle may have no mouse on the other end.
    ///
    /// A process lease is acquired before opening the endpoint. Other processes
    /// receive [`Error::ControlOwnership`]; handles in this process share it.
    pub fn open_with(api: &HidApi) -> Result<Mouse> {
        ensure_active()?;
        let info = api
            .device_list()
            .filter_map(|d| {
                let spec = proto::spec_for(d.product_id())?;
                let rank = spec
                    .product_ids
                    .iter()
                    .position(|&id| id == d.product_id())?;
                (d.vendor_id() == proto::VENDOR_ID
                    && d.interface_number() == 0
                    && d.usage_page() == 0x0001
                    && d.usage() == 0x0002)
                    .then_some((rank, d))
            })
            .min_by_key(|&(rank, _)| rank) // first of equals: enumeration order otherwise
            .map(|(_, d)| d)
            .ok_or(Error::DeviceNotFound)?;
        let spec = proto::spec_for(info.product_id()).expect("filtered on a supported product id");
        let product_id = info.product_id();
        // Enumeration is read-only. Acquire ownership only after locating a
        // supported device, but before opening its control endpoint.
        let lease = control_lease()?;
        let dev = info.open_device(api)?;
        Ok(Mouse {
            dev: ControlDevice::Hid(dev),
            spec,
            product_id,
            _control_lease: lease,
        })
    }

    /// The matched device's [`DeviceSpec`] — name, transaction id, and which
    /// features (RGB, DPI buttons) the hardware actually has.
    pub fn spec(&self) -> proto::DeviceSpec {
        self.spec
    }

    /// The USB product id this device enumerated as — one of
    /// [`DeviceSpec::product_ids`], and the id to enumerate its other HID
    /// collections by.
    pub fn product_id(&self) -> u16 {
        self.product_id
    }

    fn with_channel<T>(
        &self,
        operation: impl FnOnce(&mut HidChannel<'_>) -> Result<T>,
    ) -> Result<T> {
        COMMANDS
            .lock()
            .map_err(|_| Error::SafetyStop("command lock poisoned".into()))?
            .run(|| match &self.dev {
                ControlDevice::Hid(dev) => operation(&mut HidChannel::Hid(dev)),
                #[cfg(test)]
                ControlDevice::Simulated(io) => {
                    let mut io = io.lock().map_err(|_| {
                        Error::SafetyStop("simulated transport lock poisoned".into())
                    })?;
                    operation(&mut HidChannel::Simulated(&mut io))
                }
            })
    }

    /// Read the current device mode byte.
    pub fn get_device_mode(&self) -> Result<u8> {
        self.with_channel(|io| read_mode(io, self.spec.transaction_id))
    }

    /// Confirm the read channel, set the mode, and verify its read-back.
    pub fn set_device_mode(&self, mode: DeviceMode) -> Result<u8> {
        let txn = self.spec.transaction_id;
        let report = proto::set_device_mode_report(txn, mode);
        self.with_channel(|io| write_verified(io, &report, mode.as_byte(), |io| read_mode(io, txn)))
    }

    /// Read the current (x, y) DPI.
    pub fn get_dpi(&self) -> Result<(u16, u16)> {
        self.with_channel(|io| read_dpi(io, self.spec.transaction_id))
    }

    /// Confirm the read channel, set DPI, and verify its read-back.
    pub fn set_dpi(&self, dpi_x: u16, dpi_y: u16) -> Result<(u16, u16)> {
        let txn = self.spec.transaction_id;
        let report =
            proto::set_dpi_report(txn, self.spec.dpi_min, self.spec.dpi_max, dpi_x, dpi_y)?;
        self.with_channel(|io| write_verified(io, &report, (dpi_x, dpi_y), |io| read_dpi(io, txn)))
    }

    /// Read the current polling rate in Hz.
    pub fn get_polling_rate(&self) -> Result<u16> {
        self.with_channel(|io| read_polling(io, self.spec))
    }

    /// Validate the rate locally, confirm the read channel, write, and verify.
    pub fn set_polling_rate(&self, hz: u16) -> Result<u16> {
        let report =
            proto::set_polling_rate_report(self.spec.transaction_id, self.spec.polling, hz)?;
        self.with_channel(|io| write_verified(io, &report, hz, |io| read_polling(io, self.spec)))
    }

    /// Read a captured button id from this device's recorded keymap.
    pub fn get_button_binding(&self, button: u8) -> Result<proto::keymap::Binding> {
        proto::keymap::spec_for(self.spec.product_id)
            .filter(|km| km.buttons.iter().any(|&(id, _)| id == button))
            .ok_or_else(|| {
                Error::Proto(proto::ProtoError::KeymapNotWritable(
                    "no captured read for this device/button".into(),
                ))
            })?;
        self.with_channel(|io| read_binding(io, self.spec.transaction_id, button))
    }

    /// Write only captured, approved binds for this mouse; read before and after.
    pub fn set_button_binding(
        &self,
        km: &proto::keymap::KeymapSpec,
        button: u8,
        bind: &proto::keymap::Binding,
    ) -> Result<proto::keymap::Binding> {
        if proto::keymap::spec_for(self.spec.product_id).is_none_or(|known| known != km) {
            return Err(Error::Proto(proto::ProtoError::KeymapNotWritable(
                "keymap does not belong to this device".into(),
            )));
        }
        let txn = self.spec.transaction_id;
        let report = proto::keymap::write_binding_report(km, txn, button, bind)?;
        self.with_channel(|io| {
            write_verified(io, &report, bind.clone(), |io| {
                read_binding(io, txn, button)
            })
        })
    }

    // --- Chroma / RGB lighting ---------------------------------------------
    //
    // Each effect is applied to every zone in the spec's `rgb_zones`, in order.
    // The device acknowledges each command with status 0x02 (verified by the
    // shared transport); there is no color read-back in this protocol, so
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
        if self.spec.rgb_zones.is_empty() {
            return Ok(());
        }
        self.with_channel(|io| {
            // Effects have no read form. Confirm the control channel with the
            // existing DPI read; never invent a color-read command.
            read_dpi(io, self.spec.transaction_id)?;
            for &zone in self.spec.rgb_zones {
                io.exchange(&make(zone))?;
            }
            Ok(())
        })
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

    // Per-zone variants: same no-op convention — a zone the device doesn't
    // have (not in `rgb_zones`) is silently accepted, never an error.

    fn apply_zone(&self, zone: u8, report: [u8; REPORT_LEN]) -> Result<()> {
        if !self.spec.rgb_zones.contains(&zone) {
            return Ok(());
        }
        self.with_channel(|io| {
            read_dpi(io, self.spec.transaction_id)?;
            io.exchange(&report)?;
            Ok(())
        })
    }

    /// Static single color on one zone (a `led` id from the spec's `rgb_zones`).
    pub fn set_zone_color(&self, zone: u8, rgb: Rgb) -> Result<()> {
        self.apply_zone(
            zone,
            proto::effect_static_report(self.spec.transaction_id, zone, rgb),
        )
    }

    /// Breathing (single color) on one zone.
    pub fn set_zone_breathing(&self, zone: u8, rgb: Rgb) -> Result<()> {
        self.apply_zone(
            zone,
            proto::effect_breathing_report(self.spec.transaction_id, zone, rgb),
        )
    }

    /// Spectrum cycling on one zone.
    pub fn set_zone_spectrum(&self, zone: u8) -> Result<()> {
        self.apply_zone(
            zone,
            proto::effect_spectrum_report(self.spec.transaction_id, zone),
        )
    }

    /// Lighting off (none) on one zone.
    pub fn set_zone_off(&self, zone: u8) -> Result<()> {
        self.apply_zone(
            zone,
            proto::effect_none_report(self.spec.transaction_id, zone),
        )
    }
}

/// Paths of every auxiliary (non-control) HID collection of the given device
/// (by `product_id`) — i.e. every interface other than interface-0 mouse
/// control. These are the candidate collections for the DPI-button vendor input
/// reports; the caller probes which ones are actually readable. Pass
/// [`Mouse::product_id`] — the id the device enumerated as, not the spec's
/// canonical id — so a device with aliases finds its own collections.
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

/// A Razer mouse present on the bus, whether or not this build can drive it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedMouse {
    pub product_id: u16,
    /// The device's own USB product string, when it reports one.
    pub name: Option<String>,
    /// Whether a [`DeviceSpec`](proto::DeviceSpec) covers it.
    pub supported: bool,
}

impl std::fmt::Display for DetectedMouse {
    /// "Razer DeathAdder Chroma (1532:0043)", falling back to the id alone.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.name {
            Some(name) => write!(f, "{name} (1532:{:04X})", self.product_id),
            None => write!(f, "a Razer device (1532:{:04X})", self.product_id),
        }
    }
}

/// Every Razer *mouse* on the bus, deduped by product id.
///
/// Matched on the Generic Desktop / Mouse top-level collection, the same filter
/// [`Mouse::open_with`] uses — Razer's keyboards, headsets and mats share the
/// vendor id, and this must not report one of those as a mouse we could support.
///
/// **Enumeration only: nothing is opened and nothing is written.** That is the
/// same promise the daemon makes about unsupported hardware everywhere else
/// (see the README's safety FAQ). It reads what the OS already knows from the
/// descriptors, so the app can say *which* device it found rather than only
/// which ones it wanted.
pub fn detected_mice(api: &HidApi) -> Vec<DetectedMouse> {
    let mut found: Vec<DetectedMouse> = Vec::new();
    for d in api.device_list() {
        if d.vendor_id() != proto::VENDOR_ID || d.usage_page() != 0x0001 || d.usage() != 0x0002 {
            continue;
        }
        // One device exposes several collections; keep one entry per product
        // id, upgrading it if a later collection is the one carrying the name.
        let name = d
            .product_string()
            .map(str::to_owned)
            .filter(|s| !s.is_empty());
        match found.iter_mut().find(|m| m.product_id == d.product_id()) {
            Some(existing) => existing.name = existing.name.take().or(name),
            None => found.push(DetectedMouse {
                product_id: d.product_id(),
                name,
                supported: proto::spec_for(d.product_id()).is_some(),
            }),
        }
    }
    found
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
