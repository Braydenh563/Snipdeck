# gnest (group nesting, LIBRARY-43): built

Everything is built and merged on claude/fix-audit-library: core `GroupTree`, store V15 (`item_groups.parent`, `reparent_group`,
`restore_group`), portable `parent` (format 1.3), app (shared `GroupWalk`, controller, commands, one undo step each), Main window
indent and "Nest under..." chooser, palette command, strings (6 locales), SPEC, user guide, HANDOVER, tests (store
`group_nesting.rs`, app, portable, UI walk in `library_views_walk.rs`).

## Decisions (kept)
- `MAX_GROUP_DEPTH` = 3. `items.group_id` is the innermost group; `GroupRun.items` own members, `.run` the subtree, `.depth`.
- Ungroup lifts members and child groups to the grandparent; Delete takes the subtree along (restore brings headers back).
- Ctrl+G on marked rows all in one group makes a child of it; a mixed selection stays top level.
- Nested header moves stay inside the parent run.

## Open
- The Panel lists nested groups flat: no indent and no group colour dot (see views.md). Needs a `PanelRow` depth/colour and a
  `panel.slint` change (PANEL-* area).
- Windows behaviour unverified.
