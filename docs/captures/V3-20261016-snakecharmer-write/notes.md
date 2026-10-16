# First Snakecharmer keymap write, DeathAdder V3 (2026-10-16)

The first on-board bind written by Snakecharmer itself rather than Synapse.
Razer AppEngine was not running (only `razer_elevation_service`); the mouse
was in hardware mode, DPI 1600 x 1600, polling 1000 Hz, all binds default.

1. `charmctl keymap set back 1` wrote class `0x02` id `0x0C`
   `01 04 00 02 02 00 1e`, the bytes Synapse sent in the 2026-10-06 session,
   and the read-back reported back = `1`.
2. The [listener](listener-back-set-to-1.txt) recorded three back presses as
   keyboard `1` (`1532:00B2 IF02 KEY D1`) and three forward presses as
   `XBUTTON2`: the mouse itself types the stored key.
3. `charmctl keymap reset` wrote back's default `01 04 00 01 01 04`; a second
   reset reported nothing to do. [Final keymap](keymap-final.txt) all default,
   [final status](status-final.txt) unchanged.
