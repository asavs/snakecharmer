# DeathAdder V3 additional button write capture report

Date: 2026-10-16. Device: DeathAdder V3, USB `1532:00B2`.

The missing write captures are now complete for **wheel click, right click,
wheel up, and wheel down**. Synapse saved each control as keyboard `1`, then
restored it to Default. We recorded all eight writes and eight matching
success replies (`0x02`). These controls no longer require an inferred write
layout. Back and forward already had captured writes from the earlier session.

The installed-package registry reported Synapse `4.0.827`; the running window
host was `app-4.0.503`. The UI About version was not checked.

## Captured commands

Every new write used command class `0x02`, id `0x0C`, data size 10, and USB
interface 0. Offsets below are zero-based in the full 90-byte Razer report.

| Control | Button id | Key `1` arguments `[8..17]` | Default arguments `[8..17]` |
|---|---|---|---|
| Wheel click | `03` | `01 03 00 02 02 00 1e 00 00 00` | `01 03 00 01 01 03 00 00 00 00` |
| Right click | `02` | `01 02 00 02 02 00 1e 00 00 00` | `01 02 00 01 01 02 00 00 00 00` |
| Wheel up | `09` | `01 09 00 02 02 00 1e 00 00 00` | `01 09 00 01 01 09 00 00 00 00` |
| Wheel down | `0A` | `01 0a 00 02 02 00 1e 00 00 00` | `01 0a 00 01 01 0a 00 00 00 00` |

[keymap-reports.json](keymap-reports.json) preserves the complete 90-byte
requests and replies, including transaction ids and checksums, for exact-byte
fixtures. The transaction ids were `07` through `0C`, then `07` and `08`
after Synapse reinitialized its device page. This observation does not establish
which transaction id a Snakecharmer write implementation should use.

## What changed in the interpretation of byte 10

**Every captured write above used `[10] = 00`, including Default writes.**
The earlier `01` read replies do not require guessing a different write form.

During this session, Synapse also issued sixteen `0x02/0x8C` reads: the eight
previously observed button ids, each with request selector `00` and `01`.
All sixteen returned success, and both selectors returned the same binding
and flag for each button.

After right, wheel click, and wheel up were explicitly restored to Default,
their read replies contained `[10] = 00`; their original launch replies had
contained `01`. At this point left and the not-yet-modified wheel down still
returned `01`. Thus `[10]` is **not a fixed property of those buttons**.
An untouched/default-fallback flag is a possible explanation; its meaning
remains undecoded. Do not turn that hypothesis into a command rule.

## Hardware verification

With Synapse AppEngine and its monitor stopped, a
[60-second passive listener run](listener-after.txt) registered left, right,
wheel click, back, forward, and both scroll directions from the V3. It received
494 V3 mouse packets and no V3 keyboard events. Two synthetic left clicks were
excluded from the physical-button evidence.

The underside button changed the reported DPI from 1600 to 3200 after one
human press in hardware mode. The user then reported four more presses and an
unplug/reconnect. The [final status read](status-after-power-cycle.txt) confirmed
hardware mode, **1600 x 1600 DPI and 1000 Hz**. The
[final keymap read](keymap-after-power-cycle.txt) confirmed all seven decoded
binds were still Default. The underside mapping remains category `0x06`,
payload `06`, undecoded. Its physical DPI-cycle behavior is confirmed.

The physical listener ran before the final power cycle; the stored-default
read ran afterward. This session does not independently establish persistence
of keyboard `1` on each newly captured control. Earlier V3 back-button testing
already established nondefault persistence for that button.

## Consequences for implementation

| V3 controls | Read evidence | Write evidence |
|---|---|---|
| Back and forward | Captured previously | Captured previously |
| Right, wheel click, wheel up, wheel down | Captured, including this session | Now captured for key `1` and Default |
| Left | Captured | No captured write; mapping left unchanged |
| Underside | Captured, category undecoded | No captured write; mapping left unchanged |

The four missing captures can now join back/forward as evidence for a V3 write
implementation and its exact-byte tests. Capturing Synapse's successful writes
does not verify a Snakecharmer write implementation: that path still needs the
repository's approval condition, read-before-write, successful read-back, and
physical behavior checks. No Snakecharmer keymap write was sent in this session.
Do not extrapolate these commands to the Elite, a Naga, or another model.

## Evidence and checks

The [masked decoder report](report.md), [step markers](markers.json),
[verification summary](verification.json), and full keymap reports above are
saved alongside this handoff. Raw USB captures remain local because they can
include unrelated keyboard traffic; serial numbers are masked in the report.

The decoder was corrected to retain finite keymap query lists above its stream
threshold, so all sixteen reads remain visible. Its ten self-test checks pass.
The recorded [workspace checks](project-checks.json) also passed:
`cargo test --workspace`, `cargo clippy --release --all-targets`, and
`cargo build --release`.

See [the main research document](../../ONBOARD-KEYMAP.md#additional-v3-button-writes-2026-10-16)
for the surrounding protocol findings.
