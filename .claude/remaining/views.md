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

## NOT built (needs a fresh agent)
1. Tag nesting (`Tag.parent: Option<TagId>`; schema V14 `tags.parent BLOB REFERENCES tags(id) ON DELETE SET NULL` plus a cycle
   check in the repo). Decide first:
   (a) filtering by a parent matches its descendants (`ViewFilter` takes the expansion from a `TagTree` passed in, not stored on items);
   (b) search grammar `tag:a/b` (path of names; `tag:a` also matches descendants);
   (c) rename keeps the parent, merge reparents the loser's children to the winner, delete reparents children to the deleted tag's parent;
   (d) portable: optional `parent` on TagDto (format 1.3), a missing id is malformed, cycles refused;
   (e) UI: sidebar Tags section as an indented tree (fold state in settings), tag manager "Nest under..." row + chooser dialog;
   (f) undo: `Change::Rewrite` with `Record::Tag` already carries before/after, so add `parent` to the Tag record only.
   Tests first: core (`view_filter`), store (`tags`, cycle refusal, SCHEMA_VERSION), portable (round trip, cycle refused), app
   (`main_views`: nest, filter by parent shows children, merge/delete rules, undo), UI walk (tree rows indent, hit >= 28, 2.25 scale).
2. Group nesting (`Group.parent: Option<GroupId>`; V15 if tags take V14). Decide first: a child group's members are a contiguous
   sub-run of the parent's run; a child header is drawn indented one level inside the parent's run; a folded parent hides child
   headers; `move_run`, `join_group`, `gather` and `prune` in `crates/snipdeck-store/src/groups.rs` all assume one level
   (`items.group_id` is the innermost group, ancestors come from `item_groups.parent`). Ungroup of a parent moves children to the
   grandparent; delete takes them along. Keyboard: Alt+Arrows on a header moves the subtree; Ctrl+G on a header with marked rows
   inside makes a child. Undo: `Change::Group`/`Ungroup`/`Rename` carry `Group`, so `parent` rides along.
3. The Panel's group headers carry the colour in `GroupHeader.colour`, but `panel.slint` does not draw it (PANEL-* is another
   agent's area). To finish: pass `has-dot`/`dot` to the Panel's `GroupHeader` and add `group-colour` to `PanelRow`.
4. No palette or shortcut command for "Group colour..." (header menu only); add `Command::ColourGroup` if wanted.

## Known not mine
- Pre-existing at text scale 2.25 with a Main window 640 to 1040 px wide: Tab visits only Board, Navigation, Settings, Help
  (Search, toolbar, Filter, rows never reached; hypothesis: Search collapses). The top toolbar also overflows 480 px at 2.25.
  Documented in the walk test; not fixed.
- `snipdeck-app --test editor_window`: 5 tests fail on the base commit b44c9ca too: a_link_without_a_scheme_is_refused_inline_and_a_good_one_saves,
  a_one_character_trigger_gets_a_warning_under_the_field, a_save_refused_while_previewing_goes_back_to_the_form_and_shows_why,
  new_snippet_in_the_kebab_opens_an_empty_editor_that_saves_text_from_the_keyboard,
  with_both_a_text_and_a_title_refused_the_text_gets_the_keyboard_first.
- Windows-only behaviour is unverified. The tag manager dialogs' and group colour dialog's own focus order is not covered by the
  Tab walk. cargo deny and the Windows cargo check were not run.
