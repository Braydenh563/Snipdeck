#!/usr/bin/env python3
"""Tests for scripts/i18n.py: plural extraction, round trips, and the placeholder check.

  python3 scripts/test_i18n.py
"""
import contextlib
import io
import json
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import i18n  # noqa: E402

SLINT = """\
export component A {
    Text { text: @tr("Open"); }
    Text { text: @tr("{} item moved" | "{} items moved" % n, n); }
    Text { text: @tr("Say \\"hi\\""); }
    Text { text: @tr("menu" => "Copy {} of {}", a, b); }
}
"""


class Project:
    """A throw-away crates/snipdeck-ui with one .slint file."""

    def __init__(self, slint=SLINT):
        self.dir = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.dir.name)
        (self.root / "ui").mkdir()
        (self.root / "ui" / "a.slint").write_text(slint, encoding="utf-8")
        self.saved = i18n.ROOT
        i18n.ROOT = self.root

    def close(self):
        i18n.ROOT = self.saved
        self.dir.cleanup()

    def run(self, *args):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = i18n.main(list(args))
        return code, out.getvalue()

    def po(self, lang):
        return i18n.po_path(lang).read_text(encoding="utf-8")

    def write_po(self, lang, text):
        i18n.po_path(lang).write_text(text, encoding="utf-8")


class Base(unittest.TestCase):
    def setUp(self):
        self.p = Project()
        self.addCleanup(self.p.close)
        self.assertEqual(self.p.run("extract")[0], 0)


class Extract(Base):
    def test_a_plural_writes_msgid_plural_and_one_msgstr_per_form(self):
        self.assertIn('msgid_plural "{} items moved"', self.p.po("de"))
        self.assertIn('msgstr[1] ""', self.p.po("de"))
        self.assertNotIn('msgstr[1]', self.p.po("ja"), "ja has one plural form")
        self.assertIn('msgstr[0] ""', self.p.po("ja"))
        pot = (self.p.root / "lang" / "snipdeck-ui.pot").read_text(encoding="utf-8")
        self.assertIn('msgid "{} item moved"\nmsgid_plural "{} items moved"', pot)

    def test_plural_forms_header_follows_the_language(self):
        self.assertIn("plural=(n != 1)", self.p.po("de"))
        self.assertIn("plural=(n > 1)", self.p.po("fr"))
        self.assertIn("nplurals=1; plural=0", self.p.po("zh-Hans"))

    def test_context_and_escaped_quotes_are_kept(self):
        text = self.p.po("es")
        self.assertIn('msgctxt "menu"\nmsgid "Copy {} of {}"', text)
        self.assertIn('msgid "Say \\"hi\\""', text)

    def test_extract_keeps_existing_translations_and_adds_forms(self):
        self.p.write_po("fr", self.p.po("fr").replace('msgid "Open"\nmsgstr ""', 'msgid "Open"\nmsgstr "Ouvrir"'))
        self.p.run("extract")
        self.assertIn('msgstr "Ouvrir"', self.p.po("fr"))
        # A plural that had only a singular translation keeps it as form 0.
        fr = self.p.po("fr").replace('msgstr[0] ""', 'msgstr[0] "{} élément déplacé"', 1)
        self.p.write_po("fr", fr)
        self.p.run("extract")
        self.assertIn('msgstr[0] "{} élément déplacé"', self.p.po("fr"))

    def test_a_multi_line_entry_is_read_whole(self):
        entries = i18n.entry_keys('msgid ""\n"Long "\n"text"\nmsgstr ""\n"Langer "\n"Text"\n')
        self.assertEqual(entries[(None, "Long text")].forms, ["Langer Text"])


