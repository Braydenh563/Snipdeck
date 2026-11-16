# fix-audit-symbols (TOOLS-44..63)

## Done
44-53, 54-56, 59-61, 63 and 46 in commits on this branch; 57 kaomoji glyphs replaced + test; 62 docs; i18n 6 locales; ui_icons tests.

## Next
Full gate by the coordinator.

## Blocked / unverified
- Down from the search field enters the grid only via Tab or symbol-enter-grid-ticks: the panel.slint Down line (PANEL-7/8/11 worker) must bump root.symbol-enter-grid-ticks.
- Windows rendering of system-style emoji and kaomoji fonts: owner on Windows.
- TOOLS-45 offline catalogue-equality with emoji-test.txt: covered by the generator fixture test; a live-source equality test needs the network cache.
