# Cracking a Gaming Mouse Without Its Vendor Software — A General Method

> Reverse-engineering the USB-HID control protocol of **your own** mouse so you can
> drive its features (button remap, DPI, RGB, polling) from a tiny userspace program
> instead of the vendor's bloated suite. This is interoperability work, the same thing
> the vendor's own software does every boot. It is **not** firmware modification.
>
> Distilled from the end-to-end crack of a **Razer DeathAdder Elite** (VID `0x1532`,
> PID `0x005C`) that replaced Razer Synapse. The Razer specifics appear as the worked
> example; the *method* transfers to almost any configurable HID mouse.

---

## 0. The one mental model that makes this tractable

A modern gaming mouse is **two devices in one piece of plastic**:

1. A **boot/standard HID device** — the mouse the OS sees: X/Y movement, buttons 1–5,
   wheel. Fully standard, works with no driver.
2. A **vendor control channel** — an extra HID collection (or interface) that accepts
   **Feature reports**: a private command protocol the vendor app uses to set DPI, RGB,
   button maps, "device mode", etc. This is what you're cracking.

"Cracking the mouse" = **finding that control channel and speaking its command protocol.**
Nothing here touches firmware, bootloader, or DFU. Every command is reversible with an
unplug/replug (vendor modes reset on power-cycle — that's *why* the vendor app re-sends
them at every login).

### The golden rule (why this is safe)
**Only send commands you have read out of an open-source reference implementation.**
Never fuzz, brute-force, or guess Feature reports — a wrong write to a vendor channel is
the one way you *can* wedge the device. The whole method below is "find the documented
command, confirm it, send exactly that."

---

## 1. Reconnaissance — identify the device and its HID topology

**Goal:** VID, PID, and the list of HID collections the mouse exposes, so you can find the
control channel.

```bash
pip install hidapi          # cross-platform HID access; the workhorse
```

```python
import hid
for d in hid.enumerate():
    if d['vendor_id'] == 0x1532:                 # <-- your VID
        print(f"{d['interface_number']}  "
              f"up={d['usage_page']:#x} us={d['usage']:#x}  "
              f"{d['product_string']}\n    {d['path']}")
```

Find your VID/PID first: Windows Device Manager → mouse → Details → *Hardware Ids*
(`HID\VID_1532&PID_005C…`), or `lsusb`/`ioreg` on Linux/mac.

**Worked example — the DeathAdder Elite exposed 7 collections across 3 interfaces:**

| interface | usage_page | usage | what it is |
|-----------|-----------|-------|------------|
| **MI_00** | **0x01 (Generic Desktop)** | **0x02 (Mouse)** | **← control channel: accepts Feature reports** |
| MI_01 Col02 | 0x0C (Consumer) | 0x01 | media keys |
| MI_01 Col03–05 | 0x01 | 0x80/0x00 | system control / vendor input |
| MI_01 Col01 \KBD | 0x01 | 0x06 (Keyboard) | keyboard-protocol interface |
| MI_02 \KBD | 0x01 | 0x06 (Keyboard) | **← where DPI-button events surface in driver mode** |

**Heuristics for spotting the control channel** (which collection eats Feature reports):
- Very often **interface 0**, the base mouse TLC (`usage_page 0x01`, `usage 0x02`).
- The collection with a **vendor-defined usage page** (`0xFF00`–`0xFFFF`) if one exists.
- On Windows the raw keyboard TLCs (`usage 0x06`) are **locked by the OS** — you can't
  open them for reading. Note them and skip them.
- When unsure, don't poke — confirm by a **read-only** command in step 3.

---

## 2. Get the protocol from an open-source reference (do NOT invent it)

This is the heart of the method and the safety guarantee. For every major brand there is a
FOSS project that already reverse-engineered the protocol. Read its source, don't guess.

| brand | reference project | where the commands live |
|-------|-------------------|--------------------------|
| Razer | **OpenRazer** (`openrazer/openrazer`) | `driver/razercommon.*`, `driver/razerchromacommon.c`, `driver/razer*_driver.c` |
| Logitech | **libratbag** / **Solaar** | HID++ protocol in `src/driver-hidpp*.c` / `logitech_receiver/` |
| SteelSeries, others | **libratbag** | one `.c` driver per family under `src/` |
| many | **libratbag** (umbrella) | the ratbag driver for your family |

