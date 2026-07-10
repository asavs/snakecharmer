# Psylli

A lightweight, open configuration tool for the **Razer DeathAdder Elite** on
**Windows** — DPI, RGB lighting, and button remapping in a single **436 KB** native
exe, with no background browser, no telemetry, and negligible idle CPU.

Think of it as the **Windows sibling of [OpenRazer](https://openrazer.github.io/)**:
OpenRazer opened up Razer's HID protocol on Linux, but it's Linux-only (it ships as
kernel modules). Psylli brings that same protocol to Windows as a plain userspace app —
and adds the one genuinely useful thing OpenRazer leaves to other tools: **button
remapping** (the feature you otherwise need Razer Synapse for).

> The name: the **Psylli** were an ancient North African people renowned as
> snake charmers, said to handle serpents unharmed. Razer's mascot is a snake; this
> tames it without Synapse.

## Why

Razer Synapse runs several processes and a full Chromium instance in the background to
do what amounts to a handful of HID feature reports. Psylli talks to the mouse directly
over Win32 HID and then gets out of the way.

| | Razer Synapse | Psylli |
|---|---|---|
| Footprint | ~5 processes + Chromium | one **436 KB** static exe |
| Idle CPU | constant | negligible (blocking HID reads, no poll loop) |
| Idle RAM | hundreds of MB | < 10 MB |
| Telemetry | yes | none — local-only |

## Features

- **DPI level** — set and lock sensitivity; re-asserted at login and periodically.
- **DPI-button remap** — the two buttons behind the wheel emit private Razer vendor
  codes (`0x20`/`0x21`) the OS can't see; Psylli catches them at the HID layer and turns
  them into keystrokes (default: copy / paste).
- **Thumb-button remap** — the side Back/Forward buttons remapped to keystrokes via a
  low-level `WH_MOUSE_LL` hook that suppresses the original.
- **RGB lighting** — static / breathing / spectrum / off for both lit zones.
- **System tray** with quick DPI, lighting, reload, and quit, plus a native settings
  window (no admin, windowless until opened).

## Scope

Deliberately **one device**: the DeathAdder Elite (`VID 0x1532 / PID 0x005C`). The
protocol layer is small and well-tested; adding another Razer device is a contained job —
see [`CONTRIBUTING.md`](CONTRIBUTING.md) *(coming soon)*.

## Requirements

- Windows 10/11
- A Razer DeathAdder Elite
- [Rust](https://rustup.rs/) 1.97+ to build

## Build

```powershell
cargo build --release
```

Produces `target\release\psylli.exe` (the windowless daemon) and
`target\release\psyctl.exe` (the console control CLI).

## Run at login

```powershell
# Installs a hidden Startup-folder launcher for the current user (no admin):
.\scripts\install-autostart.ps1

# To remove it:
.\scripts\uninstall-autostart.ps1
```

## Configuration

Config lives at `%LOCALAPPDATA%\Psylli\config.toml`, written with defaults on first run
and editable from the settings window. Defaults:

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

## `psyctl` — command-line control

```
psyctl status                          device mode + DPI (read-only)
psyctl set-dpi X [Y]                    set DPI
psyctl set-mode driver|hardware         set device mode
psyctl set-color <#RRGGBB>              static color (both zones)
psyctl set-effect static [#RRGGBB]      static color effect
psyctl set-effect breathing [#RRGGBB]   breathing effect
psyctl set-effect spectrum              spectrum cycling
psyctl set-effect off                   lighting off
psyctl self-test                        test keystroke injection (F13)
psyctl where                            print config/log paths
```

## Architecture

```
psylli/
├─ crates/
│  ├─ razer-proto/   # pure protocol: report builder, CRC, mode/DPI/RGB commands (no I/O)
│  ├─ razer-hid/     # device open/enumerate, feature reports, input-report listener
│  └─ platform/      # Win32: single-instance, keystroke injection, WH_MOUSE_LL hook
└─ src/              # daemon, tray, native settings window, config, lighting
```

See [`docs/SPEC.md`](docs/SPEC.md) for the full design and the protocol notes.

## Relationship to OpenRazer & license

Psylli's protocol knowledge — the report layout, command classes, CRC, transaction IDs,
and Chroma effect encodings — is derived from **[OpenRazer](https://github.com/openrazer/openrazer)**
(`driver/razercommon.*`, `razerchromacommon.c`, `razermouse_driver.c`). OpenRazer did the
hard reverse-engineering; Psylli ports the DeathAdder Elite slice of it to Windows.

Because that makes Psylli a derivative work of OpenRazer, it is licensed under the
**GNU General Public License v2.0 or later** — the same copyleft as OpenRazer. See
[`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).

## Safety

Only documented Razer commands (sourced from OpenRazer). No firmware/bootloader/DFU, no
fuzzing of feature reports — a bad write can wedge your only mouse. The mouse never loses
left/right click; unplug/replug always restores factory behavior.
