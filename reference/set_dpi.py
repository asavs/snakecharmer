"""Set the DeathAdder Elite's DPI directly (no Synapse).

Useful because in driver mode the DPI buttons no longer change DPI in
hardware - pick your sensitivity here once. Survives mode switches but not
necessarily a power cycle (the mouse then returns to its stored stage).

Usage:
    python set_dpi.py           # show current DPI
    python set_dpi.py 1600      # set X and Y
    python set_dpi.py 1600 800  # set X and Y separately (100..16000)
"""
import sys

import razer_common as rz


def main() -> int:
    dev = rz.open_control()
    try:
        x, y = rz.get_dpi(dev)
        print(f"Current DPI: {x} x {y}")
        if len(sys.argv) < 2:
            return 0
        dpi_x = int(sys.argv[1])
        dpi_y = int(sys.argv[2]) if len(sys.argv) > 2 else dpi_x
        x, y = rz.set_dpi(dev, dpi_x, dpi_y)
        print(f"DPI now: {x} x {y}")
        return 0 if (x, y) == (dpi_x, dpi_y) else 1
    finally:
        dev.close()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except rz.RazerError as e:
        print(f"ERROR: {e}")
        raise SystemExit(1)
