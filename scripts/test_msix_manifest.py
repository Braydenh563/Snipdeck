#!/usr/bin/env python3
"""Tests for scripts/msix_manifest.py.

  python3 scripts/test_msix_manifest.py
"""
import contextlib
import io
import pathlib
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import msix_manifest as mm  # noqa: E402

TEMPLATE = mm.TEMPLATE.read_text(encoding="utf-8")
ARGS = ("Pub.Snipdeck", "CN=Pub, O=Pub", "Pub & Co", "1.2.3")


class FillTests(unittest.TestCase):
    def test_fills_everything(self):
        out = mm.fill(TEMPLATE, *ARGS)
        root = ET.fromstring(out.encode())
        ns = "{http://schemas.microsoft.com/appx/manifest/foundation/windows10}"
        ident = root.find(ns + "Identity")
        self.assertEqual(ident.get("Name"), "Pub.Snipdeck")
        self.assertEqual(ident.get("Publisher"), "CN=Pub, O=Pub")
        self.assertEqual(ident.get("Version"), "1.2.3.0")
        self.assertEqual(root.find(ns + "Properties/" + ns + "PublisherDisplayName").text, "Pub & Co")

    def test_arch_defaults_to_x64_and_accepts_arm64(self):
        ns = "{http://schemas.microsoft.com/appx/manifest/foundation/windows10}"
        for arch, want in ((None, "x64"), ("x64", "x64"), ("arm64", "arm64")):
            out = mm.fill(TEMPLATE, *ARGS) if arch is None else mm.fill(TEMPLATE, *ARGS, arch=arch)
            ident = ET.fromstring(out.encode()).find(ns + "Identity")
            self.assertEqual(ident.get("ProcessorArchitecture"), want)

    def test_bad_arch(self):
        for arch in ("", "x86", "ARM64", "arm"):
            with self.assertRaises(mm.ManifestError, msg=arch):
                mm.fill(TEMPLATE, *ARGS, arch=arch)

    def test_main_passes_arch(self):
        with tempfile.TemporaryDirectory() as d:
            out = pathlib.Path(d) / "AppxManifest.xml"
            base = ["--identity-name", "P.S", "--publisher", "CN=P", "--publisher-display-name", "P", "--version", "1.0.0"]
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(mm.main(base + ["--arch", "arm64", "--out", str(out)]), 0)
            self.assertIn('ProcessorArchitecture="arm64"', out.read_text(encoding="utf-8"))

    def test_quotes_in_values_stay_valid(self):
        out = mm.fill(TEMPLATE, "Pub.Snipdeck", 'CN="Pub"', "A<B", "1.0.0")
        ET.fromstring(out.encode())

    def test_bad_versions(self):
        for v in ("1.0", "1.0.0.0", "v1.0.0", "1.0.0-beta", "1.0.70000", "01.0.0", ""):
            with self.assertRaises(mm.ManifestError, msg=v):
                mm.fill(TEMPLATE, "a", "b", "c", v)

    def test_empty_value(self):
        with self.assertRaises(mm.ManifestError):
            mm.fill(TEMPLATE, "", "b", "c", "1.0.0")

    def test_leftover_token(self):
        with self.assertRaises(mm.ManifestError):
            mm.fill(TEMPLATE.replace("</Package>", "<X a=\"{{OTHER}}\"/></Package>"), *ARGS)

    def test_broken_xml(self):
        with self.assertRaises(mm.ManifestError):
            mm.fill(TEMPLATE.replace("</Package>", ""), *ARGS)

    def test_main_writes_file_and_fails_cleanly(self):
        with tempfile.TemporaryDirectory() as d:
            out = pathlib.Path(d) / "sub" / "AppxManifest.xml"
            base = ["--identity-name", "P.S", "--publisher", "CN=P", "--publisher-display-name", "P"]
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(mm.main(base + ["--version", "2.0.1", "--out", str(out)]), 0)
            self.assertIn('Version="2.0.1.0"', out.read_text(encoding="utf-8"))
            with contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(mm.main(base + ["--version", "x", "--out", str(out)]), 1)


NS = "{http://schemas.microsoft.com/appx/manifest/foundation/windows10}"
LANG = pathlib.Path(__file__).resolve().parent.parent / "crates" / "snipdeck-ui" / "lang"


class TemplateTests(unittest.TestCase):
    def test_registers_file_type_protocol_and_alias_from_the_table(self):
        import json
        table = json.loads((mm.ROOT / "packaging/registration.json").read_text(encoding="utf-8"))
        root = ET.fromstring(mm.fill(TEMPLATE, *ARGS).encode())
        file_types = [e.text for e in root.iter() if e.tag.endswith("}FileType")]
        protocols = [e.get("Name") for e in root.iter() if e.tag.endswith("}Protocol")]
        aliases = [e.get("Alias") for e in root.iter() if e.tag.endswith("}ExecutionAlias")]
        self.assertEqual(file_types, [table["file_extension"]])
        self.assertEqual(protocols, [table["url_scheme"]])
        self.assertEqual(aliases, [table["app_execution_alias"]])

    def test_declares_every_shipped_locale(self):
        root = ET.fromstring(mm.fill(TEMPLATE, *ARGS).encode())
        declared = {r.get("Language").lower() for r in root.iter(NS + "Resource")}
        shipped = {"en-us"} | {d.name.lower() for d in LANG.iterdir() if d.is_dir()}
        self.assertEqual(declared, shipped)

    def test_max_version_tested_is_a_tested_build_with_its_source(self):
        root = ET.fromstring(mm.fill(TEMPLATE, *ARGS).encode())
        family = root.find(NS + "Dependencies/" + NS + "TargetDeviceFamily")
        tested = family.get("MaxVersionTested")
        self.assertEqual(tested, "10.0.26300.0")
        self.assertIn("windows-results-20261017.md", TEMPLATE)


if __name__ == "__main__":
    unittest.main()
