# On-board keymap: research

**Goal:** learn the bytes Synapse writes to a mouse's on-board memory when it saves a
button bind, so Snakecharmer can write them itself and users can configure on-board binds
without Synapse.

**Status: on the V3, read, reset and write from the config are implemented and verified
on hardware. The Elite was tested with Synapse 4 and needs none of it.** The DeathAdder V3's
write (`0x02/0x0C`) and read (`0x02/0x8C`) are recorded under [Findings](#findings).
`charmctl keymap` shows the stored binds, `charmctl keymap reset` puts non-default binds
back to default, `charmctl keymap set <button> <bind>` writes one, and the config's
`[onboard]` table is written to the mouse on attach. The maintainer approved the V3 write
path on 2026-10-16. Writes cover the six controls in `KeymapSpec::writable` (right, wheel
click, back, forward, wheel up, wheel down) and replay the layouts Synapse was captured
sending. The first Snakecharmer write, with the physical check and the reset, is in
[`captures/V3-20261016-snakecharmer-write/notes.md`](captures/V3-20261016-snakecharmer-write/notes.md).
The Elite session recorded only streamed lighting while button assignments were
saved, which points to software remapping in this version. Synapse 4 remaps the Elite in
software, so it has no leftover on-board binds, and Snakecharmer's hook remap covers it.
A Synapse 3 comparison is still pending.

| V3 button | Captured read | Captured write to that button |
|---|---|---|
| Back, forward | Yes | Yes, including defaults |
| Right, wheel click, wheel up/down | Yes | Yes, key `1` and defaults (2026-10-16) |
| Left | Yes | No; left unchanged |
| Underside | Yes; category remains undecoded | No |

These captures can't reset every button: left and underside have no captured
write, so Snakecharmer never writes them. The original step 09 produced no
wheel-click write because its assignment was already Default. The additional pass
changed each of the four missing controls before restoring it, recording eight
writes with matching success replies. All eight used `[10] = 00`; that byte's
meaning in read replies remains unresolved. See the [additional V3 findings](#additional-v3-button-writes-2026-10-16).

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
2. **Read.** Done for the DeathAdder V3: `charmctl keymap` sends the read Synapse sends
   and prints each button's stored bind, flagging any that isn't the default. The daemon
   also reads on attach and shows a notice offering a reset when it finds leftover
   non-default binds that `[onboard]` doesn't cover. Showing binds in the settings window
   comes later.
3. **Reset to defaults** by replaying exactly the bytes Synapse sent for each default.
   Done for the V3's six writable buttons: `charmctl keymap reset`.
4. **Write binds from the config**, as an option alongside the hook-based thumb remap.
   Done for the V3: `charmctl keymap set` and the `[onboard]` table, written on attach
   only when different from what's stored and verified by read-back. Still open:
   on-board profiles for mice that have them.

Steps 3 and 4 write to persistent memory, unlike DPI, which Snakecharmer only ever sets
live. They shipped for the V3 after the bytes were confirmed by captures of Synapse and
by Snakecharmer's own write on hardware. Another mouse or button needs the same: a
capture of Synapse writing it, and maintainer approval.

### Open questions

- The V3 underside button's category `0x06` payload is undecoded, so it is read-only.
- The meaning of byte `[10]` in read replies (see [Findings](#findings)).
- The meaning of byte `[8]`, always `01` (possibly OpenRazer's VARSTORE).
- Multi-profile mice: the profile selector hasn't been captured, since the V3 has one.
- Left button: no captured write, so it is never written.
- Other mice: each needs its own capture. The Elite's Synapse 3 behavior is unchecked.

Recorded V3 commands can be reused in implementation and regression tests.
They do not establish keymap support for another model, even one in the same
protocol family. A device's keymap read/write allowlist must have its own
documented basis before commands are sent to it.

## When a capture is needed

The existing V3 captures supply evidence for its recorded controls and tested
assignments; contributors do not need to recapture those just to work on the
implementation. New evidence is needed for an unconfirmed mouse model, button,
function category, modifier encoding, or profile selector. Shared USB framing
and similar transaction ids do not prove shared binding behavior. In particular,
the Elite's Synapse 4 software remapping does not confirm a V3-style on-board
keymap on the Elite.

### What contributors need to repeat

| Work | Required hardware work | Synapse USB capture |
|---|---|---|
| Documentation, formatting, or pure parsing changes | Tests appropriate to the change | Usually none; reuse saved evidence |
| A mouse port using commands fully documented by OpenRazer | The usual [device verification](../AGENTS.md#the-common-task-add-support-for-a-mouse), including DPI read-back and normal physical input | None for those documented commands |
| Changes to device I/O or binding behavior on an already confirmed model | Check the affected behavior on hardware; writes need successful read-back and physical output checks | Reuse existing exact command evidence unless the change needs an unconfirmed form |
| Keymap support for another model or an unconfirmed V3 control/function/profile | Obtain the missing protocol evidence, then verify on that hardware; persistent behavior needs a power-cycle check | Targeted capture when no reference establishes the command; never assume compatibility and probe |

The [button listener](../reference/button_listener.ps1) is a passive behavioral
check. It needs a person to press physical buttons but does not need Synapse or
USBPcap. A vendor capture answers a different question: which commands Synapse
sends when saving a setting. Those command records can become reusable tests;
the physical check still matters when supporting another model or changing
device behavior. An empty listener log is inconclusive if no movement packets
arrived; it is not proof that a mouse emitted no keystrokes.

Ordinary users should not need to repeat this research for supported features.
On the DeathAdder V3, `charmctl keymap reset`, `charmctl keymap set` and the `[onboard]`
config table are the verified feature; use them directly. Another model needs the
capture described here first.

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

This is for the maintainer's decode, or one of the cases above. You can hand it to a
coding agent (see [For agents](#for-agents)), but a person has to do the clicking.

You need the mouse, Synapse, Python 3 (standard library only) and an administrator
PowerShell. About 20 minutes per mouse.

**Privacy:** the raw `.pcap` files record all USB traffic during the session, including
your keyboard. Keep them local. The session writes a `report.md` containing only the
Razer control reports, with the mouse's serial number masked, which is the file to share.

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

For the missing V3 button writes, use the separate additional pass:

```powershell
.\capture_session.ps1 -Mouse V3 -CaptureSet V3Additional
```

This pass records ten seconds idle, then wheel click, right click, wheel up,
and wheel down, each changed to key `1` and then restored to Default. Save
after every change. If a scroll assignment is unavailable, record a note for
that step rather than substituting another button. Left click and the underside
button are never changed. Step labels and `capture_set` in `markers.json`
distinguish this pass from the original session; both use step ids 01 onward.
The change before each default prevents an already-default button from
producing no write, as happened in the original V3 wheel-click step.

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

- **Send a keymap write to a mouse** other than through the shipped V3 path
  (`Mouse::set_button_binding`, buttons in `KeymapSpec::writable`), not even to test a
  hypothesis. Until this page records a confirmed format for that mouse and button and the
  maintainer has approved it, write bytes are reference data, not commands. Never write
  left or an undecoded category. A read recorded under [Findings](#findings) is fine, sent
  exactly as Synapse sent it; never sweep button ids or arguments.
- Ask for, upload, or commit the raw `.pcap` files. They contain the person's keyboard
  input. `report.md` is the shareable artifact.
- Present a guess as a finding. If two hypotheses fit the diffs, record both and say which
  extra step would tell them apart.

What is **not** a reason to stop: a status other than `0x02` in Synapse's own traffic.
`AGENTS.md` rule 3 governs commands *you* send. In a capture, a `0x05` (not supported)
reply is Synapse probing for a feature the mouse lacks, which is ordinary, and the
recording is passive either way. Note it as data and carry on.

When saving evidence to `docs/captures/`, write UTF-8. Windows PowerShell's `>` writes
UTF-16, which GitHub shows as unreadable text; pipe through `Out-File -Encoding utf8`
instead.

## Findings

### DeathAdder V3 (`1532:00B2`), Synapse 4, 1 on-board profile

Captured on 2026-10-06 using the Synapse UI (AppEngine 4.0.827; exact Synapse
version not checked). This records observed writes and their acknowledgements.
The final physical check confirmed default input with Razer AppEngine stopped
and hardware mode verified. These bytes are reference data, not an approved
keymap write path.

- **Write command:** class `0x02`, id `0x0C`, argument size 10, USB interface 0.
  Transaction ids incremented from `0x07` through `0x11`; this capture does not
  establish which transaction ids Snakecharmer should use.
- **Argument layout:** offsets below are zero-based offsets in the 90-byte report;
  arguments start at `[8]`.

  | Report offset | Observed meaning or value |
  |---|---|
  | `[8]` | Always `01`, in writes and reads; purpose unconfirmed (OpenRazer uses `0x01` as VARSTORE, persistent storage, which would fit) |
  | `[9]` | Button: `04` back, `05` forward (steps 01 vs 07). The launch read adds `01` left, `02` right, `03` wheel click, `09`/`0A` wheel up/down, and `60`, probably the underside DPI button |
  | `[10]` | Always `00` in writes; purpose unconfirmed |
  | `[11]` | Function category: `02` keyboard, `01` mouse, `00` disabled; `06` on button `60` in the launch read, meaning unknown |
  | `[12]` | Payload length: `02` keyboard (modifier + usage), `01` mouse (one button number), `00` disabled. Fits every write and every launch-read reply |
  | `[13]` | Keyboard modifier (`00` none, `01` left Ctrl, consistent with the HID modifier bitmask); mouse button number for mouse functions |
  | `[14]` | Keyboard usage; zero for captured mouse/disabled functions |
  | `[15..17]` | Always zero; purpose unconfirmed |

- **Key encoding:** consistent with USB HID keyboard usages: `1 = 1E`, `2 = 1F`,
  `A = 04`, `C = 06`. Steps 01-03 isolate `[14]`; step 04 sets `[13] = 01`
  and `[14] = 06` for Ctrl+C. Other keys and modifiers are untested.
- **Mouse functions / disabled / default:** step 05 sets `[11..14] = 01 01 03 00`
  for middle click on the back button. Step 06 zeros `[11..17]` for disabled.
  Default back is `[11..14] = 01 01 04 00` (step 10); default forward is
  `01 01 05 00` (step 08). Wheel click was already default and Save was disabled
  in step 09, so no wheel write was captured; the launch read gives it button
  `03` with default `01 01 03`.
- **Profile byte:** unconfirmed on this single-profile mouse. `[8] = 01` and
  `[10] = 00` did not vary; neither can be called a profile selector from this capture.
- **Read-back:** class `0x02`, id `0x8C`, the write's id with the high bit set, as with
  DPI (`0x04/0x05` write, `0x04/0x85` read). Not in this session: the 11 GET replies
  here only acknowledge the writes. Synapse sent it at launch, in an earlier capture
  ([masked launch report](captures/V3-20261006-131912-launch-report.md)), once per
  button with request `[8..10] = 01 <button> 00`, then again with `[10] = 01`. Each
  reply carries the stored bind in the write layout: back `01 04 00 01 01 04` and
  forward `01 05 00 01 01 05`, identical to the default writes in steps 10 and 08.
- **Confirmed by:** [masked session report](captures/V3-20261006-135244-report.md),
  11 writes and 11 matching success replies. UI assignments were checked after
  each Save. The [after-session listener](captures/V3-20261006-135244-listener-after.txt)
  recorded three back presses as `XBUTTON1`, three forward presses as `XBUTTON2`,
  two wheel clicks as `MIDDLE`, normal left/right/scroll input, and no keyboard
  events. The user reported exiting Synapse, but a subsequent read-only
  [`charmctl status`](captures/V3-20261006-135244-status-after.txt) returned driver
  mode (`0x03`). After the user reconnected the mouse with Synapse's window
  closed, [status still returned driver mode](captures/V3-20261006-135244-status-reconnected.txt)
  and Razer AppEngine background processes were still running. With the user's
  explicit approval, AppEngine was stopped and the existing OpenRazer-derived
  [`set-mode hardware` command](captures/V3-20261006-135244-mode-set-hardware.txt)
  selected hardware mode with matching read-back. The
  [final listener](captures/V3-20261006-135244-listener-hardware.txt) recorded
  three presses each of `XBUTTON1`, `XBUTTON2`, and `MIDDLE`, 506 mouse packets
  including movement, and no keyboard events. A
  [status check](captures/V3-20261006-135244-status-hardware.txt) confirmed
  hardware mode (`0x00`) during and after this test. Default inputs are therefore
  verified in hardware mode; this does not prove persistence of nondefault binds.
  A before-session listener output was not collected for this session.
- **Open questions:** what the read
  request's `[10] = 01` pass selects (a HyperShift layer would fit); why read replies
  carry `[10] = 01` for left, right and the wheel but `00` for the others; category
  `06`; fixed and reserved bytes; additional modifiers; multi-profile selection; and a second
  capture, including the Elite, before any write path ships. Synapse resent the
  disabled back mapping while saving forward changes in steps 07 and 08, so
  those windows contain two writes each.

#### V3 persistence check (2026-10-07)

**Passed for back bound to key `1`.** Tested with `charmctl` from commit
`abe2548`; all keymap reads used the recorded V3 button ids. No Snakecharmer
keymap write was sent.

1. The [baseline keymap](captures/V3-20261007-persistence/keymap-before.txt)
   showed all seven decoded buttons at their defaults, and
   [status](captures/V3-20261007-persistence/status-before.txt) showed hardware
   mode, DPI 1600 x 1600, and polling 1000 Hz.
2. Synapse's UI saved back to key `1`. The
   [read before reconnecting](captures/V3-20261007-persistence/keymap-custom-before-reconnect.txt)
   showed `back  1  <- not default`, with the other entries unchanged.
3. AppEngine was stopped with the user's approval; its absence was checked
   [before](captures/V3-20261007-persistence/appengine-before-reconnect.txt) and
   [after](captures/V3-20261007-persistence/appengine-after-reconnect.txt)
   the user unplugged the V3 for about five seconds and reconnected it, leaving
   Synapse closed.
4. Before reopening Synapse, [status after reconnecting](captures/V3-20261007-persistence/status-after-reconnect.txt)
   showed hardware mode (`0x00`) and unchanged DPI/polling. The
   [keymap after reconnecting](captures/V3-20261007-persistence/keymap-custom-after-reconnect.txt)
   still showed back bound to `1`; its output matched the pre-reconnect read.
   This confirms that this nondefault bind survives a power cycle and is
   readable in hardware mode without AppEngine running. A physical key-output
   check was not performed in this test.
5. Reopening Synapse for cleanup restored the default back bind automatically,
   confirmed by a [read after reopening](captures/V3-20261007-persistence/keymap-after-synapse-reopen.txt).
   AppEngine was stopped again and the existing `set-mode hardware` command
   returned matching read-back. The [final keymap](captures/V3-20261007-persistence/keymap-final.txt)
   showed all seven decoded buttons at default; [final status](captures/V3-20261007-persistence/status-final.txt)
   showed hardware mode, DPI 1600 x 1600, and polling 1000 Hz. AppEngine
   [remained stopped](captures/V3-20261007-persistence/appengine-final.txt).

The underside entry stayed `undecoded (category 0x06: 06)` throughout. This
test adds persistence evidence for the V3; the Elite comparison with Synapse 3 and explicit
approval of a keymap write path were still required at that point. (The maintainer approved
the V3 write path on 2026-10-16; the Elite comparison remains open.)

#### Additional V3 button writes (2026-10-16)

The [handoff report](captures/V3-20261016-additional-buttons/HANDOFF.md)
answers the earlier missing-write and byte `[10]` questions for the implementer.

The [masked report](captures/V3-20261016-additional-buttons/report.md),
[step markers](captures/V3-20261016-additional-buttons/markers.json), and
[complete keymap reports](captures/V3-20261016-additional-buttons/keymap-reports.json)
record wheel click, right click, scroll up, and scroll down changed to key `1`
and then restored to Default using Synapse's UI. All eight writes have a matching
acknowledgement with status `0x02`. They use class `0x02`, id `0x0C`, data size
10, USB interface 0, and the same layout as the original back/forward writes.

Synapse's installed-package registry reported `4.0.827`, while the running
AppEngine window was hosted by `app-4.0.503`. The UI About version was not
checked; the markers retain that distinction rather than assigning an exact
Synapse UI version. The V3 was the only connected mouse of the two research
models, and its PnP interfaces reported OK.

| Control / button id | Key `1` arguments `[8..17]` | Default arguments `[8..17]` |
|---|---|---|
| Wheel click `03` | `01 03 00 02 02 00 1e 00 00 00` | `01 03 00 01 01 03 00 00 00 00` |
| Right click `02` | `01 02 00 02 02 00 1e 00 00 00` | `01 02 00 01 01 02 00 00 00 00` |
| Scroll up `09` | `01 09 00 02 02 00 1e 00 00 00` | `01 09 00 01 01 09 00 00 00 00` |
| Scroll down `0A` | `01 0a 00 02 02 00 1e 00 00 00` | `01 0a 00 01 01 0a 00 00 00 00` |

The transaction ids were `07` through `0C` for the first six writes, then
`07` and `08` for scroll down after Synapse reinitialized its device page.
This does not establish a Snakecharmer write transaction-id policy.

Computer Use performed each assignment and Save, and checked the resulting UI
label before advancing the capture marker. Bringing Synapse fully to the
foreground resolved the helper's initial activation failures. During step 07,
Synapse briefly navigated to Linked Games; passive PnP enumeration still showed
all fifteen V3 interfaces with status OK. Returning to the V3 tab generated
device-info traffic and sixteen keymap read requests: eight observed button ids,
each queried with request selector `00` and `01`. A vendor probe `0x00/0x8E`
returned `0x05`; this is Synapse's traffic, not a command sent by Snakecharmer.

Those reads showed `[10] = 00` on right, wheel click, and scroll up after they
had been explicitly restored to Default. The original launch read had `01`
for those controls. Left and the not-yet-changed scroll down still returned
`01` in this pass. Thus `[10]` is not a fixed per-button value; a flag for an
untouched/default-fallback entry is one possible interpretation, not a decoded
meaning. Both read selectors produced the same binding/flag for each button.

The decoder now retains finite keymap request lists even when they exceed its
lighting-stream threshold; its self-test includes these sixteen queries. The
raw captures remain local at
`C:\Users\asas\snakecharmer-captures\V3-20261016-123309`.

All four tested UI assignments ended at Default; left and underside were not
changed. Computer Use was reset and Synapse AppEngine/monitor processes were
stopped, with [absence verified](captures/V3-20261016-additional-buttons/synapse-stopped.json).
The subsequent [60-second physical check](captures/V3-20261016-additional-buttons/listener-after.txt)
registered left, right, wheel click, back, forward, and both scroll directions
from the V3 interface, with 494 raw mouse packets and no V3 keyboard events.
Two synthetic left-click events were excluded from the physical-button evidence;
non-Razer keystrokes were counted without logging their values.

The [stored-bind read](captures/V3-20261016-additional-buttons/keymap-final.txt)
showed all seven decoded binds at Default. One human press of the underside
button changed the [reported DPI to 3200](captures/V3-20261016-additional-buttons/status-after-underside-once.txt)
from 1600 in hardware mode, confirming its DPI-cycle behavior without decoding
its keymap category. The user then reported four more presses followed by
unplugging and reconnecting the V3. With Synapse AppEngine/monitor still absent,
the [final status](captures/V3-20261016-additional-buttons/status-after-power-cycle.txt)
was hardware mode, 1600 x 1600 DPI, and 1000 Hz; the
[post-power-cycle keymap](captures/V3-20261016-additional-buttons/keymap-after-power-cycle.txt)
still showed all seven decoded defaults. Intermediate DPI stages were not read
individually. The underside entry remains `undecoded (category 0x06: 06)`.

No Snakecharmer keymap write was sent in this session (the first one came later the same day,
see [the write test](captures/V3-20261016-snakecharmer-write/notes.md)). These checks confirm normal physical
output before the power cycle and stored defaults afterward; they do not prove
persistence of a nondefault bind on each newly tested control.

### DeathAdder Elite (`1532:005C`), Synapse 4.0.827, profile count unconfirmed

Captured on 2026-10-07 using Synapse's UI. Version `4.0.827` was supplied by the
user. The script's profile count was set to `1` as its fallback: Customize
showed a software profile selector, but no on-board memory control or on-board
profile picker. This is not evidence of one on-board profile.

- **Observed USB traffic:** the [compact session report](captures/Elite-20261007-154904/report.md)
  covers all 10,991 control submissions to the Elite on bus 1, address 4, USB
  interface 0. Every submission was a 90-byte SET_REPORT (`0x21/0x09`, value
  `0x0300`) carrying class `0x0F`, id `0x03`, streamed lighting. All 10,991
  reports passed the Razer length/checksum filter. There were no class `0x02`
  reports, GET replies, or other control setup requests in this recording,
  including outside the marked windows.
- **Saved changes:** back to `1`, `2`, `A`, Ctrl+C, middle click, and Disabled;
  forward to `1`, then default; and back to default. Each of these nine Saves
  was followed by a visible assignment check. Wheel click was already default
  in step 09 and Save was disabled, so that step made no change.
- **UI evidence:** Keyboard Function, Mouse Function, and Disable each showed
  **Requires Razer Synapse**. The observations are recorded in the step notes.
- **Write/read commands and argument layout:** no binding command was observed,
  so no Elite keymap command or layout is confirmed. No V3 keymap request was
  sent to the Elite.
- **Post-session verification:** AppEngine was stopped using the user's existing
  approval. [Status](captures/Elite-20261007-154904/status-before-hardware.txt)
  still showed driver mode (`0x03`). The existing documented
  [`set-mode hardware`](captures/Elite-20261007-154904/mode-hardware.txt) command
  returned matching read-back. The [physical listener](captures/Elite-20261007-154904/listener-hardware.txt)
  recorded four presses each of `XBUTTON1` and `XBUTTON2`, five `MIDDLE` presses,
  scrolling, 1,513 raw mouse packets, and no keyboard events. The listener counts
  every mouse packet; it does not record motion deltas or the cursor position,
  so that count does not verify cursor movement.
  [Final status](captures/Elite-20261007-154904/status-hardware.txt) confirmed
  hardware mode (`0x00`), DPI 1800 x 1800, and polling 500 Hz; AppEngine
  [remained stopped](captures/Elite-20261007-154904/appengine-stopped.txt).
- **Interpretation:** the USB traffic and UI labels support software remapping
  for this Elite with Synapse 4. They do not establish that the Elite has no
  storage capability, or that other older Razer mice cannot carry stored binds.
- **Open questions:** behavior with Synapse 3; whether a nondefault bind changes
  physical input while Synapse runs and disappears after it exits or the mouse
  is power-cycled. This session restored defaults before quitting Synapse, so
  it did not perform that persistence test. No before-session physical listener
  output was collected.
- **Reported movement issue:** the user confirmed that moving the mouse did not
  move the cursor during the beep-to-beep listener check only. No pointer
  automation ran during that check. The user subsequently confirmed that the
  cursor moves normally after the listener finished. The listener uses
  `RIDEV_INPUTSINK`, calls the base window
  procedure, and has no input-blocking or pointer-positioning code; the cause
  remains unknown. Button/scroll events are established by the listener, but
  cursor movement during the check is not. Device commands were stopped and the
  issue reported under `AGENTS.md` rule 3; no further device commands were sent
  during this investigation.
- **Decoder limitation (since fixed):** the changing lighting payloads survived the
  idle filter, and pairwise byte diffs of them produced a 90 MB report. The linked
  compact report counts every command by step instead. `keymap_diff.py` now
  summarizes any command with more than 8 distinct payloads in a step as a stream;
  re-run on this session it gives a 3 KB report with one lighting line per step and
  no class `0x02` anywhere, agreeing with the compact report.

#### Elite physical assignment check (2026-10-07)

The [follow-up notes](captures/Elite-20261007-persistence/notes.md) test the
effect of one saved assignment, rather than infer storage from USB traffic.

1. With only the Elite connected and no Snakecharmer daemon running, Synapse 4
   saved back to key `1` in ASA-Default. Its UI displayed `1` for that button.
   The user reported `111` from three physical back-button presses while
   Synapse ran; the agent did not inject keys into the test editor.
2. All AppEngine processes were stopped and their absence checked. The saved
   assignment was left unchanged in Synapse. The user reported no new `1`s and
   normal cursor movement during a check without computer-use automation.
3. Automated listener invocations, including an elevated one, captured no input
   and are inconclusive. The identical listener run by the user from their own
   PowerShell produced three
   [`XBUTTON1` events from 1532:005C IF00](captures/Elite-20261007-persistence/listener-synapse-stopped-manual.txt),
   with no keyboard events. AppEngine's absence was checked again afterward.
4. The user was asked to unplug the Elite for five seconds, reconnect with
   Synapse closed, and repeat the listener. The
   [supplied reconnect log](captures/Elite-20261007-persistence/listener-after-reconnect-manual.txt)
   again records three `1532:005C IF00 XBUTTON1` presses and no keyboard events.
   AppEngine's absence was checked after this log was supplied. Unplug/replug
   was a human-performed step; the listener itself cannot verify a power cycle.

This confirms that this saved assignment depends on Synapse running in the
tested setup, with default output also recorded in the requested reconnect
check. The user subsequently requested complete Synapse removal, which removed
its saved local software profiles; see [the removal audit](RAZER-REMOVAL.md).
This does not establish all storage capabilities of the Elite or behavior under
Synapse 3.

An additional mode read during Synapse startup failed with an echo mismatch
(`sent 00/84, got 0f/03`). Direct device commands were stopped; no forced mode
change or further direct read was used in this follow-up. Mode after AppEngine
stopped was therefore not measured. The cursor freeze recurred with computer
use active and recovered after its runtime was reset; the exact cause remains
unknown. The human-run and automated capture contexts also differ in an
undiagnosed way.

Add one block per session:

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
