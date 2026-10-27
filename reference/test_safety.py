"""Hardware-free regression tests for the runnable reference helpers.

Install a fake hid module before importing the helpers, and fake WinDLL while
importing the Windows listener. No test can open a device or inject a key.
"""

import ctypes
import importlib.util
from pathlib import Path
import sys
import types
import unittest
from unittest.mock import Mock, patch


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


hid = types.ModuleType("hid")
hid.device = Mock(side_effect=AssertionError("a test attempted to open hardware"))
hid.enumerate = Mock(side_effect=AssertionError("a test attempted to enumerate hardware"))
with patch.dict(sys.modules, {"hid": hid}):
    rz = load("razer_common", "razer_common.py")
    win32 = Mock()
    win32.VkKeyScanW.side_effect = lambda char: ord(char.value)
    win32.SendInput.side_effect = AssertionError("a test attempted input injection")
    with patch.object(ctypes, "WinDLL", return_value=win32, create=True):
        daemon = load("dpi_button_daemon", "dpi_button_daemon.py")


def reply(command_class, command_id, args=b"", status=2):
    response = bytearray(90)
    response[0] = status
    response[6:8] = bytes([command_class, command_id])
    response[8:8 + len(args)] = args
    return bytes(response)


def dpi_reply(x, y):
    return reply(4, 0x85, bytes([0, x >> 8, x & 255, y >> 8, y & 255]))


class FakeDevice:
    def __init__(self, replies=(), send_error=None):
        self.replies = iter(replies)
        self.sent = []
        self.receives = 0
        self.send_error = send_error

    def send_feature_report(self, report):
        self.sent.append(report)
        if self.send_error is not None:
            raise self.send_error
        return len(report)

    def get_feature_report(self, report_id, length):
        self.receives += 1
        response = next(self.replies)
        if isinstance(response, Exception):
            raise response
        return response

    def commands(self):
        return [(report[7], report[8]) for report in self.sent]