**Razer protocol, as extracted from OpenRazer** — a fixed **90-byte Feature report**:

```
offset  field            notes
0       status           0x00 on send; 0x02=success / 0x01=busy on read-back
1       transaction_id   MODEL-SPECIFIC. DeathAdder Elite = 0x3F. Grep the driver.
2..3    remaining_pkts   0x00 0x00
4       protocol_type    0x00
5       data_size        # of argument bytes (e.g. 0x02)
6       command_class    e.g. 0x00
7       command_id       e.g. 0x04  (top bit set = READ: 0x84)
8..87   arguments        payload, zero-padded
88      crc              XOR of bytes 2..87  (inclusive)
89      reserved         0x00
```

On Windows/`hidapi`, report ID 0 means you prepend a `0x00` → send a **91-byte** buffer.

CRC in code (verbatim logic from OpenRazer `razer_calculate_crc`):
```python
def crc(report90):                 # report90 = the 90 bytes above w/ crc field = 0
    c = 0
    for b in report90[2:88]:       # bytes 2..87 inclusive
        c ^= b
    return c
```

**The specific commands used in the DeathAdder crack:**
```
                 st txn rem  proto sz cls id  args     …zeros…  crc rsvd
Set DRIVER mode: 00 3F 00 00 00    02 00 04  03 00     (78×00)  05  00
Set HARDWARE:    00 3F 00 00 00    02 00 04  00 00     (78×00)  06  00
Get mode (read): 00 3F 00 00 00    02 00 84  00 00     (78×00)  86  00
```
`command_class 0x00 / id 0x04` = "set device mode"; arg `0x03` = driver mode, `0x00` =
hardware/normal. `id 0x84` (0x04 | 0x80) is the read form.

> **Per-model gotcha that will bite you:** the `transaction_id` differs by model. Razer's
> `razer_attr_write_device_mode` is a big `switch` on PID. Find *your* PID's case. Using
> the wrong transaction_id just makes the device ignore the report (status stays busy) —
> annoying, not dangerous, but it's the #1 reason "it does nothing."

---

## 3. Confirm the channel with a READ before you ever WRITE

Never let your first command be a state change. Send the **read** form to the collection
you think is the control channel and check for a sane response.

```python
import hid
h = hid.Device(path=CONTROL_PATH)          # the MI_00 mouse TLC from step 1
h.send_feature_report(bytes([0x00]) + GET_MODE_90)   # leading 0x00 = report id
resp = h.get_feature_report(0, 91)
# status byte (resp[1] here, after the id) should be 0x02 = success
# arg byte should read back the current mode (0x00 hardware)
```

If you get `0x02` + plausible data, **you've found the control channel and proven the
protocol** without changing anything. If you get garbage or an exception on every
collection, revisit step 1 — you're talking to the wrong TLC (or it's an OS-locked
keyboard TLC).

Do the same read for DPI to double-confirm (it should read back a believable number like
1800×1800). Two independent sane reads = high confidence before any write.

---

## 4. Send the state-change command, then verify by read-back

```python
h.send_feature_report(bytes([0x00]) + SET_DRIVER_MODE_90)   # busy-retry: if status
# ... read mode again; it should now report 0x03                # 0x01, sleep 10ms, resend
```

