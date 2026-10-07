# Elite button-remapping capture (compact report)

Started: 2026-10-07T15:49:29.0889773-07:00
Synapse: 4.0.827 (as provided)
Device: DeathAdder Elite, USB 1532:005C; Razer control traffic at bus 1, address 4.
Profile count: entered 1 as the script fallback; actual on-board profile count is unconfirmed.

10991 validated 90-byte Razer report events across two local capture files.
Every event is a host-to-device SET, class 0x0F / id 0x03 (streamed lighting).
Class 0x02 events: **0**, including outside the marked step windows.
No GET replies were observed in this recording.

The generic decoder generated a 90 MB Cartesian diff of changing lighting frames.
This compact report groups all validated events by step and command; it does not filter out keymap commands.
Raw USB captures remain local. No serial values or keyboard traffic are included.

## All Elite USB control setup requests

Counts are submission records; IRP ids are reused and are not transaction counts.

| Request type | Request | Value | Interface | Length | Count |
|---|---|---|---|---|---|
| 0x21 | 0x09 | 0x0300 | 0 | 90 | 10991 |

## Marked steps

| Step | Action | Lighting report events | Keymap report events |
|---|---|---:|---:|
| 00 | IDLE: Synapse open on the mouse's button page. Touch nothing. | 232 | 0 |
| 01 | Set the BACK thumb button to the keyboard key 1 | 1338 | 0 |
| 02 | Set the BACK thumb button to the keyboard key 2 | 1013 | 0 |
| 03 | Set the BACK thumb button to the keyboard key A | 853 | 0 |
| 04 | Set the BACK thumb button to Ctrl + C | 924 | 0 |
| 05 | Set the BACK thumb button to middle click (a mouse function, not a key) | 1402 | 0 |
| 06 | Set the BACK thumb button to disabled (type a note if Synapse has no such option) | 1388 | 0 |
| 07 | Set the FORWARD thumb button to the keyboard key 1 | 936 | 0 |
| 08 | Set the FORWARD thumb button back to its default | 833 | 0 |
| 09 | Set the WHEEL CLICK back to its default | 474 | 0 |
| 10 | Set the BACK thumb button back to its default | 736 | 0 |
| 99 | IDLE: touch nothing. | 162 | 0 |

700 lighting report events fall outside the marked windows.

## UI observations

- Step 01: No on-board memory control or on-board profile picker visible on Customize; 1 is the script fallback, not a verified profile count. Keyboard Function explicitly says Requires Razer Synapse. Saved back to 1 and verified label.
- Step 05: Mouse Function also shows Requires Razer Synapse. Saved back to Scroll Click (middle click) and verified label.
- Step 06: Disable also shows Requires Razer Synapse. Saved and verified Disable label.
- Step 09: Wheel click already Default / Scroll Click; Save disabled; cancelled unchanged editor.

## Interpretation and limits

The saved keyboard, mouse-function, and Disabled assignments all displayed Requires Razer Synapse.
No vendor button-binding command was observed during the nine successful Saves; wheel click was already default.
This supports software remapping for this Elite with Synapse 4. It does not establish that all older Razer mice lack stored binds, nor rule out a different protocol or Synapse version.
The session restored UI defaults; a post-Synapse physical check is recorded separately.
A nondefault-bind persistence test and a Synapse 3 comparison were not performed in this session.
