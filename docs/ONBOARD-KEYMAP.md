# On-board keymap: research

**Status: capture stage.** Nothing here ships in the daemon yet, and none of the tools below
write to a mouse.

## The problem

Many Razer mice store one button profile in on-board memory. When Synapse saves a remap
there, the mouse itself sends the remapped input: a thumb button bound to `0` makes the
mouse type `0` through its own keyboard interface. No software is involved, so
**uninstalling Synapse does not remove it**, and Snakecharmer can't either. The thumb
button never sends `XBUTTON1`/`XBUTTON2`, so even Snakecharmer's thumb remap hook never
sees it.

The first case found, on a DeathAdder V3 (`1532:00B2`) after Synapse was uninstalled:

| Pressed | Sent | Through |
|---|---|---|
| back thumb | key `0` | interface 2 (the mouse's keyboard collection) |
| forward thumb | key `9` | interface 2 |
| wheel click | key `8` | interface 2 |
| scroll, left, right | normal mouse input | interface 0 |

[`reference/button_listener.ps1`](../reference/button_listener.ps1) prints this table for
any mouse. It's the quickest way to tell whether a mouse has leftover binds.

## Why it's worth decoding

- **No open-source reference has it.** OpenRazer never implemented key remapping, so this
  is the one feature that can't be ported. It has to be captured from Synapse, which
  [`AGENTS.md`](../AGENTS.md) rule 1 allows.
- **It costs nothing at runtime.** A remap stored on the mouse runs in the mouse's
  firmware: no hook, no daemon, no per-event check. That makes it a better home for button
  remaps than the global hook, by the standard in [`SPEC.md`](SPEC.md).
- **It fixes the migration trap.** Everyone leaving Synapse keeps whatever it last saved.

## Plan

1. **Capture and decode** (this page). Record Synapse making one change at a time, then
   diff the reports until the button id, key code and modifier bytes are pinned down.
2. **Detect.** If Synapse reads the keymap back, Snakecharmer can do the same read and warn
   about leftover binds without writing anything.
3. **Reset to defaults** by replaying exactly the bytes Synapse sent for each default.
4. **Remap on the mouse**, as a config option alongside the hook-based remap.

Steps 3 and 4 write to persistent memory, unlike DPI, which Snakecharmer only ever sets
live. They only ship once the bytes are confirmed on more than one mouse.

## Running a capture session

You need the mouse, Synapse, Python 3 (standard library only) and an administrator
PowerShell. About 20 minutes per mouse.

**Privacy:** the raw `.pcap` files record all USB traffic during the session, including
your keyboard. Keep them local. `keymap_diff.py` writes a `report.md` containing only the
Razer control reports, which is the file to share.

### 1. Install

```powershell
winget install desowin.USBPcap      # then reboot; the capture driver loads at boot
```

Install Synapse if it isn't already, and note its version: the session log is more useful
with it.

### 2. Check the starting state

With Synapse **quit** (tray icon, Exit), run the listener and press each button:

```powershell
cd reference
.\button_listener.ps1 -Seconds 30
```

### 3. Record

Open Synapse on the mouse's button page, then in an administrator PowerShell:

```powershell
cd reference
.\capture_session.ps1 -Mouse V3      # or -Mouse Elite
```

The script captures every USB root hub and walks you through these steps, recording a
time window for each:

| Step | Do this in Synapse | Isolates |
|---|---|---|
| 00 | nothing for 15 s | background chatter, so it can be filtered out |
| 01 | back thumb → key `1` | |
| 02 | back thumb → key `2` | the key-code byte (vs 01) |
| 03 | back thumb → key `A` | HID usage or Windows virtual-key encoding (vs 02) |
| 04 | back thumb → `Shift` + `A` | the modifier byte (vs 03) |
| 05 | forward thumb → key `1` | the button-id byte (vs 01) |
| 06 | forward thumb → default | the "default" encoding |
| 07 | wheel click → default | a third button id |
| 08 | back thumb → default | |
| 09 | nothing for 8 s | trailing writes |

After each change, make sure Synapse has saved it to on-board memory, then press Enter.
If a step isn't possible (no Shift option, say), type a short note instead of just Enter;
it's stored with that step.

The session ends with every button back at its default. **This is also the cleanup** for
a mouse carrying leftover binds.

### 4. Verify and decode

Quit Synapse, then run `.\button_listener.ps1` again. All three buttons should now send
`XBUTTON1`, `XBUTTON2` and `MIDDLE`. If they still send keys, Synapse kept the changes in
software and never wrote them to the mouse, and the capture won't contain them either.

`capture_session.ps1` runs the decoder when it finishes. To run it again:

```powershell
python keymap_diff.py $HOME\snakecharmer-captures\V3-<timestamp>
python keymap_diff.py <session> --all    # include commands that also appear while idle
```

For each step it lists the commands Synapse sent that never occurred while idle, then the
byte offsets that changed between steps for the same command. The decode lives in those
diffs: if 01 → 02 changes one byte, that's the key code.

## Findings

*None yet. Record each session here: mouse, Synapse version, the command class and id,
and what each argument byte turned out to mean.*
