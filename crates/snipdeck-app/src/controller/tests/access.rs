//! Opening the Panel (panel-v2 slice 8): what the tray click opens and where a floating Panel
//! appears when something shows it. All against the fake platform.

use super::*;
use crate::{
    placement::{self, Anchor},
    settings::{DockEdge, PanelOpenAt, PickerSide, TrayClick},
};

const FLOATING: Rect = Rect {
    x: 600,
    y: 100,
    w: 320,
    h: 500,
};

fn opening_at(open_at: PanelOpenAt) -> World {
    let mut w = world_with(
        Some(outlook()),
        Settings {
            panel_pinned: true,
            panel_open_at: open_at,
            ..Settings::new_install()
        },
    );
    assert_eq!(w.ctl.frame_changed(&display(), FLOATING), Vec::new());
    w.ctl.hide();
    w
}

/// The frame a Panel of the stored size gets at `anchor` on `display()`.
fn at_anchor(anchor: Anchor) -> Rect {
    placement::place(
        (FLOATING.w, FLOATING.h),
        anchor,
        display().work_area,
        placement::GAP as u32,
        PickerSide::Below,
    )
}

#[test]
fn the_tray_click_opens_the_main_window_when_asked_and_the_menu_still_has_the_panel() {
    let mut w = world();
    w.ctl.update_settings(|s| s.tray_click = TrayClick::Main);
    assert_eq!(
        w.ctl.on_tray(TrayEvent::TogglePanel),
        [Effect::OpenMainWindow]
    );
    assert!(w.ctl.visible(), "the click no longer toggles the Panel");
    w.ctl.hide();
    assert_eq!(
        w.ctl.on_tray(TrayEvent::TogglePanel),
        [Effect::OpenMainWindow],
        "a hidden Panel stays hidden"
    );
    assert!(!w.ctl.visible());
    assert_eq!(w.ctl.on_tray(TrayEvent::OpenPanel), [Effect::ShowPanel]);
    assert!(w.ctl.visible());
}

#[test]
fn where_you_left_it_restores_the_stored_frame() {
    let mut w = opening_at(PanelOpenAt::Last);
    w.targets.set_cursor(Some((900, 700)));
    w.ctl.show();
    assert_eq!(
        w.ctl.restore_frame(&display()),
        [Effect::PlaceWindow(FLOATING)]
    );
}

#[test]
fn at_the_mouse_places_the_panel_by_the_pointer() {
    let mut w = opening_at(PanelOpenAt::Mouse);
    w.targets.set_cursor(Some((900, 300)));
    w.ctl.show();
    let expected = at_anchor(Anchor::Point(900, 300));
    assert_eq!(
        w.ctl.restore_frame(&display()),
        [Effect::PlaceWindow(expected)]
    );
    assert!(expected.x == 900 && expected.y > 300, "{expected:?}");
}

#[test]
fn the_pointer_place_is_used_once_and_stays_inside_the_screen() {
    let mut w = opening_at(PanelOpenAt::Mouse);
    w.targets.set_cursor(Some((1915, 1035)));
    w.ctl.show();
    let placed = w.ctl.restore_frame(&display());
    let [Effect::PlaceWindow(r)] = placed.as_slice() else {
        panic!("a placement");
    };
    let area = display().work_area;
    assert!(
        r.x >= area.x
            && r.x + i32::try_from(r.w).unwrap() <= area.x + i32::try_from(area.w).unwrap()
    );
    assert!(
        r.y >= area.y
            && r.y + i32::try_from(r.h).unwrap() <= area.y + i32::try_from(area.h).unwrap()
    );
    // Restored again without a new show (the window came back by itself): the stored frame.
    assert_eq!(
        w.ctl.restore_frame(&display()),
        [Effect::PlaceWindow(FLOATING)],
        "only a show moves it to the pointer"
    );
}

#[test]
fn at_the_text_cursor_uses_the_caret_and_falls_back_to_the_mouse() {
    use snipdeck_platform::route::ScreenRect;
    let mut w = opening_at(PanelOpenAt::Caret);
    w.targets.set_cursor(Some((900, 700)));
    let caret = ScreenRect {
        x: 400,
        y: 300,
        w: 2,
        h: 18,
    };
    w.targets.set_caret(Some(caret));
    w.ctl.show();
    let at_caret = Anchor::Caret(Rect {
        x: caret.x,
        y: caret.y,
        w: caret.w,
        h: caret.h,
    });
    assert_eq!(
        w.ctl.restore_frame(&display()),
        [Effect::PlaceWindow(at_anchor(at_caret))]
    );

    w.ctl.hide();
    w.targets.set_caret(None);
    w.ctl.show();
    assert_eq!(
        w.ctl.restore_frame(&display()),
        [Effect::PlaceWindow(at_anchor(Anchor::Point(900, 700)))],
        "no caret: the mouse"
    );
}

#[test]
fn nothing_readable_leaves_the_stored_frame() {
    let mut w = opening_at(PanelOpenAt::Mouse);
    w.targets.set_cursor(None);
    w.ctl.show();
    assert_eq!(
        w.ctl.restore_frame(&display()),
        [Effect::PlaceWindow(FLOATING)]
    );
}

#[test]
fn a_docked_panel_ignores_where_it_opens() {
    let mut w = opening_at(PanelOpenAt::Mouse);
    w.ctl.dock_to(Some(DockEdge::Right));
    w.ctl.take_pending();
    w.ctl.hide();
    w.targets.set_cursor(Some((900, 700)));
    w.ctl.show();
    let docked = crate::frame::docked_rect(
        DockEdge::Right,
        display().work_area,
        (FLOATING.w, FLOATING.h),
        1.0,
    );
    assert_eq!(
        w.ctl.restore_frame(&display()),
        [Effect::PlaceWindow(docked)]
    );
}

#[test]
fn showing_a_panel_that_is_already_up_does_not_move_it() {
    let mut w = opening_at(PanelOpenAt::Mouse);
    w.ctl.show();
    w.ctl.restore_frame(&display());
    w.targets.set_cursor(Some((100, 100)));
    w.ctl.show();
    assert_eq!(
        w.ctl.restore_frame(&display()),
        [Effect::PlaceWindow(FLOATING)],
        "the tray's Open Panel on a visible Panel keeps it where it is"
    );
}
