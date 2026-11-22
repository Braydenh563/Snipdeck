# undo (LIBRARY-26..28, 33..42, 46, 47): built

All merged on claude/fix-audit-library: deferred Remove formatting, barrier step for sensitive edits, changed-fields-only save,
duplicate copy title, new item after the cursor, tag delete/merge/nest, list/board/restore/fold steps, purge pruning only affected
steps, "Delete forever" offer, withdrawn toast Undo, Help note, Ctrl+G refusals, list_text separator, stand-in title.
Since: merging a tag also links the target to trashed carriers (`Change::LinkTag`/`UnlinkTag`), one step; tests in
`crates/snipdeck-app/tests/undo_steps.rs`.

## Open
- Full UI walk (text scale 1.0/1.5/2.25, light/dark/high contrast, sizes, focus order) for the stand-in title and the Delete forever
  toast button: only an accessible-label test and a `StandInProbe` width measurement exist.
- Toast button labels "Undo", "Switch", "Open log" were English constants (items agent made the wording translatable).
- Windows CRLF / clipboard behaviour unverified.
