# views agent remaining work
## Done
- core: Sort/SortKey, ViewFilter, Tag.colour, Board.accent/icon (tests pass)
- store: migration V12 (lists sort CHECK dropped, tags.colour, boards.accent/icon), Trash preview + tests
- portable: format 1.3 (tag colour, board accent/icon, sort words), tests/views.rs, docs/format.md
- app: ListSort -> Sort everywhere, per-section sort in settings (lists in the library), ViewFilter in MainModel,
  filter chips + Filter/Sort menus, groups keep headers under a sort (sorted inside each group), folded-group delete
  confirm, Move group to list (carries the group, Regroup recorded so Undo restores), tag colour + merge (undo), board
  accent/icon editor + persistence, Trash search (title + preview)
- ui: strings in .pot + 6 .po (check --strict clean), tag colour dialog, merge dialog, group delete dialog
- SPEC: sort/filter/trash search text, group colour+nesting and tag nesting recorded as post-1.0 with reasons
- tests: crates/snipdeck-app/tests/main_views.rs (11 tests), updated main_groups/main_controller tests
## Next (state at the PAUSE; everything below is uncommitted before this WIP commit, uncompiled/unrun after it unless noted)
- Walk test `crates/snipdeck-ui/tests/library_views_walk.rs`: 8 of 9 tests passed in the last full run (h10): filter/sort buttons and
  chip row at 480/1040/1600 x 1.0/1.5/2.25 x light/dark/hc, menus inside window, chips golden, tag colour dialog (9 swatches
  inside the card), tag manager dialog goldens, tag dot pixels, board editor colour+icon, list editor sort select.
  FAILING: `tab_reaches_filter_sort_every_chip_and_clear_in_reading_order_and_scrolls_a_chip_into_view`: passes at 1040/1.0 but at
  640/2.25 Tab cycles only <none>, Board, Navigation, Settings, Help (never reaches Filter/Sort/chips). Not yet explained (the
  last run, with 640/1.0 and 640/1.5 added to isolate scale vs width, was killed by the pause). Possibly a real focus-order bug in
  narrow/rail mode or a test artefact (second harness in the same thread); investigate before trusting. If it cannot be fixed,
  drop the 640/2.25 case and report focus order as unverified there.
- Slint changes made after 36ebe05 (all inside the walk, need the full UI crate re-run with --no-fail-fast, then goldens review):
  heading-compact (Filter/Sort icon-only when room < 380px), chip row in a horizontally scrolling Flickable with reveal-on-focus
  (Clear filter + count stay beside it; count hidden when compact; row height = chip + 3*Space.s1), LabelChip.has-focus,
  ColourPicker/IconPicker/Swatch/NoColour/IconCell `cell` size that shrinks (never under 28px) so nine targets fit the row at 2.25.
  These change existing goldens (main-window ones with a chip row, list/board editors, settings goldens regenerated earlier):
  re-run all snipdeck-ui tests, view the diffs, regenerate with UPDATE_GOLDEN=1 only for intended changes.
- Known not-mine: the top toolbar (Commands/View/New item) overflows the 480px window at text scale 2.25 (pre-existing; walk
  test skips it there and says so).
- cargo fmt, clippy --workspace --all-targets --locked -- -D warnings (not run since the slint/test edits), i18n check --strict
  (no new strings since last check), then final handback report.

## Known baseline failures (not mine)
`snipdeck-app --test editor_window`: 5 tests fail identically on the base commit b44c9ca (checked in /home/user/Snipdeck with the same target dir):
a_link_without_a_scheme_is_refused_inline_and_a_good_one_saves, a_one_character_trigger_gets_a_warning_under_the_field,
a_save_refused_while_previewing_goes_back_to_the_form_and_shows_why, new_snippet_in_the_kebab_opens_an_empty_editor_that_saves_text_from_the_keyboard,
with_both_a_text_and_a_title_refused_the_text_gets_the_keyboard_first.
