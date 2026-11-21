# gnest agent remaining work (group nesting, LIBRARY-43)

## Decisions
- Depth limit `MAX_GROUP_DEPTH` = 3 levels. `items.group_id` stays the innermost group; `GroupRun.items` = own members, `GroupRun.run` = whole subtree run, `GroupRun.depth`.
- Ungroup (Keep) lifts members and child groups to the grandparent; Delete group trashes the whole subtree run (restore brings headers back).
- Undo of an ungroup of a parent uses `restore_group(group, members, children)` (no row moves). Nest/unnest = `reparent_group(id, parent, at)`; undo passes the old parent and old placement.
- Ctrl+G with marked rows all in one group makes a child of it; a mixed selection stays top level (legacy behaviour).
- Nest chooser (Main window "Nest under..." and palette) works in any sort order; Panel shows nested groups flat (no indent; PANEL-* out of scope).
- Nested header moves (Alt+Arrows/drag) stay inside the parent run.

## Done
- core, store, portable (+format.md), app model/controller/history/commands/binding, UI indent + chooser, i18n (6 locales), controller tests (group_nesting.rs), SPEC, user-guide, HANDOVER.
- UI walk tests in crates/snipdeck-ui/tests/library_views_walk.rs (nested indent matrix, nest chooser, goldens).

## Next
Final gate: fmt, clippy -D warnings, targeted tests, i18n check --strict; SubagentHandback.
