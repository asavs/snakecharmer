<!--
Adding support for a new device? This is the template for you.
Open your PR with:  ?template=add-device.md  appended to the compare URL,
or pick "add-device" from the template dropdown.

The model to copy: the DeathAdder V3 PR (#2). Adding a device is meant to be
a small, one-file diff — a DeviceSpec const, a line in SUPPORTED, and a test.
If your PR is bigger than that, something generic is missing from the crates;
flag it in the PR and we'll pull it up into the shared layer.
-->

## Device

- **Model:** <!-- e.g. Razer DeathAdder V2 -->
- **USB id:** `VID 0x1532 / PID 0x____`
- **How I read the PID:** <!-- Device Manager / `Get-PnpDevice` / lsusb -->

## Protocol source

Snakecharmer's protocol knowledge comes from **OpenRazer**. State where each
fact below came from — ideally the OpenRazer source, otherwise a USB capture.

- [ ] Found the device in OpenRazer (`driver/razermouse_driver.c` / `razermouse_driver.h`), **or** captured its traffic with Wireshark + USBPcap and documented the reports.
- **transaction_id:** `0x__`  <!-- 0x3F on the Elite, 0x1F on the V3 — grep the DEATHADDER_* cases -->
- **Notes / quirks:** <!-- anything that isn't just "same as an existing device" -->

## `DeviceSpec` fields

Fill these in — they're the entire "crack", distilled:

| field | value | how you know |
|---|---|---|
| `product_id` | `0x____` | |
| `transaction_id` | `0x__` | |
| `has_rgb` | `true` / `false` | does the hardware have addressable lighting? |
| `has_dpi_buttons` | `true` / `false` | does it have the wheel DPI buttons that emit `0x20`/`0x21` in driver mode? |
| `dpi_min` / `dpi_max` | `100` / `____` | the sensor's full range — **the UI must be actionable over all of it** |

## Checklist

- [ ] Added a `DeviceSpec` const and registered it in `SUPPORTED` (`crates/razer-proto/src/lib.rs`).
- [ ] Added a per-device test (assert the transaction id and, if the range differs, the DPI bounds).
- [ ] `cargo test --workspace` is green.
- [ ] **No changes were needed** to `razer-hid` or the daemon. *(If you did have to touch them, say why here — it usually means a generic gap to lift into the shared layer, not device-specific code.)*
- [ ] Existing devices unaffected — the byte-exact report tests still pass unchanged.

## Hardware verification

Adding a device to the table is a claim about real hardware; back it up.

- [ ] Plugged the device in and ran the daemon; the log shows `Opened <model> (PID …, txn …; rgb=…, dpi_buttons=…)` with the expected values and **no crash-loop**.
- [ ] `charmctl set-dpi <n>` then `charmctl status` — DPI takes and reads back, including a value near the device's max.
- [ ] Feature flags behave: RGB works (or is correctly skipped), DPI-button remap works (or is correctly absent).

<!-- Paste the relevant daemon.log lines / charmctl output here. -->
