# Panel dock and drag: diagnosis (2026-11-12)

Input for the docking redesign. Read-only diagnosis by the bug sweep, plus the sweep's unfinished, unreviewed patch (appendix) for reference only.

## Owner, verbatim

- "I keep trying to drag it around the middle of the screen but it keeps docking to an edge and when it does that it goes full height for a sec but then resets ot the default height and then also it reset my width I had it at as well"
- "its kinda hard to tell where to click and hold to drag the small panel around"
- "I cant drag the small panel across the edge to the corner and it starts spasaming left to right when it reaches here"
- "docking is still horrendous, It is hard to drag the small panel around, it always seems to want to be docked so I cant have it floating where I want it it doesnt show when I want it or hover my mouse near it, i hate how the window sizing and docking works it it sooooo unusable. I want it liek the toolbar with microsoft pc manager. maybe fable can have a look at it"

## Root causes found

- **symptom:** Dragging the floating Panel around the middle of the screen docks it to an edge. When it docks it is full height for a moment, then drops to the default height.
- **cause:** dock_preview tests each side of the frame against its matching edge on its own, using abs(gap). A Panel whose height is within 2*DOCK_SNAP of the work area height has both its top gap and its bottom gap inside the snap wherever x is, so every drop previews Top (Bottom if the taskbar is on top) and docks there. dock_to(Top) then runs dock_size_for -> full_height_cut, which cuts a height >= work_area.h-2 to the pre_dock height, or to DEFAULT_HEIGHT 480 (capped at work_area.h/2) when pre_dock is full height too. That is the 'default height'. The next header drag undocks through remember_frame, which restores pre_dock, the full height again ('full height for a sec'). That frame spans the axis again, so the drop re-docks to Top and cuts the height to the default. This repeats on every drag.
- **evidence:** crates/snipdeck-app/src/frame.rs:543-575 dock_preview: four independent gaps, `.map(|(gap, edge)| (gap.abs(), edge)).filter(|(gap, edge)| *gap <= snap ...)`, so a frame with h ~= work_area.h always passes for Top/Bottom. frame.rs:404-420 full_height_cut: `let full = |h| h.saturating_add(2) >= work_area.h; ... .unwrap_or_else(|| scaled(default_height, scale).min(work_area.h / 2))`. dock_ctl.rs:21 DEFAULT_HEIGHT = 480. dock_ctl.rs:292-294: pre_dock = earlier.unwrap_or((base.w, base.h)), where base is the full-height floating frame. panel.rs:755-766: an undocking drag restores pre_dock (w, h) and returns PlaceWindow at that size. frame.rs:186-199: clamp caps h at area.h, so a Panel resized as tall as it goes is exactly 'full'. The existing test frame.rs:1400 covers only 320x480 frames and has no case for a frame that spans the axis.
- **fix:** In frame::dock_preview, drop an axis on which the frame spans the work area: if both the top and bottom gaps are within the snap, neither Top nor Bottom is a candidate (same for Left/Right). A tall Panel can then still dock left or right but not top or bottom. Better long-term: decide by the cursor position (host.cursor() within the snap of an edge), as Windows does. Tests to write first: (1) frame.rs: `a_frame_as_tall_as_the_work_area_previews_no_top_or_bottom_dock`, where dock_preview(at(800, 0, 320, area.h)) == None and dock_preview(at(5, 0, 320, area.h)) == Some(Left). (2) controller/tests/dock.rs: floating world, frame_changed with a full-height frame (h 1040 on display()), drag(), frame_moved at x 800, then frame_settled returns Vec::new() and dock_edge() == None.
- **confidence:** High that the mechanism exists in the code. Medium that it is the owner's case: it needs his Panel to be nearly as tall as the work area (check the panel_geometry h in his settings.json against the work area height).

