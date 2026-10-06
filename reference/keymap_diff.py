"""Pull Razer control reports out of USBPcap captures and diff them step by step.

Companion to `capture_session.ps1` and `docs/ONBOARD-KEYMAP.md`. A session
directory holds one or more `.pcap`/`.pcapng` files (one per USB root hub)
and a `markers.json` naming each step and its start/end time. This script:

  1. reads every packet with no Wireshark dependency (the USBPcap record
     layout is 27 bytes, plus a stage byte for control transfers),
  2. keeps only payloads that are valid 90-byte Razer reports: data_size
     <= 80 and byte 88 == XOR of bytes 2..87, the same CRC the rest of this
     toolkit builds. That filter alone discards keyboard, audio, and every
     other device on the bus, so no device address is needed,
  3. assigns each report to the step whose time window contains it,
  4. prints, per step, the distinct SET_REPORTs (host -> mouse) that did not
     already occur while idle, then the byte offsets that changed between
     consecutive steps for the same command.

It only reads files. Nothing here opens or writes to a device.

usage:
  python keymap_diff.py SESSION_DIR          summary + diffs, also written to SESSION_DIR/report.md
  python keymap_diff.py SESSION_DIR --all    include commands that also occur while idle
  python keymap_diff.py --self-test          parse a synthetic capture and check the output
"""

import json
import struct
import sys
from collections import OrderedDict
from pathlib import Path

LINKTYPE_USBPCAP = 249
REPORT_LEN = 90

GET_REPORT = 0x01
SET_REPORT = 0x09


# --- capture files ----------------------------------------------------------

def read_capture(path):
    """Yield (timestamp_seconds, linktype, packet_bytes) from pcap or pcapng."""
    data = Path(path).read_bytes()
    magic = data[:4]
    if magic in (b"\xd4\xc3\xb2\xa1", b"\x4d\x3c\xb2\xa1"):
        yield from _read_pcap(data, "<", magic == b"\x4d\x3c\xb2\xa1")
    elif magic in (b"\xa1\xb2\xc3\xd4", b"\xa1\xb2\x3c\x4d"):
        yield from _read_pcap(data, ">", magic == b"\xa1\xb2\x3c\x4d")
    elif magic == b"\x0a\x0d\x0d\x0a":
        yield from _read_pcapng(data)
    else:
        raise ValueError(f"{path}: not a pcap or pcapng file")


def _read_pcap(data, e, nanos):
    linktype = struct.unpack_from(e + "I", data, 20)[0]
    off = 24
    while off + 16 <= len(data):
        sec, frac, incl, _orig = struct.unpack_from(e + "IIII", data, off)
        off += 16
        if off + incl > len(data):
            break  # truncated tail: the capture was stopped mid-write
        yield sec + frac / (1e9 if nanos else 1e6), linktype, data[off:off + incl]
        off += incl


def _read_pcapng(data):
    e = "<"
    interfaces = []  # (linktype, ticks_per_second)
    off = 0
    while off + 12 <= len(data):
        btype = struct.unpack_from(e + "I", data, off)[0]
        if btype == 0x0A0D0D0A:  # section header: re-read byte order
            e = "<" if data[off + 8:off + 12] == b"\x4d\x3c\x2b\x1a" else ">"
            interfaces = []
        blen = struct.unpack_from(e + "I", data, off + 4)[0]
        if blen < 12 or off + blen > len(data):
            break
        body = data[off + 8:off + blen - 4]
        if btype == 1:  # interface description
            linktype = struct.unpack_from(e + "H", body, 0)[0]
            interfaces.append([linktype, 1e6])
            opt = 8
            while opt + 4 <= len(body):
                code, olen = struct.unpack_from(e + "HH", body, opt)
                if code == 0:
                    break
                if code == 9 and olen >= 1:  # if_tsresol
                    v = body[opt + 4]
                    interfaces[-1][1] = 2 ** (v & 0x7F) if v & 0x80 else 10 ** v
                opt += 4 + ((olen + 3) & ~3)
        elif btype == 6:  # enhanced packet
            iface, hi, lo, cap = struct.unpack_from(e + "IIII", body, 0)
            linktype, tps = interfaces[iface]
            yield ((hi << 32) | lo) / tps, linktype, body[20:20 + cap]
        off += blen


