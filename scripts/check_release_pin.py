#!/usr/bin/env python3
"""Pin (and check the pin of) a released portable zip in tools/launcher/release-sha256.txt.

  python3 scripts/check_release_pin.py add   --tag v1.1.0 --zip dist/snipdeck-v1.1.0-windows-x64.zip [--zip ...]
  python3 scripts/check_release_pin.py check --tag v1.1.0 [--arch x64] [--arch arm64] [--zip PATH ...]

`add` appends one "<sha256>  <asset-name>" line per zip (the release workflow commits the result: see
docs/packaging.md, "Pin the release"). `check` fails when a release has no pin (so the launcher would ask
the user to accept an unpinned download) or when a pin differs from the zip's real hash. Without --arch,
`check` expects the x64 zip. Stdlib only; the file format is the one tools/launcher/release.ps1 reads.
"""
import argparse
import hashlib
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
PIN_FILE = ROOT / "tools/launcher/release-sha256.txt"
LINE_RE = re.compile(r"^([0-9A-Fa-f]{64})\s+\*?(\S+)$")
TAG_RE = re.compile(r"^v\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$")
ARCHES = ("x64", "arm64")


class PinError(Exception):
    pass


def asset_name(tag, arch="x64"):
    """The zip name release.yml writes: snipdeck-<tag>-windows-<arch>.zip."""
    if not TAG_RE.match(tag):
        raise PinError(f"tag {tag!r} is not vX.Y.Z")
    if arch not in ARCHES:
        raise PinError(f"arch {arch!r} is not one of {', '.join(ARCHES)}")
    return f"snipdeck-{tag}-windows-{arch}.zip"


def parse(text):
    """{asset name: lowercase sha256}. Comment and blank lines are skipped; a bad or repeated line is an error."""
    pins = {}
    for number, raw in enumerate(text.splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        m = LINE_RE.match(line)
        if not m:
            raise PinError(f"line {number}: expected '<sha256>  <asset-name>'")
        name = m.group(2)
        if name in pins:
            raise PinError(f"line {number}: more than one entry for {name}")
        pins[name] = m.group(1).lower()
    return pins


def sha256_of(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def add(text, zips):
    """The file text with a line for each (asset name, sha256); an existing identical pin is kept, a different one is an error."""
    pins = parse(text)
    out = text if text.endswith("\n") or not text else text + "\n"
    for name, digest in zips:
        if name in pins:
            if pins[name] != digest:
                raise PinError(f"{name} is already pinned to a different hash")
            continue
        out += f"{digest}  {name}\n"
    return out


def check(text, tag, arches, zips=()):
    """Error strings: a missing pin, or a pin that differs from the real zip."""
    pins = parse(text)
    errors = []
    real = dict(zips)
    for arch in arches:
        name = asset_name(tag, arch)
        if name not in pins:
            errors.append(f"{name} has no line in release-sha256.txt (the launcher would ask users to accept an unpinned download)")
        elif name in real and real[name] != pins[name]:
            errors.append(f"{name}: the pin {pins[name][:12]}... differs from the zip's hash {real[name][:12]}...")
    return errors


def zip_entries(paths):
    return [(pathlib.Path(p).name, sha256_of(p)) for p in paths]


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("mode", choices=("add", "check"))
    ap.add_argument("--tag", required=True, help="vX.Y.Z")
    ap.add_argument("--zip", action="append", default=[], help="a built zip (repeatable)")
    ap.add_argument("--arch", action="append", choices=ARCHES, help="check: architectures that must be pinned (default x64)")
    ap.add_argument("--file", default=str(PIN_FILE))
    a = ap.parse_args(argv)
    path = pathlib.Path(a.file)
    try:
        text = path.read_text(encoding="utf-8")
        zips = zip_entries(a.zip)
        if a.mode == "add":
            if not zips:
                raise PinError("add needs at least one --zip")
            for name, _ in zips:
                if name not in {asset_name(a.tag, arch) for arch in ARCHES}:
                    raise PinError(f"{name} is not a zip name for {a.tag}")
            path.write_text(add(text, zips), encoding="utf-8")
            print(f"pinned {len(zips)} zip(s) in {path}")
            return 0
        errors = check(text, a.tag, a.arch or ["x64"], zips)
    except (PinError, OSError) as err:
        print(f"check_release_pin: {err}", file=sys.stderr)
        return 1
    for e in errors:
        print(f"check_release_pin: {e}", file=sys.stderr)
    if errors:
        return 1
    print("check_release_pin: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
