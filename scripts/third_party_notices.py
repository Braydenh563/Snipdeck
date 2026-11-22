#!/usr/bin/env python3
"""Keeps the Rust-crate part of THIRD_PARTY_NOTICES.md complete.

  python3 scripts/third_party_notices.py          rewrite the generated section
  python3 scripts/third_party_notices.py --check  fail if the section is stale (run by scripts/gate.sh)

The section between the BEGIN/END markers lists every crate that is linked into the Windows
build (normal dependencies of the workspace crates, reached through `cargo metadata`, Slint
included) with its licence expression, and prints each distinct licence / notice text once
with the crates that carry it. The texts are the LICENSE, COPYING, NOTICE and UNLICENSE files
found in each crate's source folder (the cargo registry). Nothing outside the markers is touched.
"""
import hashlib
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
FILE = ROOT / "THIRD_PARTY_NOTICES.md"
BEGIN = "<!-- BEGIN GENERATED CRATES (python3 scripts/third_party_notices.py) -->"
END = "<!-- END GENERATED CRATES -->"
TARGET = "x86_64-pc-windows-msvc"
PREFIXES = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE", "COPYRIGHT")


def metadata():
    out = subprocess.run(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", TARGET],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout
    return json.loads(out)


def linked_crates(meta):
    """Non-workspace packages reachable from the workspace through normal dependencies."""
    by_id = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    members = set(meta["workspace_members"])
    seen, todo = set(), list(members)
    while todo:
        cur = todo.pop()
        for dep in nodes[cur]["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]) and dep["pkg"] not in seen:
                seen.add(dep["pkg"])
                todo.append(dep["pkg"])
    return sorted((by_id[i] for i in seen - members), key=lambda p: (p["name"], p["version"]))


def licence_files(pkg):
    folder = pathlib.Path(pkg["manifest_path"]).parent
    found = []
    for path in sorted(folder.iterdir()):
        if path.is_file() and path.name.upper().startswith(PREFIXES):
            text = path.read_bytes().decode("utf-8", "replace").replace("\r\n", "\n").strip()
            if text:
                found.append(text)
    return found


def render():
    crates = linked_crates(metadata())
    groups = {}  # text hash -> (text, [crate labels])
    missing = []
    for pkg in crates:
        label = f"{pkg['name']} {pkg['version']}"
        texts = licence_files(pkg)
        if not texts:
            missing.append(f"{label} ({pkg.get('license') or 'no licence field'})")
        for text in texts:
            key = hashlib.sha256(text.encode()).hexdigest()
            groups.setdefault(key, (text, []))[1].append(label)
    lines = [
        BEGIN,
        "",
        "## Rust crates (including Slint)",
        "",
        f"Snipdeck links the {len(crates)} crates below (Windows build). Slint is used under "
        "GPL-3.0-only, one of its licences. Each licence text is printed once, followed by the "
        "crates that carry it. A crate with several licence files appears under each.",
        "",
        "| Crate | Licence |",
        "|---|---|",
    ]
    for pkg in crates:
        lines.append(f"| {pkg['name']} {pkg['version']} | {pkg.get('license') or 'see its repository'} |")
    for text, labels in sorted(groups.values(), key=lambda g: g[1][0]):
        lines += ["", "### " + ", ".join(labels), "", "```text", text.replace("```", "'''"), "```"]
    if missing:
        lines += [
            "", "### Crates without a licence file in their package", "",
            "Their licence is the one named in the table above; the authors ship no separate text. Where a crate is offered under GPL-3.0-only (Slint), that text is the LICENSE file shipped with Snipdeck:", "",
        ]
        lines += [f"- {m}" for m in missing]
    lines += ["", END]
    return "\n".join(lines)


def bundled_problems(text):
    """What ships inside the program but is missing from the notices file: every font licence
    next to a bundled font must appear in it word for word, and the LICENSE must exist."""
    problems = []
    if not (ROOT / "LICENSE").is_file():
        problems.append("LICENSE is missing")
    for licence in sorted(ROOT.glob("crates/*/fonts/OFL.txt")):
        if licence.read_text(encoding="utf-8").strip() not in text:
            problems.append(f"{licence.relative_to(ROOT)} is not in THIRD_PARTY_NOTICES.md")
    for font in sorted(ROOT.glob("crates/*/fonts/*.ttf")):
        if not (font.parent / "OFL.txt").is_file():
            problems.append(f"{font.relative_to(ROOT)} has no OFL.txt beside it")
    return problems


def main():
    check = "--check" in sys.argv
    current = FILE.read_text(encoding="utf-8")
    block = render()
    if BEGIN in current:
        head, rest = current.split(BEGIN, 1)
        tail = rest.split(END, 1)[1] if END in rest else ""
        new = head.rstrip("\n") + "\n\n" + block + tail
    else:
        new = current.rstrip("\n") + "\n\n" + block + "\n"
    problems = bundled_problems(new)
    if problems:
        print("bundled assets without notices:\n  " + "\n  ".join(problems))
        return 1
    if check:
        if new != current:
            print("THIRD_PARTY_NOTICES.md is stale: run python3 scripts/third_party_notices.py")
            return 1
        print("THIRD_PARTY_NOTICES.md is current")
        return 0
    FILE.write_text(new, encoding="utf-8")
    print(f"wrote {FILE.name} ({len(new)} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
