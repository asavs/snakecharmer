# Snakecharmer

> The Windows counterpart to **[OpenRazer](https://openrazer.github.io/)**. A lightweight replacement for [Razer Synapse](https://www.razer.com/synapse), featuring the essential settings and additionally button remapping!

```
=====================================================================
                 T H E   S N A K E   C H A R M E R
                        ~ of the Nile Delta ~
=====================================================================

           \  |  /
         `.  \|/  .'                                    *
       --- (  O  ) ---            .
         .'  /|\  `.                        __
           /  |  \               __        /  \
                                /  \      /    \        __
                     __        /    \    /      \      /  \
                    /  \      /      \  /        \    /    \
         __________/____\____/________\/__________\__/______\____
          . : ' .  ' : . ' . :  : ' . : ' .  : ' . : ' .  ' :


                  _.-==-._                          ♪
                 ((_.--._))                       o  ♫
                 ( ~o  o~ )                    ~  ♪         ___
                  \  __  /                  ~  ♫          .'   `.
                .-.\    /.-.                             ( (o o) )
               /   |;  ;|   \                             \ \_/ /
              |    |;  ;|    |                          ___;   ;
              |   /;    ;\   |                         (__     ;
               \ | ========<>=== ~ o ~                    \    ;
                \|  ;    ;  |/                           .'   ;
                 |  ;    ;  |                           ;   .'
                /   `-..-'   \                          ;   ;
               |   _|    |_   |                     ____;   ;____
              _|__(_)____(_)__|_                   (_____________)
             (__________________)                   \___________/
                                          

=====================================================================
   He plays, the cobra sways -- an old agreement between them,
   older than the pyramids on the horizon.
=====================================================================
```
## Why

Synapse runs a pile of background processes, one of them a full Chromium instance, to send
what are ultimately a handful of HID feature reports. Snakecharmer talks to the mouse
directly over Win32 HID and then gets out of the way.

| | Razer Synapse | Snakecharmer | Δ |
|---|---|---|---:|
| Processes | Razer Synapse 3<br>Razer Central<br>Razer Synapse Service Process<br>Razer Synapse Service<br>RazerCentralService<br>embedded Chromium browser (CefSharp) | `snakecharmer.exe`<br><sub>`crt-static`, no runtime deps</sub> | **6 → 1** |
| On-disk size | 500 MB<br><sub>Razer's published minimum</sub> | 0.43 MB (measured)<br><sub>single static exe</sub> | **~1,160×** |
| Idle RAM | ~558 MB (measured) | 17 MB (measured)<br><sub>2.5 MB private working set</sub> | **~32×** |
| Idle CPU | ≈ 1 CPU·h/day  | ≈ 0.06 CPU·h/day (measured)<br><sub>blocking HID reads, no poll loop</sub> | **~17×** |
| Telemetry | yes | none, local-only | — |

Over half a gigabyte of RAM, held permanently, to run two side buttons and a DPI setting. Yikes man

## Features

- **DPI level** — set and lock sensitivity; re-asserted at login and periodically.
- **DPI-button remap** — the two buttons behind the wheel emit private Razer vendor codes
  (`0x20`/`0x21`) the OS can't see. Snakecharmer catches them at the HID layer and turns
  them into keystrokes (default: copy / paste).
- **Thumb-button remap** — the side Back/Forward buttons remapped to keystrokes via a
  low-level `WH_MOUSE_LL` hook that suppresses the original.
- **RGB lighting** — static, breathing, spectrum, or off, for both lit zones.
- **System tray** with quick DPI, lighting, reload, and quit, plus a native settings
  window (no admin, windowless until opened).

## Scope

One device on purpose: the DeathAdder Elite (`VID 0x1532 / PID 0x005C`). The protocol
layer is small and well-tested, so adding another mouse (Razer or not) is a contained job,
and a good one to hand an AI coding agent. See [`CONTRIBUTING.md`](CONTRIBUTING.md) and
[`CRACKING-MICE-GUIDE.md`](CRACKING-MICE-GUIDE.md).

## Requirements

- Windows 10/11
- A Razer DeathAdder Elite
- To build from source: [Rust](https://rustup.rs/) 1.97+ — or just grab a prebuilt
  `snakecharmer.exe` from the Releases page
- Optional: Python 3, only if you're using the `reference/` toolkit to crack a new device

## Build

```powershell
cargo build --release
```

Produces `target\release\snakecharmer.exe` (the windowless daemon) and
`target\release\charmctl.exe` (the console control CLI).

## Run at login

Snakecharmer doesn't install itself. This drops a hidden shortcut in your Startup folder
so the daemon launches (windowless) at each login. Skip it if you'd rather start it by
hand.

```powershell
# add the autostart shortcut (current user, no admin):
.\scripts\install-autostart.ps1

# remove it:
.\scripts\uninstall-autostart.ps1
```

## Configuration

Config lives at `%LOCALAPPDATA%\Snakecharmer\config.toml`, written with defaults on first
run and editable from the settings window. Defaults:

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

## `charmctl` — command-line control

```
charmctl status                          device mode + DPI (read-only)
charmctl set-dpi X [Y]                    set DPI
charmctl set-mode driver|hardware         set device mode
charmctl set-color <#RRGGBB>              static color (both zones)
charmctl set-effect static [#RRGGBB]      static color effect
charmctl set-effect breathing [#RRGGBB]   breathing effect
charmctl set-effect spectrum              spectrum cycling
charmctl set-effect off                   lighting off
charmctl self-test                        test keystroke injection (F13)
charmctl where                            print config/log paths
```

## Architecture

```
snakecharmer/
├─ crates/
│  ├─ razer-proto/   # pure protocol: report builder, CRC, mode/DPI/RGB commands (no I/O)
│  ├─ razer-hid/     # device open/enumerate, feature reports, input-report listener
│  └─ platform/      # Win32: single-instance, keystroke injection, WH_MOUSE_LL hook
├─ src/              # daemon, tray, native settings window, config, lighting
└─ reference/        # runnable Python recon toolkit — worked example for cracking new devices
```

See [`docs/SPEC.md`](docs/SPEC.md) for the full design and the protocol notes.

## Relationship to OpenRazer & license

Snakecharmer's protocol knowledge — the report layout, command classes, CRC, transaction
IDs, and Chroma effect encodings — comes from
**[OpenRazer](https://github.com/openrazer/openrazer)** (`driver/razercommon.*`,
`razerchromacommon.c`, `razermouse_driver.c`). OpenRazer did the hard reverse-engineering;
Snakecharmer ports the DeathAdder Elite slice of it to Windows.

That makes Snakecharmer a derivative work of OpenRazer, so it carries the same copyleft:
the **GNU General Public License v2.0 or later**. See [`LICENSE`](LICENSE) and
[`NOTICE`](NOTICE).

Thank you very much to the OpenRazer maintainers! <3

## Safety

Only documented Razer commands (sourced from OpenRazer). No firmware, bootloader, or DFU,
and no fuzzing of feature reports — a bad write can wedge your only mouse. The mouse never
loses left/right click, and unplug/replug always restores factory behavior.
