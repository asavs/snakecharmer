#!/usr/bin/env python3
"""Regenerate the device tables in docs/DEVICE-WISHLIST.md from OpenRazer.

Everything in those tables is OpenRazer's knowledge, not ours:

  driver/razermouse_driver.h                 -> USB product ids
  driver/razermouse_driver.c                 -> transaction id, polling command family
  daemon/openrazer_daemon/hardware/mouse.py  -> model names, DPI_MAX, lighting zones

Credit is *not* generated -- it lives in CLAIMED below, so rerunning this never
clobbers a contributor's row. Add an entry when a device PR merges.

Usage:  python tools/wishlist-from-openrazer.py [--offline DIR]
"""

import argparse
import ast
import json
import pathlib
import re
import sys
import urllib.request

# pid -> (status, credit, agent/model used).  Add a line when an add-device PR merges.
CLAIMED = {
    0x005C: ("shipped", "[@asavs](https://github.com/asavs)", "Claude Fable 5"),
    0x00B2: ("shipped",
             "[@asavs](https://github.com/asavs) · "
             "[#3](https://github.com/asavs/snakecharmer/pull/3)",
             "Claude Opus 4.8 (high)"),
}

RAW = "https://raw.githubusercontent.com/openrazer/openrazer/master/"
SOURCES = {
    "razermouse_driver.h": RAW + "driver/razermouse_driver.h",
    "razermouse_driver.c": RAW + "driver/razermouse_driver.c",
    "mouse.py": RAW + "daemon/openrazer_daemon/hardware/mouse.py",
}

FAMILIES = ["DeathAdder", "Viper", "Basilisk", "Naga", "Cobra", "Mamba", "Lancehead",
            "Orochi", "Abyssus", "Pro Click", "Atheris", "Diamondback", "Imperator",
            "Ouroboros", "Taipan", "HyperPolling", "Other"]
TITLES = {"HyperPolling": "Dongles", "Other": "Other / one-offs"}
# classes whose docstring does not name the model
NAME_FIX = {0x0088: "Basilisk Ultimate (Receiver)"}
ZONES = {"logo": "logo", "scroll": "wheel", "left": "left", "right": "right",
         "backlight": "backlight"}


def fetch(offline):
    out = {}
    for name, url in SOURCES.items():
        if offline:
            out[name] = (pathlib.Path(offline) / name).read_text(encoding="utf-8")
        else:
            print("fetching", url, file=sys.stderr)
            with urllib.request.urlopen(url, timeout=60) as response:
                out[name] = response.read().decode("utf-8")
    return out


def parse_devices(src):
    """mouse.py -> [{pid, name, dpi_max, zones}], resolving class inheritance."""
    classes, order = {}, []
    for node in ast.parse(src).body:
        if not isinstance(node, ast.ClassDef):
            continue
        info = {"bases": [b.id if isinstance(b, ast.Name) else b.attr for b in node.bases
                          if isinstance(b, (ast.Name, ast.Attribute))],
                "doc": (ast.get_docstring(node) or "").strip(), "attrs": {}}
        for st in node.body:
            if not (isinstance(st, ast.Assign) and len(st.targets) == 1
                    and isinstance(st.targets[0], ast.Name)):
                continue
            key = st.targets[0].id
            try:
                info["attrs"][key] = ast.literal_eval(st.value)
            except ValueError:
                if key != "METHODS":  # METHODS = Parent.METHODS + [...]
                    continue
                extra, value = [], st.value
                while isinstance(value, ast.BinOp):
                    try:
                        extra = ast.literal_eval(value.right) + extra
                    except ValueError:
                        pass
                    value = value.left
                info["attrs"]["_methods_extra"] = extra
                info["attrs"]["_methods_parent"] = (
                    value.value.id if isinstance(value, ast.Attribute)
                    and isinstance(value.value, ast.Name) else None)
        classes[node.name] = info
        order.append(node.name)

    def attr(name, key, seen=frozenset()):
        if name not in classes or name in seen:
            return None
        cls = classes[name]
        if key in cls["attrs"]:
            return cls["attrs"][key]
        for base in cls["bases"]:
            got = attr(base, key, seen | {name})
            if got is not None:
                return got
        return None

    def methods(name, seen=frozenset()):
        if name not in classes or name in seen:
            return []
        cls = classes[name]
        if "METHODS" in cls["attrs"]:
            return cls["attrs"]["METHODS"]
        if "_methods_extra" in cls["attrs"]:
            parent = cls["attrs"]["_methods_parent"]
            return methods(parent, seen | {name}) + cls["attrs"]["_methods_extra"]
        for base in cls["bases"]:
            got = methods(base, seen | {name})
            if got:
                return got
        return []

    devices = []
    for name in order:
        pid = attr(name, "USB_PID")
        if pid is None:
            continue
        implemented = methods(name)
        model = classes[name]["doc"].replace("Class for the ", "") or name
        devices.append({
            "pid": pid,
            "name": NAME_FIX.get(pid, model).replace("Razer ", "").strip(),
            "dpi_max": attr(name, "DPI_MAX"),
            "zones": [label for zone, label in ZONES.items()
                      if "set_%s_static" % zone in implemented
                      or "set_%s_spectrum" % zone in implemented],
        })
    return devices


