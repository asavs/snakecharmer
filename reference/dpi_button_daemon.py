"""DPI-button daemon for the Razer DeathAdder Elite - no Synapse required.

What it does, forever, with tiny footprint:
  1. Puts the mouse into *driver mode* (volatile, documented Razer HID command;
     resets to normal on unplug). In driver mode the firmware stops consuming
     the two DPI buttons and instead reports them as vendor HID events.
  2. Listens on the mouse's auxiliary HID collections for those events
     (16-byte input report, report ID 0x04; code 0x20 = DPI up / front button,
     code 0x21 = DPI down / rear button - same values openrazer translates
     to F13/F14 on Linux).
  3. Injects the keystrokes you configure below via SendInput.
  4. Re-asserts driver mode after unplug/replug, sleep/resume, etc.

Configure the two buttons here:
"""

# ----------------------------------------------------------------------------
# CONFIG - what each button should do. Any of:
#   "copy"  (Ctrl+C)      "paste" (Ctrl+V)
#   "key:9" "key:0"       any single character key
#   "key:f13" ... "key:f24"  (invisible keys, remap with AutoHotkey/PowerToys)
ACTIONS = {
    "dpi_up": "copy",     # front button (closer to the wheel)
    "dpi_down": "paste",  # rear button
}
# ----------------------------------------------------------------------------

import ctypes
import os
import sys
import time
from ctypes import wintypes

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hid  # noqa: E402
import razer_common as rz  # noqa: E402

LOG_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "razer_daemon.log")
MODE_RECHECK_S = 60          # periodically confirm driver mode is still set
POLL_SLEEP_S = 0.008         # input poll interval
CODE_DPI_UP = 0x20
CODE_DPI_DOWN = 0x21

# ------------------------- keystroke injection (SendInput) ------------------
user32 = ctypes.WinDLL("user32", use_last_error=True)
ULONG_PTR = ctypes.wintypes.WPARAM


class KEYBDINPUT(ctypes.Structure):
    _fields_ = (("wVk", wintypes.WORD), ("wScan", wintypes.WORD),
                ("dwFlags", wintypes.DWORD), ("time", wintypes.DWORD),
                ("dwExtraInfo", ULONG_PTR))


class _INPUTunion(ctypes.Union):
    _fields_ = (("ki", KEYBDINPUT), ("padding", ctypes.c_ubyte * 32))


class INPUT(ctypes.Structure):
    _fields_ = (("type", wintypes.DWORD), ("union", _INPUTunion))


INPUT_KEYBOARD = 1
KEYEVENTF_KEYUP = 0x0002

VK = {"ctrl": 0x11, "shift": 0x10, "alt": 0x12}
VK.update({f"f{i}": 0x70 + i - 1 for i in range(1, 25)})  # f1..f24


def _vk_for(name: str) -> int:
    name = name.lower()
    if name in VK:
        return VK[name]
    if len(name) == 1:
        code = user32.VkKeyScanW(ctypes.c_wchar(name)) & 0xFF
        if code != 0xFF:
            return code
    raise ValueError(f"unknown key {name!r}")


def _chord_for(action: str):
    if action == "copy":
        return [VK["ctrl"], _vk_for("c")]
    if action == "paste":
        return [VK["ctrl"], _vk_for("v")]
    if action.startswith("key:"):
        return [_vk_for(action[4:])]
    raise ValueError(f"unknown action {action!r}")


def send_chord(vks):
    events = [(vk, 0) for vk in vks] + [(vk, KEYEVENTF_KEYUP) for vk in reversed(vks)]
    arr = (INPUT * len(events))()
    for i, (vk, flags) in enumerate(events):
        arr[i].type = INPUT_KEYBOARD
        arr[i].union.ki = KEYBDINPUT(vk, 0, flags, 0, 0)
    if user32.SendInput(len(events), arr, ctypes.sizeof(INPUT)) != len(events):
        log(f"SendInput failed: {ctypes.get_last_error()}")


CHORDS = {"dpi_up": _chord_for(ACTIONS["dpi_up"]),
          "dpi_down": _chord_for(ACTIONS["dpi_down"])}

# ------------------------------- logging ------------------------------------


