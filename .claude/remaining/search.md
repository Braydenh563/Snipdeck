# search (LIBRARY-1..9, 50..54): built

Everything is built and merged on claude/fix-audit-library: query grammar and fold, one `SearchIndex` for Panel/Picker/Main
(`SearchNames` in `index_cache`), store V10 trigram FTS over folded title and body, marks from the matcher
(`snipdeck-app/src/search_mark.rs`), search suggestions after `tag:`/`list:` (quoted names), spoken operator description on the
three search fields, SPEC 5.2, user guide, store.md, i18n (6 locales).

## Tests
- Timing check `search_10k_items_stays_within_budget` (core `search/tests.rs`): one warm-up, best of 9 runs per query, and best
  of five chunks (scaled to 200) for removals, against `budget()`: 16 ms release (SPEC), four times that in the debug profile.
- Marks: `preview_mark_shows_the_first_line_that_matches`, `preview_mark_marks_one_line_and_none_when_no_line_matches`,
  `every_kind_of_match_is_marked`.

## Decisions (kept)
- Tags, list names and triggers are matched by the shared in-memory index (rebuilt when `change_counter` moves); FTS keeps folded
  title and body only (it needs blob reads).

## Open
- Nothing in scope. Windows-only behaviour unverified.