def function_body(lines, signature):
    """The lines of one C function, from its signature to its closing brace."""
    start = next(i for i, line in enumerate(lines) if line.startswith(signature))
    depth, opened, end = 0, False, start
    for end in range(start, len(lines)):
        depth += lines[end].count("{") - lines[end].count("}")
        opened = opened or "{" in lines[end]
        if opened and depth == 0:
            break
    return lines[start:end + 1]


def parse_driver(header, driver):
    """-> (pid -> transaction id, pid -> polling command family)."""
    pid_of = {m.group(1): int(m.group(2), 16) for m in
              re.finditer(r"#define\s+(USB_DEVICE_ID_RAZER_\w+)\s+0x([0-9A-Fa-f]+)", header)}
    lines = driver.splitlines()

    def sweep(body, value_of):
        """Collect case labels, assigning each run the value the run ends in."""
        found, pending = {}, []
        for line in body:
            case = re.search(r"case\s+(USB_DEVICE_ID_RAZER_\w+):", line)
            if case:
                pending.append(case.group(1))
                continue
            value = value_of(line)
            if value is not None and pending:
                for label in pending:
                    if label in pid_of:
                        found[pid_of[label]] = value
                pending = []
        return found

    def transaction_id(line):
        match = re.search(r"transaction_id\.id\s*=\s*0x([0-9A-Fa-f]+)", line)
        return int(match.group(1), 16) if match else None

    def polling_family(line):
        match = re.search(r"razer_chroma_misc_set_polling_rate(2?)\(", line)
        if match:
            return "Extended" if match.group(1) else "Classic"
        if re.search(r"deathadder3_5g_set_poll_rate|set_orochi2011_poll_dpi", line):
            return "own"
        return None

    dpi = function_body(lines, "static ssize_t razer_attr_write_dpi(")
    # the first switch in that function handles legacy byte-DPI models; the transaction
    # ids we want are in the second one, which the driver marks with this comment
    modern = dpi[next(i for i, line in enumerate(dpi)
                      if "New devices set the device ID properly" in line):]
    poll = function_body(lines, "static ssize_t razer_attr_write_poll_rate(")
    return sweep(modern, transaction_id), sweep(poll, polling_family)


def render(devices, txn, poll):
    def family(name):
        return next((f for f in FAMILIES if name.startswith(f)), "Other")

    def row(device):
        pid = device["pid"]
        transaction = txn.get(pid)
        if transaction is None:
            txn_cell = "—<br><sub>legacy byte-DPI</sub>"
        else:
            hint = {0x3F: "<br><sub>copy Elite</sub>", 0x1F: "<br><sub>copy V3</sub>"}
            txn_cell = "`0x%02X`%s" % (transaction,
                                       hint.get(transaction, "<br><sub>new txn path</sub>"))
        polling = {"Classic": "Classic",
                   "Extended": "Extended<br><sub>→ 8000 Hz</sub>",
                   "own": "—<br><sub>own command</sub>"}.get(poll.get(pid), "—")
        if pid in CLAIMED:
            state, who, agent = CLAIMED[pid]
            mark = "✅ cracked by" if state == "shipped" else "\U0001f527 %s by" % state
            status = "%s %s" % (mark, who)
            if agent:
                status += "<br><sub>%s</sub>" % agent
        else:
            status = "open"
        return "| %s | `1532:%04X` | %s | %s | %s | %s | %s |" % (
            device["name"], pid, txn_cell, device["dpi_max"] or "?",
            ", ".join(device["zones"]) or "—", polling, status)

    head = ("| Model | USB id | txn | Max DPI | RGB zones | Polling | Status |\n"
            "|---|---|---|---|---|---|---|")
    groups = {}
    for device in devices:
        groups.setdefault(family(device["name"]), []).append(device)
    sections = []
    for name in FAMILIES:
        if name in groups:
            rows = sorted(groups[name], key=lambda d: d["pid"])
            sections.append("### %s\n\n%s\n%s\n" % (TITLES.get(name, name), head,
                                                    "\n".join(row(d) for d in rows)))
    return "\n".join(sections)


def main():
    parser = argparse.ArgumentParser(description="Regenerate docs/DEVICE-WISHLIST.md.")
    parser.add_argument("--offline", metavar="DIR",
                        help="read the three OpenRazer files from DIR instead of fetching")
    parser.add_argument("--json", action="store_true",
                        help="dump the parsed devices as JSON and exit")
    args = parser.parse_args()

    src = fetch(args.offline)
    devices = parse_devices(src["mouse.py"])
    txn, poll = parse_driver(src["razermouse_driver.h"], src["razermouse_driver.c"])
    if args.json:
        json.dump(devices, sys.stdout, indent=1)
        return

    doc = pathlib.Path(__file__).resolve().parent.parent / "docs" / "DEVICE-WISHLIST.md"
    text = doc.read_text(encoding="utf-8")
    begin, end = "<!-- BEGIN GENERATED TABLES -->", "<!-- END GENERATED TABLES -->"
    before, rest = text.split(begin, 1)
    _, after = rest.split(end, 1)
    doc.write_text("%s%s\n\n%s%s%s" % (before, begin, render(devices, txn, poll), end, after),
                   encoding="utf-8")
    print("%d devices written to %s" % (len(devices), doc))


if __name__ == "__main__":
    main()
