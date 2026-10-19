#!/usr/bin/env python3
"""Translation catalogue maintenance for crates/snipdeck-ui.

  python3 scripts/i18n.py extract   regenerate lang/snipdeck-ui.pot and add new msgids (empty msgstr)
                                    to every lang/<locale>/LC_MESSAGES/snipdeck-ui.po
  python3 scripts/i18n.py check     fail if the .pot is stale or a locale lacks / has stale msgids

Strings come from `@tr("...")` and `@tr("ctx" => "...")` in crates/snipdeck-ui/ui/**/*.slint (the
default translation context is off, see build.rs). Plurals (`@tr("one" | "many" % n)`) are not used yet.
Untranslated strings fall back to the English source at run time, so `check` reports them as a warning
count and only fails on structural problems.
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent / "crates" / "snipdeck-ui"
LOCALES = ["es", "de", "fr", "pt-BR", "ja", "zh-Hans"]
TR = re.compile(r'@tr\(\s*(?:"((?:[^"\\]|\\.)*)"\s*=>\s*)?"((?:[^"\\]|\\.)*)"')


def collect():
    found = {}
    for path in sorted((ROOT / "ui").rglob("*.slint")):
        for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            for m in TR.finditer(line):
                ctx, msg = m.group(1), m.group(2)
                found.setdefault((ctx, msg), []).append(f"{path.relative_to(ROOT).as_posix()}:{n}")
    return found


def header(lang):
    lines = ['msgid ""', 'msgstr ""', '"Project-Id-Version: snipdeck-ui\\n"']
    if lang:
        lines.append(f'"Language: {lang}\\n"')
    lines += ['"MIME-Version: 1.0\\n"', '"Content-Type: text/plain; charset=UTF-8\\n"',
              '"Content-Transfer-Encoding: 8bit\\n"']
    lines.append('"Plural-Forms: nplurals=1; plural=0;\\n"' if lang in ("ja", "zh-Hans")
                 else '"Plural-Forms: nplurals=2; plural=(n != 1);\\n"')
    return "\n".join(lines)


def entry_keys(text):
    """(ctx, msgid) -> msgstr for an existing po/pot file (single-line strings only)."""
    entries, ctx, msgid, msgstr, cur = {}, None, None, None, None
    def flush():
        if msgid:
            entries[(ctx, msgid)] = msgstr or ""
    for line in text.splitlines():
        if line.startswith("msgctxt "):
            flush(); ctx, msgid, msgstr, cur = line[8:].strip()[1:-1], None, None, "ctx"
        elif line.startswith("msgid "):
            if cur not in ("ctx",):
                flush(); ctx = None
            msgid, msgstr, cur = line[6:].strip()[1:-1], None, "id"
        elif line.startswith("msgstr "):
            msgstr, cur = line[7:].strip()[1:-1], "str"
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
        elif line.startswith("msgid "):
            if flagged and line.strip() != 'msgid ""':
                out.add((ctx, line[6:].strip()[1:-1]))
            flagged, ctx = False, None
    return out


def write_file(path, lang, keys, existing, refs, fuzzy=frozenset()):
    parts = [header(lang)]
    for ctx, msg in keys:
        block = []
        if refs:
            block.append("#: " + " ".join(refs[(ctx, msg)]))
        if (ctx, msg) in fuzzy:
            block.append("#, fuzzy")
        if ctx:
            block.append(f'msgctxt "{ctx}"')
        block.append(f'msgid "{msg}"')
        block.append(f'msgstr "{existing.get((ctx, msg), "")}"')
        parts.append("\n".join(block))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n\n".join(parts) + "\n", encoding="utf-8")


def po_path(lang):
    return ROOT / "lang" / lang / "LC_MESSAGES" / "snipdeck-ui.po"


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else "check"
    found = collect()
    keys = sorted(found, key=lambda k: (k[0] or "", k[1]))
    pot = ROOT / "lang" / "snipdeck-ui.pot"
    if mode == "extract":
        write_file(pot, None, keys, {}, found)
        for lang in LOCALES:
            p = po_path(lang)
            text = p.read_text(encoding="utf-8") if p.exists() else ""
            write_file(p, lang, keys, entry_keys(text), None, fuzzy_keys(text))
        print(f"{len(keys)} strings, {len(LOCALES)} locales")
        return 0
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
        untranslated += sum(1 for v in have.values() if not v)
    for p in problems:
        print("error:", p)
    if untranslated:
        print(f"warning: {untranslated} untranslated entries (English fallback at run time)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
