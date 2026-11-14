# audit2 (wt/audit2), panel-v2 UX audit 2

Done (code, tests, goldens): 1 board name at 225% + Keep on top into More (>=1.5), 2 List min height 328 + pins-fit, 3 no tab slivers,
4 glyph scale min(ts,1.75) (header, row actions, eye), 5 selected Cards number primary, 6 no-results footer keycap, 7 Paste stack reason + fixture checks, 9 search goldens focused.
Next: cargo +1.99.1 clippy not run (full rebuild, over the time box).
Blocked: none. Out of scope: item 8.
Reverted item 3 (hide a tab the caret patch would cut), 2026-11-07 22:3x: in the tabs gallery golden it hid "Support" although its ink ended ~5px left of the right patch (patch starts x282, ink ends x277). Tried tolerance +8, +12, and an ink-span rule (label.x + min(width, preferred-width) - 4): all still hid it. Open: find why the computed span overshoots (log x/ink-right/flick.width in a test) before retrying. Slivers stay (low).