def log(msg: str):
    line = f"{time.strftime('%Y-%m-%d %H:%M:%S')} {msg}"
    try:
        if os.path.exists(LOG_PATH) and os.path.getsize(LOG_PATH) > 512 * 1024:
            os.replace(LOG_PATH, LOG_PATH + ".old")
        with open(LOG_PATH, "a", encoding="utf-8") as f:
            f.write(line + "\n")
    except OSError:
        pass
    print(line, flush=True)


def single_instance() -> bool:
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.CreateMutexW(None, False, "Local\\RazerDpiButtonDaemon")
    return ctypes.get_last_error() != 183  # ERROR_ALREADY_EXISTS

# ------------------------------ device handling ------------------------------


def ensure_driver_mode(ctrl) -> None:
    mode = rz.get_device_mode(ctrl)
    if mode != rz.MODE_DRIVER:
        readback = rz.set_device_mode(ctrl, rz.MODE_DRIVER)
        if readback != rz.MODE_DRIVER:
            raise rz.RazerError(f"driver mode not accepted (read back 0x{readback:02x})")
        log("Driver mode enabled (was 0x%02x)." % mode)


def open_listeners():
    """Open every readable auxiliary collection (skip IF0 mouse; Windows blocks
    reading keyboard/mouse top-level collections anyway - those raise and are
    skipped)."""
    listeners = []
    for d in hid.enumerate(rz.VENDOR_ID, rz.PRODUCT_ID):
        if d["interface_number"] == 0:
            continue
        try:
            dev = hid.device()
            dev.open_path(d["path"])
            dev.set_nonblocking(1)
            listeners.append((dev, d["path"]))
        except OSError:
            continue
    return listeners


def run_session() -> None:
    ctrl = rz.open_control()
    listeners = []
    try:
        ensure_driver_mode(ctrl)
        listeners = open_listeners()
        if not listeners:
            raise rz.RazerError("no readable auxiliary HID collections")
        log(f"Listening on {len(listeners)} collection(s).")
        pressed = {}   # path -> set of active codes
        last_check = time.monotonic()
        while True:
            got = False
            dead = []
            for dev, path in listeners:
                try:
                    data = dev.read(32)
                except (OSError, IOError, ValueError):
                    # Windows opens keyboard-class collections without read
                    # access; reading them fails. Drop just that collection.
                    dead.append((dev, path))
                    continue
                if not data:
                    continue
                got = True
                if data[0] != 0x04:
                    continue
                codes = {b for b in data[1:] if b}
                prev = pressed.get(path, set())
                for code in codes - prev:
                    if code == CODE_DPI_UP:
                        log("DPI-up pressed -> %s" % ACTIONS["dpi_up"])
                        send_chord(CHORDS["dpi_up"])
                    elif code == CODE_DPI_DOWN:
                        log("DPI-down pressed -> %s" % ACTIONS["dpi_down"])
                        send_chord(CHORDS["dpi_down"])
                    else:
                        log(f"Unmapped vendor code 0x{code:02x} (report {bytes(data).hex(' ')})")
                pressed[path] = codes
            if dead:
                for dev, path in dead:
                    listeners.remove((dev, path))
                    try:
                        dev.close()
                    except Exception:
                        pass
                log(f"Dropped {len(dead)} unreadable collection(s); {len(listeners)} remain.")
                if not listeners:
                    raise rz.RazerError("all listener collections became unreadable")
            if not got:
                time.sleep(POLL_SLEEP_S)
            if time.monotonic() - last_check > MODE_RECHECK_S:
                ensure_driver_mode(ctrl)   # re-assert after sleep/resume etc.
                last_check = time.monotonic()
    finally:
        for dev, _ in listeners:
            try:
                dev.close()
            except Exception:
                pass
        try:
            ctrl.close()
        except Exception:
            pass


def main():
    if not single_instance():
        print("Another instance is already running; exiting.")
        return
    log(f"Daemon starting. Actions: {ACTIONS}")
    while True:
        try:
            run_session()
        except (rz.RazerError, OSError, ValueError) as e:
            log(f"Session ended: {e}. Retrying in 3 s (mouse unplugged/asleep?).")
            time.sleep(3)
        except KeyboardInterrupt:
            log("Stopped by user.")
            return


if __name__ == "__main__":
    main()
