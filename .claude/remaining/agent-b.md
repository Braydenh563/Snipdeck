# agent-b remaining work (paused by owner, WIP committed)

## Done
- MAIN-39: store `find_tag`/`merge_tag` (tests/tag_merge.rs, 3 pass), controller merge/new tag, tag manager UI (golden updated).
- MAIN-40: whole text opens/saves, edited a 16 KB part at a time (pager); 5 MiB cap; no external editor (needs Windows watcher). Tests: editor_controller, editor_window, editors_logic `large_text_is_edited_a_part_at_a_time_with_a_pager`, goldens.
- MAIN-41: `HueNames` shared names; palette.rs subset test.
- MAIN-15 drop half: Slint 1.18 has DragArea/DropArea (lists already used them); Trash and tag rows now DropAreas; `main_drop_on_trash`/`main_drop_on_tag` with controller tests; main_window_logic pointer-drag assertions added (not yet run after last edit).
- MAIN-32: PNGs and scripts/tips_images.py removed; wordless drawings in ui/tip_art.slint; golden help-tips-all-light added.
- i18n: 9 new strings applied in 6 locales, check --strict passes.
- Docs: SPEC §15 bullets removed, SPEC 5.3/5.4, DESIGN.md updated.

## Next
- Last edit (trimmed rows in tip_art.slint art 2, 3, 7) needs `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test editors` (help-tips-all-light, help-tips-*), then run editors/editors_logic/main_window_logic.
- Re-run main_controller (tag drop undo test now uses ctl.undo()), main_window_logic (new trash/tag drag assertions).
- `python3 scripts/codemap.py` (CODEMAP still lists tips_images.py), `cargo fmt --all`, clippy, scripts/gate.sh --changed.
- Note: shared /tmp/target-main gives stale store builds; `touch crates/snipdeck-store/src/*.rs` before a run.

## Blocked
- none

## Windows run
- Real-mouse drag onto sidebar Trash / tag / list.
- Narrator reading of the item editor pager and the tag-manager merge dialog.
