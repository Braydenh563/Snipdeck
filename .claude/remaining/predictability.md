# predictability (show/hide/move/resize of Panel and Main)

## Scout findings (code paths)
- Panel frame events: main.rs watch_window_events (Moved -> Binding::frame_moved, Resized -> frame_changed) -> controller/panel.rs remember_frame (clamp, store per monitor id in settings.panel_geometry, newest last).
- Show: binding.rs apply ShowPanel (show + raise), then main.rs on_effect ShowPanel -> restore_frame -> panel.rs restore_frame -> placement(monitor) -> PlaceWindow -> binding.rs place() (set_position then set_size).
- Hide: binding.rs apply HidePanel -> window.hide(); the window keeps its frame.
- Reset size: commands_ctl.rs ResetPanel / app menu -> dock_ctl.rs reset_panel_geometry (drops this monitor's entry, places open_frame clamped). The list is pure Slint layout (ListView height = parent), a probe test (scroll to bottom, grow 300->900->480 logical) showed the viewport follows the window: no cached height on the Rust side for the Panel. refit()/set_height_fitted (editor_binding.rs:50, native_window.rs:225) is the item editor only, not the Panel.
- Main: main_binding.rs open (restore_place once, then fit_when_settled), watch_place stores place_of on Moved/Resized; native_window.rs restore_place fits to the work area of the monitor the window lands on (winit current_monitor is nearest on Windows).

## Defects found
1. panel.rs placement(): a monitor with no stored entry (vanished monitor, changed resolution -> new id) gives None, so show places nothing and the window keeps whatever the OS left (maybe off the work area, or oversized).
2. remember_frame stores a minimised/parked frame (Windows parks at -32000,-32000 with a tiny size): the position is stored unclamped by frame_moved and the size is clamped up, so hide/minimise + show could change the size.
3. reset_panel_geometry leaves the drag flags (dragging, drag_started, dock_preview) of a gesture in flight: a stale flag can dock or resize on the next event after Reset.

## Done / next / blocked
Fixed 1-3 (panel.rs placement fallback, frame::is_parked guard, reset clears drag flags); tests in controller/tests/predictable.rs, menus_frame.rs, panel_window.rs. Next: item F (reviewer findings). Windows-only and unverified: the real OS resize loop vs PlaceWindow, hidden-window position after a monitor is unplugged, place-before-show flicker.
