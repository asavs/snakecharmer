# Keymap capture: V3

Synapse Synapse 4; installed package 4.0.827 (About not checked), 1 on-board profile(s), started 2026-10-16T12:33:46.4604448-07:00
Notes: 01: Computer Use saved wheel click as keyboard 1; UI label confirmed; 02: Computer Use restored wheel click to Default; Scroll Click label confirmed; 03: Computer Use saved right click as keyboard 1; UI label confirmed; 04: Computer Use restored right click to Default; Right Click label confirmed; 05: Computer Use saved scroll up as keyboard 1; UI label confirmed; 06: Computer Use restored scroll up to Default; Scroll Up label confirmed; 07: Synapse briefly navigated to Linked Games; V3 stayed enumerated with OK status. Returned to V3 and saved scroll down as keyboard 1; UI label confirmed; 08: Computer Use restored scroll down to Default; all four tested controls show defaults
99 Razer reports across 2 capture file(s); 0 distinct commands seen while idle are hidden.
Devices that spoke the protocol (bus, address): 1.4

## 00  IDLE: V3 button page open. Touch nothing.
  (idle baseline)

## 01  Set WHEEL CLICK to keyboard key 1 and Save
note: Computer Use saved wheel click as keyboard 1; UI label confirmed
  SET x1   class 0x02 id 0x0C size 10 txn 0x07 | 01 03 00 02 02 00 1e 00 00 00
  GET x1   class 0x02 id 0x0C size 10 txn 0x07 | 01 03 00 02 02 00 1e 00 00 00   <- response, status 0x02

## 02  Restore WHEEL CLICK to Default and Save
note: Computer Use restored wheel click to Default; Scroll Click label confirmed
  SET x1   class 0x02 id 0x0C size 10 txn 0x08 | 01 03 00 01 01 03 00 00 00 00
  GET x1   class 0x02 id 0x0C size 10 txn 0x08 | 01 03 00 01 01 03 00 00 00 00   <- response, status 0x02

## 03  Set RIGHT CLICK to keyboard key 1 and Save
note: Computer Use saved right click as keyboard 1; UI label confirmed
  SET x1   class 0x02 id 0x0C size 10 txn 0x09 | 01 02 00 02 02 00 1e 00 00 00
  GET x1   class 0x02 id 0x0C size 10 txn 0x09 | 01 02 00 02 02 00 1e 00 00 00   <- response, status 0x02

## 04  Restore RIGHT CLICK to Default and Save
note: Computer Use restored right click to Default; Right Click label confirmed
  SET x1   class 0x02 id 0x0C size 10 txn 0x0A | 01 02 00 01 01 02 00 00 00 00
  GET x1   class 0x02 id 0x0C size 10 txn 0x0A | 01 02 00 01 01 02 00 00 00 00   <- response, status 0x02

## 05  Set WHEEL UP to keyboard key 1 and Save, or note if unavailable
note: Computer Use saved scroll up as keyboard 1; UI label confirmed
  SET x1   class 0x02 id 0x0C size 10 txn 0x0B | 01 09 00 02 02 00 1e 00 00 00
  GET x1   class 0x02 id 0x0C size 10 txn 0x0B | 01 09 00 02 02 00 1e 00 00 00   <- response, status 0x02

## 06  Restore WHEEL UP to Default and Save, or note if unavailable
note: Computer Use restored scroll up to Default; Scroll Up label confirmed
  SET x1   class 0x02 id 0x0C size 10 txn 0x0C | 01 09 00 01 01 09 00 00 00 00
  GET x1   class 0x02 id 0x0C size 10 txn 0x0C | 01 09 00 01 01 09 00 00 00 00   <- response, status 0x02

