</details>

<details>
<summary><b>…I configure a thumb-button remap? (the one global setting)</b></summary>

The thumb remap uses a system-wide `WH_MOUSE_LL` hook, so a configured Back/Forward
remap applies to **every pointing device on the PC** — including non-Razer mice —
even while no supported mouse is plugged in. A low-level hook also routes every mouse
event through an extra check, which is why it's opt-in: **the hook only exists while a
remap is configured**. At the default (`none`) there is no hook and zero overhead, and
setting both back to `none` (or quitting) removes it immediately. Everything else
(DPI, polling, lighting, DPI-button remap) is strictly per-device and never touches
the pointer's motion path.
</details>

<details>
<summary><b>…I plug in two supported mice at once?</b></summary>

Not really supported: commands go to whichever unit Windows enumerates first, and
DPI-button presses from either mouse of the same model trigger actions. Harmless, but
arbitrary — plug in one at a time.
</details>
