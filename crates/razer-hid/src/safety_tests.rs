//! Helper-level fault cases below use independent local gates. The Mouse
//! boundary cases use the production typed methods and process-wide COMMANDS
//! gate, in isolated test subprocesses with simulated feature transports.
//! Neither layer exercises HID enumeration, HidD_* calls, or device firmware.

use super::*;
use std::collections::VecDeque;

type Report = [u8; REPORT_LEN];

pub(super) struct Fake {
    steps: VecDeque<(Report, Vec<u8>)>,
    sends: usize,
    receives: usize,
    send_error: Option<Error>,
    receive_error: Option<Error>,
}

impl Fake {
    fn new(steps: impl IntoIterator<Item = (Report, Vec<u8>)>) -> Self {
        Self {
            steps: steps.into_iter().collect(),
            sends: 0,
            receives: 0,
            send_error: None,
            receive_error: None,
        }
    }
}

impl FeatureIo for Fake {
    fn send(&mut self, report: &[u8; REPORT_LEN + 1]) -> Result<()> {
        self.sends += 1;
        if let Some(error) = self.send_error.take() {
            return Err(error);
        }
        let expected = &self.steps.front().expect("unexpected device command").0;
        assert_eq!(report[0], 0, "HID report id");
        assert_eq!(&report[1..], expected);
        Ok(())
    }

    fn receive(&mut self, report: &mut [u8; REPORT_LEN + 1]) -> Result<usize> {
        self.receives += 1;
        if let Some(error) = self.receive_error.take() {
            return Err(error);
        }
        let (_, response) = self.steps.pop_front().expect("unexpected response read");
        report[..response.len()].copy_from_slice(&response);
        Ok(response.len())
    }
}

fn reply(request: Report) -> Vec<u8> {
    let mut response = request.to_vec();
    response[0] = proto::status::SUCCESS;
    response
}

fn dpi_reply(request: Report, dpi: u16) -> Vec<u8> {
    let mut response = reply(request);
    response[9..11].copy_from_slice(&dpi.to_be_bytes());
    response[11..13].copy_from_slice(&dpi.to_be_bytes());
    response
}

fn gate() -> SafetyGate {
    SafetyGate { stopped: None }
}

#[test]
fn every_non_success_status_stops_the_shared_helper_gate_without_retry() {
    for status in [0x00, 0x01, 0x03, 0x04, 0x05, 0xff] {
        let request = proto::get_dpi_report(0x1f);
        let mut response = reply(request);
        response[0] = status;
        let mut first = Fake::new([(request, response)]);
        let mut safety = gate();
        assert!(matches!(safety.run(|| first.exchange(&request)),
            Err(Error::Proto(proto::ProtoError::DeviceStatus(s))) if s == status));
        assert_eq!((first.sends, first.receives), (1, 1));

        // Another transport under this same local gate must not send.
        let mut reopened = Fake::new([]);
        assert!(matches!(
            safety.run(|| reopened.exchange(&request)),
            Err(Error::SafetyStop(_))
        ));
        assert_eq!((reopened.sends, reopened.receives), (0, 0));
    }
}

#[test]
fn malformed_response_and_wrong_echo_stop_the_channel() {
    let request = proto::get_dpi_report(0x1f);
    let mut wrong_echo = reply(request);
    wrong_echo[7] ^= 1;
    for response in [vec![], vec![0; 10], wrong_echo] {
        let mut io = Fake::new([(request, response)]);
        let mut safety = gate();
        assert!(safety.run(|| io.exchange(&request)).is_err());
        assert!(safety.check().is_err());
        assert_eq!(io.sends, 1);
    }
}

#[test]
fn report_id_is_optional_in_feature_response() {
    let request = proto::get_dpi_report(0x1f);
    let without_id = reply(request);
    let with_id = std::iter::once(0)
        .chain(without_id.iter().copied())
        .collect();
    for response in [without_id, with_id] {
        let mut io = Fake::new([(request, response)]);
        assert_eq!(io.exchange(&request).unwrap()[0], proto::status::SUCCESS);
    }
}

