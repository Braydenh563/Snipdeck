"""Tests for scripts/ui_icons.py: python3 -I scripts/test_ui_icons.py"""

import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import ui_icons  # noqa: E402


class Render(unittest.TestCase):
    def test_lists_the_svgs_sorted_with_a_generated_header(self):
        with tempfile.TemporaryDirectory() as d:
            for name in ("b-icon", "a-icon", "ignored.txt"):
                (pathlib.Path(d) / name).write_text("x")
            (pathlib.Path(d) / "a-icon.svg").write_text("<svg/>")
            (pathlib.Path(d) / "b-icon.svg").write_text("<svg/>")
            text = ui_icons.render(pathlib.Path(d))
        lines = text.splitlines()
        self.assertIn("GENERATED", lines[1])
        props = [ln for ln in lines if "out property" in ln]
        self.assertEqual(len(props), 2)
        self.assertIn("a-icon.svg", props[0])
        self.assertIn("b-icon.svg", props[1])
        self.assertTrue(text.endswith("}\n"))

    def test_committed_icons_slint_is_current(self):
        self.assertEqual(ui_icons.main(["--check"]), 0)


if __name__ == "__main__":
    unittest.main()
