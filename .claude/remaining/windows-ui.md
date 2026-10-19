# windows-ui (branch windows-ui)

## Done (all code written; NOTHING COMPILED OR TESTED locally: shared target lock + 55 min snipdeck-ui builds)
- Menu sizes to content (menu.slint: width from hidden measuring layouts, clamp 180..320; removed fixed widths in panel.slint/main_window.slint; x/y clamped with self.width/height)
- Focus-ring insets: Main list + tiles, Panel list (ring 4px; last row gets 2*ring bottom). Sidebar has >= 4px padding already; Picker rows show no ring (not changed)
- New list/board sample names "New list N"/"New board N" (starter::numbered_name, English literal like toasts) + editors.slint focus-name select-all when is-new. No new-tag name field exists, so nothing for tags
- Show Panel: toolbar IconButton + Help menu entry (index 3), Controller::main_show_panel, MainScreen.panel_hotkey
- Panel topmost: PanelView.topmost = on_top && !own_window_active; panel.slint `topmost` drives always-on-top; native_window::track_focus/note_window_event (60ms settle) via main_binding::create_window; hook set in main.rs
- frame::place_near (tested) + native_window::place_over_owner called before show in editors, tag manager, Settings, Help, Onboarding, item editor; owner = last-acted surface (note_opener in wire! macros), else Main, else Panel, else pointer monitor

## Unverified / likely first CI failures
- Slint compile of menu.slint (already fixed one error: width+min-width), main_window.slint, panel.slint, editors.slint
- New tests: main_window_logic (context menu width, show panel), main_controller (names, show panel), controller.rs topmost, frame.rs place_near, native_window ActiveWindows, starter numbered_name
- Existing tests that may expect an empty new-list/board name, or old widths/positions; goldens not regenerated (menu/list/panel visuals may change)
- clippy 1.99 pedantic on new code (native_window.rs casts, long fns)
- Windows-only: set_position before show, focus tracking via winit Focused, topmost toggling, tooltip hotkey text
