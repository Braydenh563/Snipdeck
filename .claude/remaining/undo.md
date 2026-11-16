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

## Next
- Run the targeted tests, fmt, clippy; fix failures.
- UI walk for the stand-in title and the Delete forever button (headless screenshots).

## Blocked
- Windows CRLF / clipboard behaviour is unverified (no Windows here).