# --- USBPcap records -> Razer reports ----------------------------------------

def is_razer_report(r):
    if len(r) != REPORT_LEN or r[5] > 80:
        return False
    if not any(r[1:8]):
        return False  # all-zero header: a zeroed buffer, not a command
    crc = 0
    for b in r[2:88]:
        crc ^= b
    return crc == r[88]


def extract_reports(packets):
    """Turn USBPcap records into report events.

    Control transfers arrive as a SETUP record (the 8-byte setup packet, with
    OUT data appended when the host sends some) and a completion record
    carrying IN data. Records of one transfer share an IRP id, which is how
    a GET_REPORT's returned data is matched to its setup.
    """
    setups = {}
    events = []
    for ts, linktype, pkt in packets:
        if linktype != LINKTYPE_USBPCAP or len(pkt) < 27:
            continue
        hlen, irp = struct.unpack_from("<HQ", pkt, 0)
        info = pkt[16]
        bus, dev = struct.unpack_from("<HH", pkt, 17)
        xfer = pkt[22]
        dlen = struct.unpack_from("<I", pkt, 23)[0]
        payload = pkt[hlen:hlen + dlen]
        if xfer != 2:
            continue  # Razer reports only travel as control transfers
        stage = pkt[27] if hlen >= 28 else None
        setup = None
        if stage == 0 and len(payload) >= 8:
            setup = payload[:8]
            setups[(bus, irp)] = setup
            payload = payload[8:]
        else:
            setup = setups.get((bus, irp))

        for cand in (payload[-REPORT_LEN:], payload[1:1 + REPORT_LEN]):
            if is_razer_report(cand):
                break
        else:
            continue

        b_request = setup[1] if setup else None
        if b_request == SET_REPORT or (b_request is None and not info & 1):
            kind = "SET"
        else:
            kind = "GET"
        interface = struct.unpack_from("<H", setup, 4)[0] if setup else None
        events.append({"ts": ts, "bus": bus, "dev": dev, "interface": interface,
                       "kind": kind, "report": bytes(cand)})
    events.sort(key=lambda ev: ev["ts"])
    return _dedupe(events)


def _dedupe(events):
    """A SET's data can show up in both its setup and completion records."""
    out, seen = [], set()
    for ev in events:
        key = (ev["kind"], ev["bus"], ev["dev"], ev["report"], round(ev["ts"], 2))
        if key not in seen:
            seen.add(key)
            out.append(ev)
    return out


# --- per-step analysis -------------------------------------------------------

def command_key(r):
    """Identity of a command: class, id, size and its argument bytes."""
    return (r[6], r[7], r[5], bytes(r[8:8 + r[5]]))


def fmt_cmd(r):
    args = r[8:8 + r[5]].hex(" ")
    return f"class 0x{r[6]:02X} id 0x{r[7]:02X} size {r[5]:2d} txn 0x{r[1]:02X} | {args}"


def assign_steps(events, steps):
    by_step = OrderedDict((s["id"], []) for s in steps)
    for ev in events:
        for s in steps:
            if s["start"] <= ev["ts"] <= s["end"]:
                by_step[s["id"]].append(ev)
                break
    return by_step


