# AGENTS.md

Brief for any coding agent working in this repo — Claude Code, Codex, opencode, Cursor,
Copilot, or a chat window someone is pasting into. Nothing here is model-specific, and the
main job people bring here does not need a frontier model.

## What this is

Snakecharmer is a Windows daemon that configures Razer mice over USB HID — DPI, polling
rate, RGB, button remapping — as a replacement for Razer Synapse. It is Rust, Windows-only,
and deliberately small: one static ~600 KB exe, under 10 MB idle.

All protocol knowledge comes from [OpenRazer](https://github.com/openrazer/openrazer). This
project ports slices of it to Windows; it does not invent protocol.

## The rules that matter (read before writing any device code)

These are safety rules, not style preferences. A wrong write to a mouse's vendor channel is
the one way to actually break someone's hardware.

1. **Only send commands you have read out of OpenRazer** (`driver/razermouse_driver.c`,
   `driver/razerchromacommon.c`, `driver/razercommon.*`) or observed in a USB capture of the
   vendor's own software. Never fuzz, guess, brute-force, or "try" a byte to see what it
   does.
2. **Read before you write.** Confirm a channel with the read form of a command first.
3. **STOP and report** — do not retry with different bytes, do not work around it — if:
   - a response's status byte is not `0x02`,
   - a read-back does not match what you set,
   - a command's meaning is not in the reference,
   - or the device stops enumerating, clicking, or moving.
4. **Never touch firmware, bootloader, or DFU.** No task here needs it. Refuse tasks that ask.
5. You are usually working on someone's only mouse. Everything above assumes that.

## Layout

```
crates/razer-proto/   pure protocol: report building, CRC, commands, DeviceSpec. No I/O.
crates/razer-hid/     device open/enumerate, feature reports, input-report listener
crates/platform/      Win32 FFI: single-instance, keystroke injection, mouse hook, settings UI
src/                  daemon, tray, config, lighting
reference/            runnable Python recon toolkit — poke a device with no compile cycle
tools/                dev tools: diagram point editor, wishlist generator
```

`crates/razer-proto/src/devices/` holds one file per supported device, and
`SUPPORTED` in `devices/mod.rs` is the source of truth for what the daemon will open.
Unsafe code stays confined to `crates/platform`.

## The common task: add support for a mouse

This is a **one-file diff** and should stay one. If you find yourself editing `razer-hid` or
the daemon, stop and say so in the PR — it means something generic is missing from the
shared layer, and that is a different change.

1. **Look up the device in [`docs/DEVICE-WISHLIST.md`](docs/DEVICE-WISHLIST.md).** It lists
   every Razer mouse OpenRazer already documents, with its USB id, transaction id, DPI
   ceiling, lighting zones and polling family. If it is there, **no packet capture is
   needed** — the reading is done. Verify each value against the OpenRazer source anyway;
   the table is a hint, and the driver has real asymmetries.
2. **Copy the closest file in `crates/razer-proto/src/devices/`** and change the values.
   `deathadder_elite.rs` for a `0x3F` device with lighting; `deathadder_v3.rs` for a `0x1F`
   device without.
3. **Set `diagram: None`** unless you are also drawing one. It is an `Option` and `None`
   ships — the settings window falls back to labeled control rows and the device is fully
   functional. Drawing is a separate contribution; see
   [`docs/DRAWING-MICE-GUIDE.md`](docs/DRAWING-MICE-GUIDE.md).
4. **Register it** in `SUPPORTED`, add a per-device test, and add the row to
   [`docs/SUPPORTED-DEVICES.md`](docs/SUPPORTED-DEVICES.md) — a test fails if the doc and the
   table disagree.
5. **Verify on hardware** — this part needs the human who owns the mouse. Ask them to run
   the daemon and confirm the log line, then `charmctl set-dpi <n>` and `charmctl status`
   round-trip, including a value near the device's maximum. Do not mark a device verified
   on hardware because the tests passed; that column means a person plugged it in.

## Acceptance

```
cargo test --workspace
cargo clippy --release --all-targets     # keep it warning-clean
cargo build --release
```

A change is not done until those are green. Tests for protocol code assert **exact bytes**
against the OpenRazer reference — see the `*_matches_openrazer` tests for the pattern.

## Conventions

- Match the surrounding comment density and naming. Comments here explain *why* a byte or a
  layout decision is what it is, usually citing OpenRazer or physical hardware behavior.
- Weigh every new dependency against the size and idle-cost targets in
  [`docs/SPEC.md`](docs/SPEC.md).
- Protocol code derived from a GPL project must be credited in [`NOTICE`](NOTICE) —
  that is a license obligation.
- This repo is GPL-2.0-or-later.

## Ask a human when

- A command's behavior is not documented in a reference implementation.
- A device does something the tests can't check — anything requiring a button press, a
  plug/unplug, or a look at a rendered window.
- The change would grow beyond one device file.
