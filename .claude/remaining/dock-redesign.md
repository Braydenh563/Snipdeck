# Dock redesign (Fable, one task), branch claude/dock-redesign, 2026-11-12

Design: `docs/reviews/dock-redesign.md`. Decisions: `.claude/now.md`. Owner check: `tools/windows-test/README.md`
"Dock and drag".

## Done
- Pure drag machine `dock::Drag` (Float / Preview / Slide) and reveal machine `dock::DockState`, injected clock.
- Controller `controller/dock_ctl.rs`: app-driven drag from the polled cursor, cursor-only docking after a 200 ms
  hold at the edge, pull-back escape, separate floating and docked sizes per monitor, reveal by cursor poll.
- Old path removed: OS move drag, window-rect docking, size-guessing heuristics and their four tests.
- Panel: 24 px full-width grab strip above the header (`GrabStrip` in `panel.slint`); the six-dot header grip is gone.
- Goldens regenerated for the strip (47 Panel images); chrome is 24 taller, List minimum 352, Compact 252.

## Next
- Owner runs "Dock and drag" D1-D18 on Windows; fix what it finds.
- Visual polish session: Panel goldens here will conflict with theirs; regenerate after the merge, do not hand-merge.

## Blocked / unverified (Windows only)
- Smoothness of the 16 ms cursor-polled window move and `repaint_whole` during it.
- `GetAsyncKeyState` release net when the release lands outside the window.
- The 6 px topmost tab: hover, click and the 50 ms poll under other topmost windows.
- Per-monitor DPI change in mid-drag; TouchArea capture while the window moves under the pointer.
