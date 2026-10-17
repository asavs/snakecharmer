# Snakecharmer

> The Windows counterpart to **[OpenRazer](https://openrazer.github.io/)**. A lightweight replacement for [Razer Synapse](https://www.razer.com/synapse) that covers the essential settings, plus button remapping.

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
| Kernel drivers | `RZCONTROL` + `RZVIRTUAL` virtual device drivers<br><sub>observed still registered after uninstalling Synapse</sub> | none (userland HID only) | **2 → 0** |
| On-disk size | 500 MB<br><sub>Razer's published minimum</sub> | 0.61 MB (measured)<br><sub>single static exe</sub> | **~820×** |
| Idle RAM | ~558 MB (measured) | 17 MB (measured)<br><sub>2.5 MB private working set</sub> | **~32×** |
| Idle CPU | ≈ 1 CPU·h/day  | ≈ 0.06 CPU·h/day (measured)<br><sub>blocking HID reads, no poll loop</sub> | **~17×** |
| Telemetry | yes | none, local-only | — |

Over half a gigabyte of RAM, held permanently, to run two side buttons and a DPI setting. Yikes man

## Features

- **DPI level.** Set and lock sensitivity. Snakecharmer re-applies it at login and
  periodically.
- **Polling rate.** Set and lock the report rate (up to 8000 Hz on devices that
  support it) from the settings window, the config, or `charmctl set-poll`. The
  dropdown lists only the rates the connected device supports.
- **DPI-button remap.** The two buttons behind the wheel send private Razer vendor codes
  (`0x20`/`0x21`) that the OS can't see. Snakecharmer catches them at the HID layer and turns
  them into keystrokes (default: copy / paste).
- **Thumb-button remap** (opt-in). Remaps the side Back/Forward buttons to keystrokes
  through a low-level `WH_MOUSE_LL` hook that suppresses the original. The hook exists only
  while a remap is configured. At the default (`none`/`none`) no hook is installed at
  all, so the pointer's motion path is untouched. Games accept MB4/MB5 natively,
  so most people don't need this.
- **On-board binds** (DeathAdder V3). Write a button's bind into the mouse's own memory,
  so it works with Snakecharmer closed, in games that block injected input, and on other
  PCs. See [On-board binds](#on-board-binds).
- **RGB lighting.** Static, breathing, spectrum, or off, per zone. The wheel and
  logo can have different effects and colors (settings window), or share one
  (tray menu).
- **System tray** menu with quick DPI, lighting, a Start with Windows toggle, reload, and
  quit, plus a native settings window. It needs no admin rights and shows no window
  until you open one.

## Scope

Snakecharmer supports a short, deliberate list of devices, listed in the
[supported devices table](docs/SUPPORTED-DEVICES.md). Each device is described by a small
`DeviceSpec`, so adding another mouse is a one-file diff. It's a contained job and a good
one to hand an AI coding agent.

- [`CONTRIBUTING.md`](CONTRIBUTING.md): how to contribute.
- [`CRACKING-MICE-GUIDE.md`](docs/CRACKING-MICE-GUIDE.md): how to work out a mouse's protocol.
- [`docs/DRAWING-MICE-GUIDE.md`](docs/DRAWING-MICE-GUIDE.md): how to draw its button-map
  diagram.
- [`docs/DEVICE-WISHLIST.md`](docs/DEVICE-WISHLIST.md): the mice that are up for grabs,
  what OpenRazer already knows about each one's protocol, and a prompt you can paste into
  any AI assistant to do the work.
- [`AGENTS.md`](AGENTS.md): the brief those assistants work from. It isn't specific to any
  one tool or model.

### One target, and it's Razer

The essential settings and nothing else: DPI, polling rate, button rebinds, and lighting
as a *setting*. You pick a color or a basic effect, it's applied, and the program goes
quiet. Snakecharmer does not animate your lighting, sync it to anything, or run a macro
engine, and it never will. All three need something running all the time, which is the
cost this project exists to avoid.

Other tools already do those things well, so use them:

| If you want | Use |
|---|---|
| RGB synced to your games, across brands | [SignalRGB](https://signalrgb.com/) |
| Lighting on many devices, free | [OpenRGB](https://openrgb.org/) |
| Per-application button profiles | [X-Mouse Button Control](https://www.highrez.co.uk/downloads/xmousebuttoncontrol.htm) |
| Macros | [AutoHotkey](https://www.autohotkey.com/) |
| Linux | [OpenRazer](https://openrazer.github.io/) + [Polychromatic](https://polychromatic.app/) |
| macOS | [razer-macos](https://github.com/1kc/razer-macos) |
| Razer *and* Logitech in one tool | [libratbag](https://github.com/libratbag/libratbag) + [Piper](https://github.com/libratbag/piper) |
| To install nothing at all | [usemice](https://github.com/robbieplata/usemice) |

Running Snakecharmer *alongside* one of them is fine and sometimes ideal. For example,
Snakecharmer handles the DPI buttons X-Mouse can't see, and X-Mouse does the per-app logic
Snakecharmer doesn't have. [`docs/ALTERNATIVES.md`](docs/ALTERNATIVES.md) covers the full
list, including the cases where the honest answer is "not this."

## Requirements

- Windows 10/11
- A [supported Razer mouse](docs/SUPPORTED-DEVICES.md)
- To build from source: [Rust](https://rustup.rs/) 1.97+. Or download a prebuilt
  `snakecharmer.exe` from the Releases page.
- Optional: Python 3, only if you use the `reference/` toolkit to crack a new device

## Build

```powershell
cargo build --release
```

Produces `target\release\snakecharmer.exe` (the windowless daemon) and
`target\release\charmctl.exe` (the console control CLI).

## Start with Windows

On first run the daemon registers itself to start at login, using a per-user value named
`Snakecharmer` under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (no admin, no
installer). Toggle it with the tray menu's **Start with Windows** checkbox. If you turn
it off, it stays off. The same registry value is what lists the app in Task Manager →
Startup apps, and the tray checkbox reads the real registry state, so turning it off in
either place turns it off in both.

Upgrading from a build that used `scripts\install-autostart.ps1`? Delete the leftover
`Snakecharmer.vbs` (or any hand-made `Snakecharmer.lnk`) from your Startup folder
(`Win+R` → `shell:startup`), or the daemon launches twice at login. That's harmless,
since the second instance exits on the single-instance mutex, but it clutters the log.

## Configuration

Config lives at `%LOCALAPPDATA%\Snakecharmer\config.toml`, written with defaults on first
run and editable from the settings window. Defaults:

```toml
dpi = 1800
dpi_up = "copy"           # front DPI button
dpi_down = "paste"        # rear DPI button
thumb_back = "none"       # "none" = keep native Back
thumb_forward = "none"    # "none" = keep native Forward
lighting = "keep"         # keep | static | breathing | spectrum | off (all zones)
color = "#00ff00"
reassert_interval_secs = 60
# polling_rate = 1000    # Hz, per-device (see docs/SUPPORTED-DEVICES.md); omit = leave as-is

# Optional per-zone overrides (zones: "wheel", "logo"); unset fields fall back
# to the device-wide lighting/color above. The settings window writes these.
# [zones.logo]
# lighting = "breathing"
# color = "#ff0000"

# Optional on-board binds, written to the mouse itself (DeathAdder V3 only).
# [onboard]
# back = "ctrl+c"
# wheel_click = "default"
```

### On-board binds

Some mice can store a button's bind in their own memory, and then send it with no software
running. On the DeathAdder V3, Snakecharmer can write those binds, from the config or with
`charmctl keymap set`. It also reads what's stored and can put leftover binds (from
Synapse, say) back to default.

```toml
[onboard]
back = "ctrl+c"           # button name = bind
wheel_click = "f5"
forward = "default"       # put it back to how it came
```

- **Buttons:** `wheel_click`, `back`, `forward`, `wheel_up`, `wheel_down`. Left click and
  the underside DPI button are never written. Right click can only be set to `default`, to
  undo a remap Synapse left on it. A button you don't list keeps whatever it stores.
- **Binds:** `default`, `disabled`, a mouse function (`middle click`), or any key or chord
  on a standard keyboard (`a`, `/`, `f13`, `num7`, `pageup`, `ctrl+shift+t`, `copy`).
- **Settings window:** each button has a box on the mouse picture. Click it and press the
  key or combo you want, or pick from its list (Alt+Down); it's written right away, then
  the window asks you to press the button and confirms what the mouse actually sent.
- **When it's written:** on attach, only when different from what the mouse stores, and
  checked by read-back. The mouse keeps it, so it works with Snakecharmer closed.
- **Leftovers:** on attach the daemon reads the stored binds. If it finds non-default ones
  that `[onboard]` doesn't cover, it shows a notice offering to reset them.
- **Thumb remaps:** on a mouse that stores binds, the settings window edits back and forward
  on-board instead of in software. An existing `thumb_back` / `thumb_forward` remap is moved
  into `[onboard]` on first connect (logged); one the mouse can't store stays in software.

Every write replays the layout Synapse was captured sending
([capture](docs/captures/V3-20261006-135244/report.md),
[more buttons](docs/captures/V3-20261016-additional-buttons/)); the first write from
Snakecharmer itself was checked on hardware
([notes](docs/captures/V3-20261016-snakecharmer-write/notes.md)). Other mice need their own
capture first; see [`docs/ONBOARD-KEYMAP.md`](docs/ONBOARD-KEYMAP.md).

### PC Vitals health capsule

Snakecharmer writes a small status file (a "capsule") for PC Vitals at
`%LOCALAPPDATA%\PCVitals\providers\snakecharmer.json`. It is written only on events:
when the daemon starts, connects, loses or recovers a device session, changes polling
configuration, or quits. A 45-minute lease refresh runs inside the existing session wait
loop, so it adds no polling thread and no extra device I/O. The capsule contains bounded
counters, typed incident codes, the retry delay, the configured polling rate, and a
machine-salted opaque device ID. It contains no paths, usernames, commands, window titles,
raw logs, hardware serials, or user input, and it cannot give PC Vitals authority to
change health severity, recommend a restart, or run an action. Recovered incident
episodes are kept for at most 24 hours and keep immutable IDs and start times. The
published capsule is output only; the daemon does not trust it as its own memory.

## `charmctl`: command-line control

```
charmctl devices                         list Razer mice plugged in, supported or not
charmctl status                          device mode + DPI + polling rate (read-only)
charmctl keymap                          button binds stored on the mouse
charmctl keymap reset                    reset changed binds to their defaults (V3)
charmctl keymap set <button> <bind>      write a bind to the mouse, e.g. back ctrl+c (V3)
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

Candidate features that fit the zero-overhead goal (see
[`docs/ALTERNATIVES.md`](docs/ALTERNATIVES.md) for where these came from), roughly from
most to least likely. None are promises.

- **More devices.** The protocol crate is built for it. Crack your own mouse with
  [`CRACKING-MICE-GUIDE.md`](docs/CRACKING-MICE-GUIDE.md) and send a PR.
- **Lift-off distance / surface calibration.** The protocol exists in OpenRazer, but it
  needs careful verification since it touches sensor behavior.
- **Per-application profiles.** Possible, but it adds a foreground-window watcher. It only
  lands if it provably keeps idle cost at ~0.
- **Firmware-level protocol research on owned hardware.** The next step after the
  cracking guide (see [`docs/how-deep-could-you-go.md`](docs/how-deep-could-you-go.md)):
  documenting a device's protocol at the source to speed up support for the next mouse.
  This is research, not a planned feature.

Non-goals, so you don't wait for them: game-synced RGB, audio visualizers, cloud
profiles, accounts, telemetry, and **DPI stages**. You get used to one pointer speed
and keep it, and those two buttons are far more useful rebound to something you
actually do. Reclaiming them is why this project exists. If you need the rest, see the
[alternatives](docs/ALTERNATIVES.md). Some are genuinely good.

## Relationship to OpenRazer & license

Snakecharmer's protocol knowledge (the report layout, command classes, CRC, transaction
IDs, and Chroma effect encodings) comes from
**[OpenRazer](https://github.com/openrazer/openrazer)** (`driver/razercommon.*`,
`razerchromacommon.c`, `razermouse_driver.c`). OpenRazer did the hard reverse-engineering;
Snakecharmer ports the DeathAdder Elite slice of it to Windows.

That makes Snakecharmer a derivative work of OpenRazer, so it carries the same copyleft:
the **GNU General Public License v2.0 or later**. See [`LICENSE`](LICENSE) and
[`NOTICE`](NOTICE).

Thank you very much to the OpenRazer maintainers! <3

## Safety

Snakecharmer sends only documented Razer commands (sourced from OpenRazer). It never
touches firmware, the bootloader, or DFU, and never fuzzes feature reports, because a bad
write can wedge your only mouse. It never rebinds left or right click, so you can't lock
yourself out of the mouse; the only write right click accepts is a reset to default.
Unplugging and replugging the mouse undoes everything the daemon sets live. On-board
binds are the exception: they stay on the mouse until you change them or run
`charmctl keymap reset`.

Staying in userland with documented commands is deliberate. For the full argument, and
what each level of "going deeper" would actually mean, see
[`docs/how-deep-could-you-go.md`](docs/how-deep-could-you-go.md).

### What happens if…

<details>
<summary><b>…I unplug the mouse while Snakecharmer is running?</b></summary>

The session ends and the daemon retries every 3 seconds. When a supported mouse comes
back (the same one or a different supported model), it reconnects and re-applies your
DPI, driver mode, and lighting automatically. You don't need to restart anything.
</details>

<details>
<summary><b>…I run it with a Naga, a keyboard, or any other Razer device?</b></summary>

Nothing is ever written to it. Snakecharmer only opens devices whose USB product id is
in its [supported devices table](docs/SUPPORTED-DEVICES.md). Anything else, Razer or
not, is never touched, including DeathAdder versions not yet in the table. The daemon
just waits in its 3-second retry loop for a supported mouse to appear.

It will *name* an unsupported mouse, though. Opening Settings with one plugged in says
which device it found and offers to open the
[device wishlist](docs/DEVICE-WISHLIST.md), because adding it is usually a small job:
its protocol is very likely already documented. The name comes from reading the USB
descriptors the OS already has (`charmctl devices` prints the same thing); the device
itself is still never opened or written to.
</details>

<details>
<summary><b>…I uninstalled Synapse, but a button still types a key?</b></summary>

That bind lives in the mouse's on-board memory, not on the PC. Synapse can save remaps to
the mouse itself, which then sends them with no software running, so uninstalling Synapse
leaves them in place. On the DeathAdder V3, `charmctl keymap` reads what's stored on each
button and flags anything that isn't the default, and `charmctl keymap reset` puts them
back to default; the daemon also offers that reset when it finds leftovers on attach. On
other mice that store binds on-board, Snakecharmer can't reset them yet, so **reset the
buttons to default in Synapse before uninstalling it**.
[`reference/button_listener.ps1`](reference/button_listener.ps1) shows what each button
really sends, and [`docs/ONBOARD-KEYMAP.md`](docs/ONBOARD-KEYMAP.md) is the work on
decoding it. The DeathAdder Elite is not affected: Synapse 4 remaps it in software.

For the PC software, the [Synapse removal audit](docs/RAZER-REMOVAL.md) records
the services, driver filters, installer caches, COM registrations, and other
residue found on the research machine, including what its uninstaller left behind.
</details>

<details>
<summary><b>…I configure a thumb-button remap? (the one global setting)</b></summary>

The thumb remap uses a system-wide `WH_MOUSE_LL` hook, so a configured Back/Forward
remap applies to **every pointing device on the PC**, including non-Razer mice, even
while no supported mouse is plugged in. A low-level hook also routes every mouse
event through an extra check, which is why it's opt-in: **the hook only exists while a
remap is configured**. At the default (`none`) there is no hook and zero overhead, and
setting both back to `none` (or quitting) removes it immediately. Everything else
(DPI, polling, lighting, DPI-button remap) is strictly per-device and never touches
the pointer's motion path.
</details>

<details>
<summary><b>…I plug in two supported mice at once?</b></summary>

This isn't really supported. Commands go to whichever unit Windows enumerates first,
and DPI-button presses from either mouse of the same model trigger actions. It's
harmless but unpredictable, so plug in one at a time.
</details>
