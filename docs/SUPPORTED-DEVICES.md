# Supported devices

Every device Snakecharmer can drive. The source of truth is the `SUPPORTED` table in
[`crates/razer-proto/src/lib.rs`](../crates/razer-proto/src/lib.rs); this page is the
readable copy, and a test (`supported_devices_doc_matches_table`) fails CI if the two
disagree. All protocol knowledge comes from
[OpenRazer](https://github.com/openrazer/openrazer) (`driver/razermouse_driver.c`).

| Model | USB id | txn | RGB zones | DPI buttons | DPI range | Polling (Hz) | Verified on hardware |
|---|---|---|---|---|---|---|---|
| DeathAdder Elite | `1532:005C` | `0x3F` | scroll wheel, logo | yes (`0x20`/`0x21`) | 100–16000 | 125/500/1000 | yes |
| DeathAdder V3 (wired) | `1532:00B2` | `0x1F` | none | no | 100–30000 | 125/500/1000/2000/4000/8000 | yes |

### Column notes

- **txn:** the `transaction_id` byte stamped into every 90-byte control report.
- **RGB zones:** the addressable lighting zones that effects apply to. "none" means the
  model has no lighting hardware, and lighting commands do nothing.
- **DPI buttons:** whether the model has the wheel DPI buttons that send private vendor
  codes in driver mode. Without them, driver mode and the button listeners are skipped
  entirely.
- **Polling (Hz):** the report rates the hardware accepts. Which command family the
  device uses (classic `razer_chroma_misc_set_polling_rate` or the extended
  `..._rate2` that reaches 8000 Hz) is also spec data (`DeviceSpec::polling`).
- **Verified on hardware:** someone ran the daemon against the real device and confirmed
  that DPI set/read round-trips and that unsupported features are correctly skipped. A
  table entry copied from OpenRazer doesn't count.

## On-board binds

This is tracked separately from the table above: whether Snakecharmer can write button
binds into the mouse's own memory (`charmctl keymap`, the config's `[onboard]` table).
That needs a recorded keymap and a capture of Synapse writing it. See
[`ONBOARD-KEYMAP.md`](ONBOARD-KEYMAP.md).

- **DeathAdder V3 (wired):** read, reset and write for right, wheel click, back, forward,
  wheel up and wheel down. Left and the underside DPI button are not written.
- **DeathAdder Elite:** none. Synapse 4 remaps it in software, so there are no leftover
  on-board binds, and the hook-based remap covers it.

## Button maps

Which physical control is which config key. A device may ship without a diagram: the
field is an `Option`, and when it's `None` the settings window lists the same controls as
labeled rows instead of callouts on a picture. So this section covers only the devices
that have a diagram.

Each diagram is stored as data. It lives in the device's `DeviceSpec::diagram` as a small
list of shapes, the settings window draws the connected device's copy natively (GDI+), and
the SVGs below are generated from the same data (`cargo test -p razer-proto -- --ignored
regenerate_diagram_svgs`). The drift-check test regenerates each SVG and fails if the
committed asset differs, so the doc and the UI can't diverge.
[`DRAWING-MICE-GUIDE.md`](DRAWING-MICE-GUIDE.md) covers drawing one for a new device:
finding the official schematic, the legal rules, the shape DSL, and how to check your
work.

Razer's official schematics (linked for each device below) are the reference for button placement and body proportions. The mouse outline follows the physical hardware as closely as possible but leaves out trademarked logos (the logo LED is drawn as an abstract circle with an 'S' curve). All diagram line work is original coordinate data.

### DeathAdder Elite

![DeathAdder Elite button map](assets/deathadder-elite.svg)

Razer's official diagram: [DeathAdder Elite schematic](https://dl.razerzone.com/src/aag/2043-2-en-v2.png)
(reference only, not bundled).

*Provenance:* protocol ported from OpenRazer by a Claude Fable 5 agent.

The two thumb buttons (`XBUTTON1`/`XBUTTON2`) are standard Windows buttons. Remapping
them is opt-in and uses the global hook (see the [README's safety FAQ](../README.md#safety)).
The two DPI buttons send private vendor codes (`0x20`/`0x21`) in driver mode. Rebinding
them costs nothing, because nothing touches the pointer's motion path.

### DeathAdder V3 (wired)

![DeathAdder V3 button map](assets/deathadder-v3.svg)

Razer's official diagram: [DeathAdder V3 schematic](https://dl.razerzone.com/src2/6128/6128-2-en-v1.png)
(reference only, not bundled; found via the model's
[support page](https://mysupport.razer.com/app/answers/detail/a_id/6124/)).

The V3 has no lighting and no wheel DPI buttons. Its single DPI button is on the
underside: it cycles onboard DPI stages in firmware and can't be remapped or
listened to, so Snakecharmer never touches it (driver mode and the vendor-code
listeners are skipped entirely on this model).

*Provenance:* protocol ported from OpenRazer by Claude Opus 4.8 (high).

## Not on the list?

Snakecharmer ignores your mouse and never writes to it. The daemon waits for a supported
device to appear (see the [README's safety FAQ](../README.md#safety)).

[`DEVICE-WISHLIST.md`](DEVICE-WISHLIST.md) lists every Razer mouse whose protocol OpenRazer
already knows, with its USB id, transaction id, DPI ceiling and lighting zones filled in,
and who cracked the ones that shipped. If yours is on it, most of the reading is done.
Claim it there.

Adding a device is meant to be a **one-file diff**: a `DeviceSpec` const, a line in
`SUPPORTED`, a test, and a row here. Start with
[`CRACKING-MICE-GUIDE.md`](CRACKING-MICE-GUIDE.md), then open a PR with the
[add-device template](../.github/PULL_REQUEST_TEMPLATE/add-device.md). If your device is
already in OpenRazer's `razermouse_driver.c`, you may not need a packet capture at all.