class SafetyTests(unittest.TestCase):
    def setUp(self):
        rz._stop_reason = None
        self.sleep = patch.object(rz.time, "sleep")
        self.sleep.start()
        self.addCleanup(self.sleep.stop)

    def assert_stopped_on_another_handle(self):
        later = FakeDevice()
        with self.assertRaisesRegex(rz.RazerError, "device commands stopped"):
            rz.get_device_mode(later)
        self.assertEqual(later.sent, [])

    def test_every_non_success_status_stops_without_retry(self):
        for status in [0, 1, 3, 4, 5, 255]:
            with self.subTest(status=status):
                rz._stop_reason = None
                dev = FakeDevice([reply(0, 0x84, status=status), reply(0, 0x84)])
                with self.assertRaisesRegex(rz.RazerError, "device returned status"):
                    rz.get_device_mode(dev)
                self.assertEqual(len(dev.sent), 1)
                self.assertEqual(dev.receives, 1)
                self.assert_stopped_on_another_handle()

    def test_malformed_and_wrong_echo_responses_stop(self):
        for response in [b"", b"short", reply(4, 0x85)]:
            with self.subTest(response=response):
                rz._stop_reason = None
                dev = FakeDevice([response])
                with self.assertRaises(rz.RazerError):
                    rz.get_device_mode(dev)
                self.assert_stopped_on_another_handle()

    def test_transport_errors_stop_without_further_exchange(self):
        for send_error, replies in [(OSError("send failed"), []),
                                    (None, [OSError("receive failed")])]:
            with self.subTest(send_error=send_error):
                rz._stop_reason = None
                dev = FakeDevice(replies, send_error)
                with self.assertRaises(OSError):
                    rz.get_device_mode(dev)
                self.assertEqual(len(dev.sent), 1)
                self.assertEqual(dev.receives, 0 if send_error else 1)
                self.assert_stopped_on_another_handle()

    def test_short_send_stops_before_reading(self):
        dev = FakeDevice()
        dev.send_feature_report = Mock(return_value=0)
        with self.assertRaisesRegex(rz.RazerError, "expected 91"):
            rz.get_device_mode(dev)
        self.assertEqual(dev.receives, 0)
        self.assert_stopped_on_another_handle()

    def test_mode_setter_reads_before_and_after_the_exact_write(self):
        dev = FakeDevice([reply(0, 0x84, b"\x00"), reply(0, 4),
                          reply(0, 0x84, b"\x03")])
        self.assertEqual(rz.set_device_mode(dev, 3), 3)
        self.assertEqual(dev.commands(), [(0, 0x84), (0, 4), (0, 0x84)])
        # Captured/documented mode layout: 90-byte request plus report id 0.
        expected = bytes.fromhex("00 3f 00 00 00 02 00 04 03 00") + bytes(78) + b"\x05\x00"
        self.assertEqual(dev.sent[1], b"\x00" + expected)

    def test_dpi_setter_reads_before_and_after_the_exact_write(self):
        dev = FakeDevice([dpi_reply(1800, 1800), reply(4, 5), dpi_reply(1600, 800)])
        self.assertEqual(rz.set_dpi(dev, 1600, 800), (1600, 800))
        self.assertEqual(dev.commands(), [(4, 0x85), (4, 5), (4, 0x85)])
        expected = bytes.fromhex("00 3f 00 00 00 07 04 05 00 06 40 03 20 00 00")
        expected += bytes(73) + b"\x63\x00"
        self.assertEqual(dev.sent[1], b"\x00" + expected)

    def test_failed_pre_read_prevents_a_write(self):
        for setter, response in [(lambda dev: rz.set_device_mode(dev, 3), reply(0, 0x84, status=1)),
                                 (lambda dev: rz.set_dpi(dev, 1600), dpi_reply(1800, 1800)[:10])]:
            with self.subTest(setter=setter):
                rz._stop_reason = None
                dev = FakeDevice([response])
                with self.assertRaises(rz.RazerError):
                    setter(dev)
                self.assertEqual(len(dev.sent), 1)
                self.assert_stopped_on_another_handle()

    def test_failed_write_has_no_readback_or_later_setting(self):
        dev = FakeDevice([reply(0, 0x84), reply(0, 4, status=3)])
        with self.assertRaises(rz.RazerError):
            rz.set_device_mode(dev, 3)
        self.assertEqual(dev.commands(), [(0, 0x84), (0, 4)])
        self.assert_stopped_on_another_handle()

    def test_readback_mismatch_stops_both_setters(self):
        for setter, replies in [
            (lambda dev: rz.set_device_mode(dev, 3),
             [reply(0, 0x84), reply(0, 4), reply(0, 0x84)]),
            (lambda dev: rz.set_dpi(dev, 1600),
             [dpi_reply(1800, 1800), reply(4, 5), dpi_reply(1600, 800)]),
        ]:
            with self.subTest(setter=setter):
                rz._stop_reason = None
                dev = FakeDevice(replies)
                with self.assertRaisesRegex(rz.RazerError, "read-back mismatch"):
                    setter(dev)
                self.assert_stopped_on_another_handle()

    def test_invalid_local_values_do_not_send_or_stop_the_channel(self):
        dev = FakeDevice([reply(0, 0x84)])
        for setter in [lambda: rz.set_device_mode(dev, 1), lambda: rz.set_dpi(dev, 99)]:
            with self.assertRaises(ValueError):
                setter()
        self.assertEqual(dev.sent, [])
        self.assertEqual(rz.get_device_mode(dev), 0)

    def test_optional_report_id_is_accepted(self):
        self.assertEqual(rz.get_device_mode(FakeDevice([b"\x00" + reply(0, 0x84, b"\x03")])), 3)

    def test_first_failure_is_preserved_and_blocks_reopening(self):
        rz.stop_commands(OSError("first failure"))
        rz.stop_commands(OSError("later failure"))
        with self.assertRaisesRegex(rz.RazerError, "first failure"):
            rz.open_control()
        hid.device.assert_not_called()

    def test_only_initially_unreadable_listener_candidates_are_skipped(self):
        unreadable, readable = Mock(), Mock()
        unreadable.read.side_effect = OSError("not readable")
        readable.read.return_value = []
        candidates = [{"interface_number": 1, "path": b"unreadable"},
                      {"interface_number": 2, "path": b"readable"}]
        with patch.object(hid, "enumerate", return_value=candidates), \
                patch.object(hid, "device", side_effect=[unreadable, readable]):
            self.assertEqual(daemon.open_listeners(), [(readable, b"readable")])
        unreadable.close.assert_called_once_with()
        readable.close.assert_not_called()
        self.assertIsNone(rz._stop_reason)

    def test_daemon_exits_after_one_failed_session(self):
        with patch.object(daemon, "single_instance", return_value=True), \
                patch.object(daemon, "run_session", side_effect=rz.RazerError("failed")) as run, \
                patch.object(daemon, "log"):
            self.assertEqual(daemon.main(), 1)
            run.assert_called_once_with()
        self.assert_stopped_on_another_handle()

    def test_active_listener_failure_stops_before_reassertion(self):
        ctrl = Mock()
        listener = Mock()
        listener.read.side_effect = OSError("unplugged")
        with patch.object(rz, "open_control", return_value=ctrl), \
                patch.object(daemon, "ensure_driver_mode") as ensure, \
                patch.object(daemon, "open_listeners", return_value=[(listener, b"fake")]), \
                patch.object(daemon, "log"):
            with self.assertRaisesRegex(rz.RazerError, "active input collection"):
                daemon.run_session()
            ensure.assert_called_once_with(ctrl)
        listener.close.assert_called_once_with()
        ctrl.close.assert_called_once_with()
        self.assert_stopped_on_another_handle()


if __name__ == "__main__":
    unittest.main()
