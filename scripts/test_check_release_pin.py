#!/usr/bin/env python3
"""Tests for scripts/check_release_pin.py.

  python3 scripts/test_check_release_pin.py
"""
import contextlib
import io
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import check_release_pin as crp  # noqa: E402

H1 = "a" * 64
H2 = "b" * 64
NAME = "snipdeck-v1.1.0-windows-x64.zip"


class Parse(unittest.TestCase):
    def test_real_file_parses(self):
        crp.parse(crp.PIN_FILE.read_text(encoding="utf-8"))

    def test_comments_blank_and_star_prefix(self):
        pins = crp.parse(f"# c\n\n{H1.upper()}  *{NAME}\n")
        self.assertEqual(pins, {NAME: H1})

    def test_bad_line_and_duplicate(self):
        with self.assertRaises(crp.PinError):
            crp.parse("not a pin\n")
        with self.assertRaises(crp.PinError):
            crp.parse(f"{H1}  {NAME}\n{H2}  {NAME}\n")


class Names(unittest.TestCase):
    def test_names(self):
        self.assertEqual(crp.asset_name("v1.1.0"), NAME)
        self.assertEqual(crp.asset_name("v1.1.0-beta.1", "arm64"), "snipdeck-v1.1.0-beta.1-windows-arm64.zip")
        for bad in ("1.1.0", "v1.1", "v1.1.0; rm"):
            with self.assertRaises(crp.PinError, msg=bad):
                crp.asset_name(bad)
        with self.assertRaises(crp.PinError):
            crp.asset_name("v1.1.0", "x86")


class AddAndCheck(unittest.TestCase):
    def test_missing_pin_is_reported(self):
        errors = crp.check("# nothing\n", "v1.1.0", ["x64"])
        self.assertEqual(len(errors), 1)
        self.assertIn("unpinned", errors[0])

    def test_add_then_check_passes_and_is_idempotent(self):
        text = crp.add("# header\n", [(NAME, H1)])
        self.assertEqual(crp.check(text, "v1.1.0", ["x64"], [(NAME, H1)]), [])
        self.assertEqual(crp.add(text, [(NAME, H1)]), text)

    def test_add_keeps_the_last_line_apart(self):
        text = crp.add(f"{H1}  other.zip", [(NAME, H2)])
        self.assertEqual(text.count("\n"), 2)

    def test_conflicting_pin_is_refused(self):
        text = crp.add("", [(NAME, H1)])
        with self.assertRaises(crp.PinError):
            crp.add(text, [(NAME, H2)])

    def test_pin_that_differs_from_the_zip(self):
        text = crp.add("", [(NAME, H1)])
        errors = crp.check(text, "v1.1.0", ["x64"], [(NAME, H2)])
        self.assertTrue(any("differs" in e for e in errors), errors)

    def test_every_requested_arch_needs_a_pin(self):
        text = crp.add("", [(NAME, H1)])
        errors = crp.check(text, "v1.1.0", ["x64", "arm64"])
        self.assertEqual(len(errors), 1)
        self.assertIn("arm64", errors[0])


class Cli(unittest.TestCase):
    def test_add_and_check_round_trip(self):
        with tempfile.TemporaryDirectory() as d:
            d = pathlib.Path(d)
            pin = d / "pins.txt"
            pin.write_text("# header\n", encoding="utf-8")
            zipfile = d / NAME
            zipfile.write_bytes(b"zip bytes")
            base = ["--tag", "v1.1.0", "--file", str(pin)]
            err = io.StringIO()
            with contextlib.redirect_stderr(err), contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(crp.main(["check"] + base), 1)
                self.assertEqual(crp.main(["add", "--zip", str(zipfile)] + base), 0)
                self.assertEqual(crp.main(["check", "--zip", str(zipfile)] + base), 0)
                zipfile.write_bytes(b"changed")
                self.assertEqual(crp.main(["check", "--zip", str(zipfile)] + base), 1)
                wrong = d / "other.zip"
                wrong.write_bytes(b"x")
                self.assertEqual(crp.main(["add", "--zip", str(wrong)] + base), 1)


if __name__ == "__main__":
    unittest.main()
