# search agent (LIBRARY-1..9, 50..54)

## Done
- core: query grammar + fold + new SearchIndex (per-word union, quality ranking, trigger/list fields, caps, swap_remove)
- store: v10 migration, trigram FTS over folded title/body, per-term body matches; tests
- app: one pipeline for Panel/Picker/Main (SearchNames in index_cache), marks from the matcher
- docs: SPEC 5.2 operators, user guide "Searching", store.md

## Next
- run app lib tests + tests/search_surfaces.rs; fix fallout in snipdeck-ui tests
- clippy/fmt, commit; UI walk of the marks (headless screenshot tests)

## Blocked
- none

## Decisions
- Tags, list names and triggers are matched by the shared in-memory index (every surface uses it);
  FTS keeps folded title + body only (needs blob reads). Reason: five more FTS write paths
  (tag rename/delete/merge, list rename, move, trigger set) could drift; the index already
  rebuilds on every change_counter move.
