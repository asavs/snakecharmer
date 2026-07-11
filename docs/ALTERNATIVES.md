# Alternatives — and who's better served by them

Snakecharmer does one thing well: it runs a single DeathAdder Elite on Windows with
essentially zero overhead. It is deliberately *not* a full Synapse replacement, and some
people are genuinely better served by other tools. This page maps the landscape honestly
so you can pick the right one — including when that isn't Snakecharmer.

Numbers here are qualitative on purpose. The only measured figures we publish are our
own (see the README table); for everything else, check the linked project's docs.

## The comparison

| Tool | Platform | RGB | DPI / sensor | Remap standard buttons | Remap vendor buttons (DPI up/down) | Runs resident? | Notes |
|---|---|---|---|---|---|---|---|
| [Razer Synapse](https://www.razer.com/synapse-4) | Windows | full suite, game sync | full suite | yes | yes | yes, heavily | The official everything-app. Requires an account; multiple background services. |
| [Razer Synapse Web (beta)](https://mysupport.razer.com/app/answers/detail/a_id/20619) | browser (WebHID) | basic | basic | limited | — | no | No install at all, but newer flagship devices only — the DeathAdder Elite is not supported. |
| [SignalRGB](https://signalrgb.com/) | Windows | advanced, cross-brand sync | no | no | no | yes, heavily | The choice if you want lighting synced across brands and with games. That rendering pipeline costs real CPU/RAM — the opposite trade to ours. |
| [OpenRGB](https://openrgb.org/) | Win / Linux / Mac | basic, cross-brand | no | no | no | optional | Excellent open-source lighting control; set colors and quit for zero overhead. No sensor or button features. |
| [X-Mouse Button Control](https://www.highrez.co.uk/downloads/xmousebuttoncontrol.htm) | Windows | no | no | yes, per-app profiles | **no** — Windows never sees the vendor codes | yes, light | Superb for standard-button remapping with per-application profiles (which we don't have). Cannot see the DPI buttons' `0x20`/`0x21` codes. |
| [OpenRazer](https://openrazer.github.io/) (+ [Polychromatic](https://polychromatic.app/) etc.) | Linux | full suite | full suite | yes | yes | yes, light | The gold standard on Linux, and where our protocol knowledge comes from. Not portable to Windows (kernel module). |
| [open-razerkit](https://github.com/HamzaYslmn/open-razerkit) | script | basic | basic | no | no | no (fire-and-forget) | Push a static config to the hardware and exit. Zero resident cost, no dynamic behavior. |
| [usemice](https://github.com/robbieplata/usemice) | browser (WebHID) | yes | yes | — | — | no | Open-source in-browser configurator for supported (mostly Razer) mice; nothing to install. Config only — can't listen for buttons in the background. |
| [dawctl](https://github.com/marcospb19/dawctl) | CLI | yes | yes | no | no | no | DeathAdder *Essential* only. Same spirit as us, different mouse. |
| [razer-ctl](https://github.com/tdakhran/razer-ctl) | Windows tray | — | — | — | — | yes, light | For Razer **Blade laptops** (fan/power modes), not mice. Listed because people find it searching for the same escape hatch. |
| [MiniSynapse](https://github.com/miyu/MiniSynapse) | Windows | — | — | — | — | no | Historical: piggybacked on Synapse 2 DLLs. Abandoned, does not work with modern hardware. Ancestor-in-spirit. |
| **Snakecharmer** | Windows | static / breathing / spectrum / off | DPI set + lock | thumb buttons | **yes** — native HID listener | yes, ~0 idle | One device (DeathAdder Elite), one small native exe. |

## Pick by need

- **"I want my RGB to sync with my games / across brands"** → SignalRGB. Snakecharmer
  will likely never do this: per-frame effects require exactly the always-hot rendering
  loop we exist to avoid.
- **"I just want my lighting set, on many devices, for free"** → OpenRGB.
- **"I want per-application button profiles on standard buttons"** → X-Mouse Button
  Control, possibly *alongside* Snakecharmer (we handle the DPI buttons it can't see;
  it handles per-app logic we don't have).
- **"I'm on Linux"** → OpenRazer + Polychromatic. Full stop.
- **"I don't want to install anything"** → usemice (or Synapse Web if you own a
  supported flagship device).
- **"I have a DeathAdder Essential"** → dawctl.
- **"I have a DeathAdder Elite on Windows and want it to just work, forever, for free,
  with no bloat"** → you're home.

## Pain points we could still adopt

Surveying this landscape surfaced gaps that are *compatible* with the zero-overhead
ethos — polling rate control, DPI stages, lift-off distance, per-app profiles, and more
devices. The living list (with plausibility notes and explicit non-goals) is the
[**Upcoming** section of the README](../README.md#upcoming).
