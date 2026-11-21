# views agent remaining work

## Done (all committed on claude/fix-audit-library-views)
- LIBRARY-10/11/12/43/48/55: core Sort/ViewFilter, store V12, portable 1.3, app, UI, strings, walk tests.
- Walk test `crates/snipdeck-ui/tests/library_views_walk.rs`: filter/sort buttons and chip row at 480/1040/1600 x 1.0/1.5/2.25 x
  light/dark/hc, menus, goldens, tag colour dialog, tag dot pixels, board editor, list editor sort, Tab order (1040/1.0, 640/1.0,
  640/1.5, 1600/2.25), group dot + colour dialog.
- Full `cargo test -p snipdeck-ui --no-fail-fast` green; store (356), portable (216), app lib (1336), main_views/main_controller/
  undo_steps/search_surfaces green.
- Fixed while verifying: stale `schema_version == 11` literals in store tests (now `snipdeck_store::SCHEMA_VERSION`); the `old_db`
  helpers in store/tests/recent.rs did not drop the V12 columns.
- Group colour (LIBRARY-43/48): `Group.colour: Option<ListAccent>`, store V13 (`item_groups.colour`), portable `colour` on a group
  (format 1.3), one undo step (`Change::Rename`), carried by "Move group to list", header dot + "Group colour..." menu row +
  dialog in the Main window (ColourPicker moved to `components/colour-picker.slint`), strings in .pot + 6 .po, SPEC/user-guide/format.md.

## Built since (all committed on claude/fix-audit-library-views)
- editor_window failures: stale expectations updated (LIBRARY-31 link scheme, LIBRARY-44 trigger wording); 36 pass.
- Tab order at 2.25 / 640-1040: a collapsed Search input made Slint skip every later stop; Search has a minimum width and the
  toolbar sheds Undo/Redo at `room < 340` (`toolbar-history`). Walk test covers 7 size/scale combinations.
- Nested tags, end to end: core `Tag.parent` + `TagTree` (`walk`, `ancestors`, `can_nest`, `search_paths`); store V14 + `Error::TagParent`
  + delete reparents children; portable `parent` (ancestors of used tags exported, loops and unknown ids refused, existing tags keep
  their place); app tree-ordered tags, counts incl. descendants, hierarchical Section/filter/search (`tag:a`, `tag:a/b`, `tag:b`),
  `main_nest_tag`, rename keeps parent, merge lifts children to the target, delete lifts them to the grandparent, one undo step each;
  UI indented sidebar, Tag manager nest button + dialog, strings in .pot + 6 .po, measured walk tests + goldens.
- `Command::ColourGroup` (palette "Group colour…"), opens the dialog over the header under the cursor.

## NOT built (needs a fresh agent)
1. Group nesting (`Group.parent: Option<GroupId>`). Store V15 already adds `item_groups.parent BLOB` (no REFERENCES: SQLite cannot
   DROP COLUMN with one, so the repository must enforce validity) but nothing reads or writes it. Decide first: a child group's
   members are a contiguous sub-run of the parent's run; a child header is drawn indented one level inside the parent's run; a folded
   parent hides child headers; `move_run`, `join_group`, `gather` and `prune` in `crates/snipdeck-store/src/groups.rs` all assume one
   level (`items.group_id` is the innermost group, ancestors come from `item_groups.parent`). Ungroup of a parent moves children to the
   grandparent; delete takes them along. Keyboard: Alt+Arrows on a header moves the subtree; Ctrl+G on a header with marked rows
   inside makes a child. Undo: `Change::Group`/`Ungroup`/`Rename` carry `Group`, so `parent` rides along. Portable: `parent` on GroupDto
   (format 1.3). UI: `SidebarItem.indent`-style step on the header and its cards. Tests first (store, app, portable, walk).
2. The Panel's group headers carry the colour in `GroupHeader.colour`, but `panel.slint` does not draw it (PANEL-* is another
   agent's area). To finish: pass `has-dot`/`dot` to the Panel's `GroupHeader` and add `group-colour` to `PanelRow`.
3. Editor tag chips and the filter's tag chooser still show the bare tag name, not the path (two tags can share a leaf under
   different parents only if names differ: names are unique, so this is cosmetic).

## Known not mine
- Windows-only behaviour is unverified. The tag manager dialogs' and group colour dialog's own focus order is not covered by the
  Tab walk. cargo deny and the Windows cargo check were not run.
