//! Pure Razer HID protocol for the Razer DeathAdder Elite (`VID 0x1532 / PID 0x005C`).
//!
//! This is a faithful Rust port of `reference/razer_common.py`. It contains **no I/O**:
//! it only builds the 90-byte feature reports and parses the 90-byte responses, so it is
//! fully unit-testable. The `razer-hid` crate layers device access on top.
//!
//! Protocol source: OpenRazer (github.com/openrazer/openrazer), files
//! `driver/razercommon.{c,h}`, `driver/razerchromacommon.c`, `driver/razermouse_driver.c`.
//!
//! The Razer control protocol is a 90-byte HID feature report (report ID 0):
//!
//! ```text
//! offset 0     status            (0x00 on request; response: 0x02=OK, 0x01=busy,
//!                                 0x03=failure, 0x04=timeout, 0x05=not supported)
//! offset 1     transaction_id    (0x3F for the DeathAdder Elite)
//! offset 2-3   remaining_packets (big-endian, 0)
//! offset 4     protocol_type     (0)
//! offset 5     data_size
//! offset 6     command_class
//! offset 7     command_id
//! offset 8-87  arguments (80 bytes)
//! offset 88    crc = XOR of bytes 2..87
//! offset 89    reserved (0)
//! ```

#![forbid(unsafe_code)]

/// USB vendor id for Razer.
pub const VENDOR_ID: u16 = 0x1532;
/// USB product id for the DeathAdder Elite.
pub const PRODUCT_ID: u16 = 0x005C;
/// Transaction id used by the DeathAdder Elite (per openrazer `razermouse_driver.c`).
pub const TRANSACTION_ID: u8 = 0x3F;

/// Length of a Razer control report / response, in bytes.
pub const REPORT_LEN: usize = 90;

/// Response status codes (byte 0 of a response).
pub mod status {
    pub const NEW_COMMAND: u8 = 0x00;
    pub const BUSY: u8 = 0x01;
    pub const SUCCESS: u8 = 0x02;
    pub const FAILURE: u8 = 0x03;
    pub const TIMEOUT: u8 = 0x04;
    pub const NOT_SUPPORTED: u8 = 0x05;
}

/// Device mode values (argument to the set-device-mode command).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DeviceMode {
    /// Factory behavior: the DPI buttons cycle DPI stages internally and send nothing.
    Hardware = 0x00,
    /// Driver mode: the firmware stops consuming the DPI buttons and emits vendor events.
    Driver = 0x03,
}

impl DeviceMode {
    /// Interpret a raw mode byte (as read back from the device).
    pub fn from_byte(b: u8) -> Option<DeviceMode> {
        match b {
            0x00 => Some(DeviceMode::Hardware),
            0x03 => Some(DeviceMode::Driver),
            _ => None,
        }
    }

    pub fn as_byte(self) -> u8 {
        self as u8
    }
}

/// Errors that can arise purely from building/parsing reports (no I/O).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtoError {
    /// More than 80 argument bytes were supplied.
    ArgsTooLong(usize),
    /// A DPI value fell outside the DeathAdder Elite's 100..=16000 range.
    DpiOutOfRange(u16),
    /// A response was not [`REPORT_LEN`] bytes.
    BadResponseLen(usize),
    /// The device reported a non-success status byte.
    DeviceStatus(u8),
    /// The response's command class/id did not echo the request.
    CommandEchoMismatch { sent: (u8, u8), got: (u8, u8) },
}

impl core::fmt::Display for ProtoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ProtoError::ArgsTooLong(n) => write!(f, "arguments too long: {n} > 80"),
            ProtoError::DpiOutOfRange(v) => write!(f, "DPI {v} out of range (100..=16000)"),
            ProtoError::BadResponseLen(n) => write!(f, "unexpected response length {n} (want {REPORT_LEN})"),
            ProtoError::DeviceStatus(s) => write!(f, "device returned status 0x{s:02x}"),
            ProtoError::CommandEchoMismatch { sent, got } => write!(
                f,
                "response echo mismatch: sent {:02x}/{:02x}, got {:02x}/{:02x}",
                sent.0, sent.1, got.0, got.1
            ),
        }
    }
}

impl std::error::Error for ProtoError {}

/// Compute the Razer CRC: XOR of bytes `2..=87` of the report.
pub fn crc(report: &[u8; REPORT_LEN]) -> u8 {
    report[2..88].iter().fold(0u8, |acc, &b| acc ^ b)
}