class Check(Base):
    def fill(self, lang, forms_by_id):
        text = self.p.po(lang)
        for msgid, forms in forms_by_id.items():
            if len(forms) == 1:
                text = text.replace(f'msgid "{msgid}"\nmsgstr ""', f'msgid "{msgid}"\nmsgstr "{forms[0]}"')
            else:
                for i, form in enumerate(forms):
                    text = text.replace(f'msgstr[{i}] ""', f'msgstr[{i}] "{form}"', 1)
        self.p.write_po(lang, text)

    def test_a_fresh_extract_passes_and_counts_untranslated(self):
        code, out = self.p.run("check")
        self.assertEqual(code, 0)
        self.assertIn("untranslated", out)

    def test_a_translation_that_loses_a_placeholder_fails(self):
        self.fill("de", {"Copy {} of {}": ["Kopiere {}"]})
        # that one has a context, so put it in by hand
        text = self.p.po("de").replace('msgid "Copy {} of {}"\nmsgstr ""', 'msgid "Copy {} of {}"\nmsgstr "Kopiere {}"')
        self.p.write_po("de", text)
        code, out = self.p.run("check")
        self.assertEqual(code, 1)
        self.assertIn("placeholders", out)

    def test_a_plural_with_the_wrong_number_of_forms_fails(self):
        text = self.p.po("de").replace('msgstr[0] ""\nmsgstr[1] ""\n', 'msgstr[0] "{} Element"\n', 1)
        self.p.write_po("de", text)
        self.assertIn("translation forms", self.p.run("check")[1])

    def test_a_good_plural_and_a_good_string_pass(self):
        self.fill("de", {"Open": ["Öffnen"], "{} item moved": ["{} Element verschoben", "{} Elemente verschoben"]})
        code, out = self.p.run("check")
        self.assertEqual(code, 0, out)

    def test_a_stale_catalogue_fails(self):
        (self.p.root / "ui" / "a.slint").write_text(SLINT + '\nText { text: @tr("New"); }\n', encoding="utf-8")
        code, out = self.p.run("check")
        self.assertEqual(code, 1)
        self.assertIn("stale", out)

    def test_a_plural_that_became_plain_fails(self):
        text = self.p.po("de").replace('msgid_plural "{} items moved"\n', "").replace('msgstr[0] ""\nmsgstr[1] ""', 'msgstr ""')
        self.p.write_po("de", text)
        self.assertIn("plural in the source", self.p.run("check")[1])


class TodoAndApply(Base):
    def todo(self, lang):
        return json.loads(self.p.run("todo", lang)[1])

    def test_todo_lists_untranslated_entries_with_their_plural_shape(self):
        items = {i["msgid"]: i for i in self.todo("de")}
        self.assertEqual(items["{} item moved"]["msgid_plural"], "{} items moved")
        self.assertEqual(items["{} item moved"]["forms_wanted"], 2)
        ja = {i["msgid"]: i for i in self.todo("ja")}
        self.assertEqual(ja["{} item moved"]["forms_wanted"], 1)
        self.assertEqual(items['Say "hi"']["msgid"], 'Say "hi"', "shown unescaped")
        self.assertEqual(items["Copy {} of {}"]["context"], "menu")

    def test_apply_fills_entries_and_todo_shrinks(self):
        items = self.todo("de")
        for item in items:
            item["msgstr"] = ["X" + p for p in ["{}"] * 0] or [item["msgid"].upper()] * item.get("forms_wanted", 1)
        # Upper-casing keeps "{}" as "{}" (no letters), so placeholders survive.
        path = self.p.root / "de.json"
        path.write_text(json.dumps(items), encoding="utf-8")
        code, out = self.p.run("apply", "de", str(path))
        self.assertEqual(code, 0, out)
        self.assertEqual(self.todo("de"), [])
        self.assertIn('msgstr "SAY \\"HI\\""', self.p.po("de"))
        self.assertEqual(self.p.run("check")[0], 0)

    def test_apply_refuses_a_translation_that_changes_placeholders(self):
        items = [i for i in self.todo("de") if i["msgid"] == "{} item moved"]
        items[0]["msgstr"] = ["Ein Element", "{} Elemente"]
        path = self.p.root / "de.json"
        path.write_text(json.dumps(items), encoding="utf-8")
        code, out = self.p.run("apply", "de", str(path))
        self.assertEqual(code, 1)
        self.assertIn("refused", out)
        self.assertIn('msgstr[0] ""', self.p.po("de"), "nothing was written for it")

    def test_apply_refuses_the_wrong_number_of_forms(self):
        items = [i for i in self.todo("de") if i["msgid"] == "{} item moved"]
        items[0]["msgstr"] = ["{} Element"]
        path = self.p.root / "de.json"
        path.write_text(json.dumps(items), encoding="utf-8")
        self.assertEqual(self.p.run("apply", "de", str(path))[0], 1)


class Escapes(unittest.TestCase):
    def test_escape_and_unescape_round_trip(self):
        for text in ['Say "hi"', "a\nb", "back\\slash", "plain"]:
            self.assertEqual(i18n.unescape(i18n.escape(text)), text)


if __name__ == "__main__":
    unittest.main()
