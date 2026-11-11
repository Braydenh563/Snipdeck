#!/usr/bin/env python3
"""Regenerate the Tips pictures from the golden renders.

Each picture is a crop of one light-theme golden under crates/snipdeck-ui/tests/golden/, scaled to
320 x 150 and written to crates/snipdeck-ui/ui/images/tips/. Run it after the goldens change
(UPDATE_GOLDEN=1) so the pictures follow the UI:

    python3 scripts/tips_images.py          # rewrite the pictures
    python3 scripts/tips_images.py --check  # fail if a picture is stale (no write)

Needs Pillow. Output is deterministic (no metadata), so --check compares pixels.
"""
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
GOLDEN = ROOT / "crates/snipdeck-ui/tests/golden"
OUT = ROOT / "crates/snipdeck-ui/ui/images/tips"
SIZE = (320, 150)

# (picture, golden, crop box x, y, width, height in golden pixels; the aspect is 320:150).
CROPS = [
    ("01-open-panel.png", "panel-light", (0, 0, 320, 150)),
    ("02-search.png", "panel-search-light", (0, 30, 320, 150)),
    ("03-pinned.png", "panel-pinned-overflow-light", (0, 0, 320, 150)),
    ("04-paste-next.png", "panel-cards-light", (0, 100, 320, 150)),
    ("05-templates.png", "fill-light", (0, 0, 440, 206)),
    ("06-capture.png", "panel-recent-rows-light", (0, 40, 320, 150)),
    ("07-symbols.png", "symbols-emoji-320-light", (0, 40, 320, 150)),
    ("08-commands.png", "palette-light", (240, 0, 640, 300)),
    ("09-view.png", "main-tile-light", (230, 0, 640, 300)),
    ("10-sensitive.png", "panel-light", (0, 340, 320, 150)),
]


def render(golden: str, box: tuple) -> Image.Image:
    x, y, w, h = box
    image = Image.open(GOLDEN / f"{golden}.png").convert("RGB")
    crop = image.crop((x, y, x + w, y + h))
    if crop.size != SIZE:
        crop = crop.resize(SIZE, Image.LANCZOS)
    return crop


def main() -> int:
    check = "--check" in sys.argv[1:]
    stale = []
    OUT.mkdir(parents=True, exist_ok=True)
    for name, golden, box in CROPS:
        image = render(golden, box)
        path = OUT / name
        if check:
            if not path.is_file() or Image.open(path).convert("RGB").tobytes() != image.tobytes():
                stale.append(name)
        else:
            image.save(path, optimize=True)
    if stale:
        print("stale tip pictures (run scripts/tips_images.py):", ", ".join(stale))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
