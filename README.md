# Anti-Synapse

A native Windows replacement for Razer Synapse, scoped to the **Razer DeathAdder
Elite** (`VID 0x1532 / PID 0x005C`). Button remapping, DPI, and RGB lighting —
everything Synapse did that's actually useful — in a **single 436 KB exe** with no
background browser, no telemetry, and negligible idle CPU.

> Born from the session where we removed Synapse (its `CefSharp` browser was the
> single biggest CPU hog on the machine) and then reverse-engineered the mouse's HID
> protocol to keep the one feature worth keeping.

## Why

Synapse runs five processes and a Chromium instance in the background to do what
amounts to a few HID feature reports. Anti-Synapse talks to the mouse directly over
Win32 HID and stays out of the way.

| | Razer Synapse | Anti-Synapse |
|---|---|---|
| Footprint | ~5 processes + Chromium | one **436 KB** static exe |
| Idle CPU | constant | negligible (blocking HID reads, no poll loop) |
| Idle RAM | hundreds of MB | < 10 MB |
| Telemetry | yes | none — local-only |

## Features

- **DPI level** — set and lock sensitivity, re-asserted at login and periodically.
- **DPI-button remap** — the two buttons behind the wheel emit private Razer vendor
  codes (`0x20`/`0x21`) the OS can't see; caught at the HID layer and turned into
  keystrokes (default: copy / paste).
- **Thumb-button remap** — the side Back/Forward buttons remapped to keystrokes via a
  low-level `WH_MOUSE_LL` hook that suppresses the original.
- **RGB lighting** — static / breathing / spectrum / off for both lit zones.
- **System tray** with quick DPI, lighting, reload, and quit, plus a native settings
  window (no admin, windowless until opened).

## Requirements

- Windows 10/11
- A Razer DeathAdder Elite (this is intentionally single-device)
- [Rust](https://rustup.rs/) 1.97+ to build

## Build

```powershell
cargo build --release
```

Produces `target\release\anti-synapse.exe` (the windowless daemon) and
`target\release\asctl.exe` (the console control CLI).

## Run at login

```powershell
# Installs a hidden Startup-folder launcher for the current user (no admin):
.\scripts\install-autostart.ps1

# To remove it:
.\scripts\uninstall-autostart.ps1
```

## Configuration

Config lives at `%LOCALAPPDATA%\AntiSynapse\config.toml`, written with defaults on
first run and editable from the settings window. Defaults:

```toml
dpi = 1800
dpi_up = "copy"           # front DPI button
dpi_down = "paste"        # rear DPI button
thumb_back = "none"       # "none" = keep native Back
thumb_forward = "none"    # "none" = keep native Forward
lighting = "keep"         # keep | static | breathing | spectrum | off
color = "#00ff00"
reassert_interval_secs = 60
```

## `asctl` — command-line control

```
asctl status                          device mode + DPI (read-only)
asctl set-dpi X [Y]                    set DPI
asctl set-mode driver|hardware         set device mode
asctl set-color <#RRGGBB>              static color (both zones)
asctl set-effect static [#RRGGBB]      static color effect
asctl set-effect breathing [#RRGGBB]   breathing effect
asctl set-effect spectrum              spectrum cycling
asctl set-effect off                   lighting off
asctl self-test                        test keystroke injection (F13)
asctl where                            print config/log paths
```

## Architecture

```
anti-synapse/
├─ crates/
│  ├─ razer-proto/   # pure protocol: report builder, CRC, mode/DPI/RGB commands (no I/O)
│  ├─ razer-hid/     # device open/enumerate, feature reports, input-report listener
│  └─ platform/      # Win32: single-instance, keystroke injection, WH_MOUSE_LL hook
└─ src/              # daemon, tray, native settings window, config, lighting
```

See [`docs/SPEC.md`](docs/SPEC.md) for the full design and the reverse-engineered
protocol notes.

## Safety

Only documented Razer commands (sourced from OpenRazer). No firmware/bootloader/DFU,
no fuzzing of feature reports — a bad write can wedge your only mouse. The mouse never
loses left/right click; unplug/replug always restores factory behavior.

## License

MIT