/// Build a 90-byte Razer report (with transaction id and CRC filled in).
///
/// Mirrors `build_report` in `razer_common.py`.
pub fn build_report(
    command_class: u8,
    command_id: u8,
    data_size: u8,
    args: &[u8],
) -> Result<[u8; REPORT_LEN], ProtoError> {
    if args.len() > 80 {
        return Err(ProtoError::ArgsTooLong(args.len()));
    }
    let mut buf = [0u8; REPORT_LEN];
    buf[1] = TRANSACTION_ID;
    buf[5] = data_size;
    buf[6] = command_class;
    buf[7] = command_id;
    buf[8..8 + args.len()].copy_from_slice(args);
    buf[88] = crc(&buf);
    Ok(buf)
}

// --- Command constructors -------------------------------------------------

/// set device mode: class 0x00, id 0x04, args [mode, 0x00].
pub fn set_device_mode_report(mode: DeviceMode) -> [u8; REPORT_LEN] {
    build_report(0x00, 0x04, 0x02, &[mode.as_byte(), 0x00]).expect("2 args always valid")
}

/// get device mode: class 0x00, id 0x84.
pub fn get_device_mode_report() -> [u8; REPORT_LEN] {
    build_report(0x00, 0x84, 0x02, &[]).expect("no args always valid")
}

/// set DPI (xy): class 0x04, id 0x05, args [0x00 (NOSTORE), x_hi, x_lo, y_hi, y_lo, 0, 0].
pub fn set_dpi_report(dpi_x: u16, dpi_y: u16) -> Result<[u8; REPORT_LEN], ProtoError> {
    for v in [dpi_x, dpi_y] {
        if !(100..=16000).contains(&v) {
            return Err(ProtoError::DpiOutOfRange(v));
        }
    }
    let args = [
        0x00, // no variable storage (NOSTORE), per openrazer for this device
        (dpi_x >> 8) as u8,
        (dpi_x & 0xFF) as u8,
        (dpi_y >> 8) as u8,
        (dpi_y & 0xFF) as u8,
        0x00,
        0x00,
    ];
    build_report(0x04, 0x05, 0x07, &args)
}

/// get DPI (xy): class 0x04, id 0x85.
pub fn get_dpi_report() -> [u8; REPORT_LEN] {
    build_report(0x04, 0x85, 0x07, &[]).expect("no args always valid")
}

// --- Response validation & parsing ---------------------------------------

/// Validate a raw 90-byte response against the request that produced it.
///
/// Mirrors the checks in `send_command` (`razer_common.py`): correct length,
/// success status, and matching command-class/id echo. Returns the response
/// slice on success so callers can parse arguments from it.
pub fn validate_response<'a>(
    request: &[u8; REPORT_LEN],
    response: &'a [u8],
) -> Result<&'a [u8], ProtoError> {
    if response.len() != REPORT_LEN {
        return Err(ProtoError::BadResponseLen(response.len()));
    }
    let st = response[0];
    if st != status::SUCCESS {
        return Err(ProtoError::DeviceStatus(st));
    }
    if response[6] != request[6] || response[7] != request[7] {
        return Err(ProtoError::CommandEchoMismatch {
            sent: (request[6], request[7]),
            got: (response[6], response[7]),
        });
    }
    Ok(response)
}

/// Parse the device mode from a validated get-device-mode response (byte 8).
pub fn parse_device_mode(response: &[u8]) -> u8 {
    response[8]
}

