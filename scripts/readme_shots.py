#!/usr/bin/env python3
"""Frames 2x renders of the goldens into the README screenshots in docs/images.

Render first (any test failures from the 2x scale are expected and ignored):

    SNIPDECK_SHOT_SCALE=2 SNIPDECK_SHOTS_DIR=/tmp/shots \\
        cargo test -p snipdeck-ui --no-fail-fast -- --test-threads=4
    pip install pillow pyoxipng
    python3 scripts/readme_shots.py /tmp/shots

Each shot gets rounded corners, a hairline border and a soft shadow on a transparent
background, so it sits well on GitHub's light and dark pages. The hero image composes the
Main window with the Panel and the Picker in front of it.
"""

import io
import sys
from pathlib import Path

import oxipng  # pip install pyoxipng
from PIL import Image, ImageChops, ImageDraw, ImageFilter

OUT = Path(__file__).resolve().parent.parent / "docs" / "images"
RADIUS = 20  # 10 logical px at 2x
PAD = 48  # room for the shadow

# README name -> golden name (rendered as <golden>-light.png / <golden>-dark.png).
SHOTS = {
    "main": "main",
    "main-tile": "main-tile",
    "panel": "panel",
    "picker": "picker",
    "fill": "fill",
    "editor": "item-editor-text",
    "settings": "settings",
}


def rounded(im: Image.Image, dark: bool) -> Image.Image:
    im = im.convert("RGBA")
    mask = Image.new("L", im.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, im.width - 1, im.height - 1), RADIUS, fill=255)
    im.putalpha(mask)
    border = (255, 255, 255, 40) if dark else (0, 0, 0, 38)
    ImageDraw.Draw(im).rounded_rectangle(
        (0, 0, im.width - 1, im.height - 1), RADIUS, outline=border, width=2
    )
    return im


def shadow(size, dark: bool) -> Image.Image:
    w, h = size
    sh = Image.new("RGBA", (w + 2 * PAD, h + 2 * PAD), (0, 0, 0, 0))
    ImageDraw.Draw(sh).rounded_rectangle(
        (PAD, PAD + 12, PAD + w, PAD + h + 12), RADIUS, fill=(0, 0, 0, 150 if dark else 70)
    )
    return sh.filter(ImageFilter.GaussianBlur(22))


def framed(im: Image.Image, dark: bool) -> Image.Image:
    win = rounded(im, dark)
    canvas = shadow(win.size, dark)
    canvas.alpha_composite(win, (PAD, PAD))
    return canvas


def trim_settings(im: Image.Image) -> Image.Image:
    # The Settings render is tall; show the top, which holds the everyday options. The cut
    # (1300px at 2x) sits in the gap between the Panel width row and Window behaviour, so no
    # row is clipped.
    return im.crop((0, 0, im.width, min(im.height, 1300)))


def hero(src: Path, theme: str) -> Image.Image | None:
    try:
        main = Image.open(src / f"main-{theme}.png")
        panel = Image.open(src / f"panel-{theme}.png")
        picker = Image.open(src / f"picker-{theme}.png")
    except FileNotFoundError:
        return None
    dark = theme == "dark"
    m, p, k = framed(main, dark), framed(panel, dark), framed(picker, dark)
    w = m.width + p.width // 2
    h = max(m.height + k.height // 3, p.height + 80)
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    out.alpha_composite(m, (0, 0))
    out.alpha_composite(k, (m.width // 2 - k.width // 2, m.height - (2 * k.height) // 3))
    out.alpha_composite(p, (w - p.width, 80))
    return out


def save(im: Image.Image, path: Path) -> None:
    # Lossless: a 256-colour palette flattens the light theme's selection tint into the page.
    buf = io.BytesIO()
    im.save(buf, "PNG")
    path.write_bytes(oxipng.optimize_from_memory(buf.getvalue(), level=4))
    print(f"{path.relative_to(OUT.parent.parent)}  {im.size}  {path.stat().st_size // 1024} KB")


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    src = Path(sys.argv[1])
    OUT.mkdir(parents=True, exist_ok=True)
    missing = []
    for name, golden in SHOTS.items():
        for theme in ("light", "dark"):
            f = src / f"{golden}-{theme}.png"
            if not f.exists():
                missing.append(f.name)
                continue
            im = Image.open(f)
            if name == "settings":
                im = trim_settings(im)
            save(framed(im, theme == "dark"), OUT / f"{name}-{theme}.png")
    for theme in ("light", "dark"):
        h = hero(src, theme)
        if h is not None:
            save(h, OUT / f"hero-{theme}.png")
    if missing:
        print("missing renders:", ", ".join(missing))
    return 0


if __name__ == "__main__":
    sys.exit(main())
