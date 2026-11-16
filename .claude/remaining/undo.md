# undo agent (branch claude/fix-audit-library-undo)

## Done (code written; test status in the last commit message)
- Store: tag_items, link_tag, is_stored (committed earlier).
- 26 deferred Remove formatting (slot.drop_formats, applied at Save, one undo step).
- 27 barrier step for sensitive text edits; Undo says why (UndoStopped).
- 28 save applies only changed fields (Fields::merge_onto in editor_ctl/save.rs).
- 33 duplicate "X copy" (set_copy_title from the host), cursor on the copy.
- 34 new item after the cursor row, joins its group (manual order only).
- 35 tag delete is a step; snapshots tolerate a missing tag.
- 36 undo/redo peek, apply, commit on success.
- 37 panel bulk colour recorded; tests/history_bypass.rs.
- 38 list/board/tag edits, restore, group fold are steps; purge prunes only affected steps.
- 39 undo of an Add offers "Delete forever".
- 40 toast Undo withdrawn when a later step overlaps.
- 41 Help (Shortcuts tab note) and docs/SPEC.md 5.7a.
- 42 Ctrl+G refusals toast.
- 46 list_text separator checked, CRLF on Windows.
- 47 "Sensitive item" stand-in title (item-card.slint, StandInProbe).
- i18n: 12 new strings in the .pot and six .po files.

## Verified (last run)
- Targeted tests green: snipdeck-app lib 1306, editor_controller 53, help_window 8, history 17, main_controller 102, toast_texts 5, undo_steps 12, history_bypass 1; snipdeck-ui stand_in_title_logic, i18n, deck_logic, quiet_card_logic.
- cargo fmt clean; cargo clippy --workspace --all-targets --locked -D warnings clean.

## Next (coordinator)
- Regenerate docs/CODEMAP.md (python3 scripts/codemap.py); run the full gate.
- Full UI walk (text scale 1.0/1.5/2.25, light/dark/high contrast, window sizes, focus order) for the stand-in title and the Delete forever toast button: only an accessible-label test and a StandInProbe width measurement exist.

## Blocked / unverified
- Windows CRLF / clipboard behaviour (no Windows here).
- Toast button labels "Undo", "Switch", "Open log" remain untranslated English constants (pre-existing).
