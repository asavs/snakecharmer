"""One-shot device mode switch for the Razer DeathAdder Elite.

Usage:
    python set_device_mode.py            # show current mode
    python set_device_mode.py driver     # driver mode: DPI buttons emit HID events
    python set_device_mode.py hardware   # factory behavior: DPI buttons cycle DPI internally

The setting is volatile: unplugging the mouse (or rebooting, sometimes sleep)
returns it to hardware mode. Nothing here touches firmware.
"""
import sys

import razer_common as rz

NAMES = {rz.MODE_HARDWARE: "hardware (0x00)", rz.MODE_DRIVER: "driver (0x03)"}


def main() -> int:
    dev = rz.open_control()
    try:
        current = rz.get_device_mode(dev)
        print(f"Current mode: {NAMES.get(current, hex(current))}")
        if len(sys.argv) < 2:
            return 0
        want = {"driver": rz.MODE_DRIVER, "hardware": rz.MODE_HARDWARE}.get(sys.argv[1].lower())
        if want is None:
            print("Argument must be 'driver' or 'hardware'.")
            return 2
        if want == current:
            print("Already in the requested mode.")
            return 0
        readback = rz.set_device_mode(dev, want)
        print(f"Mode after set: {NAMES.get(readback, hex(readback))}")
        if readback != want:
            print("WARNING: read-back does not match the requested mode.")
            return 1
        print("OK.")
        return 0
    finally:
        dev.close()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except rz.RazerError as e:
        print(f"ERROR: {e}")
        raise SystemExit(1)
