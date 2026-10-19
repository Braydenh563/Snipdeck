# windows-ui (branch windows-ui)

## Done (uncompiled until the shared target lock frees; see commits)
- Menu sizes to content (180..320px), clamps inside window (menu.slint, main_window.slint, panel.slint)
- Focus ring insets for Main list/tiles and Panel list (sidebar already has >= 4px padding; Picker rows show no ring)
- New list/board sample names ("New list N"/"New board N") + select-all in the editor
- Show Panel button (toolbar + Help menu) via Controller::main_show_panel
- Panel topmost only while no regular Snipdeck window is focused (set_own_window_active, native_window::track_focus)
- frame::place_near + native_window::place_over_owner before show

## Next
- Compile + run targeted tests: snipdeck-ui main_window_logic, panel_logic, snipdeck-app main_controller, lib
- Goldens for any touched visual tests; clippy 1.99.1 pedantic

## Blocked
- shared cargo target lock held by other runs (waiting)
