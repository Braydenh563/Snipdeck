# render-flicker (Opus medium worker, 2026-11-14)

## Done
- Root cause traced: docs/reviews/render-flicker-2026-11-14.md.
- Fix: opaque windows (window_kind::Plan::transparent), whole frame after OS paints for every window (repaint.rs, update_region.rs); diag `fix` removed.
- Tests: repaint (10), window_kind, diag, dock drag (never resizes), panel_window every_placement_dock_commit_and_show_draws_the_panel_whole. All pass.
- Windows steps: Quick check 20-24; diag-render.ps1 single run.
- Merged origin/claude/compassionate-pascal-pod8ux.

## Next
- gate --full, 1.99.1 clippy, push, report to hub.

## Blocked
- Windows-only verification (owner runs Quick check 20-24).
- Quick check total minutes now 25.5 (was 19.5); selftest.ps1 expects <= 15 (already over before this branch). Hub to decide which steps to retire.
