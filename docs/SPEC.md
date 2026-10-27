# Snakecharmer design

Snakecharmer is a Windows daemon that configures supported Razer mice over USB HID: DPI,
polling rate, lighting, button remaps, and on-board button binds. It replaces Razer Synapse
for those settings. This document describes how it works now. For usage, see the
[README](../README.md). For the rules any change to device code must follow, see
[AGENTS.md](../AGENTS.md).

## Goals

- Cost nothing while idle. One static exe, blocking reads instead of poll loops, no
  background browser. A tool that replaces bloat can't be bloated itself.
- Essential settings only: DPI, polling rate, button binds, and lighting as a setting.
  Apply it, then go quiet.
- Don't break the mouse. Send only commands documented in
  [OpenRazer](https://github.com/openrazer/openrazer) or seen in a USB capture of Razer's
  own software. Never touch firmware, the bootloader, or DFU.
- Adding a mouse is mostly one new file. Everything device-specific lives in one
  `DeviceSpec`; registering it takes a few lines in `devices/mod.rs` and a docs row.
- Local only. No telemetry, accounts, or network access.

## Non-goals

Animated or game-synced lighting, macros, per-application profiles, DPI stages, cloud
profiles, and accounts. Each of these needs something running all the time, which is the
cost this project exists to avoid. The README's
[alternatives table](../README.md#one-target-and-its-razer) and
[ALTERNATIVES.md](ALTERNATIVES.md) list tools that do them.

## Architecture

A Cargo workspace with one app package and four library crates:

| Crate | Contents | I/O |
|---|---|---|
| `crates/razer-proto` | 90-byte report builder, CRC, response validation; device mode, DPI, polling and lighting commands; `DeviceSpec` and `SUPPORTED` (`devices/`); the on-board keymap protocol (`keymap.rs`); references to shared device diagrams | none (`unsafe_code = "forbid"`) |
| `crates/razer-hid` | `Mouse` (open a supported device, send a feature report, typed wrappers), `Listener` for input reports, `detected_mice` for naming unsupported devices. Uses `hidapi`. | HID |
| `crates/device-diagram` | Immutable diagram schema, bounds, and SVG emitter shared by device specs and the native renderer | none (`unsafe_code = "forbid"`) |
| `crates/platform` | All Win32 FFI and all `unsafe`: keystroke injection (`SendInput`), single-instance mutex, message boxes, tray icon, `WH_MOUSE_LL` hook, settings window, GDI+ diagram renderer, runtime-drawn icon, Run-key autostart, atomic file replace. Uses `windows-sys`. | Win32 |
| `snakecharmer` (`src/`) | `daemon.rs` (session lifecycle and settings wiring), `daemon/menu.rs` (tray menu and command application), `daemon/config_application.rs` (validated config application and persistence), `config.rs`, `actions.rs` (action strings to key chords), `lighting.rs`, `onboard.rs`, `health.rs`, `logger.rs` | files |

Binaries:

| Binary | Purpose |
|---|---|
| `snakecharmer.exe` (`src/main.rs`) | The daemon. Built for the Windows subsystem, so it has no console. Exits at once if another instance holds the `Local\SnakecharmerDaemon` mutex. |
| `charmctl.exe` (`src/bin/charmctl.rs`) | Console CLI: `devices`, `status`, `keymap [reset\|set]`, `set-dpi`, `set-poll`, `set-mode`, `set-color`, `set-effect`, `self-test`, `where`. It opens the device itself and does not talk to the daemon. |
| `diagram-editor.exe` (`src/bin/diagram-editor.rs`) | Dev-only local server for dragging diagram points in a browser and writing them back to the device files. |

### Threads

| Thread | Lifetime | Job |
|---|---|---|
| Main | process | Session loop: open the mouse, apply settings, then wait on an event channel (button codes, menu and settings commands) with a timeout for the periodic re-assert. |
| Tray | process | `Shell_NotifyIcon` message pump. It also owns the mouse hook, because Windows drops a `WH_MOUSE_LL` hook whose thread doesn't pump messages. |
| Reader (one per readable HID collection) | session, only on mice with DPI buttons | Blocking read of vendor input reports; sends new codes to the main thread. |
| Settings window | while open | Runs the window's own message loop and sends events to the main thread. |
| Notice | while a dialog is up | Short-lived thread per message box, so a dialog left open never stalls the daemon. |

### Session lifecycle

1. Enumerate HID and open the first device whose product id is in `SUPPORTED`, on its
   interface-0 mouse collection (usage page `0x01`, usage `0x02`). That is the only
   collection that accepts the vendor feature reports. Other devices, Razer or not, are
   never opened.
2. Set DPI, set the polling rate if one is configured, and apply lighting unless it is
   `keep`.
3. On a mouse with a recorded keymap, move software thumb remaps into `[onboard]`, write
   the configured on-board binds, read back all binds, and offer to reset leftovers.
4. On a mouse with DPI buttons, switch to driver mode and start the reader threads.
5. Wait for events. Every `reassert_interval_secs` (default 60, minimum 5), read DPI, the
   polling rate, and on DPI-button mice the device mode, and rewrite any that drifted.
   Lighting is not re-asserted. A mouse without DPI buttons has no blocking read to notice
   an unplug, so the daemon reads its DPI every 5 seconds as a liveness check.
   Maintenance deadlines run after events as well as timeouts, so button presses and
   settings activity cannot postpone them. Reloading config updates the reassert interval
   within the current session.
6. A failed device command, read-back mismatch, or closed input collection stops device
   communication. The daemon logs and displays the failure, removes the mouse hook, and
   exits when the notice is dismissed, without recovery writes. If no supported device
   is present at startup, enumeration retries after 3 s, backing off to 30 s; tray clicks
   are still handled during that wait.

Two supported mice plugged in at once is not supported. Commands go to whichever one
enumerates first.

## Device model

Each supported mouse is one file in `crates/razer-proto/src/devices/` that defines a
`DeviceSpec`:

| Field | Meaning |
|---|---|
| `product_id` | USB PID (the vendor is always `0x1532`) |
| `transaction_id` | Byte 1 of every report; varies by model generation |
| `name` | Shown in logs, the tray, and the settings title |
| `rgb_zones` | Lighting zone ids, in apply order; empty means no lighting |
| `dpi_buttons` | Vendor codes of the wheel DPI buttons, or `None` |
| `dpi_min`, `dpi_max` | Accepted DPI range, also used for the slider |
| `polling` | Command family (`Classic` or `Extended`) and the accepted rates |
| `diagram` | Top-down schematic, or `None` |

`SUPPORTED` in `devices/mod.rs` is the source of truth for what the daemon opens:

| Device | PID | Txn | Lighting | DPI buttons | DPI max | Polling (Hz) | On-board binds |
|---|---|---|---|---|---|---|---|
| DeathAdder Elite | `0x005C` | `0x3F` | wheel, logo | `0x20` / `0x21` | 16000 | 125, 500, 1000 | no |
| DeathAdder V3 | `0x00B2` | `0x1F` | none | none | 30000 | 125, 500, 1000, 2000, 4000, 8000 | yes |

The daemon skips features a device lacks instead of treating them as errors. Without
lighting, lighting calls do nothing; without DPI buttons, there is no driver mode and no
reader thread.

To add a mouse, copy the closest device file, change the values, list it in `SUPPORTED`,
add a test, and add a row to [SUPPORTED-DEVICES.md](SUPPORTED-DEVICES.md) (a test checks
that the two agree). [DEVICE-WISHLIST.md](DEVICE-WISHLIST.md) has the OpenRazer values for
most Razer mice. `diagram: None` ships: the settings window falls back to labeled rows, and
drawing is a separate job ([DRAWING-MICE-GUIDE.md](DRAWING-MICE-GUIDE.md)). On-board keymaps
live in their own table (`keymap::SUPPORTED`) so a device port stays one file. The full
procedure is in [AGENTS.md](../AGENTS.md#the-common-task-add-support-for-a-mouse). Working
out an unknown protocol is covered in [CRACKING-MICE-GUIDE.md](CRACKING-MICE-GUIDE.md).

## Protocol

Every command is a 90-byte feature report (report ID 0, sent as a 91-byte buffer with a
leading `0x00`), and every response is 90 bytes. The layout is documented at the top of
`crates/razer-proto/src/lib.rs`: status, transaction id, data size, command class and id,
80 argument bytes, and a CRC that is the XOR of bytes 2 to 87. The shared HID transport
sends each report once and requires status `0x02` and a matching command echo. BUSY is
terminal too. A process-wide latch blocks every subsequent command after a transport,
response, or verification failure, including commands on another handle or the reset
notice thread. A mutex serializes complete read/write/read-back operations. Invalid
local values are rejected before I/O and do not trip the latch.

A global named-object lease permits only one Snakecharmer process to hold
control handles, across Windows login sessions. It is acquired after read-only
enumeration and before opening the endpoint, shared by handles in the same
process, and released after the last control handle closes or the process exits.
The CLI fails clearly while the daemon owns the mouse; quit the daemon before
standalone device commands. Enumeration needs no lease. This coordinates
participating Snakecharmer binaries only; other vendor or reference tools must
be closed separately. Ownership contention and OS acquisition failures send no
device commands and do not trip the device-failure latch.

Mode, DPI, polling-rate, and on-board bind setters read their corresponding command
before writing and verify it afterward. Lighting has no effect-read command; it confirms
the control channel with the existing DPI read and checks the effect acknowledgement.
No color-read command is invented. Tests assert encoded reports and exercise failures
through a simulated feature-report transport. The reference comparison has the
exceptions below; passing local byte tests does not resolve them.

### Reference snapshot

The 2026-10-26 source review uses
[OpenRazer `477f4d6d3ab7e9deb6310d5bd3742560c5cbe80d`](https://github.com/openrazer/openrazer/tree/477f4d6d3ab7e9deb6310d5bd3742560c5cbe80d).
[`reference/openrazer-source.json`](../reference/openrazer-source.json) records exact Git
blob IDs for the six reviewed files. This is a later review, not the unrecorded original
port revision, and it adds no hardware verification.

| Checked facts | Immutable source |
|---|---|
| 90-byte report layout, initialized header, XOR bytes 2 through 87 | [`razercommon.h`](https://github.com/openrazer/openrazer/blob/477f4d6d3ab7e9deb6310d5bd3742560c5cbe80d/driver/razercommon.h#L124), [`razercommon.c`](https://github.com/openrazer/openrazer/blob/477f4d6d3ab7e9deb6310d5bd3742560c5cbe80d/driver/razercommon.c#L111) |
| Mode read/write classes, ids, size and allowed modes | [`razerchromacommon.c`](https://github.com/openrazer/openrazer/blob/477f4d6d3ab7e9deb6310d5bd3742560c5cbe80d/driver/razerchromacommon.c#L24) |
| DPI read layout; polling command families/rate bytes | [`razerchromacommon.c`](https://github.com/openrazer/openrazer/blob/477f4d6d3ab7e9deb6310d5bd3742560c5cbe80d/driver/razerchromacommon.c#L1092) |
| Extended lighting none, static, spectrum, single-color breathing layouts | [`razerchromacommon.c`](https://github.com/openrazer/openrazer/blob/477f4d6d3ab7e9deb6310d5bd3742560c5cbe80d/driver/razerchromacommon.c#L477) |
| Elite transaction `0x3F`/classic polling; V3 and both V4 Pro ids `0x1F`/extended polling | [`razermouse_driver.c`](https://github.com/openrazer/openrazer/blob/477f4d6d3ab7e9deb6310d5bd3742560c5cbe80d/driver/razermouse_driver.c#L2129) |
| USB ids, DPI ceilings 16000/30000/45000, Elite lighting, V3/V4 Pro polling rates | [`razermouse_driver.h`](https://github.com/openrazer/openrazer/blob/477f4d6d3ab7e9deb6310d5bd3742560c5cbe80d/driver/razermouse_driver.h), [`mouse.py`](https://github.com/openrazer/openrazer/blob/477f4d6d3ab7e9deb6310d5bd3742560c5cbe80d/daemon/openrazer_daemon/hardware/mouse.py) |

**Unresolved DPI-write difference:** Snakecharmer sends argument 0 as `0x00`, named
`NOSTORE` by OpenRazer. The pinned mouse driver chooses `NOSTORE` for the supported mice,
but its [`razer_chroma_misc_set_dpi_xy` helper](https://github.com/openrazer/openrazer/blob/477f4d6d3ab7e9deb6310d5bd3742560c5cbe80d/driver/razerchromacommon.c#L1211)
ignores that parameter and unconditionally writes `VARSTORE` (`0x01`). Therefore the
current DPI write is not an exact match to that helper's output, and the source review
does not establish its persistence semantics. No selector, CRC, or other packet byte
was changed to work around this mismatch. It requires separate reference/capture
investigation under AGENTS.md before claiming equivalence or changing device code.

**Recorded polling difference:** the pinned driver sends extended polling once with
selector `0x00` and again with `0x01` for the V3 and both V4 Pro ids
([source](https://github.com/openrazer/openrazer/blob/477f4d6d3ab7e9deb6310d5bd3742560c5cbe80d/driver/razermouse_driver.c#L2282)).
Snakecharmer retains only the `0x00` request; live read-back is not evidence of persistence.
On-board keymaps and the Elite's DPI-button event codes remain separately recorded
hardware/capture evidence, not facts supplied by this snapshot.

The HID regressions distinguish local transport/verification-helper tests from `Mouse`
boundary tests. The latter call the typed methods and production process-wide latch with
private simulated devices, in isolated test subprocesses. Literal request fixtures pin
the existing packets; fabricated success replies do not establish firmware acceptance.
Enumeration, the Windows HID API, and physical mouse behavior still need hardware checks.

## Features

### Device mode and DPI buttons

The two DPI buttons behind the Elite's wheel send private vendor codes that Windows never
sees. In driver mode (`0x03`) they arrive as input reports starting with `0x04` on the
mouse's auxiliary HID collections. The daemon probes those collections, keeps the readable
ones, and runs a blocking reader on each. A new code is debounced (200 ms), mapped through
`dpi_up` or `dpi_down` to a key chord, and sent with `SendInput`. The defaults are copy and
paste.

### Thumb remaps and the mouse hook

Back and Forward are ordinary mouse buttons, so a software remap needs a global
`WH_MOUSE_LL` hook that suppresses the original. The hook is installed only while a remap
is configured. With `thumb_back` and `thumb_forward` both `none` (the default) there is no
hook, and mouse events never pass through Snakecharmer. When the hook is installed, it
passes through everything except a remapped thumb button, ignores injected events, and
uses `try_lock` and passes the event through without waiting for that mutex when it is
taken. Chord allocation and `SendInput` still run synchronously, so callback latency is
not bounded. Windows can silently remove a hook that exceeds its configured
[`LowLevelHooksTimeout`](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelmouseproc).
The hook applies to every mouse on the PC, not only the Razer one.

On a mouse that stores binds (the V3), Back and Forward are bound on-board instead, and an
existing software remap moves into `[onboard]` on first connect.

### Lighting

Static, breathing, spectrum, or off, per zone, using OpenRazer's extended matrix effect
command (class `0x0F`, id `0x02`). Each zone takes its effect and color from
`[zones.<name>]` and falls back to the device-wide `lighting` and `color`. `keep` leaves a
zone untouched. The tray menu sets all zones at once; the settings window sets each zone
separately. The protocol has no color read-back, so success means the device accepted the
command.

### Polling rate

OpenRazer has two command families. `Classic` uses class `0x00`, ids `0x05` / `0x85`, and
tops out at 1000 Hz. `Extended` uses ids `0x40` / `0xC0` and reaches 8000 Hz.
`PollingSpec.rates` lists the rates a device accepts, and the daemon sends no others. If
`polling_rate` is unset, the hardware value is left alone.

### On-board keymap

Some mice store button binds in their own memory and send them with no software running.
OpenRazer has no remapping, so this protocol was decoded from USB captures of Synapse 4 on
the DeathAdder V3. [ONBOARD-KEYMAP.md](ONBOARD-KEYMAP.md) has the evidence.

- Reads (class `0x02`, id `0x8C`) go only to the button ids in `KeymapSpec.buttons`, the
  ones Synapse was seen querying.
- Writes (class `0x02`, id `0x0C`) go only to buttons in `KeymapSpec.writable`. On the V3
  those are right, wheel click, back, forward, wheel up, and wheel down. Left click and the
  underside button have no captured write and are never written.
- The main clicks are never rebound. `write_binding_report` refuses any bind on left or
  right click other than its default, so right click can only be reset, which undoes a
  remap Synapse left behind.
- `Mouse::set_button_binding` writes, waits 50 ms, reads the bind back, and returns an
  error if it differs.
- On attach, the daemon writes only the `[onboard]` entries that differ from what the mouse
  stores. It then flags non-default binds the config doesn't cover and offers to reset
  them, once per distinct set per run.
- A bind can be the default, disabled, a mouse function seen in a capture, or a key or
  chord from the named key list.

### Config

TOML at `%LOCALAPPDATA%\Snakecharmer\config.toml`, created with defaults on first run. If
the file cannot be read or parsed, the daemon reports the error and exits before
registering autostart or applying settings; the original file is left intact.
Reloading an invalid file keeps the active settings. Config saves flush a temporary
file and replace the destination atomically. The log is
`daemon.log` in the same folder. Keys: `dpi`, `dpi_y`, `dpi_up`, `dpi_down`, `thumb_back`,
`thumb_forward`, `lighting`, `color`, `polling_rate`, `reassert_interval_secs`,
`autostart_bootstrapped`, `[zones.*]`, `[onboard]`. Defaults and examples are in the
[README](../README.md#configuration).

After a supported mouse is opened, startup validates every setting that model will
apply before sending any device command. A semantic error (such as an unsupported
polling rate) reports the configuration error and exits without applying earlier
settings. Reload normalizes thumb remaps and validates the complete candidate before
device application; invalid candidates leave the healthy session and active config
intact. Unsupported lighting, DPI-button actions and on-board tables remain ignored.
Valid candidates replace the active config only after their device application succeeds;
a device failure still stops communication rather than rolling back or retrying writes.

### Tray and settings window

The tray menu has DPI presets within the device's range, lighting presets, Settings, a
Request start with Windows check, Reload config, and Quit. A left-click on the icon opens
settings.

The settings window is built from plain Win32 common controls, with no GUI framework. It
has a DPI slider, a polling dropdown, lighting controls per zone, and the device schematic
drawn with GDI+, with each button's control placed on the schematic at its callout. The
window doesn't create controls for features the device lacks. Every change goes to the
mouse and the config file right away. After writing an on-board bind, the window asks you
to press the button and uses raw input to confirm what the mouse actually sent.
The app supplies the opened device's USB vendor and product ids to that test;
another product from the same vendor cannot satisfy it. The platform reader has
no built-in vendor selection, and settings-only previews do not register raw input.

### Start with Windows

On first run the daemon adds a `Snakecharmer` value under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, which needs no admin rights.
`autostart_bootstrapped` records that this happened so a removed registration stays
removed. The tray's **Request start with Windows** check represents this registration,
not Windows' effective startup setting. Read failures are `Unknown`, displayed as
**Startup request unavailable**; neither a toggle nor bootstrap writes an unreadable
registration. Mutation read-back errors are reported instead of being treated as off.

Windows startup settings and Task Manager can independently disable a registered app.
This unpackaged Run-key backend does not query or override that setting, so a checked
request does not establish that the app will start. Microsoft's documented
[`StartupTask.State`](https://learn.microsoft.com/en-us/uwp/api/windows.applicationmodel.startuptask)
applies to startup tasks declared in an app package; no StartupApproved binary layout is
guessed here. See the [README](../README.md#start-with-windows) for the user-facing behavior.

### PC Vitals health capsule

The `pc-vitals` Cargo feature enables this integration and its SHA-256 dependency.
It is disabled by default, including in release artifacts. The disabled reporter is
zero-sized: lifecycle calls do nothing and its lease deadline is `Duration::MAX`, so
default builds perform no health file I/O or lease time checks. CI tests and builds both modes.

PC Vitals is a separate local health monitor that reads status files from provider apps.
With the feature enabled, Snakecharmer is one provider: it writes a JSON capsule to
`%LOCALAPPDATA%\PCVitals\providers\snakecharmer.json`. The file is written only on events
(start, connect, session failure, polling change, quit). A lease refresh every 45 minutes
runs inside the session loop's existing wait, so it adds no thread and no device I/O. The
capsule holds bounded counters, typed incident codes, the retry delay, the polling rate,
and a salted device id. It holds no paths, usernames, logs, or serials, and it gives PC
Vitals no authority to set severity, recommend a restart, or run an action. Writes are
atomic and capped at 32 KiB.

## Safety

The full rules are in
[AGENTS.md](../AGENTS.md#the-rules-that-matter-read-before-writing-any-device-code). In
short:

- Send only commands read from OpenRazer or seen in a capture of Razer's software. Never
  fuzz, guess, or sweep ids.
- Read before writing. Stop on a status other than `0x02`, a read-back mismatch, or a
  device that stops responding.
- Never touch firmware, the bootloader, or DFU.
- Never rebind left or right click.
- On-board binds persist on the mouse until changed or reset. Lighting effects use
  OpenRazer's variable-storage selector. DPI writes retain selector `0x00`; the pinned
  helper differs, so this document does not claim verified DPI persistence semantics.

## Resource budget

Measured figures, from the [README](../README.md#why):

| | Measured |
|---|---|
| Exe size | 0.74 MB (738,304 bytes), default x64 MSVC release with static CRT (`opt-level = "z"`, LTO, `panic = "abort"`, stripped) |
| Idle RAM | 17 MB working set, 2.5 MB private |
| Idle CPU | about 0.06 CPU-hours per day |
| Processes | 1 |
| Kernel drivers | 0 |

MSVC builds enable `crt-static` in [`.cargo/config.toml`](../.cargo/config.toml).
CI and release builds inspect both shipped executables with
[`check-static-crt.py`](../tools/check-static-crt.py), rejecting ordinary or
delay-loaded imports of the dynamic Microsoft CRT. Windows system DLLs are
still required. Executable size was measured on 2026-10-26; RAM and CPU remain
the earlier runtime measurements.

Enabling `pc-vitals` produces an 828,416-byte daemon in the same build, adding
90,112 bytes. RAM and CPU have not been remeasured for the feature split.

Weigh every new dependency against these numbers. The app uses raw `windows-sys` instead
of a GUI framework, draws with the GDI+ that ships in Windows, and has no async runtime.
