# agent-b remaining work

## Done (code written; see commits)
- MAIN-39: store `find_tag` / `merge_tag`, controller merge/new tag, tag manager UI.
- MAIN-40: whole text opens and saves, edited a 16 KB part at a time (pager); 5 MiB cap; no external-editor action (needs a Windows watcher).
- MAIN-41: `HueNames` global is the one name list; subset test in `palette.rs`; stored ids unchanged.
- MAIN-15 drop half: Slint 1.18 does have DragArea/DropArea (lists already used them), so Trash and tag rows got DropAreas; controller `main_drop_on_trash` / `main_drop_on_tag`.
- MAIN-32: PNG crops and `scripts/tips_images.py` removed; wordless drawings in `ui/tip_art.slint`.

## Next
- Run targeted tests, goldens (UPDATE_GOLDEN=1 for item-editor-long-light, help tips), i18n extract/apply for the new strings.

## Blocked
- none

## Windows run
- Drag onto sidebar Trash / tag / list with a real mouse (DropArea needs the OS pointer path).
- Narrator reading of the item editor pager ("Part 1 of 2") and the tag manager merge dialog.
