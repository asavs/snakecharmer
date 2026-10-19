# AGENTS.md

Brief for any coding agent working in this repo: Claude Code, Codex, opencode, Cursor,
Copilot, or a chat window someone is pasting into. Nothing here is model-specific, and the
main job people bring here does not need a frontier model.

## What this is

Snakecharmer is a Windows daemon that configures Razer mice over USB HID (DPI, polling
rate, RGB, button remapping) as a replacement for Razer Synapse. It is Rust, Windows-only,
and deliberately small: one static ~0.6 MB exe, about 17 MB idle (measured).

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
3. **STOP and report** if any of these happen. Do not retry with different bytes, and do
   not work around it:
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
crates/platform/      Win32 FFI: single-instance, keystroke injection, mouse hook, tray, settings UI
src/                  daemon, config, lighting, on-board binds, charmctl
reference/            runnable Python recon toolkit — poke a device with no compile cycle
tools/                dev tools: diagram point editor, wishlist generator, Razer audit/removal PowerShell scripts
```

`crates/razer-proto/src/devices/` holds one file per supported device, and
`SUPPORTED` in `devices/mod.rs` is the source of truth for what the daemon will open.
Unsafe code stays confined to `crates/platform`.

## The common task: add support for a mouse

This should stay a small diff: one new device file, its registration lines in `devices/mod.rs`
and the re-export in `lib.rs`, and a docs row. If you find yourself editing `razer-hid` or
the daemon, stop and say so in the PR. It means something generic is missing from the
shared layer, and that is a different change.

1. **Look up the device in [`docs/DEVICE-WISHLIST.md`](docs/DEVICE-WISHLIST.md).** It lists
   every Razer mouse OpenRazer already documents, with its USB id, transaction id, DPI
   ceiling, lighting zones and polling family. If it is there, **no packet capture is
   needed**; the reading is done. Verify each value against the OpenRazer source anyway;
   the table is a hint, and the driver has real asymmetries.
   - Register a second USB id in `ALIAS_PIDS` at the top of
     [`tools/wishlist-from-openrazer.py`](tools/wishlist-from-openrazer.py) if your model is
     *one device with two ids* — wired and through its own dongle — that OpenRazer puts in
     the same `case` arm of every command Snakecharmer sends. Otherwise it gets two rows and duplicating the
     spec file is the wrong move. If the two ids land in *different* arms (Viper V3 Pro),
     they are genuinely two rows and two specs.
2. **Copy the closest file in `crates/razer-proto/src/devices/`** and change the values.
   `deathadder_elite.rs` for a `0x3F` device with lighting; `deathadder_v3.rs` for a `0x1F`
   device without.
3. **Set `diagram: None`** unless you are also drawing one. It is an `Option` and `None`
   ships. The settings window falls back to labeled control rows and the device is fully
   functional. Drawing is a separate contribution; see
   [`docs/DRAWING-MICE-GUIDE.md`](docs/DRAWING-MICE-GUIDE.md).
4. **Register it** in `SUPPORTED`, add a per-device test, and add the row to
   [`docs/SUPPORTED-DEVICES.md`](docs/SUPPORTED-DEVICES.md). A test fails if the doc and the
   table disagree.
5. **Verify on hardware.** This part needs the human who owns the mouse. Ask them to run
   the daemon and confirm the log line, then `charmctl set-dpi <n>` and `charmctl status`
   round-trip, including a value near the device's maximum. Do not mark a device verified
   on hardware because the tests passed; that column means a person plugged it in.

## On-board memory

Snakecharmer writes on-board button binds on the DeathAdder V3 only; the format was
decoded from USB captures of Synapse, in [`docs/ONBOARD-KEYMAP.md`](docs/ONBOARD-KEYMAP.md).
For a device port, just record the mouse's on-board profile count in the PR.

- **Reads are allowed** where that doc records them, sent exactly as Synapse sent them:
  the same command, the same argument pattern, and only the button ids observed. Never
  sweep ids or arguments to see what answers; that's fuzzing, and rule 1 forbids it.
- **Writes are allowed on the DeathAdder V3**, approved by the maintainer on 2026-10-16,
  for the buttons listed in `KeymapSpec::writable` (right, wheel click, back, forward,
  wheel up, wheel down), and only through `Mouse::set_button_binding`, which replays the
  layout Synapse was captured sending. Hardware check:
  [`docs/captures/V3-20261016-snakecharmer-write/notes.md`](docs/captures/V3-20261016-snakecharmer-write/notes.md).
- **Everything else stays read-only.** Adding a mouse or a button to `writable` needs a
  capture of Synapse writing it, recorded in the doc, plus maintainer approval. Never
  rebind left or right click (right may only be reset to default). Never write the
  underside button or any bind category the doc lists as undecoded, and never sweep ids or arguments to see what a write does. Other mice's keymap write bytes
  are reference data, not commands.

## Acceptance

```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
```

These match CI; [`CONTRIBUTING.md`](CONTRIBUTING.md) has the same block.

A change is not done until those are green. Tests for protocol code assert **exact bytes**
against the OpenRazer reference. See the `*_matches_openrazer` tests for the pattern.

## Conventions

- Match the surrounding comment density and naming. Comments here explain *why* a byte or a
  layout decision is what it is, usually citing OpenRazer or physical hardware behavior.
- Weigh every new dependency against the size and idle-cost targets in
  [`docs/SPEC.md`](docs/SPEC.md).
- Protocol code derived from a GPL project must be credited in [`NOTICE`](NOTICE).
  That is a license obligation.
- This repo is GPL-2.0-or-later.

## Ask a human when

- A command's behavior is not documented in a reference implementation.
- A device does something the tests can't check: anything requiring a button press, a
  plug/unplug, or a look at a rendered window.
- The change would grow beyond the small diff described above.
