# search agent (LIBRARY-1..9, 50..54)

## Done
- core: query grammar + fold + new SearchIndex (per-word union, quality ranking, trigger/list fields, caps, swap_remove)
- store: v10 migration, trigram FTS over folded title/body, per-term body matches; tests
- app: one pipeline for Panel/Picker/Main (SearchNames in index_cache), marks from the matcher
- docs: SPEC 5.2 operators, user guide "Searching", store.md

## Next
- run `cargo test -p snipdeck-app --lib --test search_surfaces --test budget` and fix fallout
  (app compiles: `cargo check -p snipdeck-app --lib --tests` was clean; tests never ran)
- clippy for snipdeck-app / snipdeck-portable / snipdeck-ui; snipdeck-ui tests that search
- UI walk of the marks (headless screenshot tests); no .slint file was changed
- optional: search_suggest quotes a multi-word name after tag:/list:

## Blocked
- snipdeck-ui (lib) build is SIGKILLed (OOM): other agents' builds use 6-9 GB of the 15 GB

## Decisions
- Tags, list names and triggers are matched by the shared in-memory index (every surface uses it);
  FTS keeps folded title + body only (needs blob reads). Reason: five more FTS write paths
  (tag rename/delete/merge, list rename, move, trigger set) could drift; the index already
  rebuilds on every change_counter move.
