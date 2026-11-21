# gnest agent remaining work (group nesting, LIBRARY-43)

## Decisions
- Depth limit `MAX_GROUP_DEPTH` = 3 levels. `items.group_id` stays the innermost group; `GroupRun.items` = own members, `GroupRun.run` = whole subtree run, `GroupRun.depth`.
- Ungroup (Keep) lifts members and child groups to the grandparent; Delete group trashes the whole subtree run (restore brings headers back).
- Undo of an ungroup of a parent uses `restore_group(group, members, children)` (no row moves). Nest/unnest = `reparent_group(id, parent, at)`; undo passes the old parent and old placement.
- Ctrl+G with marked rows all in one group F makes a child of F; mixed groups are refused with a toast.
- Nested header moves (Alt+Arrows/drag) stay inside the parent run.

## Done
- core Group.parent, GroupTree, MAX_GROUP_DEPTH; store groups.rs rewrite, reparent_group, restore_group, tests/group_nesting.rs (15 pass).

## Next
portable parent field + format doc; app (main_window model, group_ctl, history, commands, binding); UI slint indent + Nest under menu/dialog; i18n; walk tests; docs (SPEC, user-guide, format.md, HANDOVER).
