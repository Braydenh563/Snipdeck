#!/usr/bin/env python3
"""Tests for scripts/symbols.py: parsers on small fixtures, the build, and the offline checks.

  python3 -I scripts/test_symbols.py
"""
import json
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import symbols  # noqa: E402

EMOJI_TEST = """\
# emoji-test.txt
# Version: 18.0

# group: Smileys & Emotion

# subgroup: face-smiling
1F600                                                  ; fully-qualified     # \U0001F600 E1.0 grinning face
263A FE0F                                              ; fully-qualified     # \u263a\ufe0f E0.6 smiling face
263A                                                   ; unqualified         # \u263a E0.6 smiling face

# group: People & Body

# subgroup: hand-fingers-open
1F44B                                                  ; fully-qualified     # \U0001F44B E0.6 waving hand
1F44B 1F3FB                                            ; fully-qualified     # \U0001F44B\U0001F3FB E1.0 waving hand: light skin tone

# subgroup: person-role
1F469 200D 1F4BB                                       ; fully-qualified     # \U0001F469\u200d\U0001F4BB E4.0 woman technologist

# group: Flags

# subgroup: country-flag
1F1E6 1F1FA                                            ; fully-qualified     # \U0001F1E6\U0001F1FA E2.0 flag: Australia

# group: Component

# subgroup: skin-tone
1F3FB                                                  ; component           # \U0001F3FB E1.0 light skin tone
"""

CLDR_EN = {
    "annotations": {
        "annotations": {
            "\U0001F600": {"default": ["face", "grin"], "tts": ["grinning face"]},
            "\u263a": {"default": ["happy", "smile"], "tts": ["smiling face"]},
        }
    }
}

PHOSPHOR_JS = '''
var t = ((i) => (i.REGULAR = "regular", i))(t || {}), e = ((i) => (i.ARROWS = "arrows", i.PEOPLE = "people", i))(e || {});
const icons = [
  {
    name: "arrow-up",
    pascal_name: "ArrowUp",
    categories: [e.ARROWS],
    figma_category: a.ARROWS,
    tags: ["direction", "north"],
    codepoint: 1,
    published_in: 1,
    updated_in: 1
  },
  {
    name: "smiley",
    pascal_name: "Smiley",
    alias: { name: "happy-face", pascal_name: "HappyFace" },
    categories: [e.PEOPLE, e.ARROWS],
    figma_category: a.PEOPLE,
    tags: [
      "*new*",
      "face",
      "emoji"
    ],
    codepoint: 2,
    published_in: 1,
    updated_in: 1
  }
];
'''

ICON_SVGS = {
    "arrow-up": '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" fill="currentColor"><path d="M1,2L3,4Z"/></svg>',
    "smiley": '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" fill="currentColor"><circle cx="1" cy="1" r="1"/><path d="M0,0"/></svg>',
}

KAOMOJI = "# id\tgroup\ttext\ttags\nshrug\tshrug\t\u00af\\_(\u30c4)_/\u00af\tshrug,whatever\nhappy\tjoy\t(^_^)\thappy,smile,joy\n"

ART_MAP = {"1f600": ("grinning-face", False), "263a": ("smiling-face", False)}


def inputs(**over):
    base = {
        "emoji_test": EMOJI_TEST,
        "cldr": {"en": symbols.parse_cldr(CLDR_EN, "annotations"), **{l: {} for l in symbols.LANGS}},
        "art_map": ART_MAP,
        "phosphor_js": PHOSPHOR_JS,
        "icon_svgs": ICON_SVGS,
        "kaomoji": KAOMOJI,
    }
    base.update(over)
    return base


class EmojiTest(unittest.TestCase):
    def test_keeps_fully_qualified_default_tone_only(self):
        entries, stats = symbols.parse_emoji_test(EMOJI_TEST)
        self.assertEqual(
            [e["seq"] for e in entries],
            ["1f600", "263a-fe0f", "1f44b", "1f469-200d-1f4bb", "1f1e6-1f1fa"],
        )
        self.assertEqual(stats["skin_tone"], 1)
        self.assertEqual(stats["component"], 1)
        self.assertEqual(stats["other_status"], 1)

    def test_keeps_groups_in_order(self):
        entries, _ = symbols.parse_emoji_test(EMOJI_TEST)
        self.assertEqual(entries[0]["group"], "Smileys & Emotion")
        self.assertEqual(entries[2]["subgroup"], "hand-fingers-open")
        self.assertEqual(entries[2]["name"], "waving hand")

    def test_unparsable_line_is_an_error(self):
        with self.assertRaises(ValueError):
            symbols.parse_emoji_test("# group: Flags\nnot a line\n")


