#!/usr/bin/env python3
"""Symbols data pipeline: builds crates/snipdeck-symbols/data/ from pinned public sources.

  python3 -I scripts/symbols.py --cache DIR            # download (hash-checked), then generate
  python3 -I scripts/symbols.py --cache DIR --pin      # re-download everything and rewrite symbols.lock
  python3 -I scripts/symbols.py --check                # offline: outputs decode, counts, sizes, notices

Spec: docs/reviews/symbols-spec.md. Generator needs only the stdlib (art rendering also needs
cairosvg and Pillow, see `--art`). `--check` needs nothing but the stdlib and runs in CI.

Sources (all pinned by sha256 in scripts/symbols.lock, refused on a mismatch):
  emoji-test.txt (Unicode), CLDR annotations (unicode-org/cldr-json), Fluent Emoji Flat (npm
  @iconify-json/fluent-emoji-flat, a repackaging of microsoft/fluentui-emoji), Phosphor Regular
  (npm @phosphor-icons/core), and the hand-written data/symbols/kaomoji.tsv of this repo.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import pathlib
import re
import sys
import tarfile
import urllib.request
import zlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
LOCK = ROOT / "scripts" / "symbols.lock"
DATA = ROOT / "crates" / "snipdeck-symbols" / "data"
KAOMOJI_SRC = ROOT / "data" / "symbols" / "kaomoji.tsv"
NOTICES = ROOT / "THIRD_PARTY_NOTICES.md"
MANIFEST = DATA / "manifest.json"

LANGS = ["de", "es", "fr", "ja", "pt", "zh"]
EMOJI_GROUPS = [
    "Smileys & Emotion",
    "People & Body",
    "Animals & Nature",
    "Food & Drink",
    "Travel & Places",
    "Activities",
    "Objects",
    "Symbols",
    "Flags",
]
KAOMOJI_GROUPS = [
    "joy", "love", "sad", "angry", "surprise", "shrug", "greet", "sorry", "animals", "actions",
]
KAOMOJI_WIDE = 10  # characters; longer ones span both columns in the grid
SKIN_TONES = range(0x1F3FB, 0x1F400)
FE0F = 0xFE0F

# Size budgets (bytes) from the spec, enforced by --check.
BUDGET = {
    "emoji.tsv.z": 60_000,
    "kaomoji.tsv.z": 10_000,
    # Spec estimated 200 KB; measured 213 KB for all 1,512 Phosphor Regular paths (zopfli: 202 KB).
    "icons.bin.z": 220_000,
    "emoji48.bin": 2_500_000,
}
for _l in LANGS:
    BUDGET[f"names-{_l}.tsv.z"] = 50_000
ART_BUDGET = 2_500_000


# --------------------------------------------------------------------------- helpers


def deflate(data: bytes) -> bytes:
    """Raw deflate (no zlib header): `miniz_oxide::inflate::decompress_to_vec` reads it."""
    c = zlib.compressobj(9, zlib.DEFLATED, -15)
    return c.compress(data) + c.flush()


def inflate(data: bytes) -> bytes:
    return zlib.decompress(data, -15)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def clean(s: str) -> str:
    """One TSV cell: no tabs, newlines or the `|` list separator."""
    return re.sub(r"[\t\r\n|]+", " ", s).strip()


def seq_chars(seq: str) -> str:
    return "".join(chr(int(h, 16)) for h in seq.split("-"))


def strip_fe0f(seq: str) -> str:
    return "-".join(h for h in seq.split("-") if int(h, 16) != FE0F)


# --------------------------------------------------------------------------- parsers

_EMOJI_LINE = re.compile(
    r"^([0-9A-Fa-f ]+?)\s*;\s*([a-z-]+)\s*#\s*\S+\s+E(\d+(?:\.\d+)?)\s+(.*)$"
)


def parse_emoji_test(text: str) -> tuple[list[dict], dict]:
    """Fully-qualified emoji in CLDR order, default skin tone only.

    Returns (entries, stats). An entry is {seq, group, subgroup, name, version}; `seq` is lowercase
    hex joined by `-` (FE0F kept, as the Windows clipboard wants the fully-qualified form).
    """
    entries: list[dict] = []
    group = subgroup = ""
    stats = {"skin_tone": 0, "component": 0, "other_status": 0}
    for line in text.splitlines():
        if line.startswith("# group:"):
            group = line.split(":", 1)[1].strip()
        elif line.startswith("# subgroup:"):
            subgroup = line.split(":", 1)[1].strip()
        elif line and not line.startswith("#"):
            m = _EMOJI_LINE.match(line)
            if not m:
                raise ValueError(f"emoji-test: cannot parse {line!r}")
            cps, status, version, name = m.groups()
            if group == "Component":
                stats["component"] += 1
                continue
            if status != "fully-qualified":
                stats["other_status"] += 1
                continue
            seq = "-".join(c.lower() for c in cps.split())
            if any(int(c, 16) in SKIN_TONES for c in seq.split("-")):
                stats["skin_tone"] += 1  # D7: skin tones deferred
                continue
            if group not in EMOJI_GROUPS:
                raise ValueError(f"emoji-test: unknown group {group!r}")
            entries.append(
                {"seq": seq, "group": group, "subgroup": subgroup, "name": name.strip(), "version": version}
            )
    return entries, stats


def parse_cldr(obj: dict, key: str) -> dict[str, tuple[str, list[str]]]:
    """CLDR json -> {emoji string: (tts name, keywords)}. `key` is annotations or annotationsDerived."""
    out = {}
    for ch, v in obj[key]["annotations"].items():
        tts = (v.get("tts") or [""])[0]
        out[ch] = (tts, list(v.get("default") or []))
    return out


def cldr_lookup(table: dict, seq: str):
    """Annotation for an emoji sequence: tries the fully-qualified form, then without FE0F."""
    for s in (seq, strip_fe0f(seq)):
        hit = table.get(seq_chars(s))
        if hit and hit[0]:
            return hit
    return None


def parse_phosphor_catalog(js: str) -> list[dict]:
    """Name, tags and first category of every icon in @phosphor-icons/core dist/index.mjs."""
    enum = {m.group(1): m.group(2) for m in re.finditer(r'i\.([A-Z_]+) = "([^"]+)"', js)}
    pat = re.compile(
        r'name:\s*"([^"]+)",\s*pascal_name:\s*"[^"]*",\s*(?:alias:\s*\{\s*name:\s*"([^"]+)"[^}]*\},\s*)?'
        r'categories:\s*\[([^\]]*)\],.*?tags:\s*\[([^\]]*)\]',
        re.S,
    )
    icons = []
    for m in pat.finditer(js):
        name, alias, cats, tags = m.groups()
        first = re.search(r"e\.([A-Z_]+)", cats)
        taglist = [t for t in re.findall(r'"([^"]*)"', tags) if not t.startswith("*")]
        if alias and alias not in taglist:
            taglist.append(alias)  # the icon's former name stays searchable
        icons.append(
            {"name": name, "category": enum.get(first.group(1), "") if first else "", "tags": taglist}
        )
    return icons


_PATH_ONLY = re.compile(r'^<path d="([^"]+)"/>$')


def icon_payload(svg: str) -> str:
    """The part of a Phosphor svg worth storing: the path data, or the raw inner markup."""
    m = re.match(r"^<svg [^>]*>(.*)</svg>\s*$", svg, re.S)
    if not m:
        raise ValueError("icon: not an svg")
    inner = m.group(1).strip()
    p = _PATH_ONLY.match(inner)
    return p.group(1) if p else inner


def parse_kaomoji(text: str) -> list[dict]:
    """data/symbols/kaomoji.tsv: id, group, text, comma-separated tags. Validated strictly."""
    rows, ids, texts = [], set(), set()
    for n, line in enumerate(text.splitlines(), 1):
        if not line.strip() or line.startswith("#"):
            continue
        cols = line.split("\t")
        if len(cols) != 4:
            raise ValueError(f"kaomoji line {n}: want 4 tab-separated columns, got {len(cols)}")
        kid, group, kt, tags = cols
        if not re.fullmatch(r"[a-z0-9]+(-[a-z0-9]+)*", kid):
            raise ValueError(f"kaomoji line {n}: bad id {kid!r}")
        if group not in KAOMOJI_GROUPS:
            raise ValueError(f"kaomoji line {n}: unknown group {group!r}")
        taglist = [t.strip() for t in tags.split(",") if t.strip()]
        if not 2 <= len(taglist) <= 5:
            raise ValueError(f"kaomoji line {n} ({kid}): need 2 to 5 tags, got {len(taglist)}")
        if kid in ids:
            raise ValueError(f"kaomoji line {n}: duplicate id {kid}")
        if kt in texts:
            raise ValueError(f"kaomoji line {n}: duplicate text {kt}")
        if "|" in kt or kt != kt.strip() or not kt:
            raise ValueError(f"kaomoji line {n} ({kid}): text must be non-empty, trimmed, without '|'")
        ids.add(kid)
        texts.add(kt)
        rows.append({"id": kid, "group": group, "text": kt, "tags": taglist})
    return rows


# --------------------------------------------------------------------------- build (pure)


def fluent_art_map(chars: dict, icons: dict) -> dict[str, tuple[str, bool]]:
    """{fluent key (hex joined by '-', no FE0F)} -> (icon name, hflip). Follows aliases."""
    names, aliases = icons["icons"], icons.get("aliases", {})
    out = {}
    for key, name in chars.items():
        k = strip_fe0f(key.lower())
        if name in names:
            out[k] = (name, False)
        elif name in aliases and aliases[name].get("parent") in names:
            out[k] = (aliases[name]["parent"], bool(aliases[name].get("hFlip")))
    return out


def slug(name: str) -> str:
    """CLDR/emoji-test name -> Fluent icon name: 'flag: Australia' -> 'flag-australia'."""
    return re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")


def add_name_matches(art_map: dict, entries: list[dict], icons: dict) -> int:
    """Adds art for emoji the codepoint table misses but whose name is an icon or alias.

    Returns how many were added. Names with a skin tone are not in `entries`, so no ambiguity.
    """
    names, aliases = icons["icons"], icons.get("aliases", {})
    added = 0
    for e in entries:
        k = strip_fe0f(e["seq"])
        if k in art_map:
            continue
        n = slug(e["name"])
        if n in names:
            art_map[k] = (n, False)
        elif n in aliases and aliases[n].get("parent") in names:
            art_map[k] = (aliases[n]["parent"], bool(aliases[n].get("hFlip")))
        else:
            continue
        added += 1
    return added


def build_text(inputs: dict) -> tuple[dict[str, bytes], dict]:
    """Pure: parsed inputs -> ({file name: bytes}, manifest fragment).

    inputs: emoji_test (text), cldr {lang: table} with "en" and LANGS (derived merged in by the
    caller order: annotations win), art_map, phosphor_js, icon_svgs {name: svg}, kaomoji (text).
    """
    entries, stats = parse_emoji_test(inputs["emoji_test"])
    art = inputs["art_map"]
    kept, dropped = [], []
    for e in entries:
        (kept if strip_fe0f(e["seq"]) in art else dropped).append(e)

    files: dict[str, bytes] = {}
    en = inputs["cldr"]["en"]
    rows, en_missing = [], 0
    for e in kept:
        hit = cldr_lookup(en, e["seq"])
        if hit is None:
            en_missing += 1
            hit = (e["name"], [])
        name, kws = hit
        kws = [k for k in dict.fromkeys(clean(k) for k in kws) if k and k != name]
        rows.append(
            "\t".join(
                [
                    e["seq"],
                    str(EMOJI_GROUPS.index(e["group"])),
                    clean(e["subgroup"]),
                    clean(name),
                    "|".join(kws),
                ]
            )
        )
    files["emoji.tsv.z"] = deflate(("\n".join(rows) + "\n").encode())

    lang_missing = {}
    for lang in LANGS:
        table, out, missing = inputs["cldr"][lang], [], 0
        for e in kept:
            hit = cldr_lookup(table, e["seq"])
            if hit is None:
                missing += 1
                continue  # the crate falls back to English for this one
            name, kws = hit
            kws = [k for k in dict.fromkeys(clean(k) for k in kws) if k and k != name]
            out.append("\t".join([e["seq"], clean(name), "|".join(kws)]))
        lang_missing[lang] = missing
        files[f"names-{lang}.tsv.z"] = deflate(("\n".join(out) + "\n").encode())

    kao = parse_kaomoji(inputs["kaomoji"])
    krows = [
        "\t".join(
            [
                k["id"],
                k["text"],
                str(KAOMOJI_GROUPS.index(k["group"])),
                "|".join(clean(t) for t in k["tags"]),
                "1" if len(k["text"]) > KAOMOJI_WIDE else "0",
            ]
        )
        for k in kao
    ]
    files["kaomoji.tsv.z"] = deflate(("\n".join(krows) + "\n").encode())

    icons = parse_phosphor_catalog(inputs["phosphor_js"])
    svgs = inputs["icon_svgs"]
    if sorted(i["name"] for i in icons) != sorted(svgs):
        a, b = {i["name"] for i in icons}, set(svgs)
        raise ValueError(f"phosphor: catalog/asset mismatch; only catalog {sorted(a - b)[:5]}, only assets {sorted(b - a)[:5]}")
    irows = [
        "\t".join([i["name"], "|".join(clean(t) for t in i["tags"]), clean(i["category"]), icon_payload(svgs[i["name"]])])
        for i in sorted(icons, key=lambda i: i["name"])
    ]
    files["icons.bin.z"] = deflate(("\n".join(irows) + "\n").encode())

    frag = {
        "counts": {
            "emoji": len(kept),
            "kaomoji": len(kao),
            "icons": len(icons),
            "emoji_groups": {g: sum(1 for e in kept if e["group"] == g) for g in EMOJI_GROUPS},
            "names_missing_en": en_missing,
            "names_missing": lang_missing,
        },
        "emoji_groups": EMOJI_GROUPS,
        "kaomoji_groups": KAOMOJI_GROUPS,
        "coverage": {
            "emoji_test_default_tone": len(entries),
            "with_fluent_art": len(kept),
            "without_fluent_art": len(dropped),
            "skin_tone_variants_skipped": stats["skin_tone"],
        },
        "dropped": [
            {"seq": e["seq"], "name": e["name"], "group": e["group"]} for e in dropped
        ],
    }
    # Row order of emoji.tsv is the order of the art index (slice 2).
    frag["kept_seqs"] = [e["seq"] for e in kept]
    return files, frag


# --------------------------------------------------------------------------- notices

_NOTICE_HEAD = """\
# Third-party notices