def analyze(session_dir, show_all=False):
    session_dir = Path(session_dir)
    markers = json.loads((session_dir / "markers.json").read_text(encoding="utf-8"))
    steps = markers["steps"]
    captures = sorted(p for p in session_dir.iterdir() if p.suffix in (".pcap", ".pcapng"))
    if not captures:
        raise SystemExit(f"no .pcap/.pcapng files in {session_dir}")

    events = []
    for cap in captures:
        events += extract_reports(read_capture(cap))
    events.sort(key=lambda ev: ev["ts"])
    by_step = assign_steps(events, steps)

    idle_ids = [s["id"] for s in steps if s.get("idle")]
    background = {command_key(ev["report"]) for sid in idle_ids for ev in by_step[sid]}

    lines = [f"# Keymap capture: {markers.get('mouse', '?')}", ""]
    lines.append(f"{len(events)} Razer reports across {len(captures)} capture file(s); "
                 f"{len(background)} distinct commands seen while idle are hidden"
                 + (" (shown: --all)" if show_all else "") + ".")
    devices = sorted({(ev['bus'], ev['dev']) for ev in events})
    lines.append("Devices that spoke the protocol (bus, address): "
                 + (", ".join(f"{b}.{d}" for b, d in devices) or "none"))
    lines.append("")

    step_cmds = OrderedDict()
    for s in steps:
        sets = OrderedDict()
        gets = OrderedDict()
        for ev in by_step[s["id"]]:
            k = command_key(ev["report"])
            if not show_all and k in background:
                continue
            table = sets if ev["kind"] == "SET" else gets
            table.setdefault(k, [ev["report"], 0])[1] += 1
        step_cmds[s["id"]] = sets
        lines.append(f"## {s['id']}  {s['label']}")
        if s.get("note"):
            lines.append(f"note: {s['note']}")
        if not sets and not gets:
            lines.append("  (nothing new)" if not s.get("idle") else "  (idle baseline)")
        for r, n in sets.values():
            lines.append(f"  SET x{n:<3} {fmt_cmd(r)}")
        for r, n in gets.values():
            lines.append(f"  GET x{n:<3} {fmt_cmd(r)}   <- response, status 0x{r[0]:02X}")
        lines.append("")

    lines.append("# Byte changes between consecutive steps (same class/id)")
    lines.append("")
    prev_id, prev = None, None
    for sid, sets in step_cmds.items():
        if prev is not None:
            for (cls, cid, _, _), (r, _) in sets.items():
                olds = [pr for (pc, pi, _, _), (pr, _) in prev.items() if (pc, pi) == (cls, cid)]
                for old in olds:
                    diffs = [i for i in range(8, 88) if old[i] != r[i]]
                    if diffs:
                        cells = ", ".join(f"[{i}] {old[i]:02X}->{r[i]:02X}" for i in diffs)
                        lines.append(f"{prev_id} -> {sid}  class 0x{cls:02X} id 0x{cid:02X}: {cells}")
        if sets:
            prev_id, prev = sid, sets
    lines.append("")
    return "\n".join(lines)


# --- self-test ---------------------------------------------------------------

def _report(txn, cls, cid, args, status=0x00):
    r = bytearray(REPORT_LEN)
    r[0], r[1], r[5], r[6], r[7] = status, txn, len(args), cls, cid
    r[8:8 + len(args)] = args
    crc = 0
    for b in r[2:88]:
        crc ^= b
    r[88] = crc
    return bytes(r)


def _usbpcap(irp, info, stage, payload, dev=3):
    hdr = struct.pack("<HQIHBHHBBI", 28, irp, 0, 0x1B, info, 1, dev, 0x00, 2, len(payload))
    return hdr + bytes([stage]) + payload


def _setup(b_request, length, interface=0):
    bm = 0x21 if b_request == SET_REPORT else 0xA1
    return struct.pack("<BBHHH", bm, b_request, 0x0300, interface, length)


