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
- **Polling rate** — set and lock the report rate (up to 8000 Hz on devices that
  support it), from the settings window, the config, or `charmctl set-poll`; the
  dropdown offers exactly the connected device's supported rates.
- **DPI-button remap** — the two buttons behind the wheel emit private Razer vendor codes
  (`0x20`/`0x21`) the OS can't see. Snakecharmer catches them at the HID layer and turns
  them into keystrokes (default: copy / paste).
- **Thumb-button remap** — the side Back/Forward buttons remapped to keystrokes via a
  low-level `WH_MOUSE_LL` hook that suppresses the original.
- **RGB lighting** — static, breathing, spectrum, or off, for both lit zones.
- **System tray** with quick DPI, lighting, reload, and quit, plus a native settings
  window (no admin, windowless until opened).

## Scope

A short, deliberate list of devices — see
[`docs/SUPPORTED-DEVICES.md`](docs/SUPPORTED-DEVICES.md) for the current table. Each
device is described by a small `DeviceSpec`, so adding another mouse is a one-file diff
and a contained job — a good one to hand an AI coding agent. See
[`CONTRIBUTING.md`](CONTRIBUTING.md) and
[`CRACKING-MICE-GUIDE.md`](CRACKING-MICE-GUIDE.md).

Snakecharmer also isn't trying to be everything — if you want cross-brand RGB game sync,
per-app button profiles, or you're on Linux, other tools serve you better. See
[`docs/ALTERNATIVES.md`](docs/ALTERNATIVES.md) for an honest map of the landscape and
which tool fits which need.

## Requirements

- Windows 10/11
- A [supported Razer mouse](docs/SUPPORTED-DEVICES.md)
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
# polling_rate = 1000    # Hz, per-device (see docs/SUPPORTED-DEVICES.md); omit = leave as-is
```

## `charmctl` — command-line control

```
charmctl status                          device mode + DPI + polling rate (read-only)
charmctl set-dpi X [Y]                    set DPI
charmctl set-poll <hz>                    set polling rate (Hz)
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

## Upcoming

Candidate features, filtered for compatibility with the zero-overhead ethos (see
[`docs/ALTERNATIVES.md`](docs/ALTERNATIVES.md) for where these came from). Roughly in
order of plausibility; none are promises:

- **DPI stages** — cycle presets from the DPI buttons instead of remapping them; pure
  software, no new protocol work.
- **More devices** — the protocol crate is built for it; crack your own mouse with
  [`CRACKING-MICE-GUIDE.md`](CRACKING-MICE-GUIDE.md) and send a PR.
- **Lift-off distance / surface calibration** — protocol exists in OpenRazer; needs
  careful verification since it touches sensor behavior.
- **Per-application profiles** — possible, but adds a foreground-window watcher; only
  lands if it provably keeps idle cost at ~0.
- **Firmware-level protocol research on owned hardware** — the natural extension of the
  cracking guide (see [`docs/how-deep-could-you-go.md`](docs/how-deep-could-you-go.md)):
  documenting a device's protocol at the source to speed up support for the next mouse.
  Research track, not a shipping feature.

Non-goals, so you don't wait for them: game-synced RGB, audio visualizers, cloud
profiles, accounts, telemetry. If you need those, see the
[alternatives](docs/ALTERNATIVES.md) — some are genuinely good.

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

Staying in userland with documented commands is a design choice, not a limitation — for
the full argument (and the ladder of what "going deeper" would actually mean), see
[`docs/how-deep-could-you-go.md`](docs/how-deep-could-you-go.md).

### What happens if…

<details>
<summary><b>…I unplug the mouse while Snakecharmer is running?</b></summary>

The session ends and the daemon retries every 3 seconds. When a supported mouse comes
back (the same one or a different supported model), it reconnects and re-applies your
DPI, driver mode, and lighting automatically. Nothing to restart.
</details>

<details>
<summary><b>…I run it with a Naga, a keyboard, or any other Razer device?</b></summary>

Nothing is ever written to it. Snakecharmer only opens devices whose USB product id is
in its [supported-devices table](docs/SUPPORTED-DEVICES.md); anything else, Razer or
not — including DeathAdder versions not yet in the table — is never touched. The daemon
just waits in its 3-second retry loop for a supported mouse to appear.
</details>

<details>
<summary><b>…I configure a thumb-button remap? (the one global setting)</b></summary>

The thumb remap uses a system-wide `WH_MOUSE_LL` hook, so a configured Back/Forward
remap applies to **every pointing device on the PC** — including non-Razer mice —
even while no DeathAdder Elite is plugged in. The default is `none` (native
Back/Forward untouched), and quitting Snakecharmer removes the hook. Everything else
(DPI, lighting, DPI-button remap) is strictly per-device.
</details>

<details>
<summary><b>…I plug in two supported mice at once?</b></summary>

Not really supported: commands go to whichever unit Windows enumerates first, and
DPI-button presses from either mouse of the same model trigger actions. Harmless, but
arbitrary — plug in one at a time.
</details>
