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
## Next
- Picker Pinned scope through ViewFilter (check whether still needed)
- screenshot walk (min/default/large window, text scale 1.0/1.5/2.25, light/dark/high contrast, focus order) for
  toolbar filter chips, sort menu, TagManager colour/merge dialogs, group delete dialog, board editor; goldens
- docs: user guide + HANDOVER notes
- final fmt + clippy; handback report
## Blocked
- none

## Known baseline failures (not mine)
`snipdeck-app --test editor_window`: 5 tests fail identically on the base commit b44c9ca (checked in /home/user/Snipdeck with the same target dir):
a_link_without_a_scheme_is_refused_inline_and_a_good_one_saves, a_one_character_trigger_gets_a_warning_under_the_field,
a_save_refused_while_previewing_goes_back_to_the_form_and_shows_why, new_snippet_in_the_kebab_opens_an_empty_editor_that_saves_text_from_the_keyboard,
with_both_a_text_and_a_title_refused_the_text_gets_the_keyboard_first.