class Cldr(unittest.TestCase):
    def test_lookup_tries_fully_qualified_then_stripped(self):
        t = symbols.parse_cldr(CLDR_EN, "annotations")
        self.assertEqual(symbols.cldr_lookup(t, "263a-fe0f")[0], "smiling face")
        self.assertEqual(symbols.cldr_lookup(t, "1f600")[1], ["face", "grin"])
        self.assertIsNone(symbols.cldr_lookup(t, "1f44b"))


class Fluent(unittest.TestCase):
    def test_map_strips_fe0f_and_follows_aliases(self):
        chars = {"2764-fe0f": "red-heart", "1f449": "backhand-index-pointing-right", "1f600": "grinning-face", "1f4a9": "nope"}
        icons = {
            "icons": {"red-heart": {}, "backhand-index-pointing-left": {}, "grinning-face": {}},
            "aliases": {"backhand-index-pointing-right": {"parent": "backhand-index-pointing-left", "hFlip": True}},
        }
        m = symbols.fluent_art_map(chars, icons)
        self.assertEqual(m["2764"], ("red-heart", False))
        self.assertEqual(m["1f449"], ("backhand-index-pointing-left", True))
        self.assertNotIn("1f4a9", m)


class NameMatch(unittest.TestCase):
    def test_slug(self):
        self.assertEqual(symbols.slug("flag: Australia"), "flag-australia")
        self.assertEqual(symbols.slug("backhand index pointing right"), "backhand-index-pointing-right")

    def test_adds_art_by_name_for_alias_and_plain(self):
        icons = {
            "icons": {"backhand-index-pointing-left": {}, "hairy-creature": {}},
            "aliases": {"backhand-index-pointing-right": {"parent": "backhand-index-pointing-left", "hFlip": True}},
        }
        entries = [
            {"seq": "1f449", "name": "backhand index pointing right"},
            {"seq": "1fac8", "name": "hairy creature"},
            {"seq": "1f9ff", "name": "nazar amulet"},
        ]
        m = {}
        self.assertEqual(symbols.add_name_matches(m, entries, icons), 2)
        self.assertEqual(m["1f449"], ("backhand-index-pointing-left", True))
        self.assertEqual(m["1fac8"], ("hairy-creature", False))


class Phosphor(unittest.TestCase):
    def test_catalog(self):
        icons = symbols.parse_phosphor_catalog(PHOSPHOR_JS)
        self.assertEqual([i["name"] for i in icons], ["arrow-up", "smiley"])
        self.assertEqual(icons[0]["tags"], ["direction", "north"])
        self.assertEqual(icons[1]["category"], "people")
        self.assertEqual(icons[1]["tags"], ["face", "emoji", "happy-face"])  # no *new*, alias kept

    def test_payload_is_path_data_or_raw_inner(self):
        self.assertEqual(symbols.icon_payload(ICON_SVGS["arrow-up"]), "M1,2L3,4Z")
        self.assertTrue(symbols.icon_payload(ICON_SVGS["smiley"]).startswith("<circle"))


class Kaomoji(unittest.TestCase):
    def test_parses_and_validates(self):
        rows = symbols.parse_kaomoji(KAOMOJI)
        self.assertEqual([r["id"] for r in rows], ["shrug", "happy"])
        self.assertEqual(rows[1]["tags"], ["happy", "smile", "joy"])

    def test_rejects_bad_rows(self):
        bad = [
            "a\tjoy\t(^_^)\tonly",  # one tag
            "a\tnope\t(^_^)\ta,b",  # unknown group
            "A\tjoy\t(^_^)\ta,b",  # bad id
            "a\tjoy\t(^_^)\ta,b\nb\tjoy\t(^_^)\ta,b",  # duplicate text
            "a\tjoy\t(^_^)\ta,b\na\tjoy\t(^o^)\ta,b",  # duplicate id
            "a\tjoy\t(^_^)",  # missing column
        ]
        for text in bad:
            with self.subTest(text=text), self.assertRaises(ValueError):
                symbols.parse_kaomoji(text)


