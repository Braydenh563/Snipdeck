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

- Review fixes (coordinator's review of 529fd6b): hotkey routing and focus, per-edge-kind docked sizes with the
  full-height guard, button-up net that needs a seen "down", grab offset across scales and docked widths, a resize
  or a fresh dock holds the Panel out, the tab follows its monitor and work area, anchor on a docked monitor.
- Then: full-screen app in front (tab not topmost, no reveal, hotkey still works), hidden start and tray toggle
  keep the tab, an older version's docked Panel stays open until docked again, second click on the kebab closes a
  risen menu, strip tooltip (gone in a drag, docked wording), strip has no button role, dock strings translated
  (6 locales), goldens for the drag preview and the dragged strip, settings/onboarding/user-guide wording.

- Re-verification fixes (R1-R15, 6ed1fbbb on): full screen asked per monitor, the tab reveals from just past an
  inner edge and a dock there drags onto the next monitor, corner wobble keeps the first edge, same-scale monitor
  resizes nothing, legacy full-height dock mended in one reader, resize hold without a readable button,
  `drag_cancel` and `starts_tucked` removed, hidden start and every first show placed before the window exists
  (`find_monitor`), blur hint wording, docs.

- Review minors (d37d9c9) fixed: a monitor with no stored entry mends a legacy full-height dock
  (`dock_ctl::mend_full_height`, shared with `geometry_on`); `in_size_loop(panel)` counts only a loop on the
  Panel's window (`win_logic::size_loop_on_panel`); SPEC "only a docked Panel that tucks is always on top".
  Windows `snipdeck-app` not checked locally (no mingw): CI's Windows `cargo check` covers `platform.rs`.

## Next
- Owner runs "Dock and drag" D1-D23 on Windows; fix what it finds.
- A pinned floating Panel still stays above a full-screen app (only the dock steps down).
- Visual polish session: Panel goldens here will conflict with theirs; regenerate after the merge, do not hand-merge.

## Blocked / unverified (Windows only)
- Smoothness of the 16 ms cursor-polled window move and `repaint_whole` during it.
- `GetAsyncKeyState` release net when the release lands outside the window.
- The 6 px topmost tab: hover, click and the 50 ms poll under other topmost windows.
- Per-monitor DPI change in mid-drag; TouchArea capture while the window moves under the pointer.
- Full-screen probe per monitor (`target::fullscreen_covers`: frameless window covering the monitor, in front or
  at its middle; polled every 50 ms while docked): borderless games, video players, PowerPoint, F11 browsers.
- Hidden start as a tab and every first show: the window is placed before it exists (Slint applies the stored
  position and size when winit creates it); no flash and no stolen focus is the aim, unverified.
- Reveal from just past an inner edge and a drag across a shared edge on a real two-monitor desk.
- The resize hold under an elevated foreground window (button always reads up).

## Round 2 (coordinator re-verify on 771b44b)

- First show after a hidden start re-resolves monitors (`find_monitor` each time; pointer-monitor fallback). Test: `the_first_show_after_a_hidden_start_asks_again_where_the_monitors_are`.
- Resize hold: size loop first (`TargetSource::size_loop`, Windows `GUI_INMOVESIZE`), then button, then 1 s quiet. Test: `a_resize_lasts_as_long_as_the_windows_own_size_loop`. Windows-unverified: whether timers tick inside the modal size loop.
- Legacy full-height docks mended in `geometry_on` for the docked size too, and on a drag to another monitor. Two tests.
- Docs: full screen per monitor; new install unpinned; Off hides a docked Panel like a floating one.
- Open: `scripts/gate.sh --full` on the final head; send its three lines to the coordinator. No PR, no merge.

## Paused (hub relayed the owner's request to pause overnight) at d37d9c97

Done and pushed: rounds 1 and 2 (hub's round-3 re-verify confirmed all five round-2 items). No gate --full has completed on any head of this branch; the run on d37d9c97 was stopped. Not gated: do not merge on the strength of this file.

Open, verified against the code, nothing written yet (tests first):
1. `panel.rs` `placement()`: the branch for a monitor with no stored entry clamps the raw newest entry, so a legacy full-height dock (edge Some, along None) floats screen-tall at first show on another monitor. Factor the mend out of `geometry_on` into a helper (PanelGeometry + work area + scale) and use it there. Test: legacy entry on a monitor that is gone, hidden start, pointer on the other monitor, ordinary height. Limit to state: the gone monitor's work area is unknown, so "full" can only be judged against the target monitor (a 1040 legacy height carried to a 1380 work area would stay).
2. `screen.rs` `in_size_loop()` reads `GUI_INMOVESIZE` for the whole UI thread: a move/size loop on the Main or Settings window counts as the Panel's. Compare `GUITHREADINFO.hwndMoveSize` with the Panel's HWND (`TargetTracker` holds it in `panel`; needs a getter); `Some(false)` when another window is in the loop. Pure comparison in `win_logic` with a unit test.
3. `docs/SPEC.md:72`: "only a docked Panel is always on top" -> "only a docked Panel that tucks is always on top" (`topmost()` falls back to the pin when the trigger is Off).

Then: fmt, codemap, Windows clippy of snipdeck-platform, app lib tests, `scripts/gate.sh --full` once, send its three lines to the hub, final report to the owner. No PR, no merge.
