#!/usr/bin/env python3
"""Translation catalogue maintenance for crates/snipdeck-ui.

  python3 scripts/i18n.py extract   regenerate lang/snipdeck-ui.pot and add new msgids (empty msgstr)
                                    to every lang/<locale>/LC_MESSAGES/snipdeck-ui.po
  python3 scripts/i18n.py check     fail if the .pot is stale, a locale lacks / has stale msgids, a
                                    plural entry has the wrong number of forms, or a translation
                                    changes the {} / {0} placeholders of its source
  python3 scripts/i18n.py todo LOCALE   print the untranslated entries of LOCALE as JSON
  python3 scripts/i18n.py apply LOCALE FILE
                                    fill LOCALE from a JSON file written by `todo` with a
                                    "msgstr" (a list of one string per plural form) added to each entry

Strings come from `@tr("...")` and `@tr("ctx" => "...")` in crates/snipdeck-ui/ui/**/*.slint (the
default translation context is off, see build.rs). A plural, `@tr("one" | "many" % n, n)`, becomes
`msgid` + `msgid_plural` with one `msgstr[i]` per plural form of the locale (`PLURAL_FORMS`).
Untranslated strings fall back to the English source at run time, so `check` reports them as a warning
count and only fails on structural problems.
"""
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent / "crates" / "snipdeck-ui"
LOCALES = ["es", "de", "fr", "pt-BR", "ja", "zh-Hans"]
# (nplurals, plural expression) per locale. French and Brazilian Portuguese count 0 and 1 as singular.
PLURAL_FORMS = {
    None: (2, "(n != 1)"),
    "es": (2, "(n != 1)"),
    "de": (2, "(n != 1)"),
    "fr": (2, "(n > 1)"),
    "pt-BR": (2, "(n > 1)"),
    "ja": (1, "0"),
    "zh-Hans": (1, "0"),
}
STR = r'"((?:[^"\\]|\\.)*)"'
TR = re.compile(r'@tr\(\s*(?:' + STR + r'\s*=>\s*)?' + STR + r'(?:\s*\|\s*' + STR + r')?')
PLACEHOLDER = re.compile(r"\{\d*\}")


class Entry:
    """One catalogue entry: `plural` is the msgid_plural (None for a plain string), `forms` the msgstr list."""

    def __init__(self, plural=None, forms=None):
        self.plural = plural
        self.forms = forms if forms is not None else []

    def done(self):
        return bool(self.forms) and all(self.forms)


def collect():
    """(ctx, msgid) -> {"refs": [file:line], "plural": msgid_plural or None}."""
    found = {}
    for path in sorted((ROOT / "ui").rglob("*.slint")):
        for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            for m in TR.finditer(line):
                ctx, msg, plural = m.group(1), m.group(2), m.group(3)
                entry = found.setdefault((ctx, msg), {"refs": [], "plural": None})
                entry["refs"].append(f"{path.relative_to(ROOT).as_posix()}:{n}")
                if plural is not None:
                    entry["plural"] = plural
    return found


def header(lang):
    lines = ['msgid ""', 'msgstr ""', '"Project-Id-Version: snipdeck-ui\\n"']
    if lang:
        lines.append(f'"Language: {lang}\\n"')
    lines += ['"MIME-Version: 1.0\\n"', '"Content-Type: text/plain; charset=UTF-8\\n"',
              '"Content-Transfer-Encoding: 8bit\\n"']
    count, expr = PLURAL_FORMS[lang]
    lines.append(f'"Plural-Forms: nplurals={count}; plural={expr};\\n"')
    return "\n".join(lines)


def quoted(line, keyword):
    """The text between the quotes of a `keyword "..."` line."""
    return line[len(keyword):].strip()[1:-1]


