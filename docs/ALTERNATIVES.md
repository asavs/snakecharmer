# Alternatives — and who's better served by them

Snakecharmer runs a short list of [supported Razer mice](SUPPORTED-DEVICES.md) on Windows
with essentially zero overhead. It is deliberately *not* a full Synapse replacement, and
some people are genuinely better served by other tools. This page maps the landscape
honestly so you can pick the right one — including when that isn't Snakecharmer.

The only figures we've *measured* are our own (see the README table). Third-party numbers
are reported as best we can find from each project's own docs, forums, or issue tracker,
and linked where possible — if one's off, send a source and we'll fix it.

## The comparison

**Key:** ✅ full · 🟡 basic / partial · — not offered.
**Resident** = does it keep running in the background, and at what idle cost.

| Tool | OS | Resident (idle) | RGB | DPI / poll | Rebind std buttons | Rebind DPI buttons (vendor `0x20`/`0x21`) | Per-app profiles | Macros |
|---|---|---|---|---|---|---|---|---|
| [Razer Synapse 4](https://www.razer.com/synapse-4) | 🪟 | yes, heavy (500 MB+) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| [Razer Synapse Web (beta)](https://mysupport.razer.com/app/answers/detail/a_id/20619) | 🌐 | no (browser tab) | 🟡 | 🟡 | 🟡 | — | 🟡 | 🟡 |
| [SignalRGB](https://signalrgb.com/) | 🪟 | yes, heavy (10–30% CPU) | ✅ sync | — | — | — | 🟡 | — |
| [OpenRGB](https://openrgb.org/) | 🪟🐧🍎 | optional → 0% | 🟡 | — | — | — | — | — |
| [OpenRazer](https://openrazer.github.io/) + [Polychromatic](https://polychromatic.app/) / [RazerGenie](https://github.com/z3ntu/RazerGenie) / [Snake](https://github.com/bithatch/snake) | 🐧 | yes, light | ✅ | ✅ | ✅ | ✅ | 🟡 | ✅ |
| [libratbag](https://github.com/libratbag/libratbag) + [Piper](https://github.com/libratbag/piper) | 🐧 | yes, light | 🟡 | ✅ | ✅ | 🟡 *(onboard profiles)* | 🟡 | — |
| [razer-macos](https://github.com/1kc/razer-macos) | 🍎 | yes, light | ✅ | 🟡 | — | — | — | — |
| [usemice](https://github.com/robbieplata/usemice) | 🌐 | no (browser tab) | 🟡 | 🟡 | — | — | — | — |
| [open-razerkit](https://github.com/HamzaYslmn/open-razerkit) | 🪟🐧 ⌨️ | no (exits) | 🟡 | 🟡 | — | — | — | — |
| [dawctl](https://github.com/marcospb19/dawctl) | 🪟🐧 ⌨️ | no (exits) | 🟡 | ✅ | — | — | — | — |
| [X-Mouse Button Control](https://www.highrez.co.uk/downloads/xmousebuttoncontrol.htm) | 🪟 | yes, light | — | — | ✅ | — *(Windows drops the codes)* | ✅ | 🟡 |
| [AutoHotkey](https://www.autohotkey.com/) | 🪟 | yes, light | — | — | ✅ | — *(needs Synapse to translate first)* | 🟡 | ✅ |
| [razer-ctl](https://github.com/tdakhran/razer-ctl) | 🪟 | yes, light | — | — | — | — | — | — |
| [MiniSynapse](https://github.com/miyu/MiniSynapse) 💤 | 🪟 | no (exits) | 🟡 | 🟡 | — | — | — | — |
| **[Snakecharmer](https://github.com/asavs/snakecharmer)** | 🪟 | **yes, ~0 idle (17 MB)** | ✅ | ✅ | ✅ | **✅** | — | — |

**Notes on the odd ones out:**

- **libratbag / Piper** — the cross-vendor Linux stack: one tool for Razer, Logitech, and
  more. Remaps and DPI-tunes through the device's *onboard* profiles rather than a live
  hook, so behavior persists after you close it.
- **razer-macos** — the OpenRazer-family port for macOS; mostly lighting, effects, and
  battery.
- **dawctl** — DeathAdder *Essential* only; also tunes sensor frequency.
- **razer-ctl** — Razer *Blade laptops* (fan curves / power modes), not mice. Listed
  because people find it on the same "escape the suite" searches.
- **MiniSynapse** (💤 abandoned) — piggybacked on Synapse 2 DLLs at startup, then exited;
  the ancestor-in-spirit of "load config, then get out of the way." Doesn't work on modern
  hardware.
- **[openrazer-win32](https://github.com/CalcProgrammer1/openrazer-win32)** (not a row) —
  CalcProgrammer1's plumbing that wraps OpenRazer's C into a Windows DLL; consumed by
  OpenRGB rather than used directly.

## Where Snakecharmer sits

Read down the **"Rebind DPI buttons"** column: only Synapse, OpenRazer (Linux), and
Snakecharmer catch the vendor `0x20`/`0x21` codes at all — and of those, Synapse costs
half a gigabyte and OpenRazer isn't on Windows. Snakecharmer is the only **Windows** tool
that reclaims those buttons at **~0 idle cost**.

The two empty columns on our row — **per-app profiles** and **macros** — are deliberate.
Both require something always hot: a foreground-window watcher or a resident macro engine,
exactly the overhead we exist to avoid. RGB, DPI, and button rebinds (including the ones
Windows hides) with nothing running hot — that's the whole product.

## Pick by need

- **"I want my RGB to sync with my games / across brands"** → SignalRGB. Snakecharmer
  will likely never do this: per-frame effects require exactly the always-hot rendering
  loop we exist to avoid.
- **"I just want my lighting set, on many devices, for free"** → OpenRGB.
- **"I want one tool for my Razer *and* Logitech (or other) mice"** → libratbag + Piper
  (Linux). Cross-vendor is a non-goal for us by design.
- **"I want per-application button profiles on standard buttons"** → X-Mouse Button
  Control, possibly *alongside* Snakecharmer (we handle the DPI buttons it can't see;
  it handles per-app logic we don't have).
- **"I'm on Linux"** → OpenRazer + Polychromatic. Full stop.
- **"I'm on macOS with a Razer mouse"** → razer-macos.
- **"I don't want to install anything"** → usemice (or Synapse Web if you own a
  supported flagship device).
- **"I have a DeathAdder Essential"** → dawctl.
- **"I have a [supported Razer mouse](SUPPORTED-DEVICES.md) on Windows and want it to
  just work, forever, for free, with no bloat"** → you're home.

## Pain points we could still adopt

Surveying this landscape surfaced gaps that are *compatible* with the zero-overhead
ethos — polling rate control, DPI stages, lift-off distance, per-app profiles, and more
devices. The living list (with plausibility notes and explicit non-goals) is the
[**Upcoming** section of the README](../README.md#upcoming).
