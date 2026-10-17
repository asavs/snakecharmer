<!--
Adding support for a new device? This is the template for you.
Open your PR with:  ?template=add-device.md  appended to the compare URL,
or pick "add-device" from the template dropdown.

The PR to copy: the DeathAdder V3 PR, https://github.com/asavs/snakecharmer/pull/3.
Adding a device is a small diff: one new file in crates/razer-proto/src/devices/
(the DeviceSpec const and its test), the registration lines in devices/mod.rs
and the re-export in lib.rs, and a row in docs/SUPPORTED-DEVICES.md.
If your PR touches more than that, something generic is missing from the crates;
flag it in the PR and we'll pull it up into the shared layer.
-->

## Device

- **Model:** <!-- e.g. Razer DeathAdder V2 -->
- **USB id:** `VID 0x1532 / PID 0x____`
- **How I read the PID:** <!-- Device Manager / `Get-PnpDevice` / lsusb -->
- **Agent/model used (for the track record):** <!-- optional, e.g. Claude Opus 4.8 (high). Helps future contributors judge what capability level is enough. -->

## Protocol source

Snakecharmer's protocol knowledge comes from **OpenRazer**. State where each
fact below came from: ideally the OpenRazer source, otherwise a USB capture.

- [ ] Found the device in OpenRazer (`driver/razermouse_driver.c` / `razermouse_driver.h`), **or** captured its traffic with Wireshark + USBPcap and documented the reports.
- **transaction_id:** `0x__`  <!-- 0x3F on the Elite, 0x1F on the V3. Grep the DEATHADDER_* cases. -->
- **Notes / quirks:** <!-- anything that isn't just "same as an existing device" -->
- **On-board memory:** <!-- how many on-board profiles Synapse offers (1 if no picker, "none" if no on-board memory). Snakecharmer writes on-board binds on the DeathAdder V3 only. Adding them for another mouse needs a capture of Synapse writing that mouse's buttons plus maintainer approval; this PR does not need one. See "When a capture is needed" and the runbook in docs/ONBOARD-KEYMAP.md -->

## `DeviceSpec` fields

Fill these in. Together they are the whole "crack":

| field | value | how you know |
|---|---|---|
| `product_id` | `0x____` | |
| `transaction_id` | `0x__` | |
| `rgb_zones` | e.g. `&[led::SCROLL_WHEEL, led::LOGO]`, or `&[]` for none | which lighting zones does the hardware have? |
| `dpi_buttons` | `Some(DpiButtons { up: 0x__, down: 0x__ })` / `None` | does it have wheel DPI buttons, and what vendor codes do they emit in driver mode? |
| `dpi_min` / `dpi_max` | `100` / `____` | the sensor's full range. **The UI must let you set any value in it.** |
| `polling` | `PollingSpec { protocol: Classic\|Extended, rates: &[...] }` | which command family (OpenRazer `razer_chroma_misc_set_polling_rate` vs `..._rate2`) and which Hz values the hardware accepts |
| `diagram` | `None`, or `Some(Diagram { width, height, shapes: &[...] })` | **Optional. `None` is fine and ships.** Without one the settings window uses labeled control rows and the device is fully functional. Drawing it is a separate skill and a separate PR, so a protocol port never waits on an illustration. When you do want to draw one, follow [`docs/DRAWING-MICE-GUIDE.md`](../../docs/DRAWING-MICE-GUIDE.md). |

## Checklist

- [ ] Added a `DeviceSpec` const and registered it in `SUPPORTED` (`crates/razer-proto/src/devices/mod.rs`, plus the re-export in `lib.rs`).
- [ ] Added the device's row to `docs/SUPPORTED-DEVICES.md`.
- [ ] *(Only if you drew a diagram)* Regenerated the SVG (`cargo test -p razer-proto -- --ignored regenerate_diagram_svgs`) and committed `docs/assets/<device>.svg`; the Button maps section embeds it and links the model's official schematic (no bundled PNGs, no trademarked logo). **Shipping `diagram: None` is a complete PR.** Open a follow-up issue so someone can draw it later.
- [ ] Added a `#[cfg(test)] mod tests` to the device file, modeled on `deathadder_elite.rs`: product id, transaction id, DPI range, polling, RGB zones, and a set-DPI report with exact bytes.
- [ ] The `docs/SUPPORTED-DEVICES.md` row matches the format the `supported_devices_doc_matches_table` test checks: model name, transaction id as `0xNN`, DPI range with an en dash (`100–30000`), polling rates joined by `/`.
- [ ] Added the PID to `CLAIMED` in `tools/wishlist-from-openrazer.py` and regenerated `docs/DEVICE-WISHLIST.md` (or edited the row by hand).
- [ ] `dpi_buttons` is `None` unless I read the codes off my own mouse with `reference/button_listener.ps1`.
- [ ] `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` are green (the commands CI runs; see CONTRIBUTING.md).
- [ ] **No changes were needed** to `razer-hid` or the daemon. *(If you did have to touch them, say why here. It usually means a generic gap to lift into the shared layer rather than device-specific code.)*
- [ ] Existing devices are unaffected: the byte-exact report tests still pass unchanged.

## Hardware verification

Adding a device to the table is a claim about real hardware. Back it up.

- [ ] Plugged the device in and ran the daemon; the log shows `Opened <model> (PID …, txn …; rgb=…, dpi_buttons=…)` with the expected values and **no crash-loop**.
- [ ] Ran `charmctl set-dpi <n>`, then `charmctl status`. DPI applies and reads back, including a value near the device's max.
- [ ] Feature flags behave: RGB works (or is correctly skipped), DPI-button remap works (or is correctly absent).

<!-- Paste the relevant daemon.log lines / charmctl output here. -->