#[test]
fn transport_failures_stop_without_retry() {
    let request = proto::get_dpi_report(0x1f);
    for on_send in [true, false] {
        let mut io = Fake::new([(request, reply(request))]);
        let error = Error::Hid(hidapi::HidError::IoError {
            error: std::io::Error::other("device disconnected"),
        });
        if on_send {
            io.send_error = Some(error);
        } else {
            io.receive_error = Some(error);
        }
        let mut safety = gate();
        assert!(matches!(
            safety.run(|| io.exchange(&request)),
            Err(Error::Hid(_))
        ));
        assert!(safety.check().is_err());
        assert_eq!(io.sends, 1);
        assert_eq!(io.receives, usize::from(!on_send));
    }
}

#[test]
fn dpi_verification_helper_covers_each_device_spec() {
    for spec in proto::SUPPORTED {
        let get = proto::get_dpi_report(spec.transaction_id);
        let set =
            proto::set_dpi_report(spec.transaction_id, spec.dpi_min, spec.dpi_max, 1600, 1600)
                .unwrap();
        let mut io = Fake::new([
            (get, dpi_reply(get, 1800)),
            (set, reply(set)),
            (get, dpi_reply(get, 1600)),
        ]);
        assert_eq!(
            gate()
                .run(
                    || write_verified(&mut io, &set, (1600, 1600), |io| read_dpi(
                        io,
                        spec.transaction_id
                    ))
                )
                .unwrap(),
            (1600, 1600)
        );
        assert_eq!(io.sends, 3);
        assert!(io.steps.is_empty());
    }
}

#[test]
fn failed_pre_read_never_writes() {
    let spec = proto::DEATHADDER_V3;
    let get = proto::get_dpi_report(spec.transaction_id);
    let set =
        proto::set_dpi_report(spec.transaction_id, spec.dpi_min, spec.dpi_max, 1600, 1600).unwrap();
    let mut response = reply(get);
    response[0] = proto::status::BUSY;
    let mut io = Fake::new([(get, response)]);
    let mut safety = gate();
    assert!(safety
        .run(
            || write_verified(&mut io, &set, (1600, 1600), |io| read_dpi(
                io,
                spec.transaction_id
            ))
        )
        .is_err());
    assert_eq!(io.sends, 1);
    assert!(safety.check().is_err());
}

#[test]
fn failed_write_never_reads_back_or_runs_the_next_setting() {
    let spec = proto::DEATHADDER_V3;
    let get = proto::get_dpi_report(spec.transaction_id);
    let set =
        proto::set_dpi_report(spec.transaction_id, spec.dpi_min, spec.dpi_max, 1600, 1600).unwrap();
    let mut failed = reply(set);
    failed[0] = 0x03;
    let mut io = Fake::new([(get, dpi_reply(get, 1800)), (set, failed)]);
    let mut safety = gate();
    let result = safety.run(|| {
        write_verified(&mut io, &set, (1600, 1600), |io| {
            read_dpi(io, spec.transaction_id)
        })?;
        read_polling(&mut io, spec)
    });
    assert!(result.is_err());
    assert_eq!(io.sends, 2);
    assert!(safety.run(|| read_polling(&mut io, spec)).is_err());
    assert_eq!(io.sends, 2);
}

#[test]
fn read_back_mismatch_latches_and_preserves_the_original_reason() {
    let spec = proto::DEATHADDER_V3;
    let get = proto::get_dpi_report(spec.transaction_id);
    let set =
        proto::set_dpi_report(spec.transaction_id, spec.dpi_min, spec.dpi_max, 1600, 1600).unwrap();
    let mut io = Fake::new([
        (get, dpi_reply(get, 1800)),
        (set, reply(set)),
        (get, dpi_reply(get, 800)),
    ]);
    let mut safety = gate();
    let error = safety
        .run(|| {
            write_verified(&mut io, &set, (1600, 1600), |io| {
                read_dpi(io, spec.transaction_id)
            })
        })
        .unwrap_err();
    assert!(matches!(error, Error::Verify(_)));
    let reason = safety.stopped.clone();
    assert!(safety.run(|| read_polling(&mut io, spec)).is_err());
    assert_eq!(io.sends, 3);
    assert_eq!(safety.stopped, reason);
}

