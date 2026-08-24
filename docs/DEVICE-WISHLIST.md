# Device wishlist

Every Razer mouse Snakecharmer *could* drive, and who cracked the ones it already does.

The list isn't a wishlist in the "wouldn't it be nice" sense — it's the set of devices whose
protocol is **already written down in an open-source reference**
([OpenRazer](https://github.com/openrazer/openrazer)). That's the hard part of a crack, and
it's done. What's left per device is the part
[`CRACKING-MICE-GUIDE.md`](../CRACKING-MICE-GUIDE.md) walks you through: confirm the channel
with a read, fill in a `DeviceSpec`, draw the button map, verify on real hardware.

Shipped devices live in [`SUPPORTED-DEVICES.md`](SUPPORTED-DEVICES.md) — that page is the
list of mice this daemon actually talks to. This page is the board of everything else.

## Claiming one

The whole flow is ordinary GitHub; nothing to ask permission for.

1. **Open an issue** with the
   [add-a-mouse form](https://github.com/asavs/snakecharmer/issues/new?template=device-request.yml)
   and say you're working on it — or comment on an existing one, and it gets assigned to
   you. This is the only step that exists to stop two people cracking the same mouse on
   the same weekend. The form also works the other way round: if you have the hardware but
   don't want to write any code, fill it in and someone else picks it up.
2. **Crack it** — [`CRACKING-MICE-GUIDE.md`](../CRACKING-MICE-GUIDE.md) for the method, the
   [`reference/`](../reference/) Python toolkit for poking the device first, and
   [`DRAWING-MICE-GUIDE.md`](DRAWING-MICE-GUIDE.md) for the diagram. It is a genuinely good
   job to hand an AI coding agent; two of the two devices here were done that way.
3. **Open a PR** with the [add-device template](../.github/PULL_REQUEST_TEMPLATE/add-device.md)
   and `Closes #<your issue>`. Fill in the *Agent/model used* field — the track record of
   which models can complete a crack end to end is useful to the next person.
4. **On merge** the row's Status becomes ✅ cracked by you — your handle, your PR, and the
   model you drove.
   If you sourced protocol from a project that isn't already credited, add it to
   [`NOTICE`](../NOTICE) in the same PR — that's a license obligation, not a courtesy.

An unclaimed row is fair game. A claim with no PR and no reply after about a month goes back
to unclaimed; nobody is going to be precious about it.

**Labels**, so you can find the shape of work you want:
[`device-request`](https://github.com/asavs/snakecharmer/labels/device-request) — a mouse
somebody wants; [`good first issue`](https://github.com/asavs/snakecharmer/labels/good%20first%20issue) — small
and self-contained; [`diagram`](https://github.com/asavs/snakecharmer/labels/diagram) — a
working device that nobody has drawn yet, which needs **no hardware at all**;
[`needs-hardware`](https://github.com/asavs/snakecharmer/labels/needs-hardware) — the code
is written and it's waiting on someone who owns the mouse to press a button.

## Adding your mouse with an AI assistant

You do not need a particular tool or a paid model for this — the repo does the hard part.
Paste this into whatever assistant you have, with your mouse's name filled in:

> Add support for my **<mouse>** to this repo (github.com/asavs/snakecharmer).
>
> 1. Find my model in `docs/DEVICE-WISHLIST.md`. It lists my USB id, transaction id, max
>    DPI, RGB zones and polling family. If it's listed, you need no packet capture — but
>    check each value against the OpenRazer source before using it.
> 2. Copy the closest file in `crates/razer-proto/src/devices/`, change those values, set
>    `diagram: None`, register it in `SUPPORTED`, and add a row to
>    `docs/SUPPORTED-DEVICES.md`.
> 3. Run `cargo test --workspace`.
> 4. Follow the rules in `AGENTS.md`: only send commands read out of OpenRazer, read before
>    you write, and if anything reads back unexpected, stop and tell me rather than trying
>    something else.
>
> Then tell me what to run to check it against the real mouse.

[`AGENTS.md`](../AGENTS.md) is the full brief — every agent reads it, and it is where the
safety rules and the acceptance commands live.

## Reading the table

Each column is the answer to one field of the `DeviceSpec` you're going to write, taken
straight from OpenRazer:

- **USB id** — `VID:PID`. This is the whole identity of the device; the receiver and the
  wired body of the same wireless mouse have **different PIDs** and are separate rows,
  because they're separate `DeviceSpec`s.
- **txn** — the `transaction_id` byte, from the per-device switch in
  `razer_attr_write_dpi()`. `copy Elite` (`0x3F`) and `copy V3` (`0x1F`) mean a device
  Snakecharmer has already spoken that dialect to; `new txn path` (`0xFF`) is the legacy
  broadcast id, which works fine in OpenRazer but this daemon has never sent — expect to
  verify it more carefully. `legacy byte-DPI` devices take DPI as single bytes through a
  different command entirely and are the least like anything here.
- **Max DPI** — `DPI_MAX` from OpenRazer's daemon device class. The minimum is 100 on every
  modern model, but check yours; the settings UI has to stay usable across the whole range.
- **RGB zones** — which lighting zones the reference implements. `—` means no lighting, and
  lighting commands become clean no-ops (like the V3).
- **Polling** — which command family the device speaks: `Classic`
  (`razer_chroma_misc_set_polling_rate`, tops out at 1000 Hz) or `Extended` (`..._rate2`,
  reaches 8000 Hz).

Two fields you'll notice are *not* here, because no reference implementation has them:

- **`dpi_buttons`** — the vendor codes the wheel DPI buttons emit in driver mode. You read
  these off your own hardware with the listener; the Elite's are `0x20`/`0x21`.
- **`diagram`** — the button map. Optional: `None` ships, and the settings window falls
  back to labeled control rows. Draw it later, or let someone else.

The last column tracks status rather than hardware: `open`, 🔧 claimed by whoever took
it (linked to their issue), or ✅ cracked by whoever landed it, with their PR and the model
they drove.

**These cells are a starting hint, not a spec.** Read the OpenRazer source for your PID
before you write anything — the driver has real asymmetries (the Viper V3 Pro's wired PID
takes `Classic` polling while its wireless PID takes `Extended`), and a hint copied without
checking is exactly the kind of thing the golden rule is about.

<!-- BEGIN GENERATED TABLES -->

### DeathAdder

| Model | USB id | txn | Max DPI | RGB zones | Polling | Status |
|---|---|---|---|---|---|---|
| DeathAdder 3.5G | `1532:0016` | —<br><sub>legacy byte-DPI</sub> | 3500 | — | —<br><sub>own command</sub> | open |
| DeathAdder 3.5G Black | `1532:0029` | —<br><sub>legacy byte-DPI</sub> | 3500 | — | —<br><sub>own command</sub> | open |
| DeathAdder 2013 | `1532:0037` | —<br><sub>legacy byte-DPI</sub> | 6400 | logo, wheel | Classic | open |
| DeathAdder 1800 | `1532:0038` | —<br><sub>legacy byte-DPI</sub> | 1800 | — | Classic | open |
| DeathAdder Chroma | `1532:0043` | `0xFF`<br><sub>new txn path</sub> | 10000 | logo, wheel | Classic | open |
| DeathAdder 2000 | `1532:004F` | `0xFF`<br><sub>new txn path</sub> | 2000 | — | Classic | open |
| DeathAdder 3500 | `1532:0054` | `0xFF`<br><sub>new txn path</sub> | 3500 | logo, wheel | Classic | open |
| DeathAdder Elite | `1532:005C` | `0x3F`<br><sub>copy Elite</sub> | 16000 | logo, wheel | Classic | ✅ cracked by [@asavs](https://github.com/asavs)<br><sub>Claude Fable 5</sub> |
| DeathAdder Essential | `1532:006E` | `0xFF`<br><sub>new txn path</sub> | 6400 | logo, wheel | Classic | open |
| DeathAdder Essential (White Edition) | `1532:0071` | `0xFF`<br><sub>new txn path</sub> | 6400 | logo, wheel | Classic | open |
| DeathAdder V2 Pro (Wired) | `1532:007C` | `0x3F`<br><sub>copy Elite</sub> | 20000 | logo | Classic | open |
| DeathAdder V2 Pro (Wireless) | `1532:007D` | `0x3F`<br><sub>copy Elite</sub> | 20000 | logo | Classic | open |
| DeathAdder V2 | `1532:0084` | `0x3F`<br><sub>copy Elite</sub> | 20000 | logo, wheel | Classic | open |
| DeathAdder V2 Mini | `1532:008C` | `0x3F`<br><sub>copy Elite</sub> | 8500 | logo | Classic | open |
| DeathAdder Essential (2021) | `1532:0098` | `0xFF`<br><sub>new txn path</sub> | 6400 | logo | Classic | open |
| DeathAdder V2 X HyperSpeed | `1532:009C` | `0x1F`<br><sub>copy V3</sub> | 14000 | — | Classic | open |
| DeathAdder V2 Lite | `1532:00A1` | `0x1F`<br><sub>copy V3</sub> | 8500 | logo | Classic | open |
| DeathAdder V3 | `1532:00B2` | `0x1F`<br><sub>copy V3</sub> | 30000 | — | Extended<br><sub>→ 8000 Hz</sub> | ✅ cracked by [@asavs](https://github.com/asavs) · [#3](https://github.com/asavs/snakecharmer/pull/3)<br><sub>Claude Opus 4.8 (high)</sub> |
| DeathAdder V3 Pro (Wired) | `1532:00B6` | `0x1F`<br><sub>copy V3</sub> | 35000 | — | Classic | open |
| DeathAdder V3 Pro (Wireless) | `1532:00B7` | `0x1F`<br><sub>copy V3</sub> | 35000 | — | Classic | open |
| DeathAdder V4 Pro (Wired) | `1532:00BE` | `0x1F`<br><sub>copy V3</sub> | 45000 | — | Extended<br><sub>→ 8000 Hz</sub> | open |
| DeathAdder V4 Pro (Wireless) | `1532:00BF` | `0x1F`<br><sub>copy V3</sub> | 45000 | — | Extended<br><sub>→ 8000 Hz</sub> | open |
| DeathAdder V3 Pro (Wired) | `1532:00C2` | `0x1F`<br><sub>copy V3</sub> | 35000 | — | Classic | open |
| DeathAdder V3 Pro (Wireless) | `1532:00C3` | `0x1F`<br><sub>copy V3</sub> | 35000 | — | Classic | open |
| DeathAdder V3 HyperSpeed (Wired) | `1532:00C4` | `0x1F`<br><sub>copy V3</sub> | 26000 | — | Classic | open |
| DeathAdder V3 HyperSpeed (Wireless) | `1532:00C5` | `0x1F`<br><sub>copy V3</sub> | 26000 | — | Classic | open |

### Viper

| Model | USB id | txn | Max DPI | RGB zones | Polling | Status |
|---|---|---|---|---|---|---|
| Viper | `1532:0078` | `0xFF`<br><sub>new txn path</sub> | 16000 | logo | Classic | open |
| Viper Ultimate (Wired) | `1532:007A` | `0xFF`<br><sub>new txn path</sub> | 20000 | logo | Classic | open |
| Viper Ultimate (Wireless) | `1532:007B` | `0xFF`<br><sub>new txn path</sub> | 20000 | logo | Classic | open |
| Viper Mini | `1532:008A` | `0xFF`<br><sub>new txn path</sub> | 8500 | logo | Classic | open |
| Viper 8KHz | `1532:0091` | `0xFF`<br><sub>new txn path</sub> | 20000 | logo | Extended<br><sub>→ 8000 Hz</sub> | open |
| Viper Mini SE (Wired) | `1532:009E` | `0x1F`<br><sub>copy V3</sub> | 30000 | — | Extended<br><sub>→ 8000 Hz</sub> | open |
| Viper Mini SE (Wireless) | `1532:009F` | `0x1F`<br><sub>copy V3</sub> | 30000 | — | Extended<br><sub>→ 8000 Hz</sub> | open |
| Viper V2 Pro (Wired) | `1532:00A5` | `0x1F`<br><sub>copy V3</sub> | 30000 | — | Classic | open |
| Viper V2 Pro (Wireless) | `1532:00A6` | `0x1F`<br><sub>copy V3</sub> | 30000 | — | Classic | open |
| Viper V3 HyperSpeed | `1532:00B8` | `0x1F`<br><sub>copy V3</sub> | 30000 | — | Classic | open |
| Viper V3 Pro (Wired) | `1532:00C0` | `0x1F`<br><sub>copy V3</sub> | 35000 | — | Classic | open |
| Viper V3 Pro (Wireless) | `1532:00C1` | `0x1F`<br><sub>copy V3</sub> | 35000 | — | Extended<br><sub>→ 8000 Hz</sub> | open |

### Basilisk

| Model | USB id | txn | Max DPI | RGB zones | Polling | Status |
|---|---|---|---|---|---|---|
| Basilisk | `1532:0064` | `0x3F`<br><sub>copy Elite</sub> | 16000 | logo, wheel | Classic | open |
| Basilisk Essential | `1532:0065` | `0x3F`<br><sub>copy Elite</sub> | 6400 | logo | Classic | open |
| Basilisk X HyperSpeed | `1532:0083` | `0xFF`<br><sub>new txn path</sub> | 16000 | — | Classic | open |
| Basilisk V2 | `1532:0085` | `0x1F`<br><sub>copy V3</sub> | 20000 | logo, wheel | Classic | open |
| Basilisk Ultimate | `1532:0086` | `0x1F`<br><sub>copy V3</sub> | 20000 | logo, wheel, left, right | Classic | open |
| Basilisk Ultimate (Receiver) | `1532:0088` | `0x1F`<br><sub>copy V3</sub> | 20000 | logo, wheel, left, right | Classic | open |
| Basilisk V3 | `1532:0099` | `0x1F`<br><sub>copy V3</sub> | 26000 | logo, wheel | Classic | open |
| Basilisk V3 Pro (Wired) | `1532:00AA` | `0x1F`<br><sub>copy V3</sub> | 30000 | logo, wheel | Classic | open |
| Basilisk V3 Pro (Wireless) | `1532:00AB` | `0x1F`<br><sub>copy V3</sub> | 30000 | logo, wheel | Classic | open |
| Basilisk V3 X HyperSpeed | `1532:00B9` | `0x1F`<br><sub>copy V3</sub> | 18000 | wheel | Classic | open |
| Basilisk V3 35K | `1532:00CB` | `0x1F`<br><sub>copy V3</sub> | 35000 | logo, wheel | Classic | open |
| Basilisk V3 Pro 35K (Wired) | `1532:00CC` | `0x1F`<br><sub>copy V3</sub> | 35000 | logo, wheel | Classic | open |
| Basilisk V3 Pro 35K (Wireless) | `1532:00CD` | `0x1F`<br><sub>copy V3</sub> | 35000 | logo, wheel | Classic | open |
| Basilisk Mobile (Wired) | `1532:00D3` | `0x1F`<br><sub>copy V3</sub> | 18000 | — | Classic | open |
| Basilisk Mobile (Receiver) | `1532:00D4` | `0x1F`<br><sub>copy V3</sub> | 18000 | — | Classic | open |
| Basilisk V3 Pro 35K Phantom Green Edition (Wired) | `1532:00D6` | `0x1F`<br><sub>copy V3</sub> | 35000 | wheel | Classic | open |
| Basilisk V3 Pro 35K Phantom Green Edition (Wireless) | `1532:00D7` | `0x1F`<br><sub>copy V3</sub> | 35000 | wheel | Classic | open |

### Naga

| Model | USB id | txn | Max DPI | RGB zones | Polling | Status |
|---|---|---|---|---|---|---|
| Naga | `1532:0015` | —<br><sub>legacy byte-DPI</sub> | 5600 | — | Classic | open |
| Naga Epic | `1532:001F` | —<br><sub>legacy byte-DPI</sub> | 5600 | wheel | Classic | open |
| Naga 2012 | `1532:002E` | —<br><sub>legacy byte-DPI</sub> | 5600 | — | Classic | open |
| Naga Hex (Red) | `1532:0036` | —<br><sub>legacy byte-DPI</sub> | 5600 | — | Classic | open |
| Naga Epic Chroma (Wired) | `1532:003E` | `0xFF`<br><sub>new txn path</sub> | 8200 | wheel, backlight | Classic | open |
| Naga Epic Chroma (Wireless) | `1532:003F` | `0xFF`<br><sub>new txn path</sub> | 8200 | wheel, backlight | Classic | open |
| Naga 2014 | `1532:0040` | `0xFF`<br><sub>new txn path</sub> | 8200 | — | Classic | open |
| Naga Hex | `1532:0041` | —<br><sub>legacy byte-DPI</sub> | 5600 | — | Classic | open |
| Naga Hex V2 | `1532:0050` | `0x3F`<br><sub>copy Elite</sub> | 16000 | logo, wheel, backlight | Classic | open |
| Naga Chroma | `1532:0053` | `0xFF`<br><sub>new txn path</sub> | 16000 | logo, wheel, backlight | Classic | open |
| Naga Trinity | `1532:0067` | `0xFF`<br><sub>new txn path</sub> | 16000 | — | Classic | open |
| Naga Left Handed Edition 2020 | `1532:008D` | `0x1F`<br><sub>copy V3</sub> | 20000 | logo, wheel, right | Classic | open |
| Naga Pro (Wired) | `1532:008F` | `0x1F`<br><sub>copy V3</sub> | 20000 | logo, wheel | Classic | open |
| Naga Pro (Wireless) | `1532:0090` | `0x1F`<br><sub>copy V3</sub> | 20000 | logo, wheel | Classic | open |
| Naga X | `1532:0096` | `0x1F`<br><sub>copy V3</sub> | 18000 | wheel, left | Classic | open |
| Naga V2 Pro (Wired) | `1532:00A7` | `0x1F`<br><sub>copy V3</sub> | 30000 | logo | Classic | open |
| Naga V2 Pro (Wireless) | `1532:00A8` | `0x1F`<br><sub>copy V3</sub> | 30000 | logo | Classic | open |
| Naga V2 HyperSpeed (Receiver) | `1532:00B4` | `0x1F`<br><sub>copy V3</sub> | 30000 | — | Classic | open |

### Cobra

| Model | USB id | txn | Max DPI | RGB zones | Polling | Status |
|---|---|---|---|---|---|---|
| Cobra | `1532:00A3` | `0xFF`<br><sub>new txn path</sub> | 8500 | logo | Classic | open |
| Cobra Pro (Wired) | `1532:00AF` | `0x1F`<br><sub>copy V3</sub> | 30000 | logo, wheel | Classic | open |
| Cobra Pro (Wireless) | `1532:00B0` | `0x1F`<br><sub>copy V3</sub> | 30000 | logo, wheel | Classic | open |

### Mamba

| Model | USB id | txn | Max DPI | RGB zones | Polling | Status |
|---|---|---|---|---|---|---|
| Mamba 2012 (Wired) | `1532:0024` | `0xFF`<br><sub>new txn path</sub> | 6400 | wheel | Classic | open |
| Mamba 2012 (Wireless) | `1532:0025` | `0xFF`<br><sub>new txn path</sub> | 6400 | wheel | Classic | open |
| Mamba Chroma (Wired) | `1532:0044` | `0xFF`<br><sub>new txn path</sub> | 16000 | backlight | Classic | open |
| Mamba Chroma (Wireless) | `1532:0045` | `0xFF`<br><sub>new txn path</sub> | 16000 | backlight | Classic | open |
| Mamba Tournament Edition | `1532:0046` | `0xFF`<br><sub>new txn path</sub> | 16000 | backlight | — | open |
| Mamba Elite | `1532:006C` | `0x1F`<br><sub>copy V3</sub> | 16000 | logo, wheel, left, right | Classic | open |
| Mamba Wireless (Receiver) | `1532:0072` | `0x3F`<br><sub>copy Elite</sub> | 16000 | logo, wheel | Classic | open |
| Mamba Wireless (Wired) | `1532:0073` | `0x3F`<br><sub>copy Elite</sub> | 16000 | logo, wheel | Classic | open |

### Lancehead

| Model | USB id | txn | Max DPI | RGB zones | Polling | Status |
|---|---|---|---|---|---|---|
| Lancehead (Wired) | `1532:0059` | `0x3F`<br><sub>copy Elite</sub> | 16000 | logo, wheel, left, right | Classic | open |
| Lancehead (Wireless) | `1532:005A` | `0x3F`<br><sub>copy Elite</sub> | 16000 | logo, wheel, left, right | Classic | open |
| Lancehead Tournament Edition | `1532:0060` | `0x3F`<br><sub>copy Elite</sub> | 16000 | logo, wheel, left, right | Classic | open |
| Lancehead Wireless (Receiver) | `1532:006F` | `0x3F`<br><sub>copy Elite</sub> | 16000 | logo, wheel, left, right | Classic | open |
| Lancehead Wireless (Wired) | `1532:0070` | `0x3F`<br><sub>copy Elite</sub> | 16000 | logo, wheel, left, right | Classic | open |

### Orochi

| Model | USB id | txn | Max DPI | RGB zones | Polling | Status |
|---|---|---|---|---|---|---|
| Orochi 2011 | `1532:0013` | —<br><sub>legacy byte-DPI</sub> | 4000 | — | —<br><sub>own command</sub> | open |
| Orochi 2013 | `1532:0039` | `0xFF`<br><sub>new txn path</sub> | 6400 | — | Classic | open |
| Orochi (Wired) | `1532:0048` | `0xFF`<br><sub>new txn path</sub> | 8200 | backlight | Classic | open |
| Orochi V2 (Receiver) | `1532:0094` | `0x1F`<br><sub>copy V3</sub> | 18000 | — | Classic | open |
| Orochi V2 (Bluetooth) | `1532:0095` | `0x1F`<br><sub>copy V3</sub> | 18000 | — | Classic | open |

### Abyssus

| Model | USB id | txn | Max DPI | RGB zones | Polling | Status |
|---|---|---|---|---|---|---|
| Abyssus 1800 | `1532:0020` | —<br><sub>legacy byte-DPI</sub> | 1800 | — | Classic | open |
| Abyssus | `1532:0042` | —<br><sub>legacy byte-DPI</sub> | ? | — | Classic | open |
| Abyssus V2 | `1532:005B` | `0xFF`<br><sub>new txn path</sub> | 5000 | logo, wheel | Classic | open |
| Abyssus 2000 | `1532:005E` | `0xFF`<br><sub>new txn path</sub> | 2000 | — | Classic | open |
| Abyssus Elite (D.Va Edition) | `1532:006A` | `0xFF`<br><sub>new txn path</sub> | 7200 | logo | Classic | open |
| Abyssus Essential | `1532:006B` | `0xFF`<br><sub>new txn path</sub> | 7200 | logo | Classic | open |

### Pro Click

| Model | USB id | txn | Max DPI | RGB zones | Polling | Status |
|---|---|---|---|---|---|---|
| Pro Click (Receiver) | `1532:0077` | `0x1F`<br><sub>copy V3</sub> | 16000 | — | Classic | open |
| Pro Click (Wired) | `1532:0080` | `0x1F`<br><sub>copy V3</sub> | 16000 | — | Classic | open |
| Pro Click Mini (Receiver) | `1532:009A` | `0x1F`<br><sub>copy V3</sub> | 12000 | — | Classic | open |
| Pro Click V2 Vertical Edition (Wired) | `1532:00C7` | `0x1F`<br><sub>copy V3</sub> | 30000 | — | Classic | open |
| Pro Click V2 Vertical Edition (Wireless) | `1532:00C8` | `0x1F`<br><sub>copy V3</sub> | 30000 | — | Classic | open |
| Pro Click V2 (Wired) | `1532:00D0` | `0x1F`<br><sub>copy V3</sub> | 30000 | — | Classic | open |
| Pro Click V2 (Wireless) | `1532:00D1` | `0x1F`<br><sub>copy V3</sub> | 30000 | — | Classic | open |

### Atheris

| Model | USB id | txn | Max DPI | RGB zones | Polling | Status |
|---|---|---|---|---|---|---|
| Atheris (Receiver) | `1532:0062` | `0x1F`<br><sub>copy V3</sub> | 7200 | — | Classic | open |

### Diamondback

| Model | USB id | txn | Max DPI | RGB zones | Polling | Status |
|---|---|---|---|---|---|---|
| Diamondback Chroma | `1532:004C` | `0xFF`<br><sub>new txn path</sub> | 16000 | backlight | — | open |

### Imperator

| Model | USB id | txn | Max DPI | RGB zones | Polling | Status |
|---|---|---|---|---|---|---|
| Imperator 2012 | `1532:002F` | `0xFF`<br><sub>new txn path</sub> | 6400 | — | Classic | open |

### Ouroboros

| Model | USB id | txn | Max DPI | RGB zones | Polling | Status |
|---|---|---|---|---|---|---|
| Ouroboros | `1532:0032` | `0xFF`<br><sub>new txn path</sub> | 8200 | — | Classic | open |

### Taipan

| Model | USB id | txn | Max DPI | RGB zones | Polling | Status |
|---|---|---|---|---|---|---|
| Taipan | `1532:0034` | `0xFF`<br><sub>new txn path</sub> | 8200 | — | Classic | open |

### Dongles

| Model | USB id | txn | Max DPI | RGB zones | Polling | Status |
|---|---|---|---|---|---|---|
| HyperPolling Wireless Dongle | `1532:00B3` | `0x1F`<br><sub>copy V3</sub> | 30000 | — | Extended<br><sub>→ 8000 Hz</sub> | open |
<!-- END GENERATED TABLES -->

## Not in the table?

Then OpenRazer doesn't know it either, and the golden rule leaves you one honest route: a
USB capture of Razer's own software talking to your device (Wireshark + USBPcap, guide §5),
documented in the PR. Never guessed, never fuzzed. If you go to that trouble, consider
sending the findings to OpenRazer too — everything on this page exists because someone did.

Keyboards, headsets, and mats are out of scope for this repo today. The protocol layer isn't
mouse-specific, so it's a "nobody has done it" rather than a "can't", but it's a bigger diff
than the one-file kind described above.

## Onboard memory, and why it doesn't change the crack

Razer mice fall into three storage tiers: no onboard memory (settings live only while the
host software runs — DeathAdder Essential, Abyssus), a single onboard profile that mirrors
whatever the software last set (most modern esports models — Viper V3 Pro, DeathAdder V3
Pro, Orochi V2, Cobra), and five profiles in a `4+1` bank with a physical switch to cycle
them (Basilisk V2/V3 Pro, Naga Pro / V2 Pro, Naga Left-Handed, Lancehead Wireless).

Snakecharmer writes the **active state** (`NOSTORE`), the same as OpenRazer does for these
devices — it doesn't write profile banks and doesn't read them back. So the tier doesn't
change what you implement. It changes what a user sees when the daemon *isn't* running: on a
zero-memory mouse everything reverts, on the others the last-applied DPI and polling rate
survive in hardware. Worth a sentence in your PR either way.

*(Tier assignments here are from Razer's published product documentation, not from a
protocol source. Unlike everything in the table above, they are not verified against
OpenRazer or against hardware.)*

## Provenance

The tables are generated from OpenRazer `master` — `driver/razermouse_driver.h` (PIDs),
`driver/razermouse_driver.c` (transaction ids, polling families), and
`daemon/openrazer_daemon/hardware/mouse.py` (model names, `DPI_MAX`, lighting zones) — by
[`tools/wishlist-from-openrazer.py`](../tools/wishlist-from-openrazer.py). Rerun it to pick
up devices OpenRazer has added since:

```powershell
python tools/wishlist-from-openrazer.py
```

Shipped and claimed rows are recorded in the `CLAIMED` table at the top of that script, so
regenerating never clobbers credit. Last generated: 2026-08-23.
