# fix-audit-perf (branch claude/fix-audit-perf)

## Done (in the working tree; see commits)
- PERF-1/2/16: IndexCache snapshot shares items (Rc<Item>), id->position map, pinned list; patch copies pointers only; SearchIndex shares entries (Arc). Pin = edit_item_meta (keeps suggest and rich set).
- PERF-3: Main reads the shared snapshot (MainModel::with_cache).
- PERF-5: PanelModel::rows() cached (Rc<Vec<Row>>), id lookups by map.
- PERF-6/7: row build in one pass; ghosts merge in one pass; model reset in bulk.
- PERF-13/14/15: recent_shared, one store query for sensitivity (read from the store, not the snapshot: a stale flag would copy a secret), clear-list count from model.

- PERF-4/11/19 (partly): a Panel/Main shown once then hidden is not drawn; show draws once. No view==shown dirty check (View has no PartialEq).
- PERF-20/21: docs/PERF.md fixed; budget.rs probe + counter test.

- PERF-10: search index built on first use + warmed 60 ms after a frame (test: the_index_is_built_on_first_use_and_a_patch_before_it_builds_nothing).
- PERF-12: Recent thumbnails built once per entry id (test: a_recent_thumbnail_is_built_once_per_entry). The Recent model is still reset on a Recent change (images are cached, so no image is rebuilt).
- PERF-9: a cursor move builds no rows and hands out the same Rc rows; render compares rows by pointer first (test: moving_the_selection_builds_no_rows_and_hands_out_the_same_rows). Pixel damage UNVERIFIED (needs the real renderer).

- PERF-8: binding/lazy_rows.rs LazyRows model (rows built in row_data; integer counters precomputed; update compares controller rows and notifies changed/added/removed; same model kept). Tests: only the rows asked for are built; an update builds nothing. Notification calls UNTESTED (slint's peer API is private); real ListView scroll/focus UNVERIFIED. Panel only; Main still uses refreshed_model.
- PERF-4/11/19: Panel sync skips render when View == shown (View derives PartialEq). Toast tick runs redraw_toast (Panel, Main, Settings, ToastWindow redraw only if their toast differs), not redraw_all. Test: a_toast_tick_runs_only_the_toast_redraws.

## Not done
- PERF-17: see docs/PERF.md (deferral justified with numbers).
- Main window rows still VecModel (overlap with main branch).
