# search agent (LIBRARY-1..9, 50..54)

## Done
- core: query grammar + fold + new SearchIndex (per-word union, quality ranking, trigger/list fields, caps, swap_remove)
- store: v10 migration, trigram FTS over folded title/body, per-term body matches; tests
- app: one pipeline for Panel/Picker/Main (SearchNames in index_cache), marks from the matcher
- docs: SPEC 5.2 operators, user guide "Searching", store.md
- search_suggest: after tag:/list: only that kind is offered; a name with a space is quoted
  (`tag:"Email replies"`), an opened quote is closed; test parses the result back
- search fields (Panel, Picker, Main) carry a spoken description of the operators; one new
  string appended to the .pot and the 6 .po files, translated (`i18n.py check --strict` passes)

## Next
- rerun `cargo test -p snipdeck-app --lib --test search_surfaces --test budget --no-fail-fast`:
  last full lib run 1291 passed, 2 failed (both my own wrong expectations, corrected since,
  not rerun); search_surfaces and budget not reached yet
- compile + run `cargo test -p snipdeck-ui --test panel the_title_mark_fits` (new measured
  walk test, never compiled) with `-- --nocapture` for the box/contrast numbers; also
  `--test picker search_mark_under_match`, `--test panel a_search_marks`
- clippy for snipdeck-app / snipdeck-portable / snipdeck-ui (core + store clean)

## Blocked
- snipdeck-ui lib compile needs ~9 GB; it is OOM-killed whenever another agent builds it too
  (11 retries in 2 h). Helper: scratchpad/search_retry.sh waits for other snipdeck_ui compiles.

## Decisions
- Tags, list names and triggers are matched by the shared in-memory index (every surface uses it);
  FTS keeps folded title + body only (needs blob reads). Reason: five more FTS write paths
  (tag rename/delete/merge, list rename, move, trigger set) could drift; the index already
  rebuilds on every change_counter move.
