#!/usr/bin/env python3
"""Render the PNG logos an MSIX package needs into packaging/msix/Assets/.

Source: assets/logo/snipdeck-icon.svg, the full-size mark that scripts/make_logo.py writes. It is
rasterised with cairosvg in the same way make_logo.py rasterises its icons, so no new dependency.
Sizes follow docs/packaging.md, section 1, "Logos".

The package logos are generated: do not commit them. packaging/msix/.gitignore ignores Assets/.
It also writes the 300x300 Store listing logo (Partner Center, Store listing, "1:1 app tile icon") to
docs/images/store-logo-300.png. That one is committed: the owner uploads it by hand.
"""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "assets/logo/snipdeck-icon.svg"
OUT = ROOT / "packaging/msix/Assets"
LISTING = ROOT / "docs/images/store-logo-300.png"
LISTING_PX = 300

# (file stem, size in px at 100 %). The stems are the names the manifest and MakeAppx expect.
BASE = (
    ("StoreLogo", 50),
    ("Square44x44Logo", 44),
    ("Square150x150Logo", 150),
)
# Taskbar copy of the 44 px tile, drawn without a backplate.
UNPLATED = ("Square44x44Logo.targetsize-24_altform-unplated", 24)
# Scale variants: Windows puts the percentage in the file name, for example StoreLogo.scale-200.png.
SCALES = (125, 150, 200, 400)


def scaled(px, pct):
    """Pixel size at pct percent, rounded half up (50 px at 125 % is 63 px)."""
    return (px * pct + 50) // 100


def main():
    try:
        import cairosvg
    except (ImportError, OSError) as err:
        sys.exit(
            "make_msix_assets.py needs cairosvg and its native cairo library.\n"
            "  pip install cairosvg\n"
            f"  ({err})\n"
            "Nothing was written. On Windows, cairo is the hard part: run this script on Linux or macOS,\n"
            "or install the cairo DLLs first."
        )
    if not SOURCE.exists():
        sys.exit(f"Missing {SOURCE}. Run python3 scripts/make_logo.py first.")

    svg = SOURCE.read_bytes()
    OUT.mkdir(parents=True, exist_ok=True)

    def write(stem, px):
        png = cairosvg.svg2png(bytestring=svg, output_width=px, output_height=px)
        (OUT / f"{stem}.png").write_bytes(png)
        print(f"{stem}.png  {px}x{px}")

    for stem, px in BASE:
        write(stem, px)
        for pct in SCALES:
            write(f"{stem}.scale-{pct}", scaled(px, pct))
    write(*UNPLATED)
    LISTING.write_bytes(cairosvg.svg2png(bytestring=svg, output_width=LISTING_PX, output_height=LISTING_PX))
    print(f"{LISTING.relative_to(ROOT)}  {LISTING_PX}x{LISTING_PX}")


if __name__ == "__main__":
    main()
