# How deep could you go? (Is it even a driver?)

The question flips a common intuition: **a driver goes deeper into the *OS*, but a hacker goes deeper into the *device*.** Those are opposite directions.

A Windows driver (a KMDF filter driver in the mouse's HID stack) would give you *more* system-level control than the `WH_MOUSE_LL` hook. It would remap buttons even in raw-input games that bypass hooks. But that is **plumbing, not hacking**: signed, documented, WHQL/attestation-signed engineering that Microsoft approves of. It's what a vendor does. It uses the *same* documented HID protocol that was already reverse-engineered; the code just moves from ring 3 to ring 0. No new secret comes out of it.

The hacker's move is to climb **down toward the silicon**, where there is locked-up knowledge to pry open. The ladder looks like this:

| Tier | What you do | What it takes |
|---|---|---|
| **0 — Protocol RE** *(Snakecharmer did this)* | Sniff USB (Wireshark + USBPcap) while Synapse talks to the mouse; recover the vendor feature-report language | A cable and patience. This is the accessible tier of hacking, and it's real hacking. |
| **1 — Kernel filter driver** | Intercept/rewrite HID reports for *all* consumers, system-wide | Windows driver skills, a signing cert. More invasive, not more clever. |
| **2 — Firmware dump** | Pull the actual code off the mouse's MCU | See below |
| **3 — Firmware RE** | Ghidra/IDA on the dumped image: find the USB descriptors, the sensor loop, the button matrix scan, the DPI tables | ARM Cortex-M / 8051 disassembly skills |
| **4 — Custom firmware** | Reflash it with your own | Defeat any read/write protection first |

**The firmware is the real prize.** The DeathAdder Elite runs code no outside researcher has seen, behind whatever readout protection the vendor chose to enable. Recovering it, when it can be recovered at all, is a hardware-security discipline of its own, and it's the exact thing the defensive literature works to prevent. It's also beside the point for a config tool, which is why this note stops at naming it.

**The extreme case, and the offensive one, is BadUSB** (Nohl & Lell, 2014; the O.MG cable is its commercial descendant): custom firmware that makes a peripheral enumerate as a *composite* HID device, a mouse *and* a keyboard. A mouse that can *type* is a hardware implant. It injects keystrokes the moment it's plugged in, on any OS, with **no host software or driver at all**, because every OS trusts any device that claims to be a keyboard. It survives OS reinstalls because the payload lives in the peripheral, not on the disk. This is why "no driver" is the *more* dangerous design, not the less. It's textbook security research, kept here at the concept level; this document has no build recipe and doesn't need one to make the point.

So **Snakecharmer sits at the shallowest, most reversible layer on purpose**: userland, documented commands, unplug to undo. That is the *correct* choice for a config tool. Going deeper doesn't make a better mouse utility; it serves other goals, such as custom firmware features, persistence, or offense. The README's Safety section ("no firmware, no DFU, no fuzzing") declines to walk down that ladder, which is the right call for something people install.

The tier that would be *fun and legitimate* for this project is **Tier 2–3 on a mouse you own, to document the protocol in the toolkit for the next device**. That is the natural extension of `CRACKING-MICE-GUIDE`, and it is all defensive research.
