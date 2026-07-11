# Supported devices

Every device Snakecharmer can drive, mirroring the `SUPPORTED` table in
[`crates/razer-proto/src/lib.rs`](../crates/razer-proto/src/lib.rs) — that table is the
source of truth; this page is its human-readable twin, and a test
(`supported_devices_doc_matches_table`) fails CI if the two drift. All protocol knowledge is sourced
from [OpenRazer](https://github.com/openrazer/openrazer) (`driver/razermouse_driver.c`).

| Model | USB id | txn | RGB zones | DPI buttons | DPI range | Verified on hardware |
|---|---|---|---|---|---|---|
| DeathAdder Elite | `1532:005C` | `0x3F` | scroll wheel, logo | yes (`0x20`/`0x21`) | 100–16000 | ✅ |

**Column notes**

- **txn** — the `transaction_id` byte stamped into every 90-byte control report.
- **RGB zones** — the addressable lighting zones effects are applied to; "—" means the
  model has no lighting hardware, and lighting commands are clean no-ops.
- **DPI buttons** — whether the model has the wheel DPI buttons that emit private vendor
  codes in driver mode. Without them, driver mode and the button listeners are skipped
  entirely.
- **Verified on hardware** — someone ran the daemon against the real device and confirmed
  DPI set/read round-trips and correct feature gating, not just a table entry copied from
  OpenRazer.

## Not on the list?

Your mouse is ignored, never written to — the daemon simply waits for a supported device
to appear (see the Safety FAQ in the [README](../README.md#safety)).

Adding a device is designed to be a **one-file diff**: a `DeviceSpec` const, a line in
`SUPPORTED`, a test, and a row here. Start with
[`CRACKING-MICE-GUIDE.md`](../CRACKING-MICE-GUIDE.md), then open a PR with the
[add-device template](../.github/PULL_REQUEST_TEMPLATE/add-device.md). If your device is
already in OpenRazer's `razermouse_driver.c`, you may not need a packet capture at all.