def self_test():
    import tempfile

    # Synthetic only: class 0x77 is a made-up command standing in for
    # "whatever Synapse sends", so the test asserts on the tool, not on any
    # claim about real Razer protocol.
    idle = _report(0x1F, 0x00, 0x84, b"\x00\x00")
    key1 = _report(0x1F, 0x77, 0x0C, b"\x01\x04\x02\x1E\x00")
    key2 = _report(0x1F, 0x77, 0x0C, b"\x01\x04\x02\x1F\x00")
    resp = _report(0x1F, 0x00, 0x84, b"\x00\x00", status=0x02)
    keyboard_noise = b"\x00" * 8

    t0 = 1_800_000_000.0
    recs = [
        (t0 + 1, _usbpcap(1, 0, 0, _setup(GET_REPORT, 90))),
        (t0 + 1.01, _usbpcap(1, 1, 1, resp)),
        (t0 + 1.5, _usbpcap(2, 0, 0, _setup(SET_REPORT, 90) + idle)),
        (t0 + 11, _usbpcap(3, 0, 0, _setup(SET_REPORT, 90) + key1)),
        (t0 + 11.01, _usbpcap(3, 1, 2, b"")),
        (t0 + 11.5, _usbpcap(4, 0, 0, _setup(SET_REPORT, 90) + idle)),
        (t0 + 12, struct.pack("<HQIHBHHBBI", 27, 5, 0, 9, 1, 1, 4, 0x81, 1, 8) + keyboard_noise),
        (t0 + 21, _usbpcap(6, 0, 0, _setup(SET_REPORT, 90))),
        (t0 + 21.001, _usbpcap(6, 0, 1, key2)),  # OUT data as a separate DATA record
    ]
    with tempfile.TemporaryDirectory() as d:
        d = Path(d)
        pcap = bytearray(struct.pack("<IHHiIII", 0xA1B2C3D4, 2, 4, 0, 0, 65535, LINKTYPE_USBPCAP))
        for ts, rec in recs[:5]:
            pcap += struct.pack("<IIII", int(ts), int(round((ts % 1) * 1e6)), len(rec), len(rec)) + rec
        (d / "USBPcap1.pcap").write_bytes(pcap)

        def block(btype, body):
            body += b"\x00" * (-len(body) % 4)
            n = len(body) + 12
            return struct.pack("<II", btype, n) + body + struct.pack("<I", n)

        ng = block(0x0A0D0D0A, struct.pack("<IHHq", 0x1A2B3C4D, 1, 0, -1))
        ng += block(1, struct.pack("<HHI", LINKTYPE_USBPCAP, 0, 65535))
        for ts, rec in recs[5:]:
            us = int(round(ts * 1e6))
            ng += block(6, struct.pack("<IIIII", 0, us >> 32, us & 0xFFFFFFFF, len(rec), len(rec)) + rec)
        (d / "USBPcap2.pcapng").write_bytes(ng)

        (d / "markers.json").write_text(json.dumps({"mouse": "synthetic", "steps": [
            {"id": "00", "label": "idle", "idle": True, "start": t0, "end": t0 + 10},
            {"id": "01", "label": "back -> 1", "start": t0 + 10, "end": t0 + 20},
            {"id": "02", "label": "back -> 2", "start": t0 + 20, "end": t0 + 30},
        ]}), encoding="utf-8")

        out = analyze(d)

    checks = [
        ("finds 5 reports in both formats", "5 Razer reports across 2 capture file(s)" in out),
        ("hides idle chatter", out.count("class 0x00 id 0x84") == 0),
        ("step 01 shows the new SET", "SET x1   class 0x77 id 0x0C size  5 txn 0x1F | 01 04 02 1e 00" in out),
        ("SET with separate DATA record", "| 01 04 02 1f 00" in out),
        ("diff isolates the changed byte", "01 -> 02  class 0x77 id 0x0C: [11] 1E->1F" in out),
        ("ignores non-Razer traffic", "4" not in out.split("(bus, address): ")[1].split("\n")[0]),
    ]
    for name, ok in checks:
        print(("PASS " if ok else "FAIL ") + name)
    if not all(ok for _, ok in checks):
        print("\n" + out)
        raise SystemExit(1)


if __name__ == "__main__":
    args = sys.argv[1:]
    if args == ["--self-test"]:
        self_test()
    elif args and not args[0].startswith("-"):
        text = analyze(args[0], show_all="--all" in args)
        print(text)
        (Path(args[0]) / "report.md").write_text(text, encoding="utf-8")
    else:
        print(__doc__)
        raise SystemExit(2)
