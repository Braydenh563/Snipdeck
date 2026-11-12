#!/usr/bin/env python3
"""Tests for scripts/bump_version.py. Run: python3 -I scripts/test_bump_version.py"""

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import bump_version as bv  # noqa: E402

CARGO = """[workspace]
members = ["crates/*"]

[workspace.package]
version = "1.0.0"
edition = "2024"

[workspace.dependencies]
foo = { version = "1.0.0" }
"""

LOCK = """version = 4

[[package]]
name = "foo"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "snipdeck-app"
version = "1.0.0"
dependencies = [
 "foo",
]

[[package]]
name = "snipdeck-core"
version = "1.0.0"
"""

LOG = """# Changelog

## [Unreleased]

### Added
- A thing.

## [1.0.0] - 2026-10-21
"""


class BumpTest(unittest.TestCase):
    def repo(self) -> Path:
        tmp = Path(tempfile.mkdtemp())
        (tmp / "Cargo.toml").write_text(CARGO, encoding="utf-8")
        (tmp / "Cargo.lock").write_text(LOCK, encoding="utf-8")
        (tmp / "CHANGELOG.md").write_text(LOG, encoding="utf-8")
        for name in ("snipdeck-app", "snipdeck-core"):
            (tmp / "crates" / name).mkdir(parents=True)
            (tmp / "crates" / name / "Cargo.toml").write_text(
                f'[package]\nname = "{name}"\nversion.workspace = true\n', encoding="utf-8"
            )
        return tmp

    def test_bumps_cargo_lock_and_changelog(self):
        root = self.repo()
        notes = bv.bump(root, "1.1.0", "2026-11-12")
        self.assertEqual(len(notes), 2)
        cargo = (root / "Cargo.toml").read_text(encoding="utf-8")
        self.assertIn('[workspace.package]\nversion = "1.1.0"', cargo)
        self.assertIn('foo = { version = "1.0.0" }', cargo, "dependency versions untouched")
        lock = (root / "Cargo.lock").read_text(encoding="utf-8")
        self.assertIn('name = "snipdeck-app"\nversion = "1.1.0"', lock)
        self.assertIn('name = "snipdeck-core"\nversion = "1.1.0"', lock)
        self.assertIn('name = "foo"\nversion = "1.0.0"', lock, "registry crates untouched")
        log = (root / "CHANGELOG.md").read_text(encoding="utf-8")
        self.assertIn("## [Unreleased]\n\n## [1.1.0] - 2026-11-12\n\n### Added\n- A thing.", log)
        self.assertIn("## [1.0.0] - 2026-10-21", log)

    def test_running_twice_changes_nothing_more(self):
        root = self.repo()
        bv.bump(root, "1.1.0", "2026-11-12")
        before = {p: (root / p).read_text(encoding="utf-8") for p in ("Cargo.toml", "Cargo.lock", "CHANGELOG.md")}
        self.assertEqual(bv.bump(root, "1.1.0", "2026-11-13"), [])
        for p, text in before.items():
            self.assertEqual((root / p).read_text(encoding="utf-8"), text, p)

    def test_same_version_only_rolls_the_changelog(self):
        root = self.repo()
        notes = bv.bump(root, "1.0.1", "2026-11-12")
        self.assertTrue(notes[0].startswith("version 1.0.0 -> 1.0.1"))
        root = self.repo()
        (root / "CHANGELOG.md").write_text(LOG.replace("## [1.0.0] - 2026-10-21\n", ""), encoding="utf-8")
        self.assertEqual(bv.bump(root, "1.0.0", "2026-11-12"), ["CHANGELOG: Unreleased -> [1.0.0] - 2026-11-12"])
        self.assertIn('version = "1.0.0"', (root / "Cargo.toml").read_text(encoding="utf-8"))

    def test_refuses_lower_and_malformed_versions(self):
        root = self.repo()
        with self.assertRaises(ValueError):
            bv.bump(root, "0.9.0", "2026-11-12")
        self.assertEqual(bv.main(["v1.1.0", "--root", str(root)]), 1)
        self.assertEqual(bv.main(["1.1", "--root", str(root)]), 1)
        self.assertIn('version = "1.0.0"', (root / "Cargo.toml").read_text(encoding="utf-8"))

    def test_prerelease_sorts_below_its_release(self):
        self.assertLess(bv.parse("1.1.0-rc1"), bv.parse("1.1.0"))
        self.assertLess(bv.parse("1.0.9"), bv.parse("1.1.0-rc1"))

    def test_the_real_repository_parses(self):
        root = Path(__file__).resolve().parent.parent
        cargo = (root / "Cargo.toml").read_text(encoding="utf-8")
        version = bv.workspace_version(cargo)
        _, changed = bv.set_lock_versions((root / "Cargo.lock").read_text(encoding="utf-8"), bv.members(root), version)
        self.assertEqual(sorted(changed), sorted(bv.members(root)), "every workspace crate is in Cargo.lock")


if __name__ == "__main__":
    unittest.main()
