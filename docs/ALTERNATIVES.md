# Alternatives, and who they suit better

Snakecharmer runs a short list of [supported Razer mice](SUPPORTED-DEVICES.md) on Windows
with close to zero overhead. It is deliberately *not* a full Synapse replacement, and
other tools suit some people better. This page lays out the options so you can pick the
right one, including when that isn't Snakecharmer.

The only figures we've *measured* are our own (see the README table). Third-party numbers
come from each project's own docs, forums, or issue tracker, linked where possible. If
one is wrong, send a source and we'll fix it.

## Who this project competes with (only Synapse)

The tools people have built around this hardware are good, and none of them are a target. OpenRGB and SignalRGB
have spent years on lighting (effects, game sync, cross-brand control) and are
better at it than this project will ever be. X-Mouse Button Control has done
per-application button profiles well for over a decade. AutoHotkey is a macro engine with
two decades of momentum. libratbag and Piper are the cross-vendor Linux answer. And
[OpenRazer](https://openrazer.github.io/) is the reason any of this is possible at all:
they reverse-engineered Razer's protocol years ago, and every byte Snakecharmer sends was
read out of their driver. This is their work ported to Windows, GPL because it is theirs
as much as ours.

So requests for lighting effects, game sync, or macros get **pointed here instead of
built**. All three need something running all the time,
which is the exact cost this project exists to avoid.

There is one target, and it is Razer Synapse: six processes, two kernel drivers, and
half a gigabyte of resident memory to run two side buttons and a DPI setting.

## The comparison

**Key:** ✅ full · 🟡 basic / partial · — not offered.
**OS:** 🪟 Windows · 🐧 Linux · 🍎 macOS · 🌐 browser · ⌨️ command-line tool · 💤 abandoned.
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
| [secbird](https://github.com/infinityInfinite/secbird) | 🪟, 🐧 ⌨️ | yes, light (~12 MB) | ✅ | ✅ | ✅ *(on-board memory)* | ✅ *(on-board memory)* | ✅ | ✅ *(on-device)* |
| [razer-ctl](https://github.com/tdakhran/razer-ctl) | 🪟 | yes, light | — | — | — | — | — | — |
| [MiniSynapse](https://github.com/miyu/MiniSynapse) 💤 | 🪟 | no (exits) | 🟡 | 🟡 | — | — | — | — |
| **[Snakecharmer](https://github.com/asavs/snakecharmer)** | 🪟 | **yes, ~0 idle (17 MB)** | ✅ | ✅ | ✅ | **✅** | — | — |

**Notes on the odd ones out:**

- **libratbag / Piper:** the cross-vendor Linux stack, one tool for Razer, Logitech, and
  more. It remaps and sets DPI through the device's *onboard* profiles instead of a live
  hook, so the settings persist after you close it.
- **razer-macos:** the OpenRazer-family port for macOS; mostly lighting, effects, and
  battery.
- **dawctl:** DeathAdder *Essential* only; also tunes sensor frequency.
- **secbird** (Secretarybird): the closest tool to Snakecharmer, with a much wider scope:
  keyboards and other Razer devices, per-app profiles, macros stored on the device, and
  HyperShift. Button remaps are written to the mouse's on-board memory, so they need a mouse
  that has it. In beta: its author tests on a Basilisk V3, users have confirmed nine more
  devices, and the rest are listed from OpenRazer's protocol facts. On Linux it is
  command-line only.
- **razer-ctl:** for Razer *Blade laptops* (fan curves, power modes), not mice. Listed
  because people find it in the same "escape the suite" searches.
- **MiniSynapse** (💤 abandoned): loaded Synapse 2 DLLs at startup, then exited. It is the
  ancestor in spirit of "load config, then get out of the way." It doesn't work on modern
  hardware.
- **[openrazer-win32](https://github.com/CalcProgrammer1/openrazer-win32)** (not a row):
  CalcProgrammer1's wrapper that builds OpenRazer's C code into a Windows DLL. OpenRGB uses
  it; people don't run it directly.

## Where Snakecharmer sits

Read down the **"Rebind DPI buttons"** column: Synapse, OpenRazer (Linux), secbird and
Snakecharmer can rebind them. Synapse costs half a gigabyte and OpenRazer isn't on Windows.
secbird writes the remap into the mouse's on-board memory, so it needs a mouse that has
some. Snakecharmer catches the vendor `0x20`/`0x21` codes live, at **~0 idle cost**, so it
doesn't depend on on-board memory.

The two empty columns on our row, **per-app profiles** and **macros**, are deliberate.
Both need something running all the time (a foreground-window watcher or a resident macro
engine), which is exactly the overhead we exist to avoid. The whole product is RGB, DPI,
and button rebinds, including the buttons Windows hides, with nothing running hot.

## Pick by need

- **"I want my RGB to sync with my games / across brands"** → SignalRGB. Snakecharmer
  will likely never do this: per-frame effects need an always-running render loop, the
  thing we exist to avoid.
- **"I just want my lighting set, on many devices, for free"** → OpenRGB.
- **"I want one tool for my Razer *and* Logitech (or other) mice"** → libratbag + Piper
  (Linux). Cross-vendor is a non-goal for us by design.
- **"I want per-application button profiles on standard buttons"** → X-Mouse Button
  Control, possibly *alongside* Snakecharmer (we handle the DPI buttons it can't see,
  and it handles the per-app logic we don't have).
- **"I want per-game profiles, macros, or my Razer keyboard and other devices on Windows"**
  → secbird. It does more than Snakecharmer and covers far more devices, most of them not
  yet tested on hardware.
- **"I'm on Linux"** → OpenRazer + Polychromatic. Full stop.
- **"I'm on macOS with a Razer mouse"** → razer-macos.
- **"I don't want to install anything"** → usemice (or Synapse Web if you own a
  supported flagship device).
- **"I have a DeathAdder Essential"** → dawctl.
- **"I have a [supported Razer mouse](SUPPORTED-DEVICES.md) on Windows and want it to
  just work, forever, for free, with no bloat"** → you're home.

## Gaps we could still fill

Looking at these tools turned up gaps that *fit* the zero-overhead goal: lift-off
distance, per-app profiles, and more devices. (Polling rate control has since shipped, and
DPI stages became a non-goal.) The current
list, with plausibility notes and explicit non-goals, is in the
[Upcoming section of the README](../README.md#upcoming).