def entry_keys(text):
    """(ctx, msgid) -> Entry for an existing po/pot file. Handles msgid_plural, msgstr[i] and
    strings continued over several lines."""
    entries, ctx, msgid, plural, forms = {}, None, None, None, []
    cur = None  # where a continuation line goes: ("ctx"|"id"|"plural"|"str", index)

    def flush():
        if msgid:
            entries[(ctx, msgid)] = Entry(plural, list(forms))

    for raw in text.splitlines():
        line = raw.strip()
        if line.startswith("msgctxt "):
            flush()
            ctx, msgid, plural, forms, cur = quoted(line, "msgctxt"), None, None, [], ("ctx", 0)
        elif line.startswith("msgid_plural "):
            plural, cur = quoted(line, "msgid_plural"), ("plural", 0)
        elif line.startswith("msgid "):
            if cur is None or cur[0] != "ctx":
                flush()
                ctx = None
            msgid, plural, forms, cur = quoted(line, "msgid"), None, [], ("id", 0)
        elif line.startswith("msgstr["):
            index = int(line[7:line.index("]")])
            forms.extend([""] * (index + 1 - len(forms)))
            forms[index] = line[line.index("]") + 1:].strip()[1:-1]
            cur = ("str", index)
        elif line.startswith("msgstr "):
            forms = [quoted(line, "msgstr")]
            cur = ("str", 0)
        elif line.startswith('"') and line.endswith('"') and cur:
            part = line[1:-1]
            kind, index = cur
            if kind == "ctx":
                ctx = (ctx or "") + part
            elif kind == "id":
                msgid = (msgid or "") + part
            elif kind == "plural":
                plural = (plural or "") + part
            else:
                forms[index] += part
    flush()
    return entries


def fuzzy_keys(text):
    """The (ctx, msgid) keys marked `#, fuzzy` (a translator is unsure; Slint treats them as untranslated)."""
    out, flagged, ctx = set(), False, None
    for line in text.splitlines():
        if line.startswith("#,") and "fuzzy" in line:
            flagged = True
        elif line.startswith("msgctxt "):
            ctx = line[8:].strip()[1:-1]
        elif line.startswith("msgid ") and not line.startswith("msgid_plural"):
            if flagged and line.strip() != 'msgid ""':
                out.add((ctx, line[6:].strip()[1:-1]))
            flagged, ctx = False, None
    return out


def write_file(path, lang, keys, plurals, existing, refs, fuzzy=frozenset()):
    """Writes the catalogue. `plurals` maps a key to its msgid_plural; `existing` holds the
    translations to keep (Entry by key)."""
    count = PLURAL_FORMS[lang][0]
    parts = [header(lang)]
    for key in keys:
        ctx, msg = key
        block = []
        if refs:
            block.append("#: " + " ".join(refs[key]))
        if key in fuzzy:
            block.append("#, fuzzy")
        if ctx:
            block.append(f'msgctxt "{ctx}"')
        block.append(f'msgid "{msg}"')
        old = existing.get(key)
        have = old.forms if old else []
        if plurals.get(key) is not None:
            block.append(f'msgid_plural "{plurals[key]}"')
            for i in range(count):
                block.append(f'msgstr[{i}] "{have[i] if i < len(have) else ""}"')
        else:
            block.append(f'msgstr "{have[0] if have else ""}"')
        parts.append("\n".join(block))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n\n".join(parts) + "\n", encoding="utf-8")


def po_path(lang):
    return ROOT / "lang" / lang / "LC_MESSAGES" / "snipdeck-ui.po"


def unescape(text):
    """The text of a Slint/PO string literal as it reads (\\n, \\", \\\\ resolved)."""
    return re.sub(r"\\(.)", lambda m: "\n" if m.group(1) == "n" else m.group(1), text)


def escape(text):
    return text.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n")


def placeholders(text):
    return sorted(PLACEHOLDER.findall(text))


def problems_of(lang, keys, plurals, have):
    """Structural problems of one locale's entries: the wrong number of plural forms, or a
    translation whose {} placeholders differ from its source's."""
    count = PLURAL_FORMS[lang][0]
    out = []
    for key in keys:
        entry = have.get(key)
        if entry is None or not any(entry.forms):
            continue
        plural = plurals.get(key)
        wanted = count if plural is not None else 1
        if len(entry.forms) != wanted:
            out.append(f'{lang}: "{key[1]}" has {len(entry.forms)} translation forms, wants {wanted}')
            continue
        for i, form in enumerate(entry.forms):
            if not form:
                continue
            source = plural if (plural is not None and (i > 0 or count == 1)) else key[1]
            if placeholders(form) != placeholders(source):
                out.append(f'{lang}: "{key[1]}" form {i} has placeholders {placeholders(form)}, source {placeholders(source)}')
    return out


