"""Build the social preview card (1280x640) as HTML from the README art.

    python tools/social-card.py card.html

The art is the full README scene, colored the same way as the site (see
tools/build-site.py). Render it to `site/card.png`, which is both the repo's social
preview (uploaded by hand under Settings -> Social preview) and the site's og:image:

    msedge --headless --disable-gpu --hide-scrollbars --window-size=1280,640
           --screenshot=site/card.png file:///<absolute path>/card.html
"""
import importlib.util
import sys
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "build_site", Path(__file__).resolve().parent / "build-site.py"
)
build_site = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build_site)

out = Path(sys.argv[1])
art_html, _caption = build_site.readme_art()

out.write_text(f"""<!doctype html>
<html><head><meta charset="utf-8">
<style>
  :root {{
    --bg: #0f1a16; --ink: #e4ede8; --muted: #98ada3; --art: #6fcfa6;
    --sun: #e6c16a; --sand: #c6a674; --flute: #d8aa55;
  }}
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  html, body {{ width: 1280px; height: 640px; overflow: hidden; }}
  body {{
    background: var(--bg); color: var(--ink); font-family: "Segoe UI", system-ui, sans-serif;
    display: flex; align-items: center; gap: 44px; padding: 0 56px;
  }}
  pre {{ font: 17px/1.15 Consolas, "Cascadia Mono", monospace; color: var(--art); flex: none; }}
  .sun, .note {{ color: var(--sun); }}
  .sand {{ color: var(--sand); }}
  .flute {{ color: var(--flute); }}
  .text {{ display: flex; flex-direction: column; gap: 21px; }}
  h1 {{ font-size: 56px; font-weight: 700; letter-spacing: -1px; line-height: 1; }}
  .tag {{ font-size: 24px; }}
  .feat {{ font-size: 20px; color: var(--art); font-weight: 600; }}
  .facts {{ font-size: 24px; line-height: 1.5; }}
  .facts span {{ color: var(--muted); }}
  .trust {{ font-size: 19px; color: var(--muted); }}
  .url {{ font: 19px Consolas, monospace; color: var(--art); }}
</style></head>
<body>
<pre>{art_html}</pre>
<div class="text">
  <h1>Snakecharmer</h1>
  <div class="tag">Lightweight Razer mouse config</div>
  <div class="feat">DPI &middot; polling rate &middot; button binds &middot; lighting</div>
  <div class="facts"><b>558 MB &rarr; 17 MB</b> <span>RAM</span><br><b>6 &rarr; 1</b> <span>process</span></div>
  <div class="trust">Open source, copyleft, based on OpenRazer</div>
  <div class="url">github.com/asavs/snakecharmer</div>
</div>
</body></html>
""", encoding="utf-8")
