#!/usr/bin/env python3
"""Regenerate the Tips pictures from the golden renders.

Each picture is a crop of one light-theme golden under crates/snipdeck-ui/tests/golden/, scaled to
320 pixels wide (the height follows the crop) and written to crates/snipdeck-ui/ui/images/tips/. Run it after the goldens change
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
WIDTH = 320

# (picture, golden, crop box x, y, width, height in golden pixels). Every crop starts and ends in a gap between
# rows or on an edge, never mid-text; the height is free (the Help page sizes each picture from its own aspect).
CROPS = [
    ("01-open-panel.png", "panel-light", (0, 0, 320, 166)),
    ("02-search.png", "panel-search-light", (0, 0, 320, 189)),
    ("03-pinned.png", "panel-pinned-overflow-light", (0, 0, 320, 166)),
    ("04-paste-next.png", "panel-cards-light", (0, 38, 320, 174)),
    ("05-templates.png", "fill-light", (0, 0, 440, 293)),
    ("06-capture.png", "panel-recent-rows-light", (0, 40, 320, 168)),
    ("07-symbols.png", "symbols-emoji-320-light", (0, 44, 320, 126)),
    ("08-commands.png", "palette-light", (298, 44, 438, 184)),
    ("09-view.png", "main-tile-light", (492, 0, 548, 282)),
    ("10-sensitive.png", "panel-light", (0, 350, 320, 135)),
]


def render(golden: str, box: tuple) -> Image.Image:
    x, y, w, h = box
    image = Image.open(GOLDEN / f"{golden}.png").convert("RGB")
    crop = image.crop((x, y, x + w, y + h))
    if w != WIDTH:
        crop = crop.resize((WIDTH, round(h * WIDTH / w)), Image.LANCZOS)
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
