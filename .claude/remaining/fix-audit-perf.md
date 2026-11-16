# fix-audit-perf (branch claude/fix-audit-perf)

## Done (in the working tree; see commits)
- PERF-1/2/16: IndexCache snapshot shares items (Rc<Item>), id->position map, pinned list; patch copies pointers only; SearchIndex shares entries (Arc). Pin = edit_item_meta (keeps suggest and rich set).
- PERF-3: Main reads the shared snapshot (MainModel::with_cache).
- PERF-5: PanelModel::rows() cached (Rc<Vec<Row>>), id lookups by map.
- PERF-6/7: row build in one pass; ghosts merge in one pass; model reset in bulk.
- PERF-13/14/15: recent_shared, snapshot sensitive check, clear-list count from model.

## Next
- PERF-4/11/19: hidden windows skip render; dirty check.
- PERF-12 Recent diff; PERF-10 start-up; PERF-8 lazy model (L); PERF-9; PERF-17; PERF-18; PERF-20 docs + probe.

## Blocked / decisions
- PERF-18: Skeleton kept (design-system primitive with goldens); nothing slow is left to cover.
