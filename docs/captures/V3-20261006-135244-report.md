# Keymap capture: V3

Synapse 4 (AppEngine 4.0.827; exact Synapse version not checked), 1 on-board profile(s), started 2026-10-06T13:53:08.3110806-07:00
Notes: 09: Wheel click was already Default / Scroll Click; Save was disabled; cancelled unchanged editor.
22 Razer reports across 2 capture file(s); 0 distinct commands seen while idle are hidden.
Devices that spoke the protocol (bus, address): 1.4

## 00  IDLE: Synapse open on the mouse's button page. Touch nothing.
  (idle baseline)

## 01  Set the BACK thumb button to the keyboard key 1
  SET x1   class 0x02 id 0x0C size 10 txn 0x07 | 01 04 00 02 02 00 1e 00 00 00
  GET x1   class 0x02 id 0x0C size 10 txn 0x07 | 01 04 00 02 02 00 1e 00 00 00   <- response, status 0x02

## 02  Set the BACK thumb button to the keyboard key 2
  SET x1   class 0x02 id 0x0C size 10 txn 0x08 | 01 04 00 02 02 00 1f 00 00 00
  GET x1   class 0x02 id 0x0C size 10 txn 0x08 | 01 04 00 02 02 00 1f 00 00 00   <- response, status 0x02

## 03  Set the BACK thumb button to the keyboard key A
  SET x1   class 0x02 id 0x0C size 10 txn 0x09 | 01 04 00 02 02 00 04 00 00 00
  GET x1   class 0x02 id 0x0C size 10 txn 0x09 | 01 04 00 02 02 00 04 00 00 00   <- response, status 0x02

## 04  Set the BACK thumb button to Ctrl + C
  SET x1   class 0x02 id 0x0C size 10 txn 0x0A | 01 04 00 02 02 01 06 00 00 00
  GET x1   class 0x02 id 0x0C size 10 txn 0x0A | 01 04 00 02 02 01 06 00 00 00   <- response, status 0x02

## 05  Set the BACK thumb button to middle click (a mouse function, not a key)
  SET x1   class 0x02 id 0x0C size 10 txn 0x0B | 01 04 00 01 01 03 00 00 00 00
  GET x1   class 0x02 id 0x0C size 10 txn 0x0B | 01 04 00 01 01 03 00 00 00 00   <- response, status 0x02

## 06  Set the BACK thumb button to disabled (type a note if Synapse has no such option)
  SET x1   class 0x02 id 0x0C size 10 txn 0x0C | 01 04 00 00 00 00 00 00 00 00
  GET x1   class 0x02 id 0x0C size 10 txn 0x0C | 01 04 00 00 00 00 00 00 00 00   <- response, status 0x02

## 07  Set the FORWARD thumb button to the keyboard key 1
  SET x1   class 0x02 id 0x0C size 10 txn 0x0D | 01 04 00 00 00 00 00 00 00 00
  SET x1   class 0x02 id 0x0C size 10 txn 0x0E | 01 05 00 02 02 00 1e 00 00 00
  GET x1   class 0x02 id 0x0C size 10 txn 0x0D | 01 04 00 00 00 00 00 00 00 00   <- response, status 0x02
  GET x1   class 0x02 id 0x0C size 10 txn 0x0E | 01 05 00 02 02 00 1e 00 00 00   <- response, status 0x02

## 08  Set the FORWARD thumb button back to its default
  SET x1   class 0x02 id 0x0C size 10 txn 0x0F | 01 04 00 00 00 00 00 00 00 00
  SET x1   class 0x02 id 0x0C size 10 txn 0x10 | 01 05 00 01 01 05 00 00 00 00
  GET x1   class 0x02 id 0x0C size 10 txn 0x0F | 01 04 00 00 00 00 00 00 00 00   <- response, status 0x02
  GET x1   class 0x02 id 0x0C size 10 txn 0x10 | 01 05 00 01 01 05 00 00 00 00   <- response, status 0x02

## 09  Set the WHEEL CLICK back to its default
note: Wheel click was already Default / Scroll Click; Save was disabled; cancelled unchanged editor.
  (nothing new)

## 10  Set the BACK thumb button back to its default
  SET x1   class 0x02 id 0x0C size 10 txn 0x11 | 01 04 00 01 01 04 00 00 00 00
  GET x1   class 0x02 id 0x0C size 10 txn 0x11 | 01 04 00 01 01 04 00 00 00 00   <- response, status 0x02

## 99  IDLE: touch nothing.
  (idle baseline)

# Byte changes between consecutive steps (same class/id)

01 -> 02  class 0x02 id 0x0C: [14] 1E->1F
02 -> 03  class 0x02 id 0x0C: [14] 1F->04
03 -> 04  class 0x02 id 0x0C: [13] 00->01, [14] 04->06
04 -> 05  class 0x02 id 0x0C: [11] 02->01, [12] 02->01, [13] 01->03, [14] 06->00
05 -> 06  class 0x02 id 0x0C: [11] 01->00, [12] 01->00, [13] 03->00
06 -> 07  class 0x02 id 0x0C: [9] 04->05, [11] 00->02, [12] 00->02, [14] 00->1E
07 -> 08  class 0x02 id 0x0C: [9] 05->04, [11] 02->00, [12] 02->00, [14] 1E->00
07 -> 08  class 0x02 id 0x0C: [9] 04->05, [11] 00->01, [12] 00->01, [13] 00->05
07 -> 08  class 0x02 id 0x0C: [11] 02->01, [12] 02->01, [13] 00->05, [14] 1E->00
08 -> 10  class 0x02 id 0x0C: [11] 00->01, [12] 00->01, [13] 00->04
08 -> 10  class 0x02 id 0x0C: [9] 05->04, [13] 05->04