- **symptom:** Docking or undocking resets the width the owner had set.
- **cause:** pre_dock (w, h) is captured once, when dock_to runs, and nothing updates it while the Panel stays docked. A resize of a docked Panel's inner edge goes through remember_frame, which keeps the old pre_dock, so leaving the edge (header drag, kebab or Settings undock) puts back the width from before docking. Docking never forces a width (docked_rect clamps w to 260..480 just like clamp does), so restoring pre_dock.w can only undo a width the user chose. This combines with cause 1: each loop through 'undock to pre_dock, re-dock' re-applies the stored size. A second possible width change: if Windows Snap resizes the window during the OS move loop, the max-width 480 constraint plus clamp would store w=480 (see unknowns).
- **evidence:** panel.rs:752-766: `if dragged && edge.is_some_and(...) { edge = None; if let Some((w, h)) = pre_dock.take() { clamped = frame::clamp(Rect { w, h, ..clamped }, monitor); ...` takes the width from pre_dock. panel.rs:767-773: pre_dock is cleared only when the edge is gone and is otherwise carried unchanged. dock_ctl.rs:296-298: dock_to(None) does `base = Rect { w, h, ..base }` from earlier = pre_dock. frame.rs:280-282 docked_rect: `let w = size.0.clamp(min_w, max_w).min(work_area.w)`, the same width rule as clamp at frame.rs:188-191.
- **fix:** On undock, keep the current width and take only the height from pre_dock. In panel.rs:758 use `Rect { w: clamped.w, h, ..clamped }`. In dock_ctl.rs:297 use `Rect { h, ..base }`, ignoring the stored w. The existing test dragging_a_docked_panel_off_its_edge_restores_the_size_it_had_before_docking (controller/tests/dock.rs:108) still passes, because the docked w equals FLOATING.w. Tests first, in controller/tests/dock.rs: `undocking_keeps_a_width_set_while_docked`: docked(Click, Left), frame_changed(open_rect(Left) with w 400), drag(), frame_moved x 500, then placement().w == 400 and h == FLOATING.h. Write the same test for dock_to(None).
- **confidence:** High (provable from code).

- **symptom:** The Panel stays or ends up docked after a drag that ends in the middle of the screen. On the next show or reveal it jumps back to an edge.
- **cause:** The drop is inferred from a 250 ms gap between WM_MOVE events, not from the mouse button coming up. If the user pauses near an edge while still holding the button, frame_settled docks mid-gesture: dock_to places the window while Windows' modal move loop is still running. Because the Panel was floating when the drag started, drag_started is false, so no later move of the same drag undocks it. It is dragged on and dropped in the middle while settings still record edge = Some(..). restore_frame / placement then put it flush with that edge. A related stale-flag bug: drag() sets dragging = true on every header press, and only frame_settled clears it, which needs at least one Moved event. A header click with no movement leaves dragging = true, so the next non-drag move near an edge (a restore or anchor placement) previews and docks on settle.
- **evidence:** binding.rs:95 DRAG_SETTLE = 250 ms. binding.rs:1155-1169: the settle timer restarts on every frame_moved and then calls ctl.frame_settled; there is no button-state check. dock_ctl.rs:476-489 frame_settled: `if !std::mem::take(&mut self.dragging) ...; if preview.is_some() && self.dock_edge().is_none() && preview == frame::dock_preview(..) { self.dock_to(preview) }`. controller.rs:943-947 drag(): `self.drag_started = self.dock_edge().is_some(); self.dragging = true;`. panel.rs:755: `let dragged = std::mem::take(&mut self.drag_started);` is false for a drag that began floating, so later moves keep the new edge. panel.rs:808-829 placement() places a stored edge flush with docked_rect.
- **fix:** Dock only on a real release. Add a host query for whether the primary button is still down (GetAsyncKeyState(VK_LBUTTON) on Windows, false elsewhere). In the binding's settle closure, re-arm the timer while the button is held. Give frame_settled a `released: bool` argument: when it is false it returns nothing and keeps `dragging` and the preview. Also clear `dragging` and `drag_started` in restore_frame and restore_anchor, as reset_panel_geometry already does at dock_ctl.rs:441-443, so a click without movement cannot arm a later dock. Tests first, in controller/tests/dock.rs: (1) `a_pause_with_the_button_held_does_not_dock`: floating, drag(), frame_moved(near_right), frame_settled(.., released=false) returns empty and dock_edge is None; then frame_settled(.., true) docks Right. (2) `a_header_click_without_a_move_does_not_dock_a_later_placement`: drag(), restore_frame(display()), frame_moved(near_right), frame_settled returns empty.
- **confidence:** High that the code allows it. Whether the owner paused mid-drag is unverified (Windows).

