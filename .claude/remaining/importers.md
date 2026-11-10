# importers (branch wt/importers) - text-expander importers in snipdeck-portable

## Done (all in crates/snipdeck-portable; tests in tests/expanders.rs, 22 tests)
- Shared: `importers/expander.rs` (Snip, Tpl builder, `import_snips`), `importers/csvtable.rs` (RFC 4180 reader + header lookup).
- `import_beeftext(lib, path, now, opts)` - JSON export (groups -> lists).
- `import_textexpander_csv`, `import_phraseexpress_csv` - CSV (header-name lookup, positional fallback, `,` `;` tab, UTF-8/16).
- `import_espanso(lib, path, now, opts)` - file or folder (reads `match/`), YAML via `serde-saphyr` 2.1 (v1.1 slice 9 replaced serde_yaml_ng; `cargo deny check` passes, unsafe-libyaml gone).
- Cargo.lock changed (slice 9: serde_yaml_ng and unsafe-libyaml removed).

## Decisions / assumptions (none verified against real exports; no sample files offline)
- No abbreviation field exists, so title = `Name (abbr)`, or the abbreviation, or the first text line.
- Only direct equivalents become placeholders (date/time/datetime/clipboard/cursor, plus `{field:}`/`{choice:}` for Espanso forms and `choice` vars, Beeftext `#{input:}`); everything else stays verbatim with a `Warning::Note`. A snip with no placeholder is stored byte-for-byte as Text (no brace escaping); one with placeholders is `ItemKind::Template` with literal braces doubled.
- Two cursors etc. (template does not parse) -> plain text + note.
- Beeftext: keys `combos[].keyword/snippet/name/group`, `groups[].uuid/name`; also bare array, `text` as fallback for `snippet`. Variables `#{clipboard|cursor|date|time|input:X}`. `#{dateTime:...}` is Qt format -> left as written.
- TextExpander: `%clipboard`, `%|` mapped; fill-ins, `%snippet:`, `%key:`, `%URL`, `%calc`, date macros left as written + note.
- PhraseExpress: header names and macros (`{#clipboard}` `{#date}` `{#time}` `{#caret}`/`{#cursor}`) from memory of the manual - UNVERIFIED. `;`-separated common.
- Espanso: `regex` triggers, image-only matches skipped with a note; `markdown`/`html` kept as text; `_`-prefixed files skipped; alias-bomb guard = expansion budget 4x file size (min 1 MiB), nesting limit 64 from serde-saphyr.
- File size cap reuses `Limits::max_json_bytes` (default 64 MiB) for all four.

## Next
- Wire into the app (below).
- TextExpander `.textexpander`/plist and PhraseExpress XML/.pxp are not read (CSV only).
- Run the importers against real exports from each tool when available.
- `cargo deny check` for the new deps; Windows `cargo check` (nothing platform-specific was added).

## Blocked
- Nothing in this crate.

## UI wiring needed (grep only; app code NOT edited)
- `crates/snipdeck-app/src/controller/settings_ctl.rs`: `ImportSource` enum (~l.38-60, 83-85, 147-173) dispatches `Ditto/CopyQ/Folder`; `ImportKind` maps the chooser index; error mapping near l.193 (`NotDitto`). Add variants Espanso(PathBuf) (file or folder picker), Beeftext, TextExpander, PhraseExpress calling `import_espanso`, `import_beeftext`, `import_textexpander_csv`, `import_phraseexpress_csv`.
- `crates/snipdeck-ui/ui/settings.slint`: l.436 comment on `import-source-chosen(kind: int)` and the choices array ~l.1248-1253 (`@tr(".snipdeck file")`, "Ditto database", "CopyQ export", "Folder ..."): add four entries.
- `crates/snipdeck-ui/ui/import_texts.slint` (~l.58-70): add `problem-not-*` texts for `Error::Malformed` from Beeftext/Espanso files; translations files need the new `@tr` strings.
- The returned `Warning::Note` strings are English prose; the UI shows warnings via the existing report display (check how `Warning::Note` is rendered/localised).
