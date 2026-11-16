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

## Batch (Windows, unverified)
- Done: 18 (own SendInput tagged `OWN_INPUT_TAG`; hook skips only those), 28 (`wScan` via MapVirtualKeyW). Cross-checked only.
- Post-1.0 in SPEC: 27, 21, F2 on Recent.
- Next: 15, 22, 23, 31, then 2, 3, 12, 16, 17, 19, 20, 24, 25, 26, 29, 40, 45, 10k probe.
