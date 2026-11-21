#!/usr/bin/env python3
"""Finds half-built things: a name that is defined and never used anywhere else.

  python3 scripts/halfbuilt.py --check   fail on any unlisted finding (run by scripts/checks.sh)

Three scans over crates/**: (1) Rust `pub fn` / `pub const` whose name appears exactly once in
all Rust and Slint sources; (2) Slint `export component` / `export global` whose name is never
used outside its own definition and re-export lines; (3) properties of the Type, Theme and
CardDepth globals that nothing reads. A finding is either deleted or listed in ALLOW with a
reason; an ALLOW entry that no longer matches is itself an error, so the list cannot rot.
"""
import pathlib
import re
import sys
from collections import Counter

ROOT = pathlib.Path(__file__).resolve().parent.parent

ALLOW = {}


def sources(exts):
    for ext in exts:
        yield from ROOT.glob(f"crates/**/*.{ext}")


def words(text):
    return re.findall(r"[A-Za-z_][A-Za-z0-9_-]*", text)


def main():
    rust = {p: p.read_text(encoding="utf-8", errors="replace") for p in sources(["rs"])}
    slint = {p: p.read_text(encoding="utf-8", errors="replace") for p in sources(["slint"])}
    count = Counter()
    for text in list(rust.values()) + list(slint.values()):
        count.update(words(text))
    # Slint names reach Rust as snake_case (set_x, on_x) and the reverse: count both spellings.
    count_snake = Counter()
    for word, n in count.items():
        count_snake[word.replace("-", "_")] += n

    found = {}
    for path, text in rust.items():
        if "/tests/" in str(path):
            continue
        for m in re.finditer(r"^\s*pub(?:\([a-z]+\))? (?:fn|const|static) ([A-Za-z_][A-Za-z0-9_]*)", text, re.M):
            name = m.group(1)
            if count_snake[name] == 1 and name != "main":
                found[name] = f"{path.relative_to(ROOT)}: pub item `{name}` is never used"
    for path, text in slint.items():
        # The galleries and probes are test fixtures; windows and globals are used from Rust.
        if "/gallery/" in str(path):
            continue
        for m in re.finditer(r"^export (?:component|global) ([A-Za-z0-9_]+)", text, re.M):
            name = m.group(1)
            uses = sum(
                len(re.findall(rf"\b{name}\b", t))
                for t in slint.values()
                for t in [re.sub(r"^(export \{.*\}.*|import \{.*\}.*|export (component|global) " + name + r".*)$", "", t, flags=re.M)]
            )
            in_rust = any(re.search(rf"\b{name}\b", t) for t in rust.values())
            if uses == 0 and not in_rust:
                found[name] = f"{path.relative_to(ROOT)}: `{name}` is never instantiated"
    for fname in ("theme.slint", "components/card-depth.slint"):
        path = ROOT / "crates/snipdeck-ui/ui" / fname
        if not path.is_file():
            continue
        text = path.read_text(encoding="utf-8")
        for m in re.finditer(r"^\s*out property <[^>]+> ([a-z0-9-]+):", text, re.M):
            name = m.group(1)
            used = sum(len(re.findall(rf"[A-Za-z]\.{re.escape(name)}\b", t)) for t in slint.values())
            if used == 0:
                found[name] = f"{fname}: token `{name}` is never read"

    bad = [msg for name, msg in sorted(found.items()) if name not in ALLOW]
    stale = [f"ALLOW `{k}` matches nothing: remove it ({why})" for k, why in ALLOW.items() if k not in found]
    for line in bad + stale:
        print(line)
    if bad or stale:
        print("halfbuilt: delete the unused name or list it in scripts/halfbuilt.py ALLOW with a reason")
        return 1
    print(f"halfbuilt: clean ({len(ALLOW)} listed)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
