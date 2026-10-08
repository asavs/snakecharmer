# Elite physical assignment/persistence test

Physical checks complete; software profile cleanup pending. Synapse 4.0.827.

- Only the DeathAdder Elite (1532:005C) was enumerated; no Snakecharmer daemon
  process was running.
- Before Synapse launched, `charmctl status` succeeded: hardware mode 0x00,
  DPI 1800 x 1800, polling 500 Hz. This result was observed in tool output.
- An additional status read during Synapse startup failed:
  `ERROR: protocol error: response echo mismatch: sent 00/84, got 0f/03`.
  The request was a mode read. Direct device commands were stopped; no further
  direct reads or writes will be used in this run. The cause is unconfirmed;
  concurrent Synapse lighting traffic is a possible explanation.
- `status-baseline.txt` is empty because the failed CLI emitted its error on
  stderr; it must not be treated as a successful baseline.
- Physical input and power-cycle checks use Synapse's UI and human actions.
- In Synapse's ASA-Default profile, the back thumb button initially displayed
  `Mouse Button 4`. Keyboard Function was set to key `1` and saved; the Customize
  page subsequently displayed `1` for that button. The editor displayed
  `Requires Razer Synapse`; Turbo was left off.
- A separate blank Notepad tab was focused for three human thumb-button presses.
  The agent did not inject keys into Notepad. The user reported `111` for the
  requested three physical back-button presses while Synapse was running.
  This establishes the remap's effect by human observation; an independent
  Notepad text capture was not obtained. Automatic approval review blocked
  screenshots because an unrelated private document was open in another tab.
- All RazerAppEngine processes were stopped with the user's prior authorization.
  An initial non-elevated stop was denied; the elevated stop succeeded, with
  absence confirmed after processes had finished exiting. Only the Razer
  elevation service remained in the Razer process inventory.
- The 20-second Synapse-stopped listener finished with exit code 0, but recorded
  no mouse packets and no keyboard events. The user reported that the cursor was
  still unusable during this check and suspected computer use. This phase is
  **inconclusive**, not evidence that the assignment disappeared. Computer-use
  input was stopped; no direct device command was sent.
- Cleanup of the saved Synapse back-to-1 assignment is pending. Do not generalize
  this run to all Elite storage capabilities or Synapse versions.
- After resetting the computer-use JavaScript runtime, the user confirmed cursor
  movement recovered. A 30-second listener retest ran without initializing or
  using computer use, and without direct device commands. The user reported no
  new `1`s and normal cursor movement after pressing the back button. However,
  this non-elevated listener also recorded no mouse packets or keyboard events,
  so it does not independently identify the physical button output. Both the
  shell and Explorer were in Windows session 1. A passive administrator listener
  check is being performed to test whether capture permissions explain the
  empty logs; that explanation is not established yet.
- The administrator listener also completed with exit code 0 and no input
  events. Elevation did not establish a working capture. These empty logs must
  not be interpreted as evidence of any specific mouse output or lack thereof.
- **Valid physical check with Synapse stopped:** the user ran the identical
  listener from their own PowerShell terminal and supplied its output. All three
  back-button presses generated `1532:005C IF00 mouse XBUTTON1 (back) down`, with
  no keyboard events. It counted 1,194 raw mouse packets; that count is not a
  measurement of cursor movement. See `listener-synapse-stopped-manual.txt`.
  This establishes default physical button output after AppEngine stopped while
  the custom assignment had not been deliberately restored in Synapse.
- The contrast between working human-run capture and empty automated captures
  establishes a capture-context problem. Its exact cause is not diagnosed.
- **Requested power-cycle check:** with Synapse closed, the user was asked to
  unplug the Elite for five seconds, reconnect, and repeat the same human-run
  listener. The supplied log again shows three `1532:005C IF00 XBUTTON1` back
  presses and no keyboard events. See `listener-after-reconnect-manual.txt`.
  The log contains ten raw mouse packets; it does not verify cursor movement or
  independently measure whether the mouse was unplugged. Unplug/replug is a
  human-performed step, as requested in the conversation.
- **Result:** the tested back-to-1 assignment works with Synapse running, changes
  back to default physical output after AppEngine stops, and shows default
  output in the requested reconnect check. This is evidence for software
  remapping in this configuration, not proof of an absence of all on-board
  memory or support under every Synapse version.
- **Cleanup:** Synapse was reopened to restore the software profile. The
  computer-use launcher timed out, but a Synapse window subsequently appeared.
  Both activation attempts failed with `failed to activate captured window`.
  No assignment change was made during cleanup; computer-use runtime was reset
  immediately after recovery failed. Manual profile restoration is pending.
