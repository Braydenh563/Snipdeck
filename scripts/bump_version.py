#!/usr/bin/env python3
"""Sets the release version everywhere the repository records it, for the Release workflow.

    python3 scripts/bump_version.py 1.1.0 [--date 2026-11-12] [--root .]

- `[workspace.package] version` in Cargo.toml (every crate inherits it).
- The workspace crates' entries in Cargo.lock (those without a `source`), so `--locked` builds still pass.
- CHANGELOG.md: the `## [Unreleased]` section becomes `## [1.1.0] - <date>` and a new, empty
  `## [Unreleased]` goes above it. Skipped when a `## [1.1.0]` section already exists.

Refuses a version that is not X.Y.Z (optionally -prerelease) or that is lower than the current one.
Prints what changed. Standard library only; it runs nothing from the repository but this file.
"""

from __future__ import annotations

import argparse
import datetime
import re
import sys
from pathlib import Path

VERSION_RE = re.compile(r"^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.]+))?$")


def parse(version: str) -> tuple[int, int, int, tuple[int, str]]:
    """A sort key: a prerelease sorts below its release (1.1.0-rc1 < 1.1.0)."""
    m = VERSION_RE.match(version)
    if not m:
        raise ValueError(f"'{version}' is not a version like 1.0.0")
    pre = m.group(4)
    return (int(m.group(1)), int(m.group(2)), int(m.group(3)), (0, pre) if pre else (1, ""))


def workspace_version(cargo_toml: str) -> str:
    section = re.search(r"^\[workspace\.package\]\n(.*?)(?=^\[|\Z)", cargo_toml, re.M | re.S)
    if not section:
        raise ValueError("Cargo.toml has no [workspace.package] section")
    m = re.search(r'^version = "([^"]*)"', section.group(1), re.M)
    if not m:
        raise ValueError("[workspace.package] has no version")
    return m.group(1)


def set_workspace_version(cargo_toml: str, version: str) -> str:
    start = cargo_toml.index("[workspace.package]\n")
    head, rest = cargo_toml[:start], cargo_toml[start:]
    rest, n = re.subn(r'^version = "[^"]*"', f'version = "{version}"', rest, count=1, flags=re.M)
    if n != 1:
        raise ValueError("[workspace.package] has no version")
    return head + rest


def set_lock_versions(lock: str, members: set[str], version: str) -> tuple[str, list[str]]:
    """Rewrites the version of each workspace member's [[package]] block (no `source` line)."""
    blocks = lock.split("[[package]]\n")
    changed: list[str] = []
    for i, block in enumerate(blocks[1:], start=1):
        name = re.search(r'^name = "([^"]*)"', block, re.M)
        if not name or name.group(1) not in members or re.search(r"^source = ", block, re.M):
            continue
        blocks[i] = re.sub(r'^version = "[^"]*"', f'version = "{version}"', block, count=1, flags=re.M)
        changed.append(name.group(1))
    return "[[package]]\n".join(blocks), changed


def roll_changelog(text: str, version: str, date: str) -> str | None:
    """The changelog with Unreleased renamed to the version, or None if nothing to do."""
    if re.search(rf"^## \[{re.escape(version)}\]", text, re.M):
        return None
    m = re.search(r"^## \[Unreleased\][^\n]*\n", text, re.M)
    if not m:
        return None
    return text[: m.start()] + f"## [Unreleased]\n\n## [{version}] - {date}\n" + text[m.end() :]


def members(root: Path) -> set[str]:
    names = set()
    for manifest in sorted((root / "crates").glob("*/Cargo.toml")):
        m = re.search(r'^name = "([^"]*)"', manifest.read_text(encoding="utf-8"), re.M)
        if m:
            names.add(m.group(1))
    return names


def bump(root: Path, version: str, date: str) -> list[str]:
    cargo_path, lock_path, log_path = root / "Cargo.toml", root / "Cargo.lock", root / "CHANGELOG.md"
    cargo = cargo_path.read_text(encoding="utf-8")
    current = workspace_version(cargo)
    if parse(version) < parse(current):
        raise ValueError(f"{version} is lower than the current version {current}")
    notes: list[str] = []
    if version != current:
        cargo_path.write_text(set_workspace_version(cargo, version), encoding="utf-8")
        lock, changed = set_lock_versions(lock_path.read_text(encoding="utf-8"), members(root), version)
        if not changed:
            raise ValueError("no workspace crate found in Cargo.lock")
        lock_path.write_text(lock, encoding="utf-8")
        notes.append(f"version {current} -> {version} ({len(changed)} crates in Cargo.lock)")
    if log_path.exists():
        rolled = roll_changelog(log_path.read_text(encoding="utf-8"), version, date)
        if rolled is not None:
            log_path.write_text(rolled, encoding="utf-8")
            notes.append(f"CHANGELOG: Unreleased -> [{version}] - {date}")
    return notes


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("version")
    ap.add_argument("--date", default=datetime.date.today().isoformat())
    ap.add_argument("--root", default=".")
    args = ap.parse_args(argv)
    try:
        parse(args.version)
        notes = bump(Path(args.root), args.version, args.date)
    except (ValueError, OSError) as e:
        print(f"bump_version: {e}", file=sys.stderr)
        return 1
    print("\n".join(notes) if notes else "bump_version: nothing to change")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
