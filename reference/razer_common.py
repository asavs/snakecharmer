"""Shared Razer HID protocol helpers for the DeathAdder Elite (1532:005C).

Protocol source: OpenRazer (github.com/openrazer/openrazer), files
driver/razercommon.{c,h}, driver/razerchromacommon.c, driver/razermouse_driver.c.

The Razer control protocol is a 90-byte HID feature report (report ID 0):

  offset 0     status            (0x00 on request; response: 0x02=OK, 0x01=busy,
                                  0x03=failure, 0x04=timeout, 0x05=not supported)
  offset 1     transaction_id    (0x3F for the DeathAdder Elite)
  offset 2-3   remaining_packets (big-endian, 0)
  offset 4     protocol_type     (0)
  offset 5     data_size
  offset 6     command_class
  offset 7     command_id
  offset 8-87  arguments (80 bytes)
  offset 88    crc = XOR of bytes 2..87
  offset 89    reserved (0)

Commands used here (all documented in OpenRazer, nothing else is ever sent):
  set device mode : class 0x00, id 0x04, data_size 2, args [mode, 0x00]
                    mode 0x00 = hardware/normal, 0x03 = driver mode
  get device mode : class 0x00, id 0x84, data_size 2
  set DPI (xy)    : class 0x04, id 0x05, data_size 7,
                    args [0x00 (no varstore), dpi_x_hi, dpi_x_lo, dpi_y_hi, dpi_y_lo]
  get DPI (xy)    : class 0x04, id 0x85, data_size 7

The control endpoint on Windows is the interface-0 HID collection
(usage_page 0x0001, usage 0x0002 - the mouse top-level collection).
"""

import time

import hid

VENDOR_ID = 0x1532
PRODUCT_ID = 0x005C
TRANSACTION_ID = 0x3F  # DeathAdder Elite, per openrazer razermouse_driver.c

STATUS_NEW_COMMAND = 0x00
STATUS_BUSY = 0x01
STATUS_SUCCESS = 0x02
STATUS_FAILURE = 0x03
STATUS_TIMEOUT = 0x04
STATUS_NOT_SUPPORTED = 0x05

MODE_HARDWARE = 0x00
MODE_DRIVER = 0x03


class RazerError(Exception):
    pass


def build_report(command_class: int, command_id: int, data_size: int,
                 args: bytes = b"") -> bytes:
    """Build the 90-byte Razer report incl. CRC (XOR of bytes 2..87)."""
    if len(args) > 80:
        raise ValueError("arguments too long")
    buf = bytearray(90)
    buf[1] = TRANSACTION_ID
    buf[5] = data_size
    buf[6] = command_class
    buf[7] = command_id
    buf[8:8 + len(args)] = args
    crc = 0
    for i in range(2, 88):
        crc ^= buf[i]
    buf[88] = crc
    return bytes(buf)


def find_control_path() -> bytes:
    """Locate the interface-0 mouse collection (the Razer control endpoint)."""
    for d in hid.enumerate(VENDOR_ID, PRODUCT_ID):
        if d["interface_number"] == 0 and d["usage_page"] == 0x0001 and d["usage"] == 0x0002:
            return d["path"]
    raise RazerError("DeathAdder Elite (1532:005C) control interface not found - is the mouse plugged in?")


def send_command(dev: "hid.device", report: bytes) -> bytes:
    """Send one 90-byte report as feature report 0 and return the 90-byte response.

    Retries on BUSY, mirroring openrazer's razer_send_payload retry loop.
    """
    last_status = None
    for _ in range(5):
        n = dev.send_feature_report(b"\x00" + report)
        if n < 0:
            raise RazerError(f"send_feature_report failed: {dev.error()}")
        time.sleep(0.06)
        resp = bytes(dev.get_feature_report(0x00, 91))
        if len(resp) == 91:
            resp = resp[1:]  # strip report ID byte
        if len(resp) != 90:
            raise RazerError(f"unexpected response length {len(resp)}")
        last_status = resp[0]
        if last_status == STATUS_BUSY:
            time.sleep(0.1)
            continue
        if last_status != STATUS_SUCCESS:
            raise RazerError(f"device returned status 0x{last_status:02x} "
                             f"for command {report[6]:02x}/{report[7]:02x}")
        if resp[6] != report[6] or resp[7] != report[7]:
            raise RazerError("response does not match command echo")
        return resp
    raise RazerError(f"device stayed busy (status 0x{last_status:02x})")


def open_control() -> "hid.device":
    dev = hid.device()
    dev.open_path(find_control_path())
    return dev


def get_device_mode(dev: "hid.device") -> int:
    resp = send_command(dev, build_report(0x00, 0x84, 0x02))
    return resp[8]


def set_device_mode(dev: "hid.device", mode: int) -> int:
    """Set device mode and return the read-back mode for verification."""
    if mode not in (MODE_HARDWARE, MODE_DRIVER):
        raise ValueError("mode must be 0x00 (hardware) or 0x03 (driver)")
    send_command(dev, build_report(0x00, 0x04, 0x02, bytes([mode, 0x00])))
    time.sleep(0.05)
    return get_device_mode(dev)


def get_dpi(dev: "hid.device") -> tuple:
    resp = send_command(dev, build_report(0x04, 0x85, 0x07))
    dpi_x = (resp[9] << 8) | resp[10]
    dpi_y = (resp[11] << 8) | resp[12]
    return dpi_x, dpi_y


def set_dpi(dev: "hid.device", dpi_x: int, dpi_y: int = None) -> tuple:
    if dpi_y is None:
        dpi_y = dpi_x
    for v in (dpi_x, dpi_y):
        if not (100 <= v <= 16000):
            raise ValueError("DPI must be 100..16000 for the DeathAdder Elite")
    args = bytes([0x00,  # no variable storage (NOSTORE), per openrazer for this device
                  (dpi_x >> 8) & 0xFF, dpi_x & 0xFF,
                  (dpi_y >> 8) & 0xFF, dpi_y & 0xFF,
                  0x00, 0x00])
    send_command(dev, build_report(0x04, 0x05, 0x07, args))
    time.sleep(0.05)
    return get_dpi(dev)
