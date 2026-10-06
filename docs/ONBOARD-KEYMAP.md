# On-board keymap: research

**Goal:** learn the bytes Synapse writes to a mouse's on-board memory when it saves a
button bind, so Snakecharmer can write them itself and users can configure on-board binds
without Synapse.

**Status: capture stage.** The format is being decoded from captures of a DeathAdder V3
and a DeathAdder Elite. Nothing here ships in the daemon yet, and none of the tools on this
page write to a mouse. See [Findings](#findings).

## The problem

Many Razer mice store button binds in on-board memory. When Synapse saves a remap there,
the mouse itself sends the remapped input: a thumb button bound to `0` makes the mouse type
`0` through its own keyboard interface. No software is involved, so **uninstalling Synapse
does not remove it**, and Snakecharmer can't either. The thumb button never sends
`XBUTTON1`/`XBUTTON2`, so even Snakecharmer's thumb remap hook never sees it.

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
- **It costs nothing at runtime.** A bind stored on the mouse runs in the mouse's firmware:
  no hook, no daemon, no per-event check. By the standard in [`SPEC.md`](SPEC.md) that
  makes it a better home for button remaps than the global hook.
- **It ends the migration trap.** Once Snakecharmer can write on-board binds for a mouse,
  nobody leaving Synapse has to reset anything in it first.

### On-board profiles are in scope

Snakecharmer has no *software* profiles on purpose: per-app profiles need a foreground
watcher running all the time, which is the overhead this project exists to avoid. Some mice
store several *on-board* profiles instead, and switch between them in firmware, often with
a button on the underside. That costs nothing at runtime either, so supporting them fits the
same rule. The capture session collects profile data whenever the mouse has more than one.

## Plan

1. **Capture and decode** (this page). Record Synapse making one change at a time, then
   diff the reports until every byte Snakecharmer needs is pinned down.
2. **Read.** If Synapse reads the keymap back during a session, Snakecharmer can do the
   same read: show the stored binds in its settings and warn about leftovers, writing
   nothing.
3. **Reset to defaults** by replaying exactly the bytes Synapse sent for each default.
4. **Write binds from the config**, as an option alongside the hook-based thumb remap, and
   on-board profiles for mice that have them.

Steps 3 and 4 write to persistent memory, unlike DPI, which Snakecharmer only ever sets
live. They ship only after the bytes are confirmed on hardware by more than one capture.

After that, each new mouse needs no capture: its owner runs the usual
[hardware verification](../AGENTS.md#the-common-task-add-support-for-a-mouse), plus the
listener to confirm a bind took.

## When a capture is needed

Decoding happens once, on the maintainer's two mice. They are different generations
(transaction id `0x1F` and `0x3F`), so if both use the same format it very likely covers
the range. A capture from someone else is only needed when:

- **the mouse's format turns out to differ** from both decoded ones, or
- **the mouse has more than one on-board profile.** Neither decoded mouse does, so the
  profile byte needs one capture from a multi-profile mouse, once.

Anyone else can stop here. The rest of this page is the method for those cases.

## What a capture has to answer

The session's steps come from what Snakecharmer's config will need to express:

| Snakecharmer needs to write | Learned from |
|---|---|
| which button | back (01) vs forward (07); wheel click (09) |
| a key | 01 → 02 → 03: key code, and whether it's a HID usage or a Windows virtual key |
| a modifier combo (the config's `copy` is `Ctrl+C`) | 03 → 04 |
| a mouse function | 05, back → middle click |
| disabled | 06, if Synapse offers it |
| default | 08–10, which a reset replays |
| which on-board profile | 11–14, only on multi-profile mice |
| the current binds (to display them) | any GET responses that change between steps |

## Running a capture session

For the maintainer's decode, or one of the cases above. Hand it to a coding agent if you
like; see [For agents](#for-agents). Either way, a person has to do the clicking.

You need the mouse, Synapse, Python 3 (standard library only) and an administrator
PowerShell. About 20 minutes per mouse.

**Privacy:** the raw `.pcap` files record all USB traffic during the session, including
your keyboard. Keep them local. The session writes a `report.md` containing only the
Razer control reports, which is the file to share.

### 1. Install

```powershell
winget install desowin.USBPcap      # then reboot; the capture driver loads at boot
```

Install Synapse if it isn't already, and find its version (Help/About). The session asks
for it, and for how many on-board profiles Synapse offers for the mouse (1 if there's no
profile picker).

### 2. Check the starting state

With Synapse **quit** (tray icon → Exit), run the listener and press each button. Keep the
output: it shows what the mouse sent before the session.

```powershell
cd reference
.\button_listener.ps1 -Seconds 30
```

The session leaves the buttons at their defaults. If you rely on your current binds, note
them so you can set them again afterwards.

### 3. Record

Open Synapse on the mouse's button page, then in an administrator PowerShell:

```powershell
cd reference
.\capture_session.ps1 -Mouse V3      # any short name: Elite, NagaV2Pro, ...
```

It captures every USB root hub and walks you through these steps, recording a time window
for each:

| Step | Do this in Synapse |
|---|---|
| 00 | nothing for 15 s (background chatter, filtered out later) |
| 01 | back thumb → key `1` |
| 02 | back thumb → key `2` |
| 03 | back thumb → key `A` |
| 04 | back thumb → `Ctrl` + `C` |
| 05 | back thumb → middle click |
| 06 | back thumb → disabled |
| 07 | forward thumb → key `1` |
| 08 | forward thumb → default |
| 09 | wheel click → default |
| 10 | back thumb → default |
| 11 | *multi-profile only:* switch the active on-board profile to 2 |
| 12 | *multi-profile only:* in profile 2, back thumb → key `1` |
| 13 | *multi-profile only:* in profile 2, back thumb → default |
| 14 | *multi-profile only:* switch the active profile back to 1 |
| 99 | nothing for 8 s (trailing writes) |

After each change, make sure Synapse has saved it to on-board memory, then press Enter. If
a step isn't possible (no disable option, say), type a short note instead; it's stored with
that step and printed in the report.

### 4. Verify and decode

Quit Synapse, then run `.\button_listener.ps1` again. All three buttons should now send
`XBUTTON1`, `XBUTTON2` and `MIDDLE`. If they still send keys, Synapse kept the changes in
software and never wrote them to the mouse, so the capture won't contain them either: say
so in the submission.

`capture_session.ps1` runs the decoder when it finishes. To run it again:

```powershell
python keymap_diff.py $HOME\snakecharmer-captures\V3-<timestamp>
python keymap_diff.py <session> --all    # include commands that also appear while idle
```

For each step it lists the commands Synapse sent that never occurred while idle, then the
byte offsets that changed between steps for the same command. The decode lives in those
diffs: if 01 → 02 changes one byte, that's the key code.

### 5. Submit

Post `report.md` and both listener outputs as a comment on your mouse's device issue, or
open one with the
[add-a-mouse form](https://github.com/asavs/snakecharmer/issues/new?template=device-request.yml).
Never attach the raw `.pcap` files. If you've decoded it, add your mouse to
[Findings](#findings) in a PR.

## For agents

The capture is a good job to hand a coding agent, with one boundary: **the agent drives,
the person clicks.** Synapse, the admin shell and the button presses need a human at the
mouse.

What an agent can do:

- Walk the person through sections 1–5 above, one step at a time.
- Run `python reference/keymap_diff.py --self-test` before the session, to confirm the
  decoder works on this machine.
- Read `report.md` and the listener output, and work out which bytes encode the button, key,
  modifier, function and profile, using the table in
  [What a capture has to answer](#what-a-capture-has-to-answer).
- Write the result into [Findings](#findings) using the template there, and post it on
  the mouse's device issue or open a PR.

What an agent must not do, on top of the rules in [`AGENTS.md`](../AGENTS.md):

- **Send any keymap command to a mouse.** Not to test a hypothesis, not as a "read". Until
  this page records a decoded, confirmed format and the maintainer has approved a write
  path, keymap bytes are reference data, not commands.
- Ask for, upload, or commit the raw `.pcap` files. They contain the person's keyboard
  input. `report.md` is the shareable artifact.
- Present a guess as a finding. If two hypotheses fit the diffs, record both and say which
  extra step would tell them apart.

## Findings

*None yet.* Add one block per session:

```markdown
### <Mouse> (`1532:____`), Synapse <version>, <n> on-board profile(s)

- **Write command:** class 0x__, id 0x__, transaction id 0x__
- **Argument layout:** byte [n] = …, byte [n] = …
- **Key encoding:** HID usage / Windows virtual key (evidence: step 02 → 03)
- **Modifiers:** …
- **Mouse functions / disabled / default:** …
- **Profile byte:** … (or: single-profile mouse)
- **Read-back:** a GET that returns the keymap? class/id, or "none seen"
- **Confirmed by:** listener before/after, report link
- **Open questions:** …
```
