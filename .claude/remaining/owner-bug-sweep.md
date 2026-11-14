# owner-bug-sweep
Done: merged origin/claude/compassionate-pascal-pod8ux (d0b5b855); .pot/CODEMAP regenerated.
Next: full ui test run (goldens settings-* taken from theirs, regenerate+open); WIP visual group (scrollbar well, drag affordance, segmented pill); quick add closes after create (test first); Ctrl+V/right-click paste proof; files group (any type drop/paste; scroll glitch); gate --full; 1.99.1 clippy; report to hub.
Blocked: none.
Hub add (2026-11-14, after quick-add, before files): drag-reorder to first slot fails in Main and Panel (tests: row2/rowN above row1 -> index 0; swap rows 1,2 both ways; top drop slot reachable incl. top padding). Fix root cause, report file:line.
Status: visual group done (scrollbar well, pill, grab strip already in dock merge); quick add folds after create (QuickAdd.closed); paste tests pass with real keys; capture_files test added (any type -> File, PNG -> Image).
Drag-reorder: Panel had dead drop zones (4px above each row, row gap below): fixed with gap DropAreas + gap-target in panel.slint; tests in panel_logic (top slot, swap, gap). Main list layout already covered (test added, passes). Main GRID layout has the same ring dead zones: not fixed (note in report).

## Round 2
- Done: merged origin/claude/compassionate-pascal-pod8ux; strict search-paste assertion restored; Main grid gap DropAreas + test (failed at base).
- Main list: no dead zone at base (row Rectangle incl. gap/ring is the DropArea); owner may have hit group-header join or no drag auto-scroll (unverified).

## Round 3 (hub follow-up)
- Done: EdgeScroll (components/edge-scroll.slint) drag auto-scroll in Main list, Main grid, Panel list + tests (top/bottom/leave/drop at index 0); drop on a header's top half lands above the group (row-dropped-above; controller tests Main + Panel).
- Next: gate --staged, 1.99.1 clippy, push, report.
