#!/usr/bin/env python3
"""Render the MSIX package images from the site logo.

    python tools/render-msix-assets.py

Draws `site/favicon.svg` once at 1024 px with headless Edge (transparent background),
then uses Pillow to scale it into every image the manifest names, written to
`packaging/msix/Assets/`. The output is committed; rerun this only when the logo changes.

Names follow MSIX resource qualifiers, so `tools/build-msix.py` indexes them into
`resources.pri` and Windows picks the right file for the display scale:

- `Square44x44Logo` — Start menu list, taskbar, Settings > Apps. The `targetsize`
  files (plated and `altform-unplated`) are what the taskbar and Start actually use.
- `Square150x150Logo` — the Start tile. Drawn at 66% so it has the usual margin.
- `StoreLogo` — Store listing and the installer dialog.
"""
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SVG = ROOT / "site" / "favicon.svg"
OUT = ROOT / "packaging" / "msix" / "Assets"
EDGE_CANDIDATES = [
    Path(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
    Path(r"C:\Program Files\Microsoft\Edge\Application\msedge.exe"),
]
MASTER = 1024

# (file name, canvas px, logo fraction of the canvas)
IMAGES = [
    ("Square44x44Logo.scale-100.png", 44, 1.0),
    ("Square44x44Logo.scale-200.png", 88, 1.0),
    ("Square150x150Logo.scale-100.png", 150, 0.66),
    ("Square150x150Logo.scale-200.png", 300, 0.66),
    ("StoreLogo.scale-100.png", 50, 1.0),
    ("StoreLogo.scale-200.png", 100, 1.0),
]
for size in (16, 24, 32, 48, 256):
    IMAGES.append((f"Square44x44Logo.targetsize-{size}.png", size, 1.0))
    IMAGES.append((f"Square44x44Logo.targetsize-{size}_altform-unplated.png", size, 1.0))


def edge() -> Path:
    for path in EDGE_CANDIDATES:
        if path.exists():
            return path
    sys.exit("render-msix-assets: Microsoft Edge not found")


def render_master(workdir: Path) -> Image.Image:
    page = workdir / "logo.html"
    svg = SVG.read_text(encoding="utf-8").replace(
        "<svg ", f'<svg width="{MASTER}" height="{MASTER}" ', 1
    )
    page.write_text(
        "<!doctype html><html><body style='margin:0;background:transparent'>"
        f"{svg}</body></html>",
        encoding="utf-8",
    )
    shot = workdir / "master.png"
    subprocess.run(
        [
            str(edge()),
            "--headless=new",
            "--disable-gpu",
            "--hide-scrollbars",
            "--force-device-scale-factor=1",
            "--default-background-color=00000000",
            f"--window-size={MASTER},{MASTER}",
            f"--user-data-dir={workdir / 'profile'}",
            f"--screenshot={shot}",
            page.as_uri(),
        ],
        check=True,
        capture_output=True,
        timeout=60,
    )
    image = Image.open(shot).convert("RGBA")
    if image.size != (MASTER, MASTER):
        sys.exit(f"render-msix-assets: Edge produced {image.size}, wanted {MASTER}px square")
    if image.getpixel((0, 0))[3] != 0:
        sys.exit("render-msix-assets: background is not transparent")
    return image


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    workdir = Path(tempfile.mkdtemp(prefix="msix-assets-"))
    try:
        master = render_master(workdir)
    finally:
        shutil.rmtree(workdir, ignore_errors=True)
    for name, canvas, fraction in IMAGES:
        logo_px = round(canvas * fraction)
        logo = master.resize((logo_px, logo_px), Image.LANCZOS)
        out = Image.new("RGBA", (canvas, canvas), (0, 0, 0, 0))
        offset = (canvas - logo_px) // 2
        out.alpha_composite(logo, (offset, offset))
        out.save(OUT / name, optimize=True)
        print(f"wrote {name} ({canvas}px)")


if __name__ == "__main__":
    main()
