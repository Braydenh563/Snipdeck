# items agent (branch claude/fix-audit-library-items)

## Done (committed)
- 31 link scheme check, 56 Open in browser, 49 title limit (200) + counter, toast button wording translatable
- 44/45 trigger fires-at (Immediate/Delimiter) + case mode (Exact/Ignore/Adapt): core, store V11, portable, app retype, editor selectors, pre-save notes
- 32 duplicate text asked before saving in the item editor (Already saved / Open it / Save anyway)
- i18n: all new strings in pot + six .po
- merged claude/fix-audit-library (undo, search V10, tmpl); store schema is V10 (search) then V11 (mine)
- 32 for capture hotkey and paste-as-new: SavedAgain toast ("Saved to X. The same text was already in Y.") and it still saves
- 29: Text tools row (find/replace + match count, transforms, monospace switch, line numbers, one-step Undo);
  logic in snipdeck_core::text_tools

## Next
- run the targeted tests and update the 3 editor goldens after looking at the PNGs
- commit

## Not built (marked in SPEC 5.4 / HANDOVER with reasons)
- rope viewer (text over 16 KB is a read-only prefix), image editor, JPEG/WebP/GIF/BMP decoding, Replace on Image/File

## Blocked
- none
