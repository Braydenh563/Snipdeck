#!/usr/bin/env python3
"""Tests for scripts/installer_check.py: the real files pass, and each rule fails when broken.

  python3 scripts/test_installer_check.py
"""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import installer_check as ic  # noqa: E402

ISS = ic.ISS.read_text(encoding="utf-8")
MANIFEST = ic.MANIFEST.read_text(encoding="utf-8")


def iss_errors(text):
    errors = []
    ic.check_iss(errors, iss_text=text)
    return errors


def manifest_errors(text, iss_setup=None):
    errors = []
    ic.check_manifest(errors, manifest_text=text, iss_setup=iss_setup)
    return errors


class RealFiles(unittest.TestCase):
    def test_repo_passes(self):
        self.assertEqual(ic.run(), [])

    def test_mutex_comes_from_the_app(self):
        self.assertEqual(ic.rust_mutex(), "Snipdeck-main")


class IssRules(unittest.TestCase):
    def broken(self, old, new, needle):
        self.assertIn(old, ISS)
        errors = iss_errors(ISS.replace(old, new))
        self.assertTrue(any(needle in e for e in errors), errors)

    def test_app_id(self):
        self.broken("5ACCE38D", "6ACCE38D", "AppId")

    def test_privileges(self):
        self.broken("PrivilegesRequired=lowest", "PrivilegesRequired=admin", "PrivilegesRequired")

    def test_app_mutex(self):
        self.broken("AppMutex=Snipdeck-main", "AppMutex=Other", "AppMutex")

    def test_code_mutex(self):
        self.broken("MutexName = 'Snipdeck-main'", "MutexName = 'Nope'", "[Code] mutex")

    def test_missing_source(self):
        self.broken(r'{#RepoRoot}\LICENSE"', r'{#RepoRoot}\NOPE"', "does not exist")

    def test_exe_may_be_absent(self):
        self.assertEqual(iss_errors(ISS), [])

    def test_run_value_name(self):
        self.broken('ValueName: "Snipdeck"', 'ValueName: "Other"', "Run value")

    def test_run_value_flag(self):
        self.broken("uninsdeletevalue", "", "uninsdeletevalue")

    def test_version_guard(self):
        self.broken("#ifndef AppVersion\n  #error", "#ifndef AppVersion2\n  #error", "#error")


class ManifestRules(unittest.TestCase):
    def test_task_id(self):
        self.assertTrue(manifest_errors(MANIFEST.replace("SnipdeckStartup", "Other")))

    def test_executable(self):
        self.assertTrue(manifest_errors(MANIFEST.replace('Executable="snipdeck.exe"', 'Executable="x.exe"')))

    def test_unknown_asset(self):
        errors = manifest_errors(MANIFEST.replace(r"Assets\StoreLogo.png", r"Assets\Nope.png"))
        self.assertTrue(any("Nope.png" in e for e in errors), errors)

    def test_min_version_must_agree(self):
        self.assertEqual(manifest_errors(MANIFEST, {"MinVersion": "10.0.18362"}), [])
        errors = manifest_errors(MANIFEST, {"MinVersion": "10.0.17763"})
        self.assertTrue(any("MinVersion" in e for e in errors), errors)

    def test_not_xml(self):
        self.assertTrue(manifest_errors("<a>"))


if __name__ == "__main__":
    unittest.main()