- **symptom:** It is hard to tell where to click and hold to drag the small Panel.
- **cause:** The window drag starts from a full-header TouchArea that sits behind every control. It shows MouseCursor.default and draws nothing. The only empty part of the header is a spacer `Rectangle { horizontal-stretch: root.lists-in-menu ? 0 : 1; }`. With the Menu switcher (lists-in-menu) the board switcher takes the stretch, and at a narrow width the free header shrinks to padding and Space.s0-5 gaps of a few pixels. The grip pill, the only visible handle, is built only while docked (`if root.dock-slide && !root.dock-collapsed`), and it slides; it does not move the window.
- **evidence:** crates/snipdeck-ui/ui/panel.slint:1499-1507: `TouchArea { mouse-cursor: MouseCursor.default; pointer-event(event) => { ... root.drag-requested(); } }`. panel.slint:1490 switcher `horizontal-stretch: root.lists-in-menu ? 1 : 0`. The spacer is at panel.slint about line 1529 (`Rectangle { horizontal-stretch: root.lists-in-menu ? 0 : 1; }`). panel.slint:1641: `if root.dock-slide && !root.dock-collapsed: grip-pill := GripPill`. dock_ctl.rs:361-372: dock_slidable / dock_slide_offered are false while floating.
- **fix:** Give the header drag area MouseCursor.move. Add a visible, always-present drag grip (a dotted handle at least Space.hit wide) at the start of the header that calls drag-requested. A floating Panel would then always have a target, and the docked grip pill keeps its slide role. Test first: a snipdeck-ui panel_logic test (crates/snipdeck-ui/tests/panel_logic.rs) at width 260px with lists-in-menu true. It asserts that the drag grip exists and that its width is >= Space.hit. Then a ux-auditor measurement on the running app; the cursor and the hit box are unverified until measured.
- **confidence:** High (provable from the .slint).

- **symptom:** The small Panel cannot be dragged across the edge to the corner (slide oscillation).
- **cause:** The grip-pill slide is fixed by 9d1f42d, and I checked that fix by reading the code; it has not been run on Windows. SlideDrag::total measures the screen cursor from the press, so the pill moving with the window no longer feeds back into the distance. dock_slide places at origin+total. docked_rect pulls the result inside the work area, so it reaches the corner exactly and stops at it, by design. place() only sets the position or size that differs and then repaints the whole window. One gap remains for a header drag of a docked Panel: drag_started is consumed by the first Moved event even when that move is still flush with the edge. Dragging along the edge to the corner and then off it (onto the next edge) never undocks or re-docks: the Panel stays recorded on the first edge while it floats.
- **evidence:** dock.rs:194-206 SlideDrag::total: `(Some(from), Some(to)) => along(to).saturating_sub(along(from))`; tests at dock.rs:649-667. binding.rs:1000-1037 on_along_begin / on_along_drag. dock_ctl.rs:391-432 dock_slide: `docked_rect(.., Some(origin.saturating_add(total)), false)`. frame.rs:296 / 313 `inside(wanted, ...)`. binding.rs:99-110 place() compares before set_position / set_size, then repaint_whole. Remaining bug: panel.rs:755 `let dragged = std::mem::take(&mut self.drag_started); if dragged && edge.is_some_and(|e| !frame::is_flush(..))`. A flush first move clears the flag for the rest of the drag. The existing test a_drag_that_stays_on_the_edge_keeps_the_dock (controller/tests/dock.rs:392) checks only one move.
- **fix:** Keep 9d1f42d as it is. In remember_frame, read drag_started without taking it, and clear it only when the undock happens or when the drag settles (frame_settled). Test first, in controller/tests/dock.rs: `a_drag_along_the_edge_then_away_still_undocks`: docked(Click, Left), drag(), frame_moved(open_rect(Left) with y 80), which is still docked, then frame_moved(x 500), after which dock_edge() == None. After release, a drop near the Bottom edge should then dock Bottom through frame_settled.
- **confidence:** High for the remaining header-drag bug (from code). The slide fix is verified by reading only; the owner has not yet tested it on Windows.

## unknowns

