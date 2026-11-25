#!/usr/bin/env python3
"""Generate assets/logo/*: the SVGs, `snipdeck.ico` (16-256 px, PNG entries) and `tray-<n>.png`.

The wordmark is outlined from the bundled IBM Plex Sans SemiBold (no font needed to render). The .ico and
the tray PNGs are rasterised with cairosvg. Per docs/reviews/fable-analysis.md L2 the icon is drawn per size:
16/20/24 px = one card with the blue index tab and two bars; 32-48 px = two cards; 64+ = the full stack.
"""
import struct
from pathlib import Path
from fontTools.ttLib import TTFont
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen

ROOT = Path(__file__).resolve().parent.parent
FONT = ROOT / "crates/snipdeck-ui/fonts/IBMPlexSans-SemiBold.ttf"
OUT = ROOT / "assets/logo"

ICON = """<defs>
  <linearGradient id="bg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#2D2B28"/><stop offset="1" stop-color="#171613"/></linearGradient>
</defs>
<rect x="2" y="2" width="252" height="252" rx="56" fill="url(#bg)" stroke="#5D5A57" stroke-width="3"/>
<g transform="rotate(-10 128 132)"><rect x="60" y="40" width="136" height="152" rx="16" fill="#A09E9A"/></g>
<g transform="rotate(-4 128 132)"><rect x="60" y="50" width="136" height="152" rx="16" fill="#DEDCD8"/></g>
<path d="M76 62h80l40 40v90a16 16 0 0 1-16 16H76a16 16 0 0 1-16-16V78a16 16 0 0 1 16-16z" fill="#FBFAF7"/>
<path d="M156 62v26a14 14 0 0 0 14 14h26z" fill="#C6C4C0"/>
<rect x="60" y="62" width="30" height="146" rx="0" fill="#2870D4" clip-path="inset(0)"/>
<rect x="84" y="94" width="76" height="11" rx="5.5" fill="#201E1B"/>
<rect x="84" y="122" width="96" height="11" rx="5.5" fill="#A09E9A"/>
<rect x="84" y="150" width="70" height="11" rx="5.5" fill="#A09E9A"/>"""

def icon_svg(inner=ICON):
    # The left index tab: clipped to the card by drawing it as its own rounded path.
    inner = inner.replace('<rect x="60" y="62" width="30" height="146" rx="0" fill="#2870D4" clip-path="inset(0)"/>',
        '<path d="M76 62h14v146H76a16 16 0 0 1-16-16V78a16 16 0 0 1 16-16z" fill="#438AF1"/>')
    return inner

def wordmark(text, fill, size=132):
    font = TTFont(FONT); gs = font.getGlyphSet(); cmap = font.getBestCmap()
    upm = font["head"].unitsPerEm; scale = size / upm; x = 0; parts = []
    for ch in text:
        name = cmap[ord(ch)]
        pen = SVGPathPen(gs); tp = TransformPen(pen, (scale, 0, 0, -scale, x, 0)); gs[name].draw(tp)
        parts.append(pen.getCommands()); x += gs[name].width * scale
    return " ".join(parts), x

# ---- Size-specific marks (L2) -------------------------------------------------------------------
TILE = ('<defs><linearGradient id="bg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#2D2B28"/>'
        '<stop offset="1" stop-color="#171613"/></linearGradient></defs>'
        '<rect x="2" y="2" width="252" height="252" rx="56" fill="url(#bg)" stroke="#5D5A57" stroke-width="3"/>')
# One card, the blue index tab on its left edge, a title bar and a shorter preview bar (ink on paper).
CARD = ('<rect x="52" y="36" width="152" height="184" rx="22" fill="#FBFAF7"/>'
        '<path d="M74 36h10v184H74a22 22 0 0 1-22-22V58a22 22 0 0 1 22-22z" fill="#438AF1"/>'
        '<rect x="104" y="84" width="84" height="26" rx="13" fill="#201E1B"/>'
        '<rect x="104" y="134" width="56" height="26" rx="13" fill="#201E1B"/>')
BACK_CARD = '<g transform="rotate(-9 128 128)"><rect x="52" y="36" width="152" height="184" rx="22" fill="#A09E9A"/></g>'
SMALL = TILE + CARD
MEDIUM = TILE + BACK_CARD + CARD
# The tray glyph has no tile: a light card with a dark hairline reads on dark and on light taskbars.
GLYPH = ('<rect x="40" y="16" width="176" height="224" rx="28" fill="#FBFAF7" stroke="#201E1B" stroke-width="16"/>'
         '<path d="M68 24h10v208H68a28 28 0 0 1-28-28V52a28 28 0 0 1 28-28z" fill="#438AF1" stroke="#201E1B" stroke-width="16" stroke-linejoin="round"/>'
         '<rect x="108" y="76" width="84" height="30" rx="15" fill="#201E1B"/>'
         '<rect x="108" y="132" width="56" height="30" rx="15" fill="#201E1B"/>')

ICO_SIZES = (16, 20, 24, 32, 48, 64, 128, 256)
TRAY_SIZES = (16, 20, 24, 32)


def svg_doc(body):
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">{body}</svg>'


def mark_for(size):
    if size <= 24:
        return SMALL
    if size <= 48:
        return MEDIUM
    return icon_svg()


def render(body, size):
    import cairosvg
    return cairosvg.svg2png(bytestring=svg_doc(body).encode(), output_width=size, output_height=size)


def ico_bytes(images):
    """An .ico of PNG-compressed entries; `images` is [(size, png bytes)]. 256 is stored as 0."""
    head = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries, data = b"", b""
    for size, png in images:
        dim = 0 if size >= 256 else size
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(png), offset + len(data))
        data += png
    return head + entries + data


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
    (OUT / "snipdeck.ico").write_bytes(ico_bytes([(n, render(mark_for(n), n)) for n in ICO_SIZES]))
    for n in TRAY_SIZES:
        (OUT / f"tray-{n}.png").write_bytes(render(GLYPH, n))
    old = OUT / "snipdeck-logo.svg"
    if old.exists():
        old.unlink()

if __name__ == "__main__":
    main()
