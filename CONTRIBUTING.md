# Contributing to Psylli

Two kinds of contributions are welcome:

1. **Improving DeathAdder Elite support** — bug fixes, settings-window polish, footprint
   wins, more remap actions.
2. **Teaching Psylli a new mouse** — the fun one, described below.

---

## Add your own mouse

> The Psylli charmed serpents they had never met by knowing how snakes *work*, not by
> memorizing one. Psylli-the-program is the same: it speaks one mouse today, but the
> protocol layer is small and the method is general. If you have a different mouse, you
> can probably teach it a new one — and this is a near-perfect task to hand an AI coding
> agent.

**The workflow:**

1. **Fork and clone** this repo.
2. **Hand your agent [`CRACKING-MICE-GUIDE.md`](CRACKING-MICE-GUIDE.md)** — the full,
   device-agnostic method: enumerate the mouse's HID collections, find the *reference
   implementation* for your brand (OpenRazer, libratbag, Solaar…), read the protocol out
   of it, confirm the control channel with a **read** before any write, then send + verify.
   Tell the agent: *"Follow this guide to add support for my `<mouse>` to Psylli."*
   The [`reference/`](reference/) Python toolkit is the runnable version of exactly this
   method — the fastest way to poke an unknown device and confirm its protocol before you
   write any Rust.
3. **Have it produce a protocol module + byte-exact tests** — mirroring `crates/razer-proto`:
   pure, no-I/O report building with unit tests that assert the exact bytes/CRC against the
   FOSS reference (see the `*_matches_openrazer` tests for the pattern).
4. **Wire it in** — device open/enumerate in `crates/razer-hid`, and selection in the
   daemon/CLI.
5. **Open a PR** describing the mouse (VID/PID), the reference you sourced the protocol
   from, and what you verified on real hardware.

### The golden rule (non-negotiable)

**Only send commands you have read out of an open-source reference implementation** (or
observed from the vendor's own app via a USB capture). **Never fuzz, brute-force, or guess
Feature reports** — a wrong write to a vendor channel is the one way you can actually wedge
a mouse. And:

- ❌ **No firmware / bootloader / DFU.** This method never needs it.
- ✅ **Read before you write** — confirm the channel non-destructively first.
- ✅ **Verify after every write**: the device still clicks/moves/enumerates, and
  **unplug/replug restores stock**.
- ✅ If a command's meaning isn't in the reference, **stop and report** — don't experiment
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
cargo build --release      # produces psylli.exe + psyctl.exe
cargo test --release       # unit tests (protocol/CRC/config/lighting)
cargo clippy --release --all-targets   # keep it warning-clean
```

Footprint is a feature, not an afterthought — the release exe is ~436 KB and idle RAM is
under 10 MB. New dependencies are weighed against that (`docs/SPEC.md` has the targets).

## Licensing

Psylli is **GPL-2.0-or-later** (see [`LICENSE`](LICENSE)). By contributing you agree your
contribution is licensed the same way.

Protocol code is typically **derived from a GPL project** (OpenRazer, libratbag). That's
fine and expected — but **credit your source in [`NOTICE`](NOTICE)**: name the project,
its license, and the specific files you ported from, the way the existing OpenRazer
attribution does.

## PR checklist

- [ ] `cargo test` and `cargo clippy` are clean.
- [ ] New protocol code has **byte-exact tests** against the reference implementation.
- [ ] You verified on **real hardware**, and unplug/replug restores stock behavior.
- [ ] `NOTICE` credits any project you sourced the protocol from.
- [ ] No firmware/DFU, no fuzzed/guessed commands.
