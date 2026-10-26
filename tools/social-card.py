"""Build the social preview card (1280x640) as HTML from the README art.

    python tools/social-card.py README.md card.html

Render it to `site/card.png`, which is both the repo's social preview (uploaded by hand
under Settings -> Social preview) and the site's og:image:

    msedge --headless --disable-gpu --hide-scrollbars --window-size=1280,640
           --screenshot=site/card.png file:///<absolute path>/card.html
"""
import html
import sys
from pathlib import Path

readme, out = Path(sys.argv[1]), Path(sys.argv[2])
lines = readme.read_text(encoding="utf-8").splitlines()
start = next(i for i, l in enumerate(lines) if "_.-==-._" in l)
end = next(i for i, l in enumerate(lines) if "(__________________)" in l)
art = [l.rstrip() for l in lines[start : end + 1]]
indent = min(len(l) - len(l.lstrip()) for l in art if l.strip())
art = [l[indent:] for l in art]

def paint(line: str) -> str:
    s = html.escape(line)
    for note in ("♪", "♫"):
        s = s.replace(note, f'<span class="note">{note}</span>')
    return s

art_html = "\n".join(paint(l) for l in art)

out.write_text(f"""<!doctype html>
<html><head><meta charset="utf-8">
<style>
  :root {{
    --bg: #0c1a15; --ink: #eaf7f0; --muted: #8fb8a6;
    --art: #6fd3aa; --note: #f0c768;
  }}
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  html, body {{ width: 1280px; height: 640px; overflow: hidden; }}
  body {{
    background: var(--bg);
    color: var(--ink); font-family: "Segoe UI", system-ui, sans-serif;
    display: flex; align-items: center; gap: 40px; padding: 0 48px;
  }}
  pre {{
    font-family: Consolas, "Cascadia Mono", monospace; font-size: 24px;
    line-height: 1.16; color: var(--art); flex: none;
  }}
  .note {{ color: var(--note); }}
  .text {{ display: flex; flex-direction: column; gap: 24px; }}
  h1 {{ font-size: 66px; font-weight: 700; letter-spacing: -1.5px; line-height: 1; }}
  .tag {{ font-size: 28px; color: var(--muted); line-height: 1.3; }}
  .tag b {{ color: var(--ink); font-weight: 600; }}
  .feat {{ font-size: 23px; white-space: nowrap; color: var(--art); font-weight: 600; letter-spacing: 0.3px; }}
  .facts {{ font-size: 25px; line-height: 1.5; white-space: nowrap; }}
  .facts b {{ font-weight: 700; }}
  .facts span {{ color: var(--muted); }}
  .trust {{ font-size: 23px; line-height: 1.35; color: var(--muted); }}
  .url {{ font-family: Consolas, monospace; font-size: 23px; color: var(--art); }}
</style></head>
<body>
<pre>{art_html}</pre>
<div class="text">
  <h1>Snakecharmer</h1>
  <div class="tag">Configure Razer mice<br>without Synapse</div>
  <div class="feat">DPI &middot; polling rate &middot; button binds &middot; lighting</div>
  <div class="facts"><b>17 MB</b> idle RAM <span>(Synapse: ~558 MB)</span><br><b>1</b> process <span>(Synapse: 6)</span></div>
  <div class="trust">Windows &middot; open source<br>no account &middot; no telemetry</div>
  <div class="url">github.com/asavs/snakecharmer</div>
</div>
</body></html>
""", encoding="utf-8")
print(f"{len(art)} art lines, {max(map(len, art))} cols")