/// Parse (dpi_x, dpi_y) from a validated get-DPI response (bytes 9..=12).
pub fn parse_dpi(response: &[u8]) -> (u16, u16) {
    let x = ((response[9] as u16) << 8) | response[10] as u16;
    let y = ((response[11] as u16) << 8) | response[12] as u16;
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The SPEC table: driver-mode report is `... 02 00 04 03 00 ... 05 00`.
    #[test]
    fn driver_mode_report_matches_spec() {
        let r = set_device_mode_report(DeviceMode::Driver);
        assert_eq!(&r[0..10], &[0x00, 0x3F, 0x00, 0x00, 0x00, 0x02, 0x00, 0x04, 0x03, 0x00]);
        assert_eq!(r[88], 0x05, "driver-mode CRC");
        assert_eq!(r[89], 0x00, "reserved");
        // bytes 10..88 must be zero
        assert!(r[10..88].iter().all(|&b| b == 0));
    }

    #[test]
    fn hardware_mode_report_matches_spec() {
        let r = set_device_mode_report(DeviceMode::Hardware);
        assert_eq!(&r[0..10], &[0x00, 0x3F, 0x00, 0x00, 0x00, 0x02, 0x00, 0x04, 0x00, 0x00]);
        assert_eq!(r[88], 0x06, "hardware-mode CRC");
    }

    #[test]
    fn get_mode_report_matches_spec() {
        let r = get_device_mode_report();
        assert_eq!(&r[0..10], &[0x00, 0x3F, 0x00, 0x00, 0x00, 0x02, 0x00, 0x84, 0x00, 0x00]);
        assert_eq!(r[88], 0x86, "get-mode CRC");
    }

    #[test]
    #[allow(clippy::needless_range_loop)] // mirror openrazer's `for(i=2;i<88;i++)` verbatim
    fn crc_is_xor_of_bytes_2_to_87() {
        // Manual reference computation for the driver-mode report.
        let r = set_device_mode_report(DeviceMode::Driver);
        let mut expected = 0u8;
        for i in 2..88 {
            expected ^= r[i];
        }
        assert_eq!(crc(&r), expected);
        assert_eq!(r[88], expected);
    }

    #[test]
    fn set_dpi_report_bytes() {
        // 1600 = 0x0640
        let r = set_dpi_report(1600, 1600).unwrap();
        assert_eq!(&r[0..8], &[0x00, 0x3F, 0x00, 0x00, 0x00, 0x07, 0x04, 0x05]);
        // args: NOSTORE, x_hi, x_lo, y_hi, y_lo, 0, 0
        assert_eq!(&r[8..15], &[0x00, 0x06, 0x40, 0x06, 0x40, 0x00, 0x00]);
        // CRC self-consistency
        assert_eq!(r[88], crc(&r));
    }

    #[test]
    fn set_dpi_asymmetric() {
        let r = set_dpi_report(1600, 800).unwrap();
        assert_eq!(&r[9..13], &[0x06, 0x40, 0x03, 0x20]); // 0x0640, 0x0320
    }

    #[test]
    fn get_dpi_report_bytes() {
        let r = get_dpi_report();
        assert_eq!(&r[0..8], &[0x00, 0x3F, 0x00, 0x00, 0x00, 0x07, 0x04, 0x85]);
        assert_eq!(r[88], crc(&r));
    }

    #[test]
    fn dpi_range_is_enforced() {
        assert_eq!(set_dpi_report(50, 50), Err(ProtoError::DpiOutOfRange(50)));
        assert_eq!(set_dpi_report(20000, 1600), Err(ProtoError::DpiOutOfRange(20000)));
        assert!(set_dpi_report(100, 100).is_ok());
        assert!(set_dpi_report(16000, 16000).is_ok());
    }

    #[test]
    fn args_too_long_rejected() {
        let big = [0u8; 81];
        assert_eq!(build_report(0x00, 0x00, 0x00, &big), Err(ProtoError::ArgsTooLong(81)));
    }

    #[test]
    fn validate_and_parse_device_mode() {
        let req = get_device_mode_report();
        let mut resp = [0u8; REPORT_LEN];
        resp[0] = status::SUCCESS;
        resp[6] = 0x00;
        resp[7] = 0x84;
        resp[8] = 0x03; // driver
        let v = validate_response(&req, &resp).unwrap();
        assert_eq!(parse_device_mode(v), 0x03);
        assert_eq!(DeviceMode::from_byte(parse_device_mode(v)), Some(DeviceMode::Driver));
    }

    #[test]
    fn validate_and_parse_dpi() {
        let req = get_dpi_report();
        let mut resp = [0u8; REPORT_LEN];
        resp[0] = status::SUCCESS;
        resp[6] = 0x04;
        resp[7] = 0x85;
        resp[9] = 0x06;
        resp[10] = 0x40; // x = 1600
        resp[11] = 0x03;
        resp[12] = 0x20; // y = 800
        let v = validate_response(&req, &resp).unwrap();
        assert_eq!(parse_dpi(v), (1600, 800));
    }

    #[test]
    fn validate_rejects_bad_status_and_echo() {
        let req = get_device_mode_report();
        let mut resp = [0u8; REPORT_LEN];
        resp[0] = status::BUSY;
        assert_eq!(validate_response(&req, &resp), Err(ProtoError::DeviceStatus(status::BUSY)));

        resp[0] = status::SUCCESS;
        resp[6] = 0x04; // wrong class
        resp[7] = 0x84;
        assert!(matches!(
            validate_response(&req, &resp),
            Err(ProtoError::CommandEchoMismatch { .. })
        ));

        assert_eq!(validate_response(&req, &resp[..10]), Err(ProtoError::BadResponseLen(10)));
    }
}
