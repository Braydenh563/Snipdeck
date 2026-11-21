#!/usr/bin/env python3
"""Tests for scripts/i18n.py: plural extraction, round trips, and the placeholder check.

  python3 scripts/test_i18n.py
"""
import contextlib
import io
import json
import pathlib
import re
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
        self.saved = (i18n.ROOT, i18n.GLOSSARY)
        i18n.ROOT = self.root
        i18n.GLOSSARY = self.root / "no-glossary.json"

    def close(self):
        i18n.ROOT, i18n.GLOSSARY = self.saved
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

    def test_strict_fails_on_an_empty_msgstr(self):
        self.assertEqual(self.p.run("check")[0], 0)
        code, out = self.p.run("check", "--strict")
        self.assertEqual(code, 1)
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


class Style(unittest.TestCase):
    """The house-style rules, on single strings."""

    def bad(self, lang, source, form, glossary=()):
        return i18n.style_problems(lang, source, form, glossary)

    def test_placeholder_regex_sees_named_placeholders(self):
        self.assertEqual(i18n.placeholders("{n} of {0} and {}"), ["{0}", "{n}", "{}"])
        self.assertNotEqual(i18n.placeholders("{n} Dinge"), i18n.placeholders("Dinge"))

    def test_a_dash_the_source_lacks_fails(self):
        self.assertTrue(self.bad("de", "{}: press Tab", "{} \u2014 Tab drücken"))
        self.assertTrue(self.bad("de", "Pages", "Seiten \u2013 alle"))
        self.assertEqual(self.bad("de", "A \u2014 b", "A \u2014 b2"), [])

    def test_half_width_marks_after_cjk_fail_only_in_ja_and_zh(self):
        self.assertTrue(self.bad("ja", "Delete board {}?", "ボード{}を削除しますか?"))
        self.assertTrue(self.bad("zh-Hans", "Name, tag", "名称,标签"))
        self.assertTrue(self.bad("ja", "Hello: you", "こんにちは: あなた"))
        self.assertEqual(self.bad("ja", "Delete board {}?", "ボード{}を削除しますか？"), [])
        self.assertEqual(self.bad("ja", "Open {}?", "Open {} now?"), [])

    def test_end_punctuation_must_match_the_source(self):
        self.assertTrue(self.bad("ja", "Cannot read: {}.", "読めません: {}"))
        self.assertEqual(self.bad("ja", "Cannot read: {}.", "読めません：{}。"), [])
        self.assertTrue(self.bad("zh-Hans", "Cannot read.", "无法读取."))
        self.assertTrue(self.bad("de", "Done.", "Fertig"))
        self.assertTrue(self.bad("fr", "Open\u2026", "Ouvrir"))
        self.assertEqual(self.bad("fr", "Open\u2026", "Ouvrir\u2026"), [])
        self.assertEqual(self.bad("zh-Hans", "Sure?", "确定？"), [])
        self.assertEqual(self.bad("zh-Hans", "Name:", "名称："), [])

    def test_a_translation_may_not_add_end_punctuation(self):
        self.assertTrue(self.bad("de", "Open", "Öffnen."))
        self.assertTrue(self.bad("ja", "Open", "開く。"))
        self.assertEqual(self.bad("de", "Open", "Öffnen"), [])

    def test_identical_to_the_source_fails_unless_allowed(self):
        self.assertTrue(self.bad("de", "Cancel", "Cancel"))
        self.assertEqual(self.bad("de", "Emoji", "Emoji"), [])
        self.assertEqual(self.bad("de", "Panel", "Panel"), [])
        self.assertEqual(self.bad("de", "{}", "{}"), [])
        self.assertEqual(self.bad("de", "{} / {}", "{} / {}"), [])

    def test_a_glossary_term_needs_one_of_the_chosen_stems(self):
        glossary = [{"id": "item", "pattern": r"\bitems?\b", "stems": {"de": ["Textbaustein"]}}]
        self.assertTrue(self.bad("de", "Delete {} items", "{} Elemente löschen", glossary))
        self.assertEqual(self.bad("de", "Delete {} items", "{} Textbausteine löschen", glossary), [])
        self.assertEqual(self.bad("fr", "Delete {} items", "Supprimer", glossary), [], "no stems for fr")
        self.assertEqual(self.bad("de", "Open", "Öffnen", glossary), [], "pattern not in source")


    def test_a_glossary_term_can_forbid_words_and_be_case_sensitive(self):
        glossary = [{"id": "key-enter", "pattern": r"\bEnter\b", "case_sensitive": True,
                     "stems": {"de": ["Enter"]}, "avoid": {"de": ["Eingabe"]}}]
        self.assertTrue(self.bad("de", "Press Enter", "Eingabetaste drücken", glossary))
        self.assertEqual(self.bad("de", "Press Enter", "Enter drücken", glossary), [])
        self.assertEqual(self.bad("de", "enter the name", "Namen eingeben", glossary), [], "case matters")

    def test_cognates_are_allowed_only_in_their_locale(self):
        self.assertEqual(self.bad("fr", "Menu", "Menu"), [])
        self.assertTrue(self.bad("ja", "Menu", "Menu"))

    def test_an_abbreviation_full_stop_is_not_added_punctuation(self):
        self.assertEqual(self.bad("de", "{} min ago", "vor {} Min."), [])
        self.assertTrue(self.bad("de", "Open the board", "Das Board öffnen."))


class Wired(Base):
    def test_check_reports_a_dropped_named_placeholder(self):
        (self.p.root / "ui" / "a.slint").write_text(SLINT + '\nText { text: @tr("Page {n}"); }\n', encoding="utf-8")
        self.p.run("extract")
        self.p.write_po("de", self.p.po("de").replace('msgid "Page {n}"\nmsgstr ""', 'msgid "Page {n}"\nmsgstr "Seite"'))
        code, out = self.p.run("check")
        self.assertEqual(code, 1)
        self.assertIn("placeholders", out)

    def test_check_reports_a_style_problem(self):
        self.p.write_po("de", self.p.po("de").replace('msgid "Open"\nmsgstr ""', 'msgid "Open"\nmsgstr "Open"'))
        code, out = self.p.run("check")
        self.assertEqual(code, 1)
        self.assertIn("identical to its source", out)

    def test_the_real_glossary_is_well_formed(self):
        i18n.GLOSSARY = self.p.saved[1]
        terms = i18n.load_glossary()
        self.assertTrue(terms)
        for term in terms:
            self.assertTrue(term["id"])
            re.compile(term["pattern"])
            if "stems" in term:
                self.assertEqual(sorted(term["stems"]), sorted(i18n.LOCALES), term["id"])
            self.assertTrue(set(term.get("avoid", {})) <= set(i18n.LOCALES), term["id"])


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
