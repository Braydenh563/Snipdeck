"""Generate Snipdeck colour primitives from OKLCH and verify WCAG contrast.

Usage: python3 design/tools/palette.py          -> prints markdown tables + writes design/tokens/palette.json
       python3 design/tools/palette.py --check  -> writes nothing; exits 1 on a failed contrast check or when
                                                   design/tokens/palette.json is not what this file generates
(`cargo test -p snipdeck-app palette` repeats the new-token contrast checks from the JSON, so CI runs them.)
"""
import json, math, pathlib, sys

def oklch_to_srgb(L, C, h):
    a, b = C * math.cos(math.radians(h)), C * math.sin(math.radians(h))
    l_ = L + 0.3963377774 * a + 0.2158037573 * b
    m_ = L - 0.1055613458 * a - 0.0638541728 * b
    s_ = L - 0.0894841775 * a - 1.2914855480 * b
    l, m, s = l_ ** 3, m_ ** 3, s_ ** 3
    r = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s
    g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s
    bl = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s
    def enc(x):
        x = min(max(x, 0.0), 1.0)
        return 12.92 * x if x <= 0.0031308 else 1.055 * x ** (1 / 2.4) - 0.055
    return tuple(round(enc(v) * 255) for v in (r, g, bl))

def in_gamut(L, C, h):
    a, b = C * math.cos(math.radians(h)), C * math.sin(math.radians(h))
    l_ = L + 0.3963377774 * a + 0.2158037573 * b
    m_ = L - 0.1055613458 * a - 0.0638541728 * b
    s_ = L - 0.0894841775 * a - 1.2914855480 * b
    l, m, s = l_ ** 3, m_ ** 3, s_ ** 3
    rgb = (4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
           -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
           -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s)
    return all(-1e-4 <= v <= 1 + 1e-4 for v in rgb)

def fit(L, C, h):
    while C > 0 and not in_gamut(L, C, h):
        C -= 0.002
    return L, max(C, 0), h

def hexs(rgb): return "#%02X%02X%02X" % rgb

def lum(rgb):
    def ch(c):
        c /= 255
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4
    r, g, b = map(ch, rgb)
    return 0.2126 * r + 0.7152 * g + 0.0722 * b

