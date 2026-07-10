# Psylli — Design Spec

A native Windows replacement for Razer Synapse, scoped to the **Razer DeathAdder
Elite** (USB `VID 0x1532 / PID 0x005C`). Everything Synapse did that's actually
useful — button remapping, DPI, lighting — with none of the five-process, Chromium-
in-the-background tax, answering only to the user.

> Born from the session where we removed Synapse (its `CefSharp` browser was the
> single biggest CPU hog on the machine) and then reverse-engineered the mouse's HID
> protocol to keep the one feature worth keeping. This is the finished version of that.

## Footprint targets (the whole point)
Mirrors the [pc-vitals](../../pc-vitals/) ethos — an anti-bloat tool that hogs
resources refutes itself.

- Static exe **≤ 2 MB** (`crt-static`, no runtime deps)
- Steady-state RAM **< 10 MB**
- **Negligible CPU** while idle — blocking HID reads, not poll loops
- Zero background browser, zero telemetry, local-only

## Capabilities (v1 — all four, per user decision)
1. **DPI-button remap** — the two buttons behind the wheel emit private Razer vendor
   codes (`0x20`/`0x21`) invisible to the OS; we catch them at the HID layer and inject
   keystrokes.
2. **DPI level** — set/lock sensitivity to any value, applied at login.
3. **RGB lighting** — static / breathing / spectrum / off, for the mouse's two lit
   zones (scroll wheel + logo).
4. **Thumb-button remap** — the two side buttons are *standard* mouse buttons the OS
   sees as Back/Forward; remapping them to keystrokes needs a low-level input hook
   (`WH_MOUSE_LL`) that can suppress the original. This is the one genuinely harder
   subsystem (it's what XMBC does).

## UI
- **Tray icon** (`Shell_NotifyIcon`) with a right-click menu (quick DPI, lighting,
  reload, quit) and a **settings window**: DPI slider, color picker + effect dropdown,
  per-button action dropdowns, Apply / Save.
- Config persisted as TOML in `%LOCALAPPDATA%\Psylli\config.toml`.
- Runs at login via a Startup-folder shortcut (no admin), windowless until opened.

## Architecture (Rust, Win32-direct)

```
psylli/
├─ crates/
│  ├─ razer-proto/      # pure protocol: report builder, CRC, mode/DPI/RGB commands
│  │                    #   pure protocol from OpenRazer — no I/O, unit-testable
│  ├─ razer-hid/        # device open/enumerate, feature reports, input-report listener
│  │                    #   (hidapi crate, or windows-rs HID); the CefSharp-free core
│  └─ input-hook/       # WH_MOUSE_LL hook for thumb-button remap + suppression
└─ src/                 # the app: tray, settings window (egui/eframe or native),
                        #   config load/save, wiring, login autostart
```

Candidate deps (keep the tree lean for the ≤2 MB target — audit each):
- HID: `hidapi` **or** raw `windows` crate HID APIs
- Tray + window: evaluate `tray-icon` + `egui/eframe` vs. native `windows` crate
  (`Shell_NotifyIcon` + a dialog). eframe is heavier but far faster to build; a native
  Win32 window is the true-featherweight path. **Decide at Phase 3.**
- Hook: `windows` crate `SetWindowsHookEx(WH_MOUSE_LL)`
- Config: `serde` + `toml`

## Protocol reference (reverse-engineered, from OpenRazer + this machine)
90-byte feature report (report ID 0 → 91-byte buffer on Windows with a leading `0x00`),
sent to the **interface-0 mouse HID collection**. CRC = XOR of bytes `2..87`.
DeathAdder Elite uses **transaction_id `0x3F`**.

```
                 st  txn rem   proto sz  cls id  args      ...zeros... crc rsvd
Driver mode:     00  3F  00 00 00    02  00  04  03 00     (78x00)     05  00
Hardware mode:   00  3F  00 00 00    02  00  04  00 00     (78x00)     06  00
Get mode (read): 00  3F  00 00 00    02  00  84  00 00     (78x00)     86  00
```
- **DPI set/get:** command_class `0x04`, ids `0x05`/`0x85` (see OpenRazer `razermouse_driver.c`).
- **DPI buttons in driver mode:** 16-byte input report, ID `0x04`, on the keyboard-
  protocol interface, vendor codes `0x20` (up) / `0x21` (down).
- **RGB (Chroma):** command_class `0x03` matrix effects — **to be confirmed** from
  OpenRazer `razerchromacommon.c` for this device (static/breathing/spectrum/none).
- Full protocol reference: [OpenRazer](https://github.com/openrazer/openrazer) (`driver/razercommon.*`, `razerchromacommon.c`).

## Safety
- Only documented Razer commands (OpenRazer-sourced). No firmware/bootloader/DFU. No
  fuzzing feature reports — a bad write can wedge the user's **only** mouse.
- Never leave the mouse unable to left/right-click. Unplug/replug always restores
  factory behavior.

## Phasing (multi-session)
- **P1 — Foundation:** toolchain, workspace, `razer-proto` ported + unit-tested;
  `razer-hid` opens the device and does set-mode / set-DPI / get-mode from Rust.
  *Milestone: Rust talks to the mouse.*
- **P2 — Daemon core:** input-report listener + keystroke injection for the DPI buttons,
  running headless.
- **P3 — RGB + tray:** Chroma commands; tray icon + menu.
- **P4 — Settings window:** DPI slider, color picker, button dropdowns, config persistence.
- **P5 — Thumb-button remap:** `WH_MOUSE_LL` hook + suppression; login autostart; polish
  to the footprint gate.

## Milestones
| # | Deliverable |
|---|---|
| M1 | Rust sets driver mode + DPI on the real mouse (P1) |
| M2 | Headless daemon: DPI buttons remap + injection, running headless (P2) |
| M3 | Tray app sets RGB + DPI (P3–P4) |
| M4 | Full app incl. thumb remap, within the ≤2 MB / <10 MB gate (P5) |