**Verification checklist after the write (all were checked in the real crack):**
- Read-back of mode returns the value you set (`0x03`). ✔
- **All** collections still enumerate (device didn't drop off the bus). ✔
- Left/right click + movement still work (you're using the mouse — trivially confirmed). ✔
- Unplug/replug returns it to stock (proves reversibility). ✔

---

## 5. Find where the "hidden" buttons now surface — and catch them

This is the part people miss. On the DeathAdder, "driver mode" does **not** make the DPI
buttons into "button 6/7" (Windows only has 5 mouse buttons, and the firmware never sends
mouse-buttons for them). Instead, ground-truth from OpenRazer's `razer_raw_event`:

> In driver mode the firmware stops consuming the DPI buttons for internal DPI switching
> and instead emits a **16-byte Input report, report ID `0x04`, on the keyboard-protocol
> interface**, carrying **vendor codes `0x20` (DPI-up) / `0x21` (DPI-down)**.

So the buttons *do* become visible — just as a vendor input report on a different
collection, not as standard buttons. No standard remapper (X-Mouse etc.) can see them.
The fix is a tiny **listener/injector daemon**:

1. Open every **readable** aux collection (skip the OS-locked keyboard TLCs).
2. `read()` input reports in a loop.
3. When a report matches `id 0x04` + a known vendor code, **inject** the keystroke/action
   you want (Copy, Paste, key 9/0, F13…) via `SendInput`/`keybd_event` (Win),
   `uinput` (Linux).
4. **Log every code you see**, including unrecognized ones — so one real button press
   tells you the exact mapping if it differs on your model.
5. **Re-assert driver mode every ~60 s** (and on resume/replug), because power events
   reset it. This is exactly what the vendor daemon does.

> Honest-uncertainty note from the original run: the agent could send/verify everything
> but **could not press the buttons itself**. The daemon was written to log any vendor
> code received, so a single human press against the log file settled the final mapping.
> (It did: Copy fired correctly.) **Build the press-to-action path to be self-diagnosing.**

---

## 6. Make it persist across logins

Vendor modes die on power-cycle, so re-apply at login:
- **Windows, no admin:** a `.lnk` in the Startup folder launching `pythonw.exe`
  (windowless) → your daemon. (Scheduled Task at logon also works.)
- **Linux:** a `systemd --user` service, or udev rule + the daemon.

The daemon's own 60-second re-assert loop then handles sleep/wake and replug within a
session.

---

## 7. Generalizing to *other* mice — the checklist

Run this exact sequence for any new mouse:

1. **VID/PID** from Device Manager / `lsusb`.
2. **Enumerate HID collections** (step 1 script) → tabulate interface/usage_page/usage.
3. **Locate the FOSS reference** for the brand (OpenRazer / libratbag / Solaar) and open
   the driver for *your* family. Read out: report length, framing, CRC/checksum,
   per-model transaction/device id, and the specific command opcodes you need.
4. **Guess the control channel** (interface 0 mouse TLC, or the vendor-usage-page TLC).
5. **READ-confirm** it (step 3) before any write.
6. **WRITE + read-back + full verification checklist** (step 4).
7. If a feature's events are "invisible," **find which collection they surface on** and
   write a **self-logging listener** (step 5).
8. **Persist** at login (step 6).

### Brand reality-check
- **Razer:** 90-byte Feature report, XOR CRC, per-model transaction_id. As above.
- **Logitech:** completely different — **HID++** (short 7-byte / long 20-byte reports over
  a vendor collection, feature-index negotiation). Read Solaar/libratbag; none of the
  Razer byte layout applies, but *the method* (enumerate → read reference → confirm →
  send) is identical.
- **SteelSeries / Corsair / others:** each has its own framing; libratbag is the first
  place to look. If **no** FOSS reference exists for your exact model, capture the vendor
  app's real traffic with **Wireshark + USBPcap** (Win) / `usbmon` (Linux) and match
  packets to actions — but that is observation of documented-by-the-vendor behavior, still
  **not** fuzzing.

---

## 8. Hard safety constraints (carry these to every mouse)

- ❌ **No firmware / bootloader / DFU.** Ever. This method never needs it.
- ❌ **No fuzzing or guessing Feature reports.** Only send commands lifted from a reference
  implementation or observed from the vendor's own app.
- ✅ **Read before you write.** Confirm the channel non-destructively first.
- ✅ **Verify the device still fully works** (click, move, enumerate) after every write, and
  confirm **unplug/replug restores stock** — your escape hatch.
- ✅ If an effect is uncertain or a command's meaning isn't in the reference, **stop and
  report**, don't experiment on your only mouse.

---

## Appendix — where the worked example lives
Both live in this repository:
- Prototype (Python): [`reference/`](reference/) — `razer_common.py` (protocol/CRC),
  `dpi_button_daemon.py` (listener/injector), `set_device_mode.py`, `set_dpi.py`.
- Native rewrite (Rust): `crates/razer-proto` (pure protocol + unit tests asserting the
  exact driver-mode bytes/CRC), `crates/razer-hid`, plus tray/RGB/settings/thumb-hook.
  See [`docs/SPEC.md`](docs/SPEC.md).
