//! Show, hide, move, resize and Reset size of the Panel through the controller: the window
//! comes back where and as big as it was left, a frame from a monitor that is gone is brought
//! into the work area, and a Reset leaves no half-done gesture behind. Frames are fixed
//! values; nothing here reads the clock or a screen.

use super::*;
use crate::{
    frame::Monitor,
    settings::{DockEdge, PanelGeometry},
};

fn second_display() -> Monitor {
    Monitor {
        id: Monitor::id_of("DISPLAY2", 1280, 1024),
        work_area: Rect {
            x: 1920,
            y: 0,
            w: 1280,
            h: 984,
        },
        scale: 1.0,
        taskbar: None,
    }
}

const LEFT: Rect = Rect {
    x: 300,
    y: 120,
    w: 410,
    h: 700,
};

fn inside(frame: Rect, area: Rect) -> bool {
    frame.x >= area.x
        && frame.y >= area.y
        && i64::from(frame.x) + i64::from(frame.w) <= i64::from(area.x) + i64::from(area.w)
        && i64::from(frame.y) + i64::from(frame.h) <= i64::from(area.y) + i64::from(area.h)
}

#[test]
fn hide_and_show_put_the_panel_back_at_the_same_place_and_size() {
    let mut w = world();
    let m = display();
    assert_eq!(w.ctl.frame_changed(&m, LEFT), Vec::new());
    // The window is hidden, then shown on the monitor it is on: the same frame, twice over.
    for _ in 0..2 {
        assert_eq!(
            w.ctl.restore_frame(&m),
            vec![Effect::PlaceWindow(LEFT)],
            "shown again where and as big as it was left"
        );
    }
}

#[test]
fn a_parked_or_minimised_frame_is_not_a_place_the_user_chose() {
    let mut w = world();
    let m = display();
    w.ctl.frame_changed(&m, LEFT);
    // Windows parks a minimised window far off screen with a title-bar sized frame.
    let parked = Rect {
        x: -32000,
        y: -32000,
        w: 160,
        h: 28,
    };
    assert_eq!(w.ctl.frame_moved(&m, parked), Vec::new());
    assert_eq!(w.ctl.frame_changed(&m, parked), Vec::new());
    assert_eq!(w.ctl.placement(&m), Some(LEFT), "size and place unchanged");
}

#[test]
fn a_frame_saved_on_a_monitor_that_is_gone_comes_back_inside_the_one_that_is_left() {
    let mut w = world();
    let (gone, primary) = (second_display(), display());
    let kept = Rect {
        x: 3100,
        y: 600,
        w: 450,
        h: 700,
    };
    // Moved by hand; the monitor is allowed to be crossed, so the place is stored as it is.
    w.ctl.frame_moved(&gone, kept);
    let effects = w.ctl.restore_frame(&primary);
    let [Effect::PlaceWindow(placed)] = effects.as_slice() else {
        panic!("one placement, got {effects:?}");
    };
    assert!(
        inside(*placed, primary.work_area),
        "{placed:?} lies outside {:?}",
        primary.work_area
    );
    assert_eq!((placed.w, placed.h), (kept.w, kept.h), "the size it had");
}

#[test]
fn a_monitor_never_used_for_the_panel_gets_the_size_it_was_left_with_even_after_a_dock() {
    let mut w = world();
    let (gone, primary) = (second_display(), display());
    w.ctl.frame_changed(
        &gone,
        Rect {
            x: 2000,
            y: 40,
            w: 380,
            h: 640,
        },
    );
    w.ctl.dock_to(Some(DockEdge::Left));
    w.ctl.take_pending();
    let placed = w.ctl.restore_frame(&primary);
    let [Effect::PlaceWindow(rect)] = placed.as_slice() else {
        panic!("one placement, got {placed:?}");
    };
    assert!(inside(*rect, primary.work_area));
    assert_eq!(
        (rect.w, rect.h),
        (380, 640),
        "the floating size, not the dock's full height"
    );
}

#[test]
fn nothing_is_placed_while_no_frame_was_ever_stored() {
    let mut w = world();
    assert_eq!(w.ctl.restore_frame(&display()), Vec::new());
}

#[test]
fn a_stored_frame_larger_than_the_work_area_is_shrunk_to_it() {
    let mut w = world();
    let m = display();
    // What an older file, or a smaller work area (a taller taskbar), can leave behind.
    w.ctl.update_settings(|s| {
        s.remember_panel_geometry(PanelGeometry {
            monitor_id: m.id.clone(),
            x: -50,
            y: -50,
            w: 2000,
            h: 3000,
            edge: None,
            pre_dock: None,
        });
    });
    let effects = w.ctl.restore_frame(&m);
    let [Effect::PlaceWindow(placed)] = effects.as_slice() else {
        panic!("one placement, got {effects:?}");
    };
    assert!(inside(*placed, m.work_area), "{placed:?}");
    assert_eq!(placed.w, 480, "the widest the Panel goes");
    assert_eq!(placed.h, m.work_area.h, "as tall as the work area");
}

#[test]
fn after_reset_a_resize_drag_is_placed_stored_and_shown_as_one_frame() {
    let mut w = world();
    let m = display();
    w.ctl.frame_changed(&m, LEFT);
    let effects = w.ctl.reset_panel_geometry();
    let [Effect::PlaceWindow(default)] = effects.as_slice() else {
        panic!("one placement, got {effects:?}");
    };
    let default = *default;
    assert!(inside(default, m.work_area));
    // The window reports the placement back (a move, then a size): nothing more to fix.
    assert_eq!(w.ctl.frame_moved(&m, default), Vec::new());
    assert_eq!(w.ctl.frame_changed(&m, default), Vec::new());
    assert_eq!(w.ctl.placement(&m), Some(default));
    // Then the user drags the bottom right grip: each step is remembered as it is.
    for (dw, dh) in [(20, 40), (60, 120), (100, 200)] {
        let grown = Rect {
            w: default.w + dw,
            h: default.h + dh,
            ..default
        };
        assert_eq!(w.ctl.frame_changed(&m, grown), Vec::new(), "{grown:?}");
        assert_eq!(w.ctl.placement(&m), Some(grown));
        assert_eq!(w.ctl.restore_frame(&m), vec![Effect::PlaceWindow(grown)]);
    }
}

#[test]
fn reset_ends_a_drag_in_flight_so_the_next_move_cannot_dock_or_resize_it() {
    let mut w = world();
    let m = display();
    w.ctl.frame_changed(&m, LEFT);
    w.ctl.dock_to(Some(DockEdge::Left));
    w.ctl.take_pending();
    // The header was pressed on the docked Panel; before it moved, Reset ran.
    w.ctl.drag();
    let effects = w.ctl.reset_panel_geometry();
    let [Effect::PlaceWindow(default)] = effects.as_slice() else {
        panic!("one placement, got {effects:?}");
    };
    // A stray move near the edge, then quiet: no preview, no dock.
    let near_edge = Rect {
        x: 4,
        y: default.y,
        ..*default
    };
    w.ctl.frame_moved(&m, near_edge);
    assert_eq!(w.ctl.view().dock_preview, None);
    assert_eq!(w.ctl.frame_settled(&m, near_edge), Vec::new());
    assert_eq!(w.ctl.dock_edge(), None);
}
