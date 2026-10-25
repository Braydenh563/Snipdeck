# panel-v2 remaining work (spec: docs/reviews/panel-v2-spec.md, slices 1-3 of section 5)

Done: slice 1 committed; slice 2 settings model (7 fields, 5 enums, starts_visible) tests green.
Next: slice 2 commit after gate; slice 3 list row (work in git stash "slice3": item-card.slint quiet row, panel wiring, tests, icon text-t).
Blocked: CI step for `python3 design/tools/palette.py --check` (hook refuses edits to .github/workflows; the owner decides).
  Covered instead by the Rust test in crates/snipdeck-app/src/palette.rs (type_tile_glyphs..., search_mark..., toast_action...).
Deviations: search.mark light = blue-300 (spec says blue-200 = 1.16:1 vs bg.selected, below its own 1.3 check);
  kind tiles light X-200 / dark X-900 (the mock's quiet-row `ntile`; its dark `tile` values are not scale steps).
