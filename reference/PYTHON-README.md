# DeathAdder Elite DPI buttons without Synapse

Makes the two DPI buttons behind the scroll wheel do **Copy / Paste** (or any
keys you choose) on Windows, with no Razer Synapse installed.

## How it works

Razer mice have a volatile "device mode" HID feature (protocol taken verbatim
from the open-source [OpenRazer](https://github.com/openrazer/openrazer) Linux
driver — the same reversible command Synapse sends at every boot; **firmware is
never touched**, and unplugging the mouse always restores factory behavior):

- **Hardware mode `0x00`** (factory): the DPI buttons cycle DPI stages inside
  the firmware and send *nothing* to the PC.
- **Driver mode `0x03`**: the firmware stops handling them and instead sends a
  vendor HID event — a 16-byte input report (ID `0x04`) on the auxiliary
  keyboard-style interface, code `0x20` = DPI up, `0x21` = DPI down.

Important honesty note: even in driver mode the buttons **do not become mouse
buttons 6/7** — Windows only has 5 mouse buttons, and the mouse reports these
as vendor events that no stock driver understands (on Linux, OpenRazer's
kernel driver translates them to F13/F14). So X-Mouse Button Control cannot
see them directly. Instead, `dpi_button_daemon.py` does the whole job: it
enables driver mode, listens for codes `0x20`/`0x21`, and injects the
keystrokes you configure (default: Ctrl+C / Ctrl+V).

## Files

| File | Purpose |
|---|---|
| `razer_common.py` | Razer HID protocol (90-byte feature report, CRC, device-mode & DPI commands) |
| `dpi_button_daemon.py` | The background daemon: enables driver mode + maps the buttons. **Edit `ACTIONS` at the top to change mappings** (`"copy"`, `"paste"`, `"key:9"`, `"key:0"`, `"key:f13"`, …) |
| `set_device_mode.py` | Manual mode switch: `python set_device_mode.py driver\|hardware` (no arg = show) |
| `set_dpi.py` | Set sensitivity directly, since DPI buttons no longer cycle DPI: `python set_dpi.py 1600` |
| `install_startup.ps1` | Adds a Startup-folder shortcut so the daemon runs (windowless) at every login |
| `uninstall_startup.ps1` | Removes autostart, stops the daemon, restores hardware mode |
| `razer_daemon.log` | Daemon log (created at runtime; shows every button press it saw) |

## Setup

```powershell
python -m pip install hidapi          # already done
powershell -ExecutionPolicy Bypass -File install_startup.ps1
```

Then log out/in, or start it immediately:

```powershell
Start-Process "C:\Program Files\Python313\pythonw.exe" -ArgumentList '"C:\Users\asas\razer-deathadder-nosynapse\dpi_button_daemon.py"'
```

Press the DPI buttons and check `razer_daemon.log` — every press is logged.

## Changing what the buttons do

Edit the `ACTIONS` dict at the top of `dpi_button_daemon.py`, e.g.:

```python
ACTIONS = {"dpi_up": "key:9", "dpi_down": "key:0"}
```

then restart the daemon (`uninstall`/re-run, or kill pythonw and relaunch).
You can also map them to `key:f13`/`key:f14` and let AutoHotkey/PowerToys
handle them, mirroring what OpenRazer does on Linux.

## Safety / recovery

- Device mode is **volatile**: unplug + replug the mouse and it is 100% back
  to factory behavior. Nothing persistent is ever written to the mouse.
- Only two documented OpenRazer commands are ever sent: set/get device mode
  (class `0x00`, id `0x04`/`0x84`) and set/get DPI (class `0x04`, id `0x05`/`0x85`),
  transaction id `0x3F` (the DeathAdder Elite's, per `razermouse_driver.c`).
- Left/right click, movement, wheel and the two side buttons are standard HID
  in both modes and are unaffected.
- While the daemon is *not* running but the mouse is in driver mode, the DPI
  buttons simply do nothing (they resume working the moment the daemon runs,
  or after a replug).

## Protocol reference (as researched from OpenRazer)

90-byte feature report, report ID 0, sent to the interface-0 (mouse) HID
collection; CRC = XOR of bytes 2..87:

```
offset:  0      1    2  3   4     5     6     7     8..87   88   89
        status  txn  remaining  proto  size  class  id      args  crc  rsvd
driver:  00     3F   00 00     00     02    00     04      03 00  05   00
hw:      00     3F   00 00     00     02    00     04      00 00  06   00
```

Response status `0x02` = success. Read-back with class `0x00`, id `0x84`.