Unverified, because it is Windows-only behaviour and I am on Linux: (a) Windows Snap. Nothing in the crates opts the Panel out of Snap: a grep for maximize, snap or THICKFRAME finds nothing for the Panel, and Effect::DragWindow runs winit drag_window, which is the system move loop (main.rs:633-639). If Windows 11 snaps or maximizes the no-frame, resizable Panel when the cursor touches a screen edge, that alone gives 'full height for a sec' and a width of 480 (the max-width constraint), which frame_changed then stores. To check, the owner runs with SNIPDECK_DIAG=render and SNIPDECK_LOG=1, drags, and sends the log. A 'diag window panel ... resized' line during the drag, without an 'effect PlaceWindow' line just before it, means the size came from Windows Snap, not from our code. (b) Whether the owner's Panel is nearly as tall as the work area (cause 1). That needs his settings.json panel_geometry entry (h, edge, pre_dock, along) and the work area height. (c) Whether he paused while holding the button (cause 3). The same log shows a frame_settled-driven 'effect PlaceWindow' with no button release. (d) The order of winit Moved and Resized events after place() on Windows: are they delivered synchronously, which the binding skips through try_borrow_mut, or deferred, and read with final values? This does not change the causes above, but the log would confirm it. (e) 'Middle of the screen' may be the seam between two monitors: there dock_preview's abs(gap) on the first monitor's right work-area edge would also dock. It is worth asking the owner whether he has more than one monitor. No cargo was run, and no file was edited.

## Appendix: unfinished sweep patch (not applied)

