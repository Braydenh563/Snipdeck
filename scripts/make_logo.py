#!/usr/bin/env python3
"""Generate assets/logo/*.svg. Wordmark is outlined from the bundled IBM Plex Sans SemiBold (no font needed to render)."""
from pathlib import Path
from fontTools.ttLib import TTFont
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen

ROOT = Path(__file__).resolve().parent.parent
FONT = ROOT / "crates/snipdeck-ui/fonts/IBMPlexSans-SemiBold.ttf"
OUT = ROOT / "assets/logo"

ICON = """<defs>
  <linearGradient id="bg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#438AF1"/><stop offset="1" stop-color="#195AB3"/></linearGradient>
</defs>
<rect width="256" height="256" rx="58" fill="url(#bg)"/>
<g transform="rotate(-10 128 132)"><rect x="60" y="40" width="136" height="152" rx="16" fill="#fff" fill-opacity=".32"/></g>
<g transform="rotate(-4 128 132)"><rect x="60" y="50" width="136" height="152" rx="16" fill="#fff" fill-opacity=".62"/></g>
<path d="M76 62h80l40 40v90a16 16 0 0 1-16 16H76a16 16 0 0 1-16-16V78a16 16 0 0 1 16-16z" fill="#FBFAF7"/>
<path d="M156 62v26a14 14 0 0 0 14 14h26z" fill="#C6C4C0"/>
<rect x="60" y="62" width="30" height="146" rx="0" fill="#2870D4" clip-path="inset(0)"/>
<rect x="84" y="94" width="76" height="11" rx="5.5" fill="#201E1B"/>
<rect x="84" y="122" width="96" height="11" rx="5.5" fill="#A09E9A"/>
<rect x="84" y="150" width="70" height="11" rx="5.5" fill="#A09E9A"/>"""

def icon_svg(inner=ICON):
    # The left index tab: clipped to the card by drawing it as its own rounded path.
    inner = inner.replace('<rect x="60" y="62" width="30" height="146" rx="0" fill="#2870D4" clip-path="inset(0)"/>',
        '<path d="M76 62h14v146H76a16 16 0 0 1-16-16V78a16 16 0 0 1 16-16z" fill="#2870D4"/>')
    return inner

def wordmark(text, fill, size=132):
    font = TTFont(FONT); gs = font.getGlyphSet(); cmap = font.getBestCmap()
    upm = font["head"].unitsPerEm; scale = size / upm; x = 0; parts = []
    for ch in text:
        name = cmap[ord(ch)]
        pen = SVGPathPen(gs); tp = TransformPen(pen, (scale, 0, 0, -scale, x, 0)); gs[name].draw(tp)
        parts.append(pen.getCommands()); x += gs[name].width * scale
    return " ".join(parts), x

def main():
    OUT.mkdir(parents=True, exist_ok=True)
    body = icon_svg()
    (OUT / "snipdeck-icon.svg").write_text(
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" role="img" aria-label="Snipdeck"><title>Snipdeck</title>{body}</svg>\n')
    for tag, fill in (("light", "#201E1B"), ("dark", "#FBFAF7")):
        d, w = wordmark("Snipdeck", fill)
        W = 256 + 40 + w
        (OUT / f"snipdeck-logo-{tag}.svg").write_text(
            f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W:.0f} 256" role="img" aria-label="Snipdeck"><title>Snipdeck</title>'
            f'{body}<path transform="translate(296 168)" d="{d}" fill="{fill}"/></svg>\n')
    old = OUT / "snipdeck-logo.svg"
    if old.exists():
        old.unlink()

main()
