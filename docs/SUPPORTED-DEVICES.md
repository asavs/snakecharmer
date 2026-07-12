# Supported devices

Every device Snakecharmer can drive, mirroring the `SUPPORTED` table in
[`crates/razer-proto/src/lib.rs`](../crates/razer-proto/src/lib.rs) — that table is the
source of truth; this page is its human-readable twin, and a test
(`supported_devices_doc_matches_table`) fails CI if the two drift. All protocol knowledge is sourced
from [OpenRazer](https://github.com/openrazer/openrazer) (`driver/razermouse_driver.c`).

| Model | USB id | txn | RGB zones | DPI buttons | DPI range | Polling (Hz) | Verified on hardware |
|---|---|---|---|---|---|---|---|
| DeathAdder Elite | `1532:005C` | `0x3F` | scroll wheel, logo | yes (`0x20`/`0x21`) | 100–16000 | 125/500/1000 | ✅ |
| DeathAdder V3 (wired) | `1532:00B2` | `0x1F` | — | — | 100–30000 | 125/500/1000/2000/4000/8000 | ✅ |

**Column notes**

- **txn** — the `transaction_id` byte stamped into every 90-byte control report.
- **RGB zones** — the addressable lighting zones effects are applied to; "—" means the
  model has no lighting hardware, and lighting commands are clean no-ops.
- **DPI buttons** — whether the model has the wheel DPI buttons that emit private vendor
  codes in driver mode. Without them, driver mode and the button listeners are skipped
  entirely.
- **Polling (Hz)** — the report rates the hardware accepts. Which command family the
  device speaks (classic `razer_chroma_misc_set_polling_rate` vs the extended
  `..._rate2` that reaches 8000 Hz) is spec data too (`DeviceSpec::polling`).
- **Verified on hardware** — someone ran the daemon against the real device and confirmed
  DPI set/read round-trips and correct feature gating, not just a table entry copied from
  OpenRazer.

## Button maps

Which physical control is which config key. Each diagram is **data**, not artwork: it
lives in the device's `DeviceSpec::diagram` as a small list of shapes, the settings
window renders the connected device's copy natively (GDI+), and the SVGs below are
generated from the same data (`cargo test -p razer-proto -- --ignored
regenerate_diagram_svgs`). The drift-check test regenerates each SVG and fails if the
committed asset differs, so the doc and the UI can't diverge. Drawing one for a new
device — finding the official schematic, the legal rules, the shape DSL, the
verification loop — is covered in [`DRAWING-MICE-GUIDE.md`](DRAWING-MICE-GUIDE.md).

Razer's official schematics (linked per device below) are used as a positional reference for button placement and body proportions. The outline and shape of the mouse are drawn as accurately as possible to the physical hardware, while avoiding any trademarked logos (the logo LED is drawn as an abstract circle with an 'S' curve). All diagram line work is original coordinate data.

### DeathAdder Elite

![DeathAdder Elite button map](assets/deathadder-elite.svg)

Razer's official diagram: [DeathAdder Elite schematic](https://dl.razerzone.com/src/aag/2043-2-en-v2.png)
(reference only, not bundled).

The two thumb buttons (`XBUTTON1`/`XBUTTON2`) are standard Windows buttons — remapping
them is opt-in and uses the global hook (see the README's safety FAQ). The two DPI
buttons emit private vendor codes (`0x20`/`0x21`) in driver mode; rebinding them is
free — nothing touches the pointer's motion path.

### DeathAdder V3 (wired)

![DeathAdder V3 button map](assets/deathadder-v3.svg)

Razer's official diagram: [DeathAdder V3 schematic](https://dl.razerzone.com/src2/6128/6128-2-en-v1.png)
(reference only, not bundled; found via the model's
[support page](https://mysupport.razer.com/app/answers/detail/a_id/6124/)).

## Not on the list?

Your mouse is ignored, never written to — the daemon simply waits for a supported device
to appear (see the Safety FAQ in the [README](../README.md#safety)).

Adding a device is designed to be a **one-file diff**: a `DeviceSpec` const, a line in
`SUPPORTED`, a test, and a row here. Start with
[`CRACKING-MICE-GUIDE.md`](../CRACKING-MICE-GUIDE.md), then open a PR with the
[add-device template](../.github/PULL_REQUEST_TEMPLATE/add-device.md). If your device is
already in OpenRazer's `razermouse_driver.c`, you may not need a packet capture at all.
