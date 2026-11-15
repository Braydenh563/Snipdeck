#!/usr/bin/env python3
"""Fill packaging/msix/AppxManifest.xml into an output file.

  python3 scripts/msix_manifest.py --identity-name Pub.Snipdeck --publisher "CN=Pub" \\
      --publisher-display-name "Pub" --version 1.0.0 --out dist/msix/AppxManifest.xml

The version X.Y.Z becomes X.Y.Z.0 (the Store needs the last part to be 0). Refuses a leftover
{{TOKEN}}, a bad version, or XML that is not well formed. Stdlib only.
"""
import argparse
import pathlib
import re
import sys
import xml.etree.ElementTree as ET
from xml.sax.saxutils import escape

ROOT = pathlib.Path(__file__).resolve().parent.parent
TEMPLATE = ROOT / "packaging/msix/AppxManifest.xml"
VERSION_RE = re.compile(r"^(0|[1-9][0-9]{0,4})\.(0|[1-9][0-9]{0,4})\.(0|[1-9][0-9]{0,4})$")


class ManifestError(Exception):
    pass


def msix_version(version):
    """X.Y.Z -> X.Y.Z.0; each part 0..65535."""
    m = VERSION_RE.match(version)
    if not m or any(int(p) > 65535 for p in m.groups()):
        raise ManifestError(f"version {version!r} is not X.Y.Z (numbers up to 65535)")
    return version + ".0"


def attr(value):
    """Escape a value for use inside the template's double-quoted attributes and element text."""
    return escape(value, {'"': "&quot;"})


def fill(template, identity_name, publisher, publisher_display_name, version):
    for label, value in (("identity name", identity_name), ("publisher", publisher),
                         ("publisher display name", publisher_display_name)):
        if not value or not value.strip():
            raise ManifestError(f"{label} is empty")
    values = {
        "IDENTITY_NAME": identity_name,
        "PUBLISHER": publisher,
        "PUBLISHER_DISPLAY_NAME": publisher_display_name,
        "VERSION": msix_version(version),
    }
    # Tokens inside XML comments are documentation, not placeholders: fill only outside comments.
    parts = re.split(r"(<!--.*?-->)", template, flags=re.S)
    for i in range(0, len(parts), 2):
        for key, value in values.items():
            parts[i] = parts[i].replace("{{" + key + "}}", attr(value))
    out = "".join(parts)
    leftovers = re.findall(r"\{\{[^}]*\}\}", re.sub(r"<!--.*?-->", "", out, flags=re.S))
    if leftovers:
        raise ManifestError(f"unfilled tokens: {', '.join(sorted(set(leftovers)))}")
    if "{{" in re.sub(r"<!--.*?-->", "", out, flags=re.S):
        raise ManifestError("a '{{' is left in the manifest")
    try:
        ET.fromstring(out.encode("utf-8"))
    except ET.ParseError as err:
        raise ManifestError(f"the filled manifest is not well-formed XML: {err}") from err
    return out


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--identity-name", required=True)
    ap.add_argument("--publisher", required=True)
    ap.add_argument("--publisher-display-name", required=True)
    ap.add_argument("--version", required=True, help="X.Y.Z (written as X.Y.Z.0)")
    ap.add_argument("--template", default=str(TEMPLATE))
    ap.add_argument("--out", required=True)
    a = ap.parse_args(argv)
    try:
        text = fill(pathlib.Path(a.template).read_text(encoding="utf-8"), a.identity_name,
                    a.publisher, a.publisher_display_name, a.version)
    except ManifestError as err:
        print(f"msix_manifest: {err}", file=sys.stderr)
        return 1
    out = pathlib.Path(a.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(text, encoding="utf-8")
    print(f"wrote {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
