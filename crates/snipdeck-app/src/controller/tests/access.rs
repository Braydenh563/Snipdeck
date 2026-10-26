//! Opening the Panel (panel-v2 slice 8): what the tray click opens and where a floating Panel
//! appears when something shows it. All against the fake platform.

use super::*;
use crate::{
    placement::Anchor,
    settings::{DockEdge, PanelOpenAt, TrayClick},
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
    // 4 px below the pointer (the gap), at its x: the 320x500 stored size fits there.
    let expected = Rect {
        x: 900,
        y: 304,
        w: 320,
        h: 500,
    };
    assert_eq!(
        w.ctl.restore_frame(&display()),
        [Effect::PlaceWindow(expected)]
    );
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
    assert_eq!(
        w.ctl.restore_frame(&display()),
        [Effect::PlaceWindow(Rect {
            x: 400,
            y: 336, // the caret's bottom (318) and one line (18) more
            w: 320,
            h: 500
        })]
    );

    w.ctl.hide();
    w.targets.set_caret(None);
    w.ctl.show();
    assert_eq!(
        w.ctl.restore_frame(&display()),
        [Effect::PlaceWindow(Rect {
            x: 900,
            y: 196, // no room below (704 + 500 > 1040): above, 4 px clear
            w: 320,
            h: 500
        })],
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

fn monitor(
    name: &str,
    work_area: Rect,
    scale: f32,
    taskbar: Option<DockEdge>,
) -> crate::frame::Monitor {
    crate::frame::Monitor {
        id: crate::frame::Monitor::id_of(name, work_area.w, work_area.h),
        work_area,
        taskbar,
        scale,
    }
}

#[test]
fn a_pointer_on_another_monitor_places_the_panel_there_at_that_monitors_scale() {
    let mut w = opening_at(PanelOpenAt::Mouse);
    let second = monitor(
        "DISPLAY2",
        Rect {
            x: 1920,
            y: 0,
            w: 1280,
            h: 1400,
        },
        2.0,
        None,
    );
    w.targets.add_monitor(second);
    w.targets.set_cursor(Some((2500, 300)));
    w.ctl.show();
    // 320x500 at scale 1.0 is 640x1000 at 2.0; the gap is 8 px there.
    assert_eq!(
        w.ctl.restore_frame(&display()),
        [Effect::PlaceWindow(Rect {
            x: 2500,
            y: 308,
            w: 640,
            h: 1000
        })]
    );
}

#[test]
fn a_monitor_left_of_the_primary_has_negative_coordinates_and_holds_the_panel() {
    let mut w = opening_at(PanelOpenAt::Mouse);
    w.targets.add_monitor(monitor(
        "DISPLAY2",
        Rect {
            x: -1280,
            y: 0,
            w: 1280,
            h: 984,
        },
        1.0,
        None,
    ));
    w.targets.set_cursor(Some((-300, 200)));
    w.ctl.show();
    assert_eq!(
        w.ctl.restore_frame(&display()),
        [Effect::PlaceWindow(Rect {
            x: -320, // pulled in from -300: the right edge stops at the monitor's (0)
            y: 204,
            w: 320,
            h: 500
        })]
    );
}

#[test]
fn a_dock_edge_set_after_an_anchored_open_is_kept_for_that_monitor() {
    let mut w = opening_at(PanelOpenAt::Mouse);
    let second = monitor(
        "DISPLAY2",
        Rect {
            x: 1920,
            y: 0,
            w: 1280,
            h: 984,
        },
        1.0,
        Some(DockEdge::Bottom),
    );
    w.targets.add_monitor(second.clone());
    w.targets.set_cursor(Some((2500, 300)));
    w.ctl.show();
    w.ctl.restore_frame(&display());
    // The Panel is on the second monitor now, with its taskbar known: it never docks there.
    w.ctl.dock_to(Some(DockEdge::Bottom));
    assert_eq!(w.ctl.take_pending(), Vec::new(), "not onto the taskbar");
    w.ctl.dock_to(Some(DockEdge::Left));
    let docked = w.ctl.take_pending();
    assert!(
        matches!(docked.as_slice(), [Effect::PlaceWindow(_)]),
        "{docked:?}"
    );

    w.ctl.hide();
    w.ctl.show();
    assert_eq!(
        w.ctl.restore_frame(&second),
        docked,
        "the second monitor opens docked where it was"
    );
}

#[test]
fn a_window_that_cannot_name_its_monitor_still_gets_the_anchor_once() {
    let mut w = opening_at(PanelOpenAt::Mouse);
    w.targets.set_cursor(Some((900, 300)));
    w.ctl.show();
    let at = Rect {
        x: 900,
        y: 304,
        w: 320,
        h: 500,
    };
    assert_eq!(w.ctl.restore_anchor(), [Effect::PlaceWindow(at)]);
    assert_eq!(w.ctl.restore_anchor(), Vec::new(), "used once");
}
