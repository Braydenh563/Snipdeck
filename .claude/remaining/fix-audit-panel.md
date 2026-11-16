# fix-audit-panel remaining

## Done (pushed 93c81c3)
PANEL-1,5(fuzzy/fold),6,7,8,9,11,14,30(name only; 6px is DESIGN §5),32,33,34,35,37,38,39,42; translations; goldens.

## PANEL-13: code and tests done (paste tail polled by arm_paste/paste_tick); gating now
App crate not cross-checkable for Windows here (no mingw gcc): platform.rs one-line change unverified.

## Next (order)
2,3,4 palette commands/state names/disabled reasons; 5 Page/Home/End;
10,12,15-29,31,36,40,41,43,44,45; 10k probe; SPEC post-1.0 notes.

## Blocked
none

## Batch: palette (PANEL-2/4/44/43)
- done: ten new commands (Hide Panel, Close Panel when I click away, Undo, Redo, New board, Lock, Quit, Paste as plain text, Restore, Delete for good), state-named toggles, unavailable Symbols/Colours rows with a reason (Hint), i18n, tests.
- done: PANEL-44 chip accessible description; PANEL-43 recorded post-1.0 in SPEC.
- next: dynamic board/list/tag rows (PANEL-2), PANEL-3, PANEL-5 Page/Home/End, then 10, 12, 15-29, 31, 36, 40, 41, 45.
