//! These tests call Mouse methods, including their local validation and the
//! production global latch. Each terminal case gets its own process: production
//! safety state is never reset. Only test-owned named leases and fake I/O exist.

use super::*;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn child(case: &str) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "safety_tests::mouse_boundary::child_case", "--nocapture"])
        .env("SNAKECHARMER_TEST_MOUSE_CASE", case)
        .output().unwrap();
    assert!(output.status.success(), "case {case}:\n{}{}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}

fn mouse(spec: proto::DeviceSpec, io: Fake) -> Mouse {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = format!("Local\\SnakecharmerMouseBoundary-{}-{}",
        std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
    Mouse {
        dev: ControlDevice::Simulated(Mutex::new(io)),
        spec,
        product_id: spec.product_id,
        _control_lease: Arc::new(platform::process_lease::ProcessLease::acquire(&name).unwrap()),
    }
}

fn assert_io(mouse: &Mouse, sends: usize, remaining: usize) {
    assert_eq!(io_counts(mouse), (sends, sends, remaining));
}

fn io_counts(mouse: &Mouse) -> (usize, usize, usize) {
    let ControlDevice::Simulated(io) = &mouse.dev else { panic!("test opened real HID") };
    let io = io.lock().unwrap();
    (io.sends, io.receives, io.steps.len())
}

// Literal headers, argument bytes and checksums are independent of the report
// builders called by Mouse. DPI fixtures pin existing Snakecharmer behavior:
// selector 0x00 is not a claim of parity with the current OpenRazer helper.
fn request(header: [u8; 8], args: &[u8], checksum: u8) -> Report {
    let mut bytes = [0; REPORT_LEN];
    bytes[..8].copy_from_slice(&header);
    bytes[8..8 + args.len()].copy_from_slice(args);
    bytes[88] = checksum;
    bytes
}

fn get_dpi(txn: u8) -> Report {
    request([0, txn, 0, 0, 0, 7, 4, 0x85], &[], 0x86)
}

fn set_dpi(txn: u8) -> Report {
    request([0, txn, 0, 0, 0, 7, 4, 5], &[0, 6, 0x40, 3, 0x20, 0, 0], 0x63)
}

fn dpi_response(request: Report, x: u16, y: u16) -> Vec<u8> {
    let mut response = reply(request);
    response[9..11].copy_from_slice(&x.to_be_bytes());
    response[11..13].copy_from_slice(&y.to_be_bytes());
    response
}

fn assert_latched_on_other_mouse(spec: proto::DeviceSpec) {
    assert!(matches!(ensure_active(), Err(Error::SafetyStop(_))));
    let other = mouse(spec, Fake::new([]));
    assert!(matches!(other.get_dpi(), Err(Error::SafetyStop(_))));
    assert_io(&other, 0, 0);
}

#[test]
fn typed_mouse_writes_use_literal_requests_and_read_write_read() {
    for case in ["dpi-elite", "dpi-v3", "dpi-v4", "mode", "poll-classic", "poll-extended", "binding", "lighting"] {
        child(case);
    }
}

#[test]
fn typed_mouse_validation_rejects_before_io_without_stopping_valid_reads() { child("invalid"); }

#[test]
fn typed_mouse_non_success_reads_stop_other_mouse_commands() {
    for status in [0, 1, 3, 4, 5, 255] { child(&format!("status-{status}")); }
}

#[test]
fn typed_mouse_transport_and_parser_failures_latch() {
    for case in ["send-error", "receive-error", "short-response", "wrong-echo", "unknown-polling"] {
        child(case);
    }
}

#[test]
fn typed_mouse_failed_pre_read_prevents_write_and_latches() { child("pre-read"); }

#[test]
fn typed_mouse_failed_write_prevents_readback_and_latches() { child("write-ack"); }

#[test]
fn typed_mouse_readback_mismatch_latches_across_mouse_values() { child("mismatch"); }

#[test]
fn typed_mouse_lighting_failure_blocks_later_zone_and_other_mouse_commands() { child("lighting-failure"); }

#[test]
fn external_failure_latches_typed_mouse_commands() { child("external-stop"); }

#[test]
#[ignore = "subprocess entry; exercised by the named Mouse boundary tests"]
fn child_case() {
    let Ok(case) = std::env::var("SNAKECHARMER_TEST_MOUSE_CASE") else { return };
    let v3 = proto::DEATHADDER_V3;
    match case.as_str() {
        "dpi-elite" | "dpi-v3" | "dpi-v4" => {
            let (spec, txn) = match case.as_str() {
                "dpi-elite" => (proto::DEATHADDER_ELITE, 0x3f),
                "dpi-v4" => (proto::DEATHADDER_V4_PRO, 0x1f),
                _ => (v3, 0x1f),
            };
            let get = get_dpi(txn);
            let set = set_dpi(txn);
            let mouse = mouse(spec, Fake::new([
                (get, dpi_response(get, 1800, 1800)), (set, reply(set)),
                (get, dpi_response(get, 1600, 800)),
            ]));
            assert_eq!(mouse.set_dpi(1600, 800).unwrap(), (1600, 800));
            assert_io(&mouse, 3, 0);
        }
        "mode" => {
            let get = request([0, 0x3f, 0, 0, 0, 2, 0, 0x84], &[], 0x86);
            let set = request([0, 0x3f, 0, 0, 0, 2, 0, 4], &[3, 0], 5);
            let mut after = reply(get);
            after[8] = 3;
            let mouse = mouse(proto::DEATHADDER_ELITE,
                Fake::new([(get, reply(get)), (set, reply(set)), (get, after)]));
            assert_eq!(mouse.set_device_mode(DeviceMode::Driver).unwrap(), 3);
            assert_io(&mouse, 3, 0);
        }
        "poll-classic" | "poll-extended" => {
            let (spec, get, set, rate_index, rate_byte) = if case == "poll-classic" {
                (proto::DEATHADDER_ELITE,
                    request([0, 0x3f, 0, 0, 0, 1, 0, 0x85], &[], 0x84),
                    request([0, 0x3f, 0, 0, 0, 1, 0, 5], &[2], 6), 8, 2)
            } else {
                (v3, request([0, 0x1f, 0, 0, 0, 1, 0, 0xc0], &[], 0xc1),
                    request([0, 0x1f, 0, 0, 0, 2, 0, 0x40], &[0, 0x10], 0x52), 9, 0x10)
            };
            let mut response = reply(get);
            response[rate_index] = rate_byte;
            let mouse = mouse(spec, Fake::new([(get, response.clone()), (set, reply(set)), (get, response)]));
            assert_eq!(mouse.set_polling_rate(500).unwrap(), 500);
            assert_io(&mouse, 3, 0);
        }
        "binding" => {
            let get = request([0, 0x1f, 0, 0, 0, 10, 2, 0x8c], &[1, 4, 0], 0x81);
            let set = request([0, 0x1f, 0, 0, 0, 10, 2, 0x0c], &[1, 4, 0, 0, 0], 1);
            let mut before = reply(get);
            before[11..14].copy_from_slice(&[1, 1, 4]);
            let mouse = mouse(v3, Fake::new([(get, before), (set, reply(set)), (get, reply(get))]));
            assert_eq!(mouse.set_button_binding(proto::keymap::spec_for(v3.product_id).unwrap(),
                4, &proto::keymap::Binding::Disabled).unwrap(), proto::keymap::Binding::Disabled);
            assert_io(&mouse, 3, 0);
        }
        "lighting" | "lighting-failure" => {
            let get = get_dpi(0x3f);
            let wheel = request([0, 0x3f, 0, 0, 0, 9, 0x0f, 2], &[1, 1, 1, 0, 0, 1, 255, 0, 0], 0xfb);
            let logo = request([0, 0x3f, 0, 0, 0, 9, 0x0f, 2], &[1, 4, 1, 0, 0, 1, 255, 0, 0], 0xfe);
            let mut ack = reply(wheel);
            if case == "lighting-failure" { ack[0] = 3; }
            let mut steps = vec![(get, dpi_response(get, 1800, 1800)), (wheel, ack)];
            if case == "lighting" { steps.push((logo, reply(logo))); }
            let mouse = mouse(proto::DEATHADDER_ELITE, Fake::new(steps));
            let result = mouse.set_color(Rgb::new(255, 0, 0));
            if case == "lighting" {
                result.unwrap();
                assert_io(&mouse, 3, 0);
            } else {
                assert!(matches!(result, Err(Error::Proto(proto::ProtoError::DeviceStatus(3)))));
                assert_io(&mouse, 2, 0);
                assert!(matches!(mouse.set_zone_color(proto::led::LOGO, Rgb::new(255, 0, 0)),
                    Err(Error::SafetyStop(_))));
                assert_io(&mouse, 2, 0);
                assert_latched_on_other_mouse(v3);
            }
        }
        "invalid" => {
            let get = get_dpi(0x1f);
            let mouse = mouse(v3, Fake::new([(get, dpi_response(get, 1800, 1800))]));
            assert!(matches!(mouse.set_dpi(99, 1800), Err(Error::Proto(proto::ProtoError::DpiOutOfRange(99)))));
            assert!(matches!(mouse.set_dpi(1800, 30001), Err(Error::Proto(proto::ProtoError::DpiOutOfRange(30001)))));
            assert!(matches!(mouse.set_polling_rate(250), Err(Error::Proto(proto::ProtoError::PollingRateUnsupported { .. }))));
            assert!(matches!(mouse.get_button_binding(0x7f), Err(Error::Proto(proto::ProtoError::KeymapNotWritable(_)))));
            let km = proto::keymap::spec_for(v3.product_id).unwrap();
            assert!(mouse.set_button_binding(km, 0x60, &proto::keymap::Binding::Disabled).is_err());
            assert!(mouse.set_button_binding(km, 2, &proto::keymap::Binding::Disabled).is_err());
            let other_km = proto::keymap::KeymapSpec { product_id: 0, ..*km };
            assert!(mouse.set_button_binding(&other_km, 4, &proto::keymap::Binding::Disabled).is_err());
            mouse.set_color(Rgb::new(255, 0, 0)).unwrap(); // no lighting on V3
            assert_io(&mouse, 0, 1);
            ensure_active().unwrap();
            assert_eq!(mouse.get_dpi().unwrap(), (1800, 1800));
            assert_io(&mouse, 1, 0);
        }
        "pre-read" | "write-ack" | "mismatch" => {
            let get = get_dpi(0x1f);
            let set = set_dpi(0x1f);
            let mut before = dpi_response(get, 1800, 1800);
            if case == "pre-read" { before[0] = 1; }
            let mut steps = vec![(get, before)];
            if case != "pre-read" {
                let mut ack = reply(set);
                if case == "write-ack" { ack[0] = 3; }
                steps.push((set, ack));
                if case == "mismatch" { steps.push((get, dpi_response(get, 1600, 1600))); }
            }
            let count = steps.len();
            let mouse = mouse(v3, Fake::new(steps));
            let error = mouse.set_dpi(1600, 800).unwrap_err();
            if case == "mismatch" { assert!(matches!(error, Error::Verify(_))); }
            else { assert!(matches!(error, Error::Proto(proto::ProtoError::DeviceStatus(_)))); }
            assert_io(&mouse, count, 0);
            assert_latched_on_other_mouse(v3);
        }
        "external-stop" => {
            stop_commands(&Error::Verify("simulated listener failure".into()));
            let mouse = mouse(v3, Fake::new([]));
            assert!(matches!(mouse.set_device_mode(DeviceMode::Driver), Err(Error::SafetyStop(_))));
            assert_io(&mouse, 0, 0);
            assert_latched_on_other_mouse(v3);
        }
        "send-error" | "receive-error" | "short-response" | "wrong-echo" | "unknown-polling" => {
            let get = if case == "unknown-polling" {
                request([0, 0x1f, 0, 0, 0, 1, 0, 0xc0], &[], 0xc1)
            } else { get_dpi(0x1f) };
            let response = match case.as_str() {
                "short-response" => vec![0; 10],
                "wrong-echo" => { let mut response = reply(get); response[7] ^= 1; response }
                _ => reply(get),
            };
            let mut io = Fake::new([(get, response)]);
            let error = || Error::Hid(hidapi::HidError::IoError {
                error: std::io::Error::other("simulated transport failure"),
            });
            if case == "send-error" { io.send_error = Some(error()); }
            if case == "receive-error" { io.receive_error = Some(error()); }
            let mouse = mouse(v3, io);
            let error = if case == "unknown-polling" { mouse.get_polling_rate().unwrap_err() }
                else { mouse.get_dpi().unwrap_err() };
            assert!(error.requires_stop());
            let (receives, remaining) = match case.as_str() {
                "send-error" => (0, 1), "receive-error" => (1, 1), _ => (1, 0),
            };
            assert_eq!(io_counts(&mouse), (1, receives, remaining));
            assert_latched_on_other_mouse(v3);
        }
        case if case.starts_with("status-") => {
            let status: u8 = case[7..].parse().unwrap();
            let get = get_dpi(0x1f);
            let mut response = dpi_response(get, 1800, 1800);
            response[0] = status;
            let mouse = mouse(v3, Fake::new([(get, response)]));
            assert!(matches!(mouse.get_dpi(), Err(Error::Proto(proto::ProtoError::DeviceStatus(s))) if s == status));
            assert_io(&mouse, 1, 0);
            assert_latched_on_other_mouse(v3);
        }
        other => panic!("unknown isolated mouse test {other}"),
    }
}
