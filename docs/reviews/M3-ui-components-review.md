# UI component review (Opus, de300a7..4efcf3f). Fix in order; add a test per fix.
HIGH
1 item-card.slint:563-580: revealed secret stays revealed if focus leaves while Space held (no focus-lost clear; label+preview leak). Fix: focus-changed-event clears held-reveal/reveal-armed, emit reveal-requested(false); test.
2 hotkey_recorder.slint:153-156,230-231: scope above touch with focus-on-click true swallows 1st click (focus ring from mouse). Fix: focus-on-click:false on scope; mock_single_click test.
3 template_field.slint:17,44: invisible Tab stop swallows typing for text/date. Fix: focus-on-tab-navigation only for choice kind; forward-focus to TextField otherwise; test.
MEDIUM
4 hotkey_recorder.slint:249-255: key name from event.text has Shift/AltGr applied (Ctrl+Alt+Shift+1 -> '!'); Shift alone treated as modifier-only hotkey; Shift+Tab recorded; unmapped keys emit private-use chars. Fix: reject Shift-only chords (require Ctrl/Alt/Win), let Shift+Tab move focus, map unmapped keys to rejection; test via mock key events.
5 High contrast: accent-text == bg-selected (hc-highlight) -> invisible glyph/check on selected: tag.slint:31,53; sidebar-item.slint:64; template_field.slint:167; item-card.slint:181,27. Use text-on-selected; add HC tests (measure pixels).
6 225% text scale overflow of fixed heights: keycap 20, tag 24, menu rows 28, choice 24, hotkey recorder 28/chips 20, tab 32. Use max(fixed, Type.line-*+padding) like deck; add 225% goldens/size tests.
7 dialog.slint:30-36: no focus trap; clear-focus on close. Add Tab wrap inside card + focus-restore hook.
8 tab.slint:224-243: '+' add-list has no keyboard path (DESIGN 9.4). Add FocusScope/key.
LOW
9 selected-tab focus ring covers 2px underline (tab.slint:85-100). 10 menu index/count include separators (menu.slint:210-211; test primitives_logic.rs:353 locks Some(7); fix to actionable count). 11 literal sizes / derived colours bypass tokens (choice,feedback,keycap,menu,swatch,tag; StateColor; Radius.md vs sm in tag/swatch; stale comment card-depth.slint:3-5). 12 duplicated logic (HotkeyChip vs Keycap; TemplateField choice vs Radio; clear buttons; focus-ring rect x15; pointer handler x3). 13 API inconsistency (label/a11y-label/tooltip/text; disabled vs enabled). 14 masked sensitive card announced as title only (add hint). 15 12-char title floor wrong for CJK (item-card.slint:196).
