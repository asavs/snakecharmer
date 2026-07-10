//! Pure Razer HID protocol for the Razer DeathAdder Elite (`VID 0x1532 / PID 0x005C`).
//!
//! This contains **no I/O**: it only builds the 90-byte feature reports and parses the
//! 90-byte responses, so it is fully unit-testable. The `razer-hid` crate layers device
//! access on top.
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
    /// A color string was not `#RRGGBB` / `RRGGBB`.
    BadColor(String),
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
            ProtoError::BadColor(s) => write!(f, "bad color {s:?} (want #RRGGBB)"),
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
/// Builds the 90-byte control report (per OpenRazer `razercommon.c`).
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

// --- Chroma / RGB lighting ------------------------------------------------
//
// The DeathAdder Elite (PID 0x005C) uses OpenRazer's *extended matrix effect*
// family (`razer_chroma_extended_matrix_effect_*` in `razerchromacommon.c`),
// dispatched with transaction_id 0x3F for this device (see the DEATHADDER_ELITE
// cases in `razermouse_driver.c`). NOTE: these are command_class 0x0F /
// command_id 0x02 — *not* the class-0x03 "standard" LED commands. The base
// builder (`razer_chroma_extended_matrix_effect_base`) lays out:
//   arg[0] = variable_storage (VARSTORE = 0x01)
//   arg[1] = led_id           (SCROLL_WHEEL_LED 0x01 / LOGO_LED 0x04)
//   arg[2] = effect_id
// with a per-effect data_size and trailing arguments.

/// Variable-storage selector (persist to the device's own store).
pub const VARSTORE: u8 = 0x01;
/// Non-persistent storage selector.
pub const NOSTORE: u8 = 0x00;

/// The DeathAdder Elite's two lit zones.
pub mod led {
    pub const SCROLL_WHEEL: u8 = 0x01;
    pub const LOGO: u8 = 0x04;
}

/// A 24-bit RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }

    /// Parse `#RRGGBB` or `RRGGBB` (case-insensitive).
    pub fn parse_hex(s: &str) -> Result<Rgb, ProtoError> {
        let h = s.trim().trim_start_matches('#');
        if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ProtoError::BadColor(s.to_string()));
        }
        let v = u32::from_str_radix(h, 16).map_err(|_| ProtoError::BadColor(s.to_string()))?;
        Ok(Rgb::new((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }
}

/// The `razer_chroma_extended_matrix_effect_base`: class 0x0F, id 0x02.
fn extended_matrix_effect_base(
    arg_size: u8,
    variable_storage: u8,
    led_id: u8,
    effect_id: u8,
) -> [u8; REPORT_LEN] {
    build_report(0x0F, 0x02, arg_size, &[variable_storage, led_id, effect_id])
        .expect("3 args always valid")
}

/// "None" (off) effect for a zone. `razer_chroma_extended_matrix_effect_none`.
pub fn effect_none_report(led_id: u8) -> [u8; REPORT_LEN] {
    extended_matrix_effect_base(0x06, VARSTORE, led_id, 0x00)
}

/// "Spectrum" cycling effect for a zone. `..._effect_spectrum`.
pub fn effect_spectrum_report(led_id: u8) -> [u8; REPORT_LEN] {
    extended_matrix_effect_base(0x06, VARSTORE, led_id, 0x03)
}

/// "Static" single-color effect for a zone. `..._effect_static`.
pub fn effect_static_report(led_id: u8, rgb: Rgb) -> [u8; REPORT_LEN] {
    let mut r = extended_matrix_effect_base(0x09, VARSTORE, led_id, 0x01);
    // arguments[5]=0x01, arguments[6..9]=RGB  (arguments start at byte offset 8)
    r[8 + 5] = 0x01;
    r[8 + 6] = rgb.r;
    r[8 + 7] = rgb.g;
    r[8 + 8] = rgb.b;
    r[88] = crc(&r);
    r
}

/// "Breathing" (single-color) effect for a zone. `..._effect_breathing_single`.
pub fn effect_breathing_report(led_id: u8, rgb: Rgb) -> [u8; REPORT_LEN] {
    let mut r = extended_matrix_effect_base(0x09, VARSTORE, led_id, 0x02);
    // arguments[3]=0x01, arguments[5]=0x01, arguments[6..9]=RGB
    r[8 + 3] = 0x01;
    r[8 + 5] = 0x01;
    r[8 + 6] = rgb.r;
    r[8 + 7] = rgb.g;
    r[8 + 8] = rgb.b;
    r[88] = crc(&r);
    r
}

// --- Response validation & parsing ---------------------------------------

/// Validate a raw 90-byte response against the request that produced it.
///
/// Validates the response (per OpenRazer's report handling): correct length,
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

    // --- Chroma tests: assert exact bytes against OpenRazer doc examples ----

    #[test]
    fn chroma_common_header() {
        // All effects: class 0x0F, id 0x02, txn 0x3F, arg[0]=VARSTORE.
        let r = effect_spectrum_report(led::SCROLL_WHEEL);
        assert_eq!(r[0], 0x00); // status
        assert_eq!(r[1], 0x3F); // transaction id
        assert_eq!(r[6], 0x0F); // command class
        assert_eq!(r[7], 0x02); // command id
        assert_eq!(r[8], VARSTORE);
    }

    #[test]
    fn chroma_spectrum_matches_openrazer() {
        // Doc: data_size 06, args 01 <led> 03 00 00 00
        let r = effect_spectrum_report(led::LOGO);
        assert_eq!(r[5], 0x06); // data_size
        assert_eq!(&r[8..14], &[VARSTORE, led::LOGO, 0x03, 0x00, 0x00, 0x00]);
        assert_eq!(r[88], crc(&r));
    }

    #[test]
    fn chroma_none_matches_openrazer() {
        // Doc: 010500000000 (data_size 06)
        let r = effect_none_report(led::SCROLL_WHEEL);
        assert_eq!(r[5], 0x06);
        assert_eq!(&r[8..14], &[VARSTORE, led::SCROLL_WHEEL, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(r[88], crc(&r));
    }

    #[test]
    fn chroma_static_red_matches_openrazer() {
        // Doc pattern: data_size 09, args 01 <led> 01 00 00 01 RR GG BB
        let r = effect_static_report(led::SCROLL_WHEEL, Rgb::new(0xFF, 0x00, 0x00));
        assert_eq!(r[5], 0x09);
        assert_eq!(
            &r[8..17],
            &[VARSTORE, led::SCROLL_WHEEL, 0x01, 0x00, 0x00, 0x01, 0xFF, 0x00, 0x00]
        );
        assert_eq!(r[88], crc(&r));
    }

    #[test]
    fn chroma_breathing_green_matches_openrazer() {
        // Doc: 01 05 02 01 00 01 00 ff 00 (data_size 09)
        let r = effect_breathing_report(led::LOGO, Rgb::new(0x00, 0xFF, 0x00));
        assert_eq!(r[5], 0x09);
        assert_eq!(
            &r[8..17],
            &[VARSTORE, led::LOGO, 0x02, 0x01, 0x00, 0x01, 0x00, 0xFF, 0x00]
        );
        assert_eq!(r[88], crc(&r));
    }

    #[test]
    fn rgb_hex_parsing() {
        assert_eq!(Rgb::parse_hex("#FF8800"), Ok(Rgb::new(0xFF, 0x88, 0x00)));
        assert_eq!(Rgb::parse_hex("00ff00"), Ok(Rgb::new(0, 255, 0)));
        assert!(Rgb::parse_hex("#12345").is_err());
        assert!(Rgb::parse_hex("gggggg").is_err());
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
