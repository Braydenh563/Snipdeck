# Edge dock (V1-PLAN step 3a) — remaining work

Branch `agent-dock`, from 1d6b6a4. Agent: Sonnet implementation.

## Done (written, not yet built when this line was written; see Next)
- `settings.rs`: `DockEdge`, `DockTrigger`, `DockSettings` (defaults Off / 150 / 400, clamps 0..=2000 / 0..=5000), `PanelGeometry.edge` (lenient read, not written when floating).
- `frame.rs`: `handle_rect`, `docked_rect`, `is_flush`, `nearest_edge`, `Rect::along`, `Rect::geometry(monitor, edge)`.
- `dock.rs`: pure `DockState` machine (`on`, `apply_config`, `needs_tick`, `is_collapsed`) on the injected clock's `Timestamp`; unit tests for every transition plus an exhaustive sequence check (no hidden panel without a way back).
- `controller/dock_ctl.rs`: `dock_event`, `dock_after_use`, `dock_to`, `dock_toggle`, `take_pending`, `dock_resync`; `View.dock_collapsed/dock_vertical/docked`; hotkey, show, drag, frame_changed, placement, restore_frame wired. Tests in `controller/tests/dock.rs`.
- `binding.rs`: dock tick timer (50 ms, only while a delay runs), handle/panel hover callbacks, pending placements applied on redraw, paste -> `Pasted`, collapse re-placement 50 ms later.
- Slint: `components/dock-handle.slint`, `panel.slint` (`dock-*` properties, `panel-area` TouchArea wrapping the panel, handle overlay, min size lifted while collapsed), `settings.slint` "Panel dock" group.
- Settings page binding + tests; i18n: 24 new strings translated in six locales (`i18n.py check` clean).

## Next
- Build and run the tests in order (see the final report for results); fix what fails.
- Regenerate only the Slint goldens that legitimately change (settings page), read before/after.
- `python3 scripts/codemap.py`; clippy/doc workspace-wide at the end.

## Deviations from the brief (reasons)
- Types: a new 4-variant `DockEdge` in `settings.rs` instead of `frame::Edge` (that one has corner variants used for resizing; a dock edge has none).
- `handle_rect` / `docked_rect` take a `scale` (logical 6 px and the 260..480 logical width need it); `docked_rect`'s size is `(w, h)`.
- Kebab: one row "Dock to the nearest edge" / "Undock the panel" instead of five (a five-row dock group would make the menu taller than the smallest Panel). The five-way choice (Floating / Left / Right / Top / Bottom) is in Settings -> "Panel dock". Kebab words are literal English like the rest of the kebab (the Rust side cannot reach the catalogues; existing limitation).
- Docking does not change the trigger: with the trigger Off a docked panel stays open (flush, full height). Choosing Hover/Click/Both in Settings makes it collapse.
- The edge is cleared when a header drag moves a docked panel off its edge (first move after `drag()`), not on every frame report, so the half-applied frames of `PlaceWindow` can never undock it.
- Hotkey with the dock on folds/unfolds the handle instead of hiding the window (a hidden window has no handle to find).
- "Panel hovered" is a TouchArea that wraps the whole panel (an ancestor sees pointer moves before its children and forwards them, so no click is swallowed).

## Blocked
Nothing.

## Windows-unverified (owner to try on Windows)
- Hover reaching the 6 px strip at the very edge of the screen (the pointer stops at the edge pixel; the OS may report it outside the window).
- Strip DPI rounding at 125 % / 150 % (6 logical px -> 7.5 -> 8 px; the work-area edge and the strip must not leave a gap).
- Interaction with an auto-hiding taskbar on the same edge (the work area then excludes nothing: the strip and the taskbar reveal compete).
- `WS_EX_NOACTIVATE` window still gets mouse moves (assumed yes) and Slint's `has-hover` refreshing after the window grows under a stationary pointer (the OS may not send a mouse move; if not, the open panel only reports a leave after the pointer has been over it once).
- The window's minimum size really lifts when collapsed (a 6 px wide window is accepted by winit/Windows; the placement is asked again 50 ms after the collapse in case it was held to the old minimum).
- Always-on-top strip over full-screen apps, multi-monitor edges between two monitors (the strip sits on the monitor's own work area edge).