def load():
    found = collect()
    keys = sorted(found, key=lambda k: (k[0] or "", k[1]))
    plurals = {k: found[k]["plural"] for k in keys}
    refs = {k: found[k]["refs"] for k in keys}
    return keys, plurals, refs


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    mode = argv[0] if argv else "check"
    keys, plurals, refs = load()
    pot = ROOT / "lang" / "snipdeck-ui.pot"
    if mode == "extract":
        write_file(pot, None, keys, plurals, {}, refs)
        for lang in LOCALES:
            p = po_path(lang)
            text = p.read_text(encoding="utf-8") if p.exists() else ""
            write_file(p, lang, keys, plurals, entry_keys(text), None, fuzzy_keys(text))
        print(f"{len(keys)} strings, {len(LOCALES)} locales")
        return 0
    if mode == "todo":
        lang = argv[1]
        have = entry_keys(po_path(lang).read_text(encoding="utf-8"))
        fuzzy = fuzzy_keys(po_path(lang).read_text(encoding="utf-8"))
        count = PLURAL_FORMS[lang][0]
        todo = []
        for key in keys:
            entry = have.get(key)
            if entry is not None and entry.done() and key not in fuzzy:
                continue
            item = {"msgid": unescape(key[1]), "where": refs[key][0]}
            if key[0]:
                item["context"] = key[0]
            if plurals[key] is not None:
                item["msgid_plural"] = unescape(plurals[key])
                item["forms_wanted"] = count
            todo.append(item)
        json.dump(todo, sys.stdout, ensure_ascii=False, indent=1)
        print()
        return 0
    if mode == "apply":
        lang, source = argv[1], pathlib.Path(argv[2])
        text = po_path(lang).read_text(encoding="utf-8")
        have = entry_keys(text)
        by_text = {(k[0], unescape(k[1])): k for k in keys}
        applied, bad = 0, []
        for item in json.loads(source.read_text(encoding="utf-8")):
            key = by_text.get((item.get("context"), item["msgid"]))
            forms = item.get("msgstr")
            if key is None or not forms:
                continue
            if isinstance(forms, str):
                forms = [forms]
            entry = Entry(plurals[key], [escape(f) for f in forms])
            wrong = problems_of(lang, [key], plurals, {key: entry})
            if wrong:
                bad += wrong
                continue
            have[key] = entry
            applied += 1
        write_file(po_path(lang), lang, keys, plurals, have, None)
        print(f"{lang}: applied {applied}, refused {len(bad)}")
        for line in bad:
            print("refused:", line)
        return 1 if bad else 0
    problems, untranslated = [], 0
    if not pot.exists() or entry_keys(pot.read_text(encoding="utf-8")).keys() != set(keys):
        problems.append("lang/snipdeck-ui.pot is stale: run `python3 scripts/i18n.py extract`")
    for lang in LOCALES:
        p = po_path(lang)
        if not p.exists():
            problems.append(f"missing {p.relative_to(ROOT)}")
            continue
        have = entry_keys(p.read_text(encoding="utf-8"))
        if set(have) != set(keys):
            problems.append(f"{lang}: msgids differ from the source (run extract)")
        for key, entry in have.items():
            if key in plurals and (plurals[key] is None) != (entry.plural is None):
                problems.append(f'{lang}: "{key[1]}" is a plural in the source but not here (run extract)')
        untranslated += sum(1 for e in have.values() if not e.done())
        problems += problems_of(lang, keys, plurals, have)
    for p in problems:
        print("error:", p)
    if untranslated:
        print(f"warning: {untranslated} untranslated entries (English fallback at run time)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