#[test]
fn verification_helper_handles_mode_and_both_polling_families() {
    let txn = 0x3f;
    let get = proto::get_device_mode_report(txn);
    let set = proto::set_device_mode_report(txn, DeviceMode::Driver);
    let mut after = reply(get);
    after[8] = DeviceMode::Driver.as_byte();
    let mut io = Fake::new([(get, reply(get)), (set, reply(set)), (get, after)]);
    gate()
        .run(|| {
            write_verified(&mut io, &set, DeviceMode::Driver.as_byte(), |io| {
                read_mode(io, txn)
            })
        })
        .unwrap();
    assert!(io.steps.is_empty());

    for spec in [proto::DEATHADDER_ELITE, proto::DEATHADDER_V3] {
        let get = proto::get_polling_rate_report(spec.transaction_id, spec.polling.protocol);
        let set = proto::set_polling_rate_report(spec.transaction_id, spec.polling, 500).unwrap();
        let mut response = reply(get);
        match spec.polling.protocol {
            proto::PollingProtocol::Classic => response[8] = 0x02,
            proto::PollingProtocol::Extended => response[9] = 0x10,
        }
        let mut io = Fake::new([(get, response.clone()), (set, reply(set)), (get, response)]);
        gate()
            .run(|| write_verified(&mut io, &set, 500, |io| read_polling(io, spec)))
            .unwrap();
        assert!(io.steps.is_empty());
    }
}

#[test]
fn verification_helper_handles_captured_binding() {
    let km = proto::keymap::spec_for(0x00b2).unwrap();
    let bind = proto::keymap::Binding::Disabled;
    let get = proto::keymap::read_binding_report(0x1f, 0x04);
    let set = proto::keymap::write_binding_report(km, 0x1f, 0x04, &bind).unwrap();
    let mut before = reply(get);
    before[11..14].copy_from_slice(&[0x01, 0x01, 0x04]);
    let after = reply(get); // captured disabled category and length are both zero
    let mut io = Fake::new([(get, before), (set, reply(set)), (get, after)]);
    gate()
        .run(|| {
            write_verified(&mut io, &set, bind.clone(), |io| {
                read_binding(io, 0x1f, 0x04)
            })
        })
        .unwrap();
    assert!(io.steps.is_empty());
}

#[test]
fn parser_failures_stop_and_local_validation_does_not() {
    let spec = proto::DEATHADDER_V3;
    let get = proto::get_polling_rate_report(spec.transaction_id, spec.polling.protocol);
    let mut io = Fake::new([(get, reply(get))]); // zero is not a polling-rate encoding
    let mut safety = gate();
    assert!(matches!(
        safety.run(|| read_polling(&mut io, spec)),
        Err(Error::Proto(proto::ProtoError::UnknownPollingRate(0)))
    ));
    assert!(safety.check().is_err());

    let mut safety = gate();
    assert!(safety
        .run(|| proto::set_dpi_report(0x1f, 100, 30000, 1, 1).map_err(Error::from))
        .is_err());
    assert!(safety.check().is_ok());
    assert!(!Error::DeviceNotFound.requires_stop());
    assert!(!Error::ControlOwnership(platform::process_lease::Error::Busy).requires_stop());
    assert!(
        !Error::ControlOwnership(platform::process_lease::Error::Os(std::io::Error::other(
            "ownership unavailable"
        )))
        .requires_stop()
    );
    assert!(Error::Verify("input collection closed".into()).requires_stop());
}

#[test]
fn wrong_button_reply_stops_before_an_onboard_write() {
    let km = proto::keymap::spec_for(0x00b2).unwrap();
    let get = proto::keymap::read_binding_report(0x1f, 0x04);
    let set =
        proto::keymap::write_binding_report(km, 0x1f, 0x04, &proto::keymap::Binding::Disabled)
            .unwrap();
    let mut wrong_button = reply(get);
    wrong_button[9] = 0x05;
    let mut io = Fake::new([(get, wrong_button)]);
    let mut safety = gate();
    assert!(matches!(
        safety.run(
            || write_verified(&mut io, &set, proto::keymap::Binding::Disabled, |io| {
                read_binding(io, 0x1f, 0x04)
            })
        ),
        Err(Error::Proto(proto::ProtoError::KeymapButtonMismatch { .. }))
    ));
    assert_eq!(io.sends, 1);
    assert!(safety.check().is_err());
}

#[test]
fn external_stop_blocks_commands_and_keeps_the_first_failure() {
    let mut safety = gate();
    safety.stop(&Error::Verify("input collection closed".into()));
    let first = safety.stopped.clone();
    safety.stop(&Error::Verify("later failure".into()));
    assert_eq!(safety.stopped, first);
    let mut io = Fake::new([]);
    assert!(safety.run(|| read_dpi(&mut io, 0x1f)).is_err());
    assert_eq!(io.sends, 0);
}

#[path = "mouse_boundary_tests.rs"]
mod mouse_boundary;