```diff
diff --git a/crates/snipdeck-app/src/binding.rs b/crates/snipdeck-app/src/binding.rs
index a9551e0..d5e0c18 100644
--- a/crates/snipdeck-app/src/binding.rs
+++ b/crates/snipdeck-app/src/binding.rs
@@ -1163,7 +1163,7 @@ where
                     let (core, monitor) = (Rc::clone(&core), monitor.clone());
                     settle.start(slint::TimerMode::SingleShot, DRAG_SETTLE, move || {
                         if core.controller.try_borrow_mut().is_ok() {
-                            core.act_effects(|ctl| ctl.frame_settled(&monitor, frame));
+                            core.act_effects(|ctl| ctl.frame_settled(&monitor, frame, true));
                         }
                     });
                 })
diff --git a/crates/snipdeck-app/src/controller/dock_ctl.rs b/crates/snipdeck-app/src/controller/dock_ctl.rs
index c22bbbd..d229e49 100644
--- a/crates/snipdeck-app/src/controller/dock_ctl.rs
+++ b/crates/snipdeck-app/src/controller/dock_ctl.rs
@@ -473,7 +473,7 @@ where
     /// The Panel stopped moving after a drag (`frame_moved` went quiet): a drag released near
     /// an allowed edge ([`frame::dock_preview`]) docks there. Nothing happens after a move that
     /// was not a drag (a restore, a placement), so a Panel kept near an edge stays floating.
-    pub fn frame_settled(&mut self, monitor: &Monitor, frame: Rect) -> Vec<Effect> {
+    pub fn frame_settled(&mut self, monitor: &Monitor, frame: Rect, _released: bool) -> Vec<Effect> {
         if !std::mem::take(&mut self.dragging) {
             return Vec::new();
         }
diff --git a/crates/snipdeck-app/src/controller/tests/dock.rs b/crates/snipdeck-app/src/controller/tests/dock.rs
index 30cce1b..341bceb 100644
--- a/crates/snipdeck-app/src/controller/tests/dock.rs
+++ b/crates/snipdeck-app/src/controller/tests/dock.rs
@@ -602,7 +602,7 @@ fn a_drag_released_near_an_allowed_edge_docks_there_and_near_the_taskbar_does_no
     };
     w.ctl.frame_moved(&m, near_right);
     assert_eq!(w.ctl.view().dock_preview, None);
-    assert_eq!(w.ctl.frame_settled(&m, near_right), Vec::new());
+    assert_eq!(w.ctl.frame_settled(&m, near_right, true), Vec::new());
     assert_eq!(w.ctl.dock_edge(), None);
 
     // A drag: the preview follows the frame, and the drop docks to the previewed edge.
@@ -612,7 +612,7 @@ fn a_drag_released_near_an_allowed_edge_docks_there_and_near_the_taskbar_does_no
     w.ctl.frame_moved(&m, near_right);
     assert_eq!(w.ctl.view().dock_preview, Some(DockEdge::Right));
     assert_eq!(
-        w.ctl.frame_settled(&m, near_right),
+        w.ctl.frame_settled(&m, near_right, true),
         place(open_rect(DockEdge::Right))
     );
     assert_eq!(w.ctl.dock_edge(), Some(DockEdge::Right));
@@ -628,10 +628,131 @@ fn a_drag_released_near_an_allowed_edge_docks_there_and_near_the_taskbar_does_no
     w.ctl.frame_moved(&m, near_bottom);
     assert_eq!(w.ctl.dock_edge(), None, "the first move of a drag undocks");
     assert_eq!(w.ctl.view().dock_preview, None);
-    assert_eq!(w.ctl.frame_settled(&m, near_bottom), Vec::new());
+    assert_eq!(w.ctl.frame_settled(&m, near_bottom, true), Vec::new());
     assert_eq!(w.ctl.dock_edge(), None);
 }
 
+#[test]
+fn dropping_a_full_height_panel_in_the_middle_does_not_dock_it() {
+    let mut w = floating(DockTrigger::Both);
+    let m = display();
+    let tall = Rect {
+        y: 0,
+        h: m.work_area.h,
+        ..FLOATING
+    };
+    assert_eq!(w.ctl.frame_changed(&m, tall), Vec::new());
+    assert_eq!(w.ctl.drag(), [Effect::DragWindow]);
+    let middle = Rect { x: 800, ..tall };
+    w.ctl.frame_moved(&m, middle);
+    assert_eq!(w.ctl.view().dock_preview, None);
+    assert_eq!(w.ctl.frame_settled(&m, middle, true), Vec::new());
+    assert_eq!(w.ctl.dock_edge(), None);
+    assert_eq!(
+        w.ctl.placement(&m),
+        Some(middle),
+        "the size it had stays, it is not cut to the default height"
+    );
+}
+
+#[test]
+fn undocking_by_a_drag_keeps_a_width_set_while_docked() {
+    let mut w = docked(DockTrigger::Click, DockEdge::Left);
+    let wider = Rect {
+        w: 400,
+        ..open_rect(DockEdge::Left)
+    };
+    assert_eq!(w.ctl.frame_changed(&display(), wider), Vec::new());
+    assert_eq!(w.ctl.dock_edge(), Some(DockEdge::Left));
+    assert_eq!(w.ctl.drag(), [Effect::DragWindow]);
+    let away = Rect { x: 500, ..wider };
+    let back = Rect {
+        h: FLOATING.h,
+        ..away
+    };
+    assert_eq!(w.ctl.frame_moved(&display(), away), place(back));
+    assert_eq!(w.ctl.dock_edge(), None);
+    assert_eq!(w.ctl.placement(&display()), Some(back));
+}
+
+#[test]
+fn floating_again_keeps_a_width_set_while_docked() {
+    let mut w = docked(DockTrigger::Click, DockEdge::Left);
+    let wider = Rect {
+        w: 400,
+        ..open_rect(DockEdge::Left)
+    };
+    w.ctl.frame_changed(&display(), wider);
+    w.ctl.dock_to(None);
+    assert_eq!(
+        w.ctl.take_pending(),
+        place(Rect {
+            w: 400,
+            ..restored(DockEdge::Left)
+        })
+    );
+}
+
+#[test]
+fn a_pause_with_the_button_held_does_not_dock() {
+    let mut w = floating(DockTrigger::Both);
+    let m = display();
+    let near_right = Rect {
+        x: 1590,
+        ..FLOATING
+    };
+    w.ctl.drag();
+    w.ctl.frame_moved(&m, near_right);
+    assert_eq!(w.ctl.frame_settled(&m, near_right, false), Vec::new());
+    assert_eq!(w.ctl.dock_edge(), None, "still being dragged");
+    assert_eq!(w.ctl.view().dock_preview, Some(DockEdge::Right));
+    // The button comes up: the drop docks.
+    assert_eq!(
+        w.ctl.frame_settled(&m, near_right, true),
+        place(open_rect(DockEdge::Right))
+    );
+    assert_eq!(w.ctl.dock_edge(), Some(DockEdge::Right));
+}
+
+#[test]
+fn a_header_click_without_a_move_does_not_dock_a_later_placement() {
+    let mut w = floating(DockTrigger::Both);
+    let m = display();
+    w.ctl.drag();
+    w.ctl.restore_frame(&m);
+    let near_right = Rect {
+        x: 1590,
+        ..FLOATING
+    };
+    w.ctl.frame_moved(&m, near_right);
+    assert_eq!(w.ctl.frame_settled(&m, near_right, true), Vec::new());
+    assert_eq!(w.ctl.dock_edge(), None);
+}
+
+#[test]
+fn a_drag_along_the_edge_and_then_away_still_undocks() {
+    let mut w = docked(DockTrigger::Click, DockEdge::Left);
+    let m = display();
+    w.ctl.drag();
+    let slid = Rect {
+        y: 80,
+        ..open_rect(DockEdge::Left)
+    };
+    w.ctl.frame_moved(&m, slid);
+    assert_eq!(w.ctl.dock_edge(), Some(DockEdge::Left), "still flush");
+    w.ctl.frame_moved(&m, Rect { x: 500, ..slid });
+    assert_eq!(w.ctl.dock_edge(), None, "off the edge at last");
+    // Dropped at the bottom edge: it docks there.
+    let near_bottom = Rect {
+        x: 800,
+        y: 535,
+        ..FLOATING
+    };
+    w.ctl.frame_moved(&m, near_bottom);
+    w.ctl.frame_settled(&m, near_bottom, true);
+    assert_eq!(w.ctl.dock_edge(), Some(DockEdge::Bottom));
+}
+
 #[test]
 fn reset_size_and_position_floats_the_panel_clear_of_every_edge() {
     let mut w = docked(DockTrigger::Both, DockEdge::Left);
diff --git a/crates/snipdeck-app/src/controller/tests/predictable.rs b/crates/snipdeck-app/src/controller/tests/predictable.rs
index f0d8f72..c10d7cf 100644
--- a/crates/snipdeck-app/src/controller/tests/predictable.rs
+++ b/crates/snipdeck-app/src/controller/tests/predictable.rs
@@ -202,6 +202,6 @@ fn reset_ends_a_drag_in_flight_so_the_next_move_cannot_dock_or_resize_it() {
     };
     w.ctl.frame_moved(&m, near_edge);
     assert_eq!(w.ctl.view().dock_preview, None);
-    assert_eq!(w.ctl.frame_settled(&m, near_edge), Vec::new());
+    assert_eq!(w.ctl.frame_settled(&m, near_edge, true), Vec::new());
     assert_eq!(w.ctl.dock_edge(), None);
 }
diff --git a/crates/snipdeck-app/src/frame.rs b/crates/snipdeck-app/src/frame.rs
index 98f3a6f..7555201 100644
--- a/crates/snipdeck-app/src/frame.rs
+++ b/crates/snipdeck-app/src/frame.rs
@@ -1441,6 +1441,38 @@ mod tests {
         );
     }
 
+    #[test]
+    fn a_frame_as_tall_as_the_work_area_previews_no_top_or_bottom_dock() {
+        let work = area();
+        // Both its ends are within the snap of the top and the bottom wherever it is across.
+        for x in [5, 800, 1590] {
+            let preview = dock_preview(at(x, 0, 320, work.h), work, None, 1.0);
+            assert!(
+                !matches!(preview, Some(DockEdge::Top | DockEdge::Bottom)),
+                "x {x}: {preview:?}"
+            );
+        }
+        assert_eq!(dock_preview(at(800, 0, 320, work.h), work, None, 1.0), None);
+        assert_eq!(
+            dock_preview(at(5, 0, 320, work.h), work, None, 1.0),
+            Some(DockEdge::Left)
+        );
+        // Nearly as tall (within the snap on both ends) is the same; one end free is not.
+        assert_eq!(
+            dock_preview(at(800, 6, 320, work.h - 20), work, None, 1.0),
+            None
+        );
+        assert_eq!(
+            dock_preview(at(800, 6, 320, work.h - 100), work, None, 1.0),
+            Some(DockEdge::Top)
+        );
+        // The same across the width.
+        assert_eq!(
+            dock_preview(at(0, 300, work.w, 480), work, None, 1.0),
+            None
+        );
+    }
+
     #[test]
     fn a_window_taller_than_the_work_area_is_cut_and_moved_onto_it() {
         let area = at(0, 0, 1920, 1040);

```
