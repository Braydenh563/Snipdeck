# fix-audit-perf (branch claude/fix-audit-perf)

## Done (in the working tree; see commits)
- PERF-1/2/16: IndexCache snapshot shares items (Rc<Item>), id->position map, pinned list; patch copies pointers only; SearchIndex shares entries (Arc). Pin = edit_item_meta (keeps suggest and rich set).
- PERF-3: Main reads the shared snapshot (MainModel::with_cache).
- PERF-5: PanelModel::rows() cached (Rc<Vec<Row>>), id lookups by map.
- PERF-6/7: row build in one pass; ghosts merge in one pass; model reset in bulk.
- PERF-13/14/15: recent_shared, one store query for sensitivity (read from the store, not the snapshot: a stale flag would copy a secret), clear-list count from model.

- PERF-4/11/19 (partly): a Panel/Main shown once then hidden is not drawn; show draws once. No view==shown dirty check (View has no PartialEq).
- PERF-20/21: docs/PERF.md fixed; budget.rs probe + counter test.

## Not done
- PERF-8 lazy model (L), PERF-9, PERF-10 start-up deferral, PERF-12 Recent diff/thumb cache, PERF-17 tip-host timer: not started (context/time); PERF-9 likely eased by PERF-7.

## Blocked / decisions
- PERF-18: Skeleton kept (design-system primitive with goldens); nothing slow is left to cover.