Snipdeck is licensed GPL-3.0-only. The Symbols library bundles the data below. Each source is
permissive and compatible with GPL-3.0; the copyright and permission notices are kept here as
their licences require. The sources and their sha256 hashes are pinned in `scripts/symbols.lock`.

Emoji art: Microsoft Fluent Emoji (MIT). Emoji names and search keywords: Unicode CLDR (Unicode
License v3). Icons: Phosphor (MIT). The kaomoji list is original work of this repository
(GPL-3.0-only).
"""

NOTICE_SECTIONS = [
    ("Unicode License v3", "unicode", "Unicode `emoji-test.txt` (the emoji set and order) and CLDR annotations (names and keywords)."),
    ("MIT: Microsoft Fluent Emoji", "fluent", "Emoji art, \"Flat\" style, rasterised to PNG (`microsoft/fluentui-emoji`, via the `@iconify-json/fluent-emoji-flat` npm package)."),
    ("MIT: Phosphor Icons", "phosphor", "Icon glyphs, Regular weight (`@phosphor-icons/core`)."),
]


def build_notices(licences: dict[str, str]) -> str:
    out = [_NOTICE_HEAD]
    for title, key, what in NOTICE_SECTIONS:
        out.append(f"\n## {title}\n\n{what}\n\n```text\n{licences[key].strip()}\n```\n")
    return "".join(out)


# --------------------------------------------------------------------------- sources


def load_lock() -> dict:
    return json.loads(LOCK.read_text())


def fetch(src: dict, cache: pathlib.Path, pin: bool = False) -> bytes:
    """The bytes of one pinned source, from the cache when it matches, else downloaded."""
    cache.mkdir(parents=True, exist_ok=True)
    f = cache / sha256(src["url"].encode())[:16]
    data = None
    if f.exists() and not pin:
        data = f.read_bytes()
        if src.get("sha256") and sha256(data) != src["sha256"]:
            data = None
    if data is None:
        req = urllib.request.Request(src["url"], headers={"User-Agent": "snipdeck-symbols/1"})
        with urllib.request.urlopen(req, timeout=120) as r:
            data = r.read()
        f.write_bytes(data)
    if not pin and sha256(data) != src["sha256"]:
        raise SystemExit(
            f"REFUSED: {src['url']}\n  sha256 {sha256(data)} != locked {src['sha256']}\n"
            "  The upstream file changed. Review it, then re-pin with --pin."
        )
    return data


def tar_members(data: bytes, wanted) -> dict[str, bytes]:
    """Reads (never extracts) the members for which wanted(name) is true."""
    out = {}
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as t:
        for m in t:
            if m.isfile() and wanted(m.name):
                out[m.name] = t.extractfile(m).read()
    return out


def gather(lock: dict, cache: pathlib.Path) -> dict:
    src = lock["sources"]
    get = lambda k: fetch(src[k], cache)  # noqa: E731
    cldr = {}
    for lang in ["en", *LANGS]:
        base = parse_cldr(json.loads(get(f"cldr-annotations-{lang}")), "annotations")
        derived = parse_cldr(json.loads(get(f"cldr-derived-{lang}")), "annotationsDerived")
        cldr[lang] = {**derived, **base}
    fl = tar_members(get("fluent-npm"), lambda n: n in ("package/icons.json", "package/chars.json"))
    fluent_icons = json.loads(fl["package/icons.json"])
    art_map = fluent_art_map(json.loads(fl["package/chars.json"]), fluent_icons)
    add_name_matches(art_map, parse_emoji_test(get("emoji-test").decode())[0], fluent_icons)
    ph = tar_members(
        get("phosphor-npm"),
        lambda n: n == "package/dist/index.mjs" or re.fullmatch(r"package/assets/regular/[^/]+\.svg", n) is not None,
    )
    svgs = {
        n.rsplit("/", 1)[1][: -len(".svg")]: b.decode()
        for n, b in ph.items()
        if n.startswith("package/assets/regular/")
    }
    return {
        "emoji_test": get("emoji-test").decode(),
        "cldr": cldr,
        "art_map": art_map,
        "fluent_icons": fluent_icons,
        "phosphor_js": ph["package/dist/index.mjs"].decode(),
        "icon_svgs": svgs,
        "kaomoji": KAOMOJI_SRC.read_text(encoding="utf-8"),
        "licences": {
            "unicode": get("licence-unicode").decode(),
            "fluent": get("licence-fluent").decode(),
            "phosphor": tar_members(get("phosphor-npm"), lambda n: n == "package/LICENSE")["package/LICENSE"].decode(),
        },
    }


def generate(cache: pathlib.Path, art: bool) -> None:
    lock = load_lock()
    inputs = gather(lock, cache)
    files, frag = build_text(inputs)
    DATA.mkdir(parents=True, exist_ok=True)
    manifest = {
        "emoji_version": re.search(r"# Version: (\S+)", inputs["emoji_test"]).group(1),
        "cldr_version": "49.0.0",
        "sources": {k: v["sha256"] for k, v in lock["sources"].items()},
        **{k: v for k, v in frag.items() if k != "kept_seqs"},
    }
    if art:
        files["emoji48.bin"], manifest["art"] = render_art(inputs, frag["kept_seqs"])
    for name, blob in files.items():
        (DATA / name).write_bytes(blob)
    manifest["files"] = {n: {"bytes": len(b), "sha256": sha256(b)} for n, b in sorted(files.items())}
    MANIFEST.write_text(json.dumps(manifest, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    NOTICES.write_text(build_notices(inputs["licences"]), encoding="utf-8")
    report(manifest)


def report(m: dict) -> None:
    c, cov = m["counts"], m["coverage"]
    p = print
    p(f"emoji {m['emoji_version']}: {cov['emoji_test_default_tone']} default-tone fully-qualified; "
      f"skin-tone variants skipped (D7): {cov['skin_tone_variants_skipped']}")
    pct = 100.0 * cov["with_fluent_art"] / max(1, cov["emoji_test_default_tone"])
    p(f"Fluent coverage: {cov['with_fluent_art']} / {cov['emoji_test_default_tone']} ({pct:.1f} %), "
      f"dropped {cov['without_fluent_art']}")
    by_group: dict[str, list[str]] = {}
    for d in m["dropped"]:
        by_group.setdefault(d["group"], []).append(d["name"])
    for g, names in by_group.items():
        shown = "" if g == "Flags" else ": " + ", ".join(names[:40]) + (" ..." if len(names) > 40 else "")
        p(f"  dropped in {g}: {len(names)}{shown}")
    p(f"kept per group: {c['emoji_groups']}")
    p(f"kaomoji {c['kaomoji']}, icons {c['icons']}")
    p(f"names missing in English (fall back to emoji-test): {c['names_missing_en']}; "
      f"other languages (fall back to en): {c['names_missing']}")
    for n, f in m["files"].items():
        p(f"  {n}: {f['bytes']} bytes")


# --------------------------------------------------------------------------- check (offline)


def check() -> list[str]:
    """Every problem found, empty when the committed outputs are consistent."""
    bad = []
    if not MANIFEST.exists():
        return [f"missing {MANIFEST.relative_to(ROOT)}; run scripts/symbols.py"]
    m = json.loads(MANIFEST.read_text(encoding="utf-8"))
    lock = load_lock()
    for k, s in lock["sources"].items():
        if not re.fullmatch(r"[0-9a-f]{64}", s.get("sha256", "")):
            bad.append(f"lock: {k} has no sha256")
        if m.get("sources", {}).get(k) != s.get("sha256"):
            bad.append(f"manifest was built from a different {k} than symbols.lock pins")
    lines = {}
    for name, info in m["files"].items():
        f = DATA / name
        if not f.exists():
            bad.append(f"missing {name}")
            continue
        blob = f.read_bytes()
        if len(blob) != info["bytes"] or sha256(blob) != info["sha256"]:
            bad.append(f"{name}: differs from the manifest (regenerate)")
        if len(blob) > BUDGET.get(name, 10**9):
            bad.append(f"{name}: {len(blob)} bytes is over the {BUDGET[name]} budget")
        if name.endswith(".z"):
            try:
                lines[name] = [l for l in inflate(blob).decode().split("\n") if l]
            except Exception as e:  # noqa: BLE001
                bad.append(f"{name}: does not inflate ({e})")
    c = m["counts"]
    want = {"emoji.tsv.z": c["emoji"], "kaomoji.tsv.z": c["kaomoji"], "icons.bin.z": c["icons"]}
    for n, cnt in want.items():
        if n in lines and len(lines[n]) != cnt:
            bad.append(f"{n}: {len(lines[n])} rows, manifest says {cnt}")
    for lang in LANGS:
        n = f"names-{lang}.tsv.z"
        exp = c["emoji"] - c["names_missing"][lang]
        if n in lines and len(lines[n]) != exp:
            bad.append(f"{n}: {len(lines[n])} rows, manifest implies {exp}")
    if "emoji48.bin" in m["files"] and (DATA / "emoji48.bin").exists():
        bad += check_art(m, (DATA / "emoji48.bin").read_bytes())
    elif "emoji48.bin" in BUDGET and "art" in m:
        bad.append("manifest lists art but emoji48.bin is missing")
    if NOTICES.exists():
        text = NOTICES.read_text(encoding="utf-8")
        for title, _key, _what in NOTICE_SECTIONS:
            if f"## {title}" not in text:
                bad.append(f"THIRD_PARTY_NOTICES.md: missing section '{title}'")
        for needle in ("Copyright (c) Microsoft Corporation", "Copyright (c) 2023 Phosphor Icons", "UNICODE LICENSE V3"):
            if needle not in text:
                bad.append(f"THIRD_PARTY_NOTICES.md: missing licence text ({needle})")
    else:
        bad.append("missing THIRD_PARTY_NOTICES.md")
    return bad


def check_art(m: dict, blob: bytes) -> list[str]:
    bad = []
    if blob[:4] != b"SDE1":
        return ["emoji48.bin: bad magic"]
    px = int.from_bytes(blob[4:6], "little")
    count = int.from_bytes(blob[6:10], "little")
    art = m.get("art", {})
    if (px, count) != (art.get("px"), art.get("count")):
        bad.append(f"emoji48.bin: header {px}px x {count} != manifest {art.get('px')}px x {art.get('count')}")
    if len(blob) > ART_BUDGET:
        bad.append(f"emoji48.bin: {len(blob)} bytes is over the {ART_BUDGET} budget")
    end = 10 + 8 * count
    if len(blob) < end:
        return bad + ["emoji48.bin: truncated index"]
    for i in range(count):
        off = int.from_bytes(blob[10 + 8 * i : 14 + 8 * i], "little")
        ln = int.from_bytes(blob[14 + 8 * i : 18 + 8 * i], "little")
        if off < end or off + ln > len(blob) or blob[off : off + 8] != b"\x89PNG\r\n\x1a\n":
            bad.append(f"emoji48.bin: entry {i} is not a PNG inside the file")
            break
    return bad


# --------------------------------------------------------------------------- art (slice 2)


ART_PX = (48, 40)  # preferred size first; the smaller one is used only if the first is over budget
ART_COLOURS = 128  # palette size after quantisation (alpha kept in the palette)


def art_svg(icons: dict, name: str, hflip: bool, px: int) -> bytes:
    """One Fluent icon as a standalone svg of px by px."""
    body = icons["icons"][name]["body"]
    if hflip:
        body = f'<g transform="translate(32 0) scale(-1 1)">{body}</g>'
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="{px}" height="{px}">{body}</svg>'
    ).encode()


def pack_art(pngs: list[bytes], px: int) -> bytes:
    """emoji48.bin: b"SDE1", u16 px, u32 count, count x (u32 offset, u32 length), then the PNGs.

    Entry i is the PNG of row i of emoji.tsv. All integers little-endian.
    """
    head = 10 + 8 * len(pngs)
    out = bytearray(b"SDE1" + px.to_bytes(2, "little") + len(pngs).to_bytes(4, "little"))
    off = head
    for p in pngs:
        out += off.to_bytes(4, "little") + len(p).to_bytes(4, "little")
        off += len(p)
    for p in pngs:
        out += p
    return bytes(out)


def render_art(inputs: dict, kept_seqs: list[str]):  # pragma: no cover - needs cairosvg and Pillow
    """Rasterises Fluent Flat to palette PNGs. Returns (emoji48.bin bytes, manifest "art" entry)."""
    try:
        import cairosvg
        from PIL import Image
    except ImportError as e:
        raise SystemExit(f"--art needs cairosvg and Pillow (pip install cairosvg pillow): {e}")
    icons, art = inputs["fluent_icons"], inputs["art_map"]
    for px in ART_PX:
        pngs = []
        for seq in kept_seqs:
            name, flip = art[strip_fe0f(seq)]
            raw = cairosvg.svg2png(bytestring=art_svg(icons, name, flip, px), output_width=px, output_height=px)
            im = Image.open(io.BytesIO(raw)).convert("RGBA")
            im = im.quantize(colors=ART_COLOURS, method=Image.Quantize.FASTOCTREE, dither=Image.Dither.NONE)
            buf = io.BytesIO()
            im.save(buf, "PNG", optimize=True)
            pngs.append(buf.getvalue())
        blob = pack_art(pngs, px)
        if len(blob) <= ART_BUDGET:
            return blob, {"px": px, "count": len(pngs), "bytes": len(blob), "colours": ART_COLOURS, "style": "Fluent Emoji Flat"}
        print(f"art at {px} px is {len(blob)} bytes (> {ART_BUDGET}); trying the next size", file=sys.stderr)
    raise SystemExit("emoji art is over budget at every size")


# --------------------------------------------------------------------------- main


def pin(cache: pathlib.Path) -> None:
    lock = load_lock()
    for k, s in lock["sources"].items():
        s.pop("sha256", None)
        data = fetch(s, cache, pin=True)
        s["sha256"], s["bytes"] = sha256(data), len(data)
        print(f"pinned {k}: {s['sha256']} ({len(data)} bytes)")
    LOCK.write_text(json.dumps(lock, indent=1) + "\n")


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--check", action="store_true", help="offline consistency check of the committed outputs")
    ap.add_argument("--pin", action="store_true", help="re-download every source and rewrite symbols.lock")
    ap.add_argument("--art", action="store_true", help="also render emoji48.bin (needs cairosvg and Pillow)")
    ap.add_argument("--cache", type=pathlib.Path, default=pathlib.Path.home() / ".cache" / "snipdeck-symbols")
    a = ap.parse_args(argv)
    if a.check:
        bad = check()
        for b in bad:
            print("symbols --check:", b, file=sys.stderr)
        if not bad:
            print("symbols --check: ok")
        return 1 if bad else 0
    if a.pin:
        pin(a.cache)
        return 0
    generate(a.cache, a.art)
    return 0


if __name__ == "__main__":
    sys.exit(main())
