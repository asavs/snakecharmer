#!/usr/bin/env python3
"""Build the GitHub Pages site from `site/` into an output directory.

    python tools/build-site.py _site

`site/index.html` is a template. The parts that would otherwise go stale are filled
from the docs they come from:

- {{ART}}, {{CAPTION}}: the ASCII scene in the README's opening code block
- {{MICE}}: the table in docs/SUPPORTED-DEVICES.md, with credits from the wishlist
- {{OPEN_COUNT}}, {{FAMILIES}}: the `open` rows in docs/DEVICE-WISHLIST.md

Every other file in `site/` is copied as is. The build fails if a source doc no longer
parses, so CI catches the drift before Pages publishes a broken page.
"""
import html
import re
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SITE = ROOT / "site"
REPO = "https://github.com/asavs/snakecharmer"


def fail(msg: str) -> None:
    sys.exit(f"build-site: {msg}")


def md_links(text: str) -> str:
    """Escape `text`, then turn its [label](url) links into anchors."""
    out, pos = [], 0
    for m in re.finditer(r"\[([^\]]+)\]\(([^)]+)\)", text):
        out.append(html.escape(text[pos : m.start()]))
        out.append(f'<a href="{html.escape(m.group(2))}">{html.escape(m.group(1))}</a>')
        pos = m.end()
    out.append(html.escape(text[pos:]))
    return "".join(out)


def table_rows(lines: list[str]) -> list[list[str]]:
    """Cells of each body row of the first markdown table in `lines`."""
    rows, seen_header = [], False
    for line in lines:
        if not line.startswith("|"):
            if rows:
                break
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if not seen_header:
            seen_header = True
        elif not set("".join(cells)) <= set("-: "):
            rows.append(cells)
    return rows


# --- ASCII scene ----------------------------------------------------------------

def span(cls: str, text: str) -> str:
    return f'<span class="{cls}">{html.escape(text)}</span>' if text else ""


def paint_scene(scene: list[str]) -> str:
    """Escape the scene and color it by region: sun, desert, flute, notes."""
    sun = next((i for i, l in enumerate(scene) if "(  O  )" in l), None)
    ground = next((i for i, l in enumerate(scene) if "__________/" in l), None)
    if sun is None or ground is None:
        fail("README scene changed: can't find the sun or the ground line")
    sun_rows = range(sun - 2, sun + 3)
    desert_rows = range(sun + 1, ground + 2)
    sun_cols = 26

    out = []
    for i, line in enumerate(scene):
        if i in sun_rows or i in desert_rows:
            head, tail = line[:sun_cols], line[sun_cols:]
            out.append(
                (span("sun", head) if i in sun_rows else span("sand", head))
                + (span("sand", tail) if i in desert_rows else html.escape(tail))
            )
            continue
        s = html.escape(line)
        s = re.sub(r"(=+&lt;&gt;=+)", r'<span class="flute">\1</span>', s)
        s = re.sub(r"([♪♫])", r'<span class="note">\1</span>', s)
        out.append(s)
    return "\n".join(out)


def readme_art() -> tuple[str, str]:
    lines = (ROOT / "README.md").read_text(encoding="utf-8").splitlines()
    try:
        start = lines.index("```") + 1
        block = lines[start : lines.index("```", start)]
    except ValueError:
        fail("README no longer opens with the ASCII art code block")
    rules = [i for i, l in enumerate(block) if l.startswith("=====")]
    if len(rules) != 4:
        fail(f"README art should have 4 ===== rules, found {len(rules)}")
    scene = block[rules[1] + 1 : rules[2]]
    while scene and not scene[0].strip():
        scene.pop(0)
    while scene and not scene[-1].strip():
        scene.pop()
    caption = " ".join(l.strip() for l in block[rules[2] + 1 : rules[3]])
    return paint_scene([l.rstrip() for l in scene]), html.escape(caption)


# --- Devices ----------------------------------------------------------------------

def wishlist() -> tuple[dict[str, str], int, list[tuple[str, int]]]:
    """Credits by USB id, the open-row count, and open rows per family."""
    text = (ROOT / "docs" / "DEVICE-WISHLIST.md").read_text(encoding="utf-8")
    m = re.search(r"<!-- BEGIN GENERATED TABLES -->(.*)<!-- END GENERATED TABLES -->", text, re.S)
    if not m:
        fail("DEVICE-WISHLIST.md lost its generated-table markers")
    credits, families, total = {}, [], 0
    for section in re.split(r"^### ", m.group(1), flags=re.M)[1:]:
        name, _, body = section.partition("\n")
        rows = table_rows(body.splitlines())
        if name.strip() == "Dongles":
            continue  # receivers, not mice; they don't belong in "N more mice"
        open_rows = sum(1 for r in rows if r[-1] == "open")
        total += open_rows
        if open_rows:
            families.append((name.strip(), open_rows))
        for r in rows:
            status = r[-1]
            if status.startswith("✅"):
                who = re.sub(r"<br>.*", "", status).replace("✅ cracked by", "").strip()
                for pid in re.findall(r"`(1532:[0-9A-Fa-f]{4})`", r[1]):
                    credits[pid.upper()] = who
    if total == 0 or not families:
        fail("DEVICE-WISHLIST.md has no open rows; did the table format change?")
    return credits, total, families


def mice_table(credits: dict[str, str]) -> str:
    lines = (ROOT / "docs" / "SUPPORTED-DEVICES.md").read_text(encoding="utf-8").splitlines()
    rows = table_rows(lines)
    if not rows or len(rows[0]) != 8:
        fail("SUPPORTED-DEVICES.md table changed shape (expected 8 columns)")
    out = []
    for model, usb, _txn, zones, _dpi_buttons, dpi, polling, _verified in rows:
        pids = [p.upper() for p in re.findall(r"`(1532:[0-9A-Fa-f]{4})`", usb)]
        max_dpi = int(dpi.split("–")[-1])
        max_poll = int(polling.split("/")[-1])
        lighting = "—" if zones == "none" else html.escape(zones)
        who = next((credits[p] for p in pids if p in credits), "")
        out.append(
            f"        <tr><th scope=\"row\">{html.escape(model)}</th>"
            f"<td>{max_dpi:,}</td><td>{max_poll:,} Hz</td><td>{lighting}</td>"
            f"<td>{md_links(who) if who else '—'}</td></tr>"
        )
    return "\n".join(out)


def main() -> None:
    if len(sys.argv) != 2:
        fail("usage: build-site.py <output-dir>")
    out = Path(sys.argv[1])
    art, caption = readme_art()
    credits, open_count, families = wishlist()
    named = [f"{html.escape(name)} ({n})" for name, n in families]
    family_list = ", ".join(named[:-1]) + ", and " + named[-1]
    page = (SITE / "index.html").read_text(encoding="utf-8")
    for token, value in {
        "{{ART}}": art,
        "{{CAPTION}}": caption,
        "{{MICE}}": mice_table(credits),
        "{{OPEN_COUNT}}": str(open_count),
        "{{FAMILIES}}": family_list,
        "{{REPO}}": REPO,
    }.items():
        if token not in page:
            fail(f"site/index.html is missing {token}")
        page = page.replace(token, value)
    if "{{" in page:
        fail("site/index.html has an unknown {{token}}")

    if out.exists():
        shutil.rmtree(out)
    shutil.copytree(SITE, out)
    (out / "index.html").write_text(page, encoding="utf-8")
    print(f"built {out}: {open_count} open mice across {len(families)} families")


if __name__ == "__main__":
    main()