def contrast(a, b):
    la, lb = sorted((lum(a), lum(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)

# --- primitives -------------------------------------------------------------
NEUTRAL_HUE, NEUTRAL_C = 80, 0.006          # warm "card stock" grey
NEUTRAL_L = [0.985, 0.965, 0.935, 0.895, 0.82, 0.70, 0.58, 0.47, 0.37, 0.29, 0.235, 0.20, 0.17]
NEUTRAL_STEPS = [0, 25, 50, 100, 200, 300, 400, 500, 600, 700, 800, 850, 900]

HUES = {  # name: (hue, peak chroma)
    "blue": (258, 0.17),    # Index Blue (brand)
    "teal": (195, 0.11),
    "green": (150, 0.15),
    "amber": (75, 0.15),
    "orange": (48, 0.17),
    "red": (25, 0.19),
    "rose": (0, 0.16),
    "violet": (295, 0.16),
    "slate": (250, 0.03),
}
SCALE_L = {100: 0.96, 200: 0.91, 300: 0.84, 400: 0.74, 500: 0.64, 600: 0.555, 700: 0.48, 800: 0.40, 900: 0.32}
# chroma envelope: low at extremes, peak mid
SCALE_CF = {100: 0.22, 200: 0.38, 300: 0.6, 400: 0.85, 500: 1.0, 600: 1.0, 700: 0.92, 800: 0.8, 900: 0.65}

prims = {}
for step, L in zip(NEUTRAL_STEPS, NEUTRAL_L):
    c = NEUTRAL_C * (0.6 if L > 0.9 else 1.0)
    prims[f"neutral-{step}"] = (L, c, NEUTRAL_HUE)
for name, (h, cmax) in HUES.items():
    for step, L in SCALE_L.items():
        prims[f"{name}-{step}"] = fit(L, cmax * SCALE_CF[step], h)

hexmap = {k: hexs(oklch_to_srgb(*v)) for k, v in prims.items()}
rgbmap = {k: oklch_to_srgb(*v) for k, v in prims.items()}

# --- semantic themes ----------------------------------------------------------
THEMES = {
  "light": {
    "bg.app": "neutral-25", "bg.surface": "neutral-0", "bg.sunken": "neutral-50",
    "bg.hover": "neutral-50", "bg.selected": "blue-100", "bg.overlay": "neutral-0", "bg.overlay-hover": "neutral-50",
    "border.subtle": "neutral-100", "border.default": "neutral-200", "border.strong": "neutral-400",
    "text.primary": "neutral-850", "text.secondary": "neutral-500", "text.tertiary": "neutral-400",
    "text.on-accent": "neutral-0",
    "ink.default": "neutral-850", "ink.hover": "neutral-700", "ink.pressed": "neutral-600", "text.on-ink": "neutral-0",
    "accent.default": "blue-600", "accent.hover": "blue-700", "accent.pressed": "blue-800",
    "accent.text": "blue-700", "accent.subtle": "blue-100", "focus.ring": "blue-600",
    "success.fg": "green-700", "success.bg": "green-100",
    "warning.fg": "amber-800", "warning.bg": "amber-100",
    "danger.fg": "red-700", "danger.bg": "red-100", "danger.solid": "red-600",
    "info.fg": "teal-700", "info.bg": "teal-100",
  },
  "dark": {
    "bg.app": "neutral-900", "bg.surface": "neutral-850", "bg.sunken": "neutral-900",
    "bg.hover": "neutral-800", "bg.selected": "blue-900", "bg.overlay": "neutral-800", "bg.overlay-hover": "neutral-700",
    "border.subtle": "neutral-800", "border.default": "neutral-700", "border.strong": "neutral-400",
    "text.primary": "neutral-50", "text.secondary": "neutral-300", "text.tertiary": "neutral-400",
    "text.on-accent": "neutral-0",
    "ink.default": "neutral-50", "ink.hover": "neutral-100", "ink.pressed": "neutral-200", "text.on-ink": "neutral-850",
    "accent.default": "blue-600", "accent.hover": "blue-700", "accent.pressed": "blue-800",
    "accent.text": "blue-300", "accent.subtle": "blue-900", "focus.ring": "blue-400",
    "success.fg": "green-300", "success.bg": "green-900",
    "warning.fg": "amber-300", "warning.bg": "amber-900",
    "danger.fg": "red-300", "danger.bg": "red-900", "danger.solid": "red-600",
    "info.fg": "teal-300", "info.bg": "teal-900",
  },
}

# Item kinds (Panel v2 type tile): kind.<k>.tile / kind.<k>.glyph, one hue each. Light: step 200 tile with a 700 glyph;
# dark: step 900 tile with a 300 glyph (the D v3 mock's quiet-row values).
KIND_HUE = {"text": "amber", "template": "violet", "code": "slate", "link": "teal",
            "sensitive": "rose", "image": "green", "file": "orange"}
for _k, _hue in KIND_HUE.items():
    THEMES["light"][f"kind.{_k}.tile"], THEMES["light"][f"kind.{_k}.glyph"] = f"{_hue}-200", f"{_hue}-700"
    THEMES["dark"][f"kind.{_k}.tile"], THEMES["dark"][f"kind.{_k}.glyph"] = f"{_hue}-900", f"{_hue}-300"
# Search match mark (Panel v2): a span behind the matched text, no underline. Light blue-300 rather than the mock's
# blue-200 (1.16:1 against bg.selected; the check below wants 1.3:1).
THEMES["light"].update({"search.mark": "blue-300", "search.mark-text": "neutral-850", "toast.action": "blue-300"})
THEMES["dark"].update({"search.mark": "blue-800", "search.mark-text": "neutral-0", "toast.action": "blue-700"})

CHECKS = [  # (fg, bg, min, label)
  ("text.primary", "bg.surface", 7.0, "body text"),
  ("text.primary", "bg.app", 7.0, "body text on app bg"),
  ("text.secondary", "bg.surface", 4.5, "secondary text"),
  ("text.secondary", "bg.hover", 4.5, "secondary on hover row"),
  ("text.primary", "bg.selected", 4.5, "text on selected row"),
  ("text.primary", "bg.overlay-hover", 4.5, "text on highlighted menu row"),
  ("text.secondary", "bg.overlay-hover", 4.5, "secondary on highlighted menu row"),
  ("text.tertiary", "bg.surface", 3.0, "tertiary (icons/hints, non-essential)"),
  ("border.strong", "bg.surface", 3.0, "input borders (UI)"),
  ("text.on-ink", "ink.default", 4.5, "primary (ink) button label"),
  ("text.on-ink", "ink.hover", 4.5, "primary (ink) hover"),
  ("text.on-ink", "ink.pressed", 4.5, "primary (ink) pressed"),
  ("ink.default", "bg.app", 3.0, "ink button vs app bg (UI)"),
  ("text.on-accent", "accent.default", 4.5, "accent label (selection chip)"),
  ("text.on-accent", "accent.hover", 4.5, "button label hover"),
  ("accent.text", "bg.surface", 4.5, "links/accent text"),
  ("focus.ring", "bg.surface", 3.0, "focus ring"),
  ("success.fg", "success.bg", 4.5, "success"),
  ("warning.fg", "warning.bg", 4.5, "warning"),
  ("danger.fg", "danger.bg", 4.5, "danger"),
  ("danger.fg", "bg.surface", 4.5, "danger text"),
  ("info.fg", "info.bg", 4.5, "info"),
  ("text.on-accent", "danger.solid", 4.5, "destructive button"),
  ("search.mark-text", "search.mark", 4.5, "matched text on the search mark"),
  ("search.mark", "bg.selected", 1.3, "search mark vs selected row (mark stays visible)"),
  ("search.mark", "bg.surface", 1.3, "search mark vs list surface"),
  ("search.mark", "bg.hover", 1.3, "search mark vs hovered row"),
  ("toast.action", "ink.default", 4.5, "toast action label on ink"),
] + [(f"kind.{k}.glyph", f"kind.{k}.tile", 4.5, f"{k} glyph on its type tile") for k in KIND_HUE]

out = ["## Primitives\n", "| Token | Hex | OKLCH |", "|---|---|---|"]
for k, (L, C, h) in prims.items():
    out.append(f"| `{k}` | `{hexmap[k]}` | `oklch({L*100:.1f}% {C:.3f} {h})` |")
fails = 0
for theme, m in THEMES.items():
    out += [f"\n## Contrast — {theme}\n", "| Pair | Ratio | Min | Pass |", "|---|---|---|---|"]
    for fg, bg, mn, label in CHECKS:
        r = contrast(rgbmap[m[fg]], rgbmap[m[bg]])
        ok = r >= mn
        fails += not ok
        out.append(f"| {label}: `{fg}` on `{bg}` | {r:.2f} | {mn} | {'✓' if ok else '✗'} |")
# list accents: the edge vs surface must be >=3:1 (UI component) in both themes. The 4px item label bar (the
# eight non-rose hues) also sits on a hovered and on a selected card, so it is checked on those fills too.
out += ["\n## List accent edges (UI ≥ 3:1)\n", "| Accent | Light (600 on surface) | Dark (400 on surface) |", "|---|---|---|"]
for name in HUES:
    lr = contrast(rgbmap[f"{name}-600"], rgbmap[THEMES['light']['bg.surface']])
    dr = contrast(rgbmap[f"{name}-400"], rgbmap[THEMES['dark']['bg.surface']])
    fails += (lr < 3) + (dr < 3)
    out.append(f"| {name} | {lr:.2f} {'✓' if lr>=3 else '✗'} | {dr:.2f} {'✓' if dr>=3 else '✗'} |")
out += ["\n## Item label bar on hovered and selected cards (UI ≥ 3:1)\n", "| Label | Light hover | Light selected | Dark hover | Dark selected |", "|---|---|---|---|---|"]
for name in HUES:
    if name == "rose":
        continue  # no item label uses rose
    cells = []
    for theme, step in (("light", 600), ("dark", 400)):
        for fill in ("bg.hover", "bg.selected"):
            r = contrast(rgbmap[f"{name}-{step}"], rgbmap[THEMES[theme][fill]])
            fails += r < 3
            cells.append(f"{r:.2f} {'✓' if r >= 3 else '✗'}")
    out.append(f"| {name} | " + " | ".join(cells) + " |")
print("\n".join(out))
print(f"\nFAILURES: {fails}")
root = pathlib.Path(__file__).resolve().parents[1]
generated = json.dumps({
    "primitives": hexmap,
    "themes": {t: {k: hexmap[v] for k, v in m.items()} for t, m in THEMES.items()},
    "semantic_refs": THEMES,
}, indent=2)
target = root / "tokens" / "palette.json"
if "--check" in sys.argv[1:]:
    stale = not target.exists() or target.read_text() != generated
    if stale:
        print("design/tokens/palette.json is stale: run python3 design/tools/palette.py and commit it")
    sys.exit(1 if fails or stale else 0)
(root / "tokens").mkdir(exist_ok=True)
target.write_text(generated)
