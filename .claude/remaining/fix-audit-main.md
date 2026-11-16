# fix-audit-main remaining

## Done (tests in brackets)
- 48-54, 62: toolbar tiers, scaled minimum size, content-high rows [main_fit.rs, main_window_logic toolbar tests]
- 55: scrolling welcome window [onboarding_fit.rs]
- 1, 2, 11, 12: board on cross-board hits, capped search line, block reorder, announced moves [main_window.rs, main_controller.rs]
- 5: chooser anchors (clamped; verify in a test)
- goldens regenerated for moved tiers; DESIGN.md thresholds updated

## Next
- strings for "First {} matches in {}" in .pot + six .po
- 3, 4, 6-10, 13-21, 23-25, 27-47, 56-61, then 22 (Detail pane) and the not-reached files
- 26 stays with LIBRARY-11