## 07  Set WHEEL DOWN to keyboard key 1 and Save, or note if unavailable
note: Synapse briefly navigated to Linked Games; V3 stayed enumerated with OK status. Returned to V3 and saved scroll down as keyboard 1; UI label confirmed
  SET x1   class 0x00 id 0x81 size  2 txn 0x00 | 00 00
  SET x2   class 0x00 id 0x82 size 22 txn 0x01 | 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  SET x2   class 0x00 id 0x86 size  3 txn 0x02 | 00 00 00
  SET x1   class 0x00 id 0x84 size  2 txn 0x01 | 00 00
  SET x1   class 0x00 id 0x04 size  2 txn 0x02 | 03 00
  SET x1   class 0x00 id 0xB3 size  1 txn 0x03 | 00
  SET x1   class 0x00 id 0x87 size  4 txn 0x04 | 00 00 00 00
  SET x3   class 0x04 id 0x06 size 38 txn 0x05 | 01 03 05 00 01 90 01 90 00 00 01 03 20 03 20 00 00 02 06 40 06 40 00 00 03 0c 80 0c 80 00 00 04 19 00 19 00 00 00
  SET x3   class 0x00 id 0xC0 size  2 txn 0x06 | 01 00
  SET x4   class 0x04 id 0x86 size 80 txn 0x07 | 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  SET x1   class 0x06 id 0x8E size 14 txn 0x0A | 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  SET x1   class 0x06 id 0x80 size  2 txn 0x0C | 00 00
  SET x1   class 0x05 id 0x8A size  1 txn 0x0D | 00
  SET x1   class 0x05 id 0x80 size  1 txn 0x0E | 00
  SET x1   class 0x05 id 0x81 size 80 txn 0x0F | 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  SET x1   class 0x04 id 0x85 size  7 txn 0x10 | 01 00 00 00 00 00 00
  SET x1   class 0x00 id 0x8E size  2 txn 0x12 | 01 00
  SET x1   class 0x02 id 0x8C size 10 txn 0x13 | 01 01 00 00 00 00 00 00 00 00
  SET x1   class 0x02 id 0x8C size 10 txn 0x14 | 01 02 00 00 00 00 00 00 00 00
  SET x1   class 0x02 id 0x8C size 10 txn 0x15 | 01 03 00 00 00 00 00 00 00 00
  SET x1   class 0x02 id 0x8C size 10 txn 0x16 | 01 04 00 00 00 00 00 00 00 00
  SET x1   class 0x02 id 0x8C size 10 txn 0x17 | 01 05 00 00 00 00 00 00 00 00
  SET x1   class 0x02 id 0x8C size 10 txn 0x18 | 01 09 00 00 00 00 00 00 00 00
  SET x1   class 0x02 id 0x8C size 10 txn 0x19 | 01 0a 00 00 00 00 00 00 00 00
  SET x1   class 0x02 id 0x8C size 10 txn 0x1A | 01 60 00 00 00 00 00 00 00 00
  SET x1   class 0x02 id 0x8C size 10 txn 0x1B | 01 01 01 00 00 00 00 00 00 00
  SET x1   class 0x02 id 0x8C size 10 txn 0x1C | 01 02 01 00 00 00 00 00 00 00
  SET x1   class 0x02 id 0x8C size 10 txn 0x1D | 01 03 01 00 00 00 00 00 00 00
  SET x1   class 0x02 id 0x8C size 10 txn 0x1E | 01 04 01 00 00 00 00 00 00 00
  SET x1   class 0x02 id 0x8C size 10 txn 0x00 | 01 05 01 00 00 00 00 00 00 00
  SET x1   class 0x02 id 0x8C size 10 txn 0x01 | 01 09 01 00 00 00 00 00 00 00
  SET x1   class 0x02 id 0x8C size 10 txn 0x02 | 01 0a 01 00 00 00 00 00 00 00
  SET x1   class 0x02 id 0x8C size 10 txn 0x03 | 01 60 01 00 00 00 00 00 00 00
  SET x1   class 0x02 id 0x0C size 10 txn 0x07 | 01 0a 00 02 02 00 1e 00 00 00
  GET x2   class 0x00 id 0x82 size 22 txn 0x01 | (serial number, redacted)   <- response, status 0x02
  GET x2   class 0x00 id 0x86 size  3 txn 0x02 | 00 00 00   <- response, status 0x02
  GET x1   class 0x00 id 0x84 size  2 txn 0x01 | 00 00   <- response, status 0x02
  GET x1   class 0x00 id 0x04 size  2 txn 0x02 | 03 00   <- response, status 0x02
  GET x1   class 0x00 id 0xB3 size  1 txn 0x03 | 00   <- response, status 0x02
  GET x1   class 0x00 id 0x87 size  4 txn 0x04 | 01 01 00 00   <- response, status 0x02
  GET x3   class 0x04 id 0x06 size 38 txn 0x05 | 01 03 05 00 01 90 01 90 00 00 01 03 20 03 20 00 00 02 06 40 06 40 00 00 03 0c 80 0c 80 00 00 04 19 00 19 00 00 00   <- response, status 0x02
  GET x3   class 0x00 id 0xC0 size  2 txn 0x06 | 01 08   <- response, status 0x02
  GET x4   class 0x04 id 0x86 size 80 txn 0x07 | 01 03 05 01 01 90 01 90 00 00 02 03 20 03 20 00 00 03 06 40 06 40 00 00 04 0c 80 0c 80 00 00 05 19 00 19 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00   <- response, status 0x02
  GET x1   class 0x06 id 0x8E size 14 txn 0x0A | 00 64 00 02 00 00 00 01 fa 00 00 00 08 00   <- response, status 0x02
  GET x1   class 0x06 id 0x80 size  2 txn 0x0C | 00 00   <- response, status 0x02
  GET x1   class 0x05 id 0x8A size  1 txn 0x0D | 05   <- response, status 0x02
  GET x1   class 0x05 id 0x80 size  1 txn 0x0E | 01   <- response, status 0x02
  GET x1   class 0x05 id 0x81 size  2 txn 0x0F | 05 01   <- response, status 0x02
  GET x1   class 0x04 id 0x85 size  7 txn 0x10 | 01 06 40 06 40 00 00   <- response, status 0x02
  GET x1   class 0x00 id 0x8E size  2 txn 0x12 | 01 00   <- response, status 0x05
  GET x2   class 0x02 id 0x8C size 10 txn 0x13 | 01 01 01 01 01 01 00 00 00 00   <- response, status 0x02
  GET x2   class 0x02 id 0x8C size 10 txn 0x14 | 01 02 00 01 01 02 00 00 00 00   <- response, status 0x02
  GET x2   class 0x02 id 0x8C size 10 txn 0x15 | 01 03 00 01 01 03 00 00 00 00   <- response, status 0x02
  GET x2   class 0x02 id 0x8C size 10 txn 0x16 | 01 04 00 01 01 04 00 00 00 00   <- response, status 0x02
  GET x2   class 0x02 id 0x8C size 10 txn 0x17 | 01 05 00 01 01 05 00 00 00 00   <- response, status 0x02
  GET x2   class 0x02 id 0x8C size 10 txn 0x18 | 01 09 00 01 01 09 00 00 00 00   <- response, status 0x02
  GET x2   class 0x02 id 0x8C size 10 txn 0x19 | 01 0a 01 01 01 0a 00 00 00 00   <- response, status 0x02
  GET x2   class 0x02 id 0x8C size 10 txn 0x1A | 01 60 00 06 01 06 00 00 00 00   <- response, status 0x02
  GET x1   class 0x02 id 0x0C size 10 txn 0x07 | 01 0a 00 02 02 00 1e 00 00 00   <- response, status 0x02