class Build(unittest.TestCase):
    def test_set_is_the_intersection_with_fluent_in_catalog_order(self):
        files, frag = symbols.build_text(inputs())
        rows = symbols.inflate(files["emoji.tsv.z"]).decode().splitlines()
        self.assertEqual([r.split("\t")[0] for r in rows], ["1f600", "263a-fe0f"])
        self.assertEqual(frag["coverage"]["without_fluent_art"], 3)
        self.assertEqual({d["name"] for d in frag["dropped"]}, {"waving hand", "woman technologist", "flag: Australia"})
        self.assertEqual(frag["kept_seqs"], ["1f600", "263a-fe0f"])

    def test_names_and_keywords_from_cldr_with_fallback(self):
        files, frag = symbols.build_text(inputs(art_map={**ART_MAP, "1f44b": ("waving-hand", False)}))
        rows = [r.split("\t") for r in symbols.inflate(files["emoji.tsv.z"]).decode().splitlines()]
        self.assertEqual(rows[0][3:], ["grinning face", "face|grin"])
        self.assertEqual(rows[2][3], "waving hand")  # not in CLDR: emoji-test name
        self.assertEqual(frag["counts"]["names_missing_en"], 1)

    def test_other_languages_list_only_what_they_name(self):
        ja = symbols.parse_cldr({"annotations": {"annotations": {"\U0001F600": {"default": ["\u9854"], "tts": ["\u306b\u3063\u3053\u308a"]}}}}, "annotations")
        cldr = inputs()["cldr"]
        cldr["ja"] = ja
        files, frag = symbols.build_text(inputs(cldr=cldr))
        rows = symbols.inflate(files["names-ja.tsv.z"]).decode().splitlines()
        self.assertEqual(rows, ["1f600\t\u306b\u3063\u3053\u308a\t\u9854"])
        self.assertEqual(frag["counts"]["names_missing"]["ja"], 1)
        self.assertEqual(frag["counts"]["names_missing"]["de"], 2)

    def test_kaomoji_and_icons_files(self):
        files, frag = symbols.build_text(inputs(kaomoji=KAOMOJI + "wide\tjoy\t\\(^o^)/ \\(^o^)/ \\(^o^)/\tcheer,joy\n"))
        k = [r.split("\t") for r in symbols.inflate(files["kaomoji.tsv.z"]).decode().splitlines()]
        self.assertEqual(k[1][:4], ["happy", "(^_^)", "0", "happy|smile|joy"])
        self.assertEqual([r[4] for r in k], ["0", "0", "1"])  # only the long one is wide
        i = symbols.inflate(files["icons.bin.z"]).decode().splitlines()
        self.assertEqual(i[0], "arrow-up\tdirection|north\tarrows\tM1,2L3,4Z")
        self.assertEqual(frag["counts"]["icons"], 2)

    def test_icon_catalog_must_match_assets(self):
        with self.assertRaises(ValueError):
            symbols.build_text(inputs(icon_svgs={"arrow-up": ICON_SVGS["arrow-up"]}))

    def test_deflate_round_trips(self):
        data = ("caf\u00e9 \u306d\u3053\n" * 50).encode()
        self.assertEqual(symbols.inflate(symbols.deflate(data)), data)

    def test_ids_are_the_sequence(self):
        # The persisted id of an emoji is its lowercase hex sequence: this must never change.
        files, _ = symbols.build_text(inputs())
        first = symbols.inflate(files["emoji.tsv.z"]).decode().splitlines()[1].split("\t")[0]
        self.assertEqual(first, "263a-fe0f")


class Notices(unittest.TestCase):
    def test_every_section_carries_its_licence(self):
        text = symbols.build_notices({"unicode": "UNICODE LICENSE V3 text", "fluent": "MIT Microsoft", "phosphor": "MIT Phosphor"})
        for title, _k, _w in symbols.NOTICE_SECTIONS:
            self.assertIn(f"## {title}", text)
        self.assertIn("MIT Phosphor", text)


class Committed(unittest.TestCase):
    def test_committed_outputs_pass_check(self):
        if not symbols.MANIFEST.exists():
            self.skipTest("data not generated yet")
        self.assertEqual(symbols.check(), [])

    def test_lock_pins_every_source(self):
        lock = json.loads(symbols.LOCK.read_text())
        for k, s in lock["sources"].items():
            self.assertRegex(s["sha256"], r"^[0-9a-f]{64}$", k)
            self.assertTrue(s["url"].startswith("https://"), k)


if __name__ == "__main__":
    unittest.main()
