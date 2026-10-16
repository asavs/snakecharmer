# Contributing to Snakecharmer

Two kinds of contributions are welcome:

1. **Improving DeathAdder Elite support:** bug fixes, settings-window polish, smaller
   footprint, more remap actions.
2. **Teaching Snakecharmer a new mouse:** the fun one, described below.

---

## Charm your own snake... uh, mouse (or keyboard! and probably headset??)

> Snakecharmer supports a [handful of mice](docs/SUPPORTED-DEVICES.md) today, but the protocol layer is small and the
> method is general. If you've got a different device, you can probably teach it one, and
> it's a good job to hand an AI coding agent.

### The workflow

1. **Pick a mouse and claim it.** [`docs/DEVICE-WISHLIST.md`](docs/DEVICE-WISHLIST.md)
   lists every Razer mouse whose protocol is already in OpenRazer, with its USB id and
   transaction id, and who cracked the ones that shipped. Open an issue saying you're taking
   one so two people don't crack the same mouse the same weekend. Then fork or clone.
2. **Hand your agent [`CRACKING-MICE-GUIDE.md`](CRACKING-MICE-GUIDE.md).** It's the full
   method and works for any device: enumerate the mouse's HID collections, find the
   *reference implementation* for your brand (OpenRazer, libratbag, Solaar…), read the
   protocol out of it, confirm the control channel with a **read** before any write, then
   send and verify. Tell the agent: *"Follow this guide to add support for my `<mouse>` to
   Snakecharmer."* The [`reference/`](reference/) Python toolkit is the same method as
   runnable scripts. It's the fastest way to poke an unknown device and confirm its
   protocol before you write any Rust.
   Models known to have completed a crack end to end so far: Claude Fable 5 (DeathAdder
   Elite) and Claude Opus 4.8 high (DeathAdder V3). That gives you a sense of the
   capability level needed.
3. **Have it produce a protocol module and byte-exact tests,** following `crates/razer-proto`:
   pure, no-I/O report building with unit tests that assert the exact bytes/CRC against the
   FOSS reference (see the `*_matches_openrazer` tests for the pattern).
4. **Register it:** add the device file to `SUPPORTED` in `crates/razer-proto/src/devices/`
   and its row to [`docs/SUPPORTED-DEVICES.md`](docs/SUPPORTED-DEVICES.md). That's the whole
   port; `razer-hid` and the daemon pick it up from there. If they need changing, say so in
   the PR (see [`AGENTS.md`](AGENTS.md)).
5. **Open a PR** describing the mouse (VID/PID), the reference you sourced the protocol
   from, and what you verified on real hardware.

### The golden rule (non-negotiable)

**Only send commands you have read out of an open-source reference implementation** (or
observed from the vendor's own app in a USB capture). **Never fuzz, brute-force, or guess
Feature reports.** A wrong write to a vendor channel is the one way you can actually wedge
a mouse. Also:

- **No firmware, bootloader, or DFU.** This method never needs it.
- **Read before you write.** Confirm the channel non-destructively first.
- **Verify after every write:** the device still clicks, moves, and enumerates, and
  **unplugging and replugging restores stock behavior**.
- If a command's meaning isn't in the reference, **stop and report**. Don't experiment
  on someone's only mouse.

`CRACKING-MICE-GUIDE.md` §8 has the full safety checklist. It applies to every PR.

---

## Project layout

```
crates/
  razer-proto/   # pure protocol: report builder, CRC, commands. No I/O — unit-testable.
  razer-hid/     # device open/enumerate, feature reports, input-report listener
  platform/      # Win32 FFI: single-instance, keystroke injection, WH_MOUSE_LL hook
src/             # daemon, tray, native settings window, config, lighting
reference/       # runnable Python recon toolkit — the worked example to adapt for a new device
```

A new device is mostly **new protocol code + tests** in the proto layer, plus a small
amount of wiring. Keep unsafe code confined to `platform`.

## Dev setup

```powershell
cargo build --release      # produces snakecharmer.exe + charmctl.exe
cargo test --release       # unit tests (protocol/CRC/config/lighting)
cargo clippy --release --all-targets   # keep it warning-clean
```

Keep it small: the release exe is under 1 MB and idle RAM about 17 MB. Weigh new
dependencies against that (`docs/SPEC.md` has the targets).

## Licensing

Snakecharmer is **GPL-2.0-or-later** (see [`LICENSE`](LICENSE)). By contributing you agree your
contribution is licensed the same way.

Protocol code is typically **derived from a GPL project** (OpenRazer, libratbag). That's
fine and expected, but **credit your source in [`NOTICE`](NOTICE)**: name the project,
its license, and the specific files you ported from, the way the existing OpenRazer
attribution does.

## PR checklist

- [ ] `cargo test` and `cargo clippy` are clean.
- [ ] New protocol code has **byte-exact tests** against the reference implementation.
- [ ] You verified on **real hardware**, and unplug/replug restores stock behavior.
- [ ] `NOTICE` credits any project you sourced the protocol from.
- [ ] No firmware/DFU, no fuzzed/guessed commands.