## 08  Restore WHEEL DOWN to Default and Save, or note if unavailable
note: Computer Use restored scroll down to Default; all four tested controls show defaults
  SET x1   class 0x02 id 0x0C size 10 txn 0x08 | 01 0a 00 01 01 0a 00 00 00 00
  GET x1   class 0x02 id 0x0C size 10 txn 0x08 | 01 0a 00 01 01 0a 00 00 00 00   <- response, status 0x02

## 99  IDLE: touch nothing.
  (idle baseline)

# Byte changes between consecutive steps (same class/id)

01 -> 02  class 0x02 id 0x0C: [11] 02->01, [12] 02->01, [13] 00->03, [14] 1E->00
02 -> 03  class 0x02 id 0x0C: [9] 03->02, [11] 01->02, [12] 01->02, [13] 03->00, [14] 00->1E
03 -> 04  class 0x02 id 0x0C: [11] 02->01, [12] 02->01, [13] 00->02, [14] 1E->00
04 -> 05  class 0x02 id 0x0C: [9] 02->09, [11] 01->02, [12] 01->02, [13] 02->00, [14] 00->1E
05 -> 06  class 0x02 id 0x0C: [11] 02->01, [12] 02->01, [13] 00->09, [14] 1E->00
06 -> 07  class 0x02 id 0x0C: [9] 09->0A, [11] 01->02, [12] 01->02, [13] 09->00, [14] 00->1E
07 -> 08  class 0x02 id 0x0C: [11] 02->01, [12] 02->01, [13] 00->0A, [14] 1E->00
