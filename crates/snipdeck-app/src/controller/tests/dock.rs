//! The edge dock through the controller: the hover and click reveal, the collapse on leaving,
//! on a paste and on the hotkey, the placements it asks the window for, and the edge each
//! monitor remembers. Time is the controller's injected clock, set by hand.

use snipdeck_core::{UseMode, UseOutcome};

use super::*;
use crate::{
    dock::{DockEvent, TICK},
    frame,
    settings::{DockEdge, DockSettings, DockTrigger},
};

/// The frame the Panel has floating before it is docked.
const FLOATING: Rect = Rect {
    x: 600,
    y: 100,
    w: 320,
    h: 500,
};

/// A world whose dock trigger is `trigger`, with the Panel floating on `display()`.
fn floating(trigger: DockTrigger) -> World {
    let mut settings = Settings::default();
    settings.dock.trigger = trigger;
    let mut w = world_with(Some(outlook()), settings);
    assert_eq!(w.ctl.frame_changed(&display(), FLOATING), Vec::new());
    w
}

/// The same, docked to `edge` (the placement it queued is taken).
fn docked(trigger: DockTrigger, edge: DockEdge) -> World {
    let mut w = floating(trigger);
    w.ctl.dock_to(Some(edge));
    w.ctl.take_pending();
    w
}

fn open_rect(edge: DockEdge) -> Rect {
    frame::docked_rect(edge, display().work_area, (FLOATING.w, FLOATING.h), 1.0)
}

fn handle(edge: DockEdge) -> Rect {
    frame::handle_rect(edge, display().work_area, open_rect(edge).along(edge), 1.0)
}

/// Moves the controller's clock `ms` past its start.
fn advance(w: &World, ms: i64) {
    let start = at("2026-10-17T10:30:00Z").as_millisecond();
    w.clock
        .set(Timestamp::from_millisecond(start + ms).expect("a moment"));
}

fn place(rect: Rect) -> Vec<Effect> {
    vec![Effect::PlaceWindow(rect)]
}

#[test]
fn docking_places_the_panel_flush_with_the_edge_and_remembers_it() {
    let mut w = floating(DockTrigger::Hover);
    assert_eq!(w.ctl.dock_edge(), None);
    w.ctl.dock_to(Some(DockEdge::Right));
    assert_eq!(w.ctl.take_pending(), place(open_rect(DockEdge::Right)));
    assert_eq!(w.ctl.dock_edge(), Some(DockEdge::Right));
    let stored = &w.ctl.settings().panel_geometry;
    assert_eq!(stored.len(), 1, "one entry per monitor");
    assert_eq!(stored[0].edge, Some(DockEdge::Right));
    assert_eq!(Rect::of(&stored[0]), open_rect(DockEdge::Right));
    assert_eq!(
        w.ctl.placement(&display()),
        Some(open_rect(DockEdge::Right)),
        "shown again, it goes back to the edge"
    );
}

#[test]
fn floating_again_clears_the_edge_and_keeps_the_place() {
    let mut w = docked(DockTrigger::Hover, DockEdge::Left);
    w.ctl.dock_to(None);
    assert_eq!(w.ctl.take_pending(), place(open_rect(DockEdge::Left)));
    assert_eq!(w.ctl.dock_edge(), None);
    assert!(!w.ctl.view().docked);
}

#[test]
fn nothing_is_queued_before_the_panels_monitor_is_known() {
    let mut w = world();
    w.ctl.dock_to(Some(DockEdge::Left));
    assert_eq!(w.ctl.take_pending(), Vec::new());
    assert_eq!(w.ctl.settings().panel_geometry, Vec::new());
}

#[test]
fn resting_the_pointer_on_the_handle_opens_the_panel_after_the_delay() {
    let mut w = docked(DockTrigger::Hover, DockEdge::Right);
    // Collapse first (the hotkey), then hover.
    assert_eq!(
        w.ctl.dock_event(DockEvent::Hotkey),
        place(handle(DockEdge::Right))
    );
    assert!(w.ctl.view().dock_collapsed);
    advance(&w, 1000);
    assert_eq!(w.ctl.dock_event(DockEvent::HandleHover(true)), Vec::new());
    assert!(w.ctl.dock_needs_tick(), "the delay is running");
    advance(&w, 1100);
    assert_eq!(w.ctl.dock_event(DockEvent::Tick), Vec::new());
    advance(&w, 1150);
    assert_eq!(
        w.ctl.dock_event(DockEvent::Tick),
        place(open_rect(DockEdge::Right))
    );
    assert!(!w.ctl.view().dock_collapsed);
    assert!(!w.ctl.dock_needs_tick());
}

#[test]
fn leaving_the_handle_early_cancels_and_the_delay_is_the_settings() {
    let mut w = docked(DockTrigger::Hover, DockEdge::Right);
    w.ctl.update_settings(|s| s.dock.hover_delay_ms = 600);
    w.ctl.dock_event(DockEvent::Hotkey);
    w.ctl.dock_event(DockEvent::HandleHover(true));
    advance(&w, 500);
    assert_eq!(
        w.ctl.dock_event(DockEvent::Tick),
        Vec::new(),
        "not yet at 600 ms"
    );
    w.ctl.dock_event(DockEvent::HandleHover(false));
    advance(&w, 900);
    assert_eq!(w.ctl.dock_event(DockEvent::Tick), Vec::new());
    assert!(w.ctl.view().dock_collapsed);
    assert!(!w.ctl.dock_needs_tick());
}

#[test]
fn a_click_on_the_handle_opens_the_panel() {
    for trigger in [DockTrigger::Click, DockTrigger::Both] {
        let mut w = docked(trigger, DockEdge::Top);
        w.ctl.dock_event(DockEvent::Hotkey);
        assert_eq!(
            w.ctl.dock_event(DockEvent::HandleClick),
            place(open_rect(DockEdge::Top)),
            "{trigger:?}"
        );
    }
    let mut hover_only = docked(DockTrigger::Hover, DockEdge::Top);
    hover_only.ctl.dock_event(DockEvent::Hotkey);
    assert_eq!(
        hover_only.ctl.dock_event(DockEvent::HandleClick),
        Vec::new()
    );
}

#[test]
fn leaving_the_open_panel_collapses_it_after_the_leave_delay() {
    let mut w = docked(DockTrigger::Both, DockEdge::Bottom);
    assert_eq!(w.ctl.dock_event(DockEvent::PanelHover(false)), Vec::new());
    assert!(w.ctl.dock_needs_tick());
    advance(&w, 399);
    assert_eq!(w.ctl.dock_event(DockEvent::Tick), Vec::new());
    advance(&w, 400);
    assert_eq!(
        w.ctl.dock_event(DockEvent::Tick),
        place(handle(DockEdge::Bottom))
    );
    assert!(w.ctl.view().dock_collapsed);
    assert!(!w.ctl.view().dock_vertical, "a bottom handle runs across");
}

#[test]
fn coming_back_to_the_panel_cancels_the_collapse() {
    let mut w = docked(DockTrigger::Both, DockEdge::Left);
    w.ctl.dock_event(DockEvent::PanelHover(false));
    advance(&w, 200);
    w.ctl.dock_event(DockEvent::PanelHover(true));
    advance(&w, 5000);
    assert_eq!(w.ctl.dock_event(DockEvent::Tick), Vec::new());
    assert!(!w.ctl.view().dock_collapsed);
}

#[test]
fn the_panel_hotkey_folds_and_unfolds_a_docked_panel_instead_of_hiding_it() {
    let mut w = docked(DockTrigger::Click, DockEdge::Left);
    assert_eq!(
        w.ctl.on_hotkey(HotkeyAction::TogglePanel),
        place(handle(DockEdge::Left))
    );
    assert!(w.ctl.visible(), "still on screen, as a handle");
    assert_eq!(
        w.ctl.on_hotkey(HotkeyAction::TogglePanel),
        place(open_rect(DockEdge::Left))
    );
    assert!(w.ctl.visible());
}

#[test]
fn the_hotkey_hides_the_panel_as_before_when_the_dock_is_off_or_it_floats() {
    let mut off = docked(DockTrigger::Off, DockEdge::Left);
    assert_eq!(
        off.ctl.on_hotkey(HotkeyAction::TogglePanel),
        [Effect::HidePanel]
    );
    let mut loose = floating(DockTrigger::Both);
    assert_eq!(
        loose.ctl.on_hotkey(HotkeyAction::TogglePanel),
        [Effect::HidePanel]
    );
}

#[test]
fn showing_a_hidden_panel_opens_it_even_if_it_was_a_handle() {
    let mut w = docked(DockTrigger::Click, DockEdge::Right);
    w.ctl.dock_event(DockEvent::Hotkey);
    assert!(w.ctl.view().dock_collapsed);
    w.ctl.hide();
    assert_eq!(w.ctl.show(), [Effect::ShowPanel]);
    assert!(!w.ctl.view().dock_collapsed);
}

#[test]
fn a_paste_from_the_panel_collapses_it_and_a_copy_does_not() {
    let mut w = docked(DockTrigger::Hover, DockEdge::Right);
    let copied = w.ctl.activate_row(0, UseMode::Copy);
    assert_eq!(copied, UseResult::Done(UseOutcome::Copied));
    assert_eq!(w.ctl.dock_after_use(&copied), Vec::new());
    assert!(!w.ctl.view().dock_collapsed);

    let pasted = w.ctl.activate_row(0, UseMode::Paste);
    assert_eq!(pasted, UseResult::Done(UseOutcome::Pasted));
    assert_eq!(
        w.ctl.dock_after_use(&pasted),
        place(handle(DockEdge::Right))
    );
    assert!(w.ctl.view().dock_collapsed);

    assert_eq!(w.ctl.dock_after_use(&UseResult::Failed), Vec::new());
    assert_eq!(w.ctl.dock_after_use(&UseResult::NeedsAnswers), Vec::new());
}

#[test]
fn a_floating_panel_never_collapses() {
    let mut w = floating(DockTrigger::Both);
    for event in [
        DockEvent::PanelHover(false),
        DockEvent::Pasted,
        DockEvent::Hotkey,
        DockEvent::Tick,
    ] {
        assert_eq!(w.ctl.dock_event(event), Vec::new(), "{event:?}");
    }
    assert!(!w.ctl.view().dock_collapsed);
    assert!(!w.ctl.dock_active());
}

#[test]
fn collapsing_closes_an_open_menu() {
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    w.ctl.open_app_menu();
    assert!(w.ctl.view().app_menu.is_some());
    w.ctl.dock_event(DockEvent::Hotkey);
    assert_eq!(w.ctl.view().app_menu, None);
}

#[test]
fn the_handle_strip_is_not_remembered_or_clamped_as_the_panel_frame() {
    let mut w = docked(DockTrigger::Click, DockEdge::Right);
    let before = w.ctl.settings().panel_geometry.clone();
    w.ctl.dock_event(DockEvent::Hotkey);
    // The window reports the strip it was just made.
    assert_eq!(
        w.ctl.frame_changed(&display(), handle(DockEdge::Right)),
        Vec::new()
    );
    assert_eq!(w.ctl.settings().panel_geometry, before);
    assert_eq!(
        w.ctl.placement(&display()),
        Some(open_rect(DockEdge::Right))
    );
    // Open again: frames count once more.
    w.ctl.dock_event(DockEvent::Hotkey);
    let narrower = Rect {
        w: 300,
        ..open_rect(DockEdge::Right)
    };
    assert_eq!(w.ctl.frame_changed(&display(), narrower), Vec::new());
    assert_eq!(w.ctl.settings().panel_geometry[0].w, 300);
    assert_eq!(
        w.ctl.settings().panel_geometry[0].edge,
        Some(DockEdge::Right)
    );
}

#[test]
fn a_resize_of_the_docked_panel_keeps_its_edge() {
    let mut w = docked(DockTrigger::Click, DockEdge::Left);
    let wider = Rect {
        w: 400,
        ..open_rect(DockEdge::Left)
    };
    w.ctl.frame_changed(&display(), wider);
    assert_eq!(w.ctl.dock_edge(), Some(DockEdge::Left));
    assert_eq!(w.ctl.placement(&display()), Some(wider));
}

#[test]
fn dragging_a_docked_panel_away_undocks_it() {
    let mut w = docked(DockTrigger::Click, DockEdge::Left);
    assert_eq!(w.ctl.drag(), [Effect::DragWindow]);
    let away = Rect {
        x: 500,
        ..open_rect(DockEdge::Left)
    };
    w.ctl.frame_changed(&display(), away);
    assert_eq!(w.ctl.dock_edge(), None);
    assert!(!w.ctl.view().docked);
    assert_eq!(w.ctl.placement(&display()), Some(away));
}

#[test]
fn a_drag_that_stays_on_the_edge_keeps_the_dock() {
    let mut w = docked(DockTrigger::Click, DockEdge::Left);
    w.ctl.drag();
    let slid = Rect {
        y: 80,
        ..open_rect(DockEdge::Left)
    };
    w.ctl.frame_changed(&display(), slid);
    assert_eq!(w.ctl.dock_edge(), Some(DockEdge::Left));
}

#[test]
fn a_press_on_a_floating_header_changes_nothing_about_docking() {
    let mut w = floating(DockTrigger::Click);
    w.ctl.drag();
    w.ctl.frame_changed(&display(), Rect { x: 10, ..FLOATING });
    assert_eq!(w.ctl.dock_edge(), None);
}

#[test]
fn switching_the_dock_off_opens_a_collapsed_panel() {
    let mut w = docked(DockTrigger::Click, DockEdge::Right);
    w.ctl.dock_event(DockEvent::Hotkey);
    w.ctl.update_settings(|s| s.dock.trigger = DockTrigger::Off);
    assert_eq!(w.ctl.take_pending(), place(open_rect(DockEdge::Right)));
    assert!(!w.ctl.view().dock_collapsed);
    assert_eq!(w.ctl.take_pending(), Vec::new(), "queued once");
}

#[test]
fn undocking_a_collapsed_panel_opens_it_where_it_floats() {
    let mut w = docked(DockTrigger::Click, DockEdge::Right);
    w.ctl.dock_event(DockEvent::Hotkey);
    w.ctl.dock_to(None);
    assert_eq!(w.ctl.take_pending(), place(open_rect(DockEdge::Right)));
    assert!(!w.ctl.view().dock_collapsed);
    assert!(!w.ctl.dock_active());
}

#[test]
fn the_kebab_docks_to_the_nearest_edge_and_then_undocks() {
    let mut w = floating(DockTrigger::Both);
    let near_left = Rect {
        x: 40,
        y: 300,
        ..FLOATING
    };
    w.ctl.frame_changed(&display(), near_left);
    let row = APP_MENU
        .iter()
        .position(|e| *e == AppMenuEntry::Dock)
        .expect("in menu");
    // The Panel sits 40 px from the left edge: that is the nearest.
    w.ctl.open_app_menu();
    assert_eq!(w.ctl.choose_app_menu(row), place(open_rect(DockEdge::Left)));
    assert_eq!(w.ctl.dock_edge(), Some(DockEdge::Left));
    assert!(w.ctl.view().docked);
    assert_eq!(w.ctl.view().app_menu, None);
    assert_eq!(w.ctl.choose_app_menu(row), place(open_rect(DockEdge::Left)));
    assert_eq!(w.ctl.dock_edge(), None);
}

#[test]
fn each_monitor_remembers_its_own_edge() {
    let mut w = docked(DockTrigger::Click, DockEdge::Left);
    let second = frame::Monitor {
        id: frame::Monitor::id_of("DISPLAY2", 1280, 1024),
        work_area: Rect {
            x: 1920,
            y: 0,
            w: 1280,
            h: 984,
        },
        scale: 1.0,
    };
    let there = Rect {
        x: 2200,
        y: 50,
        w: 300,
        h: 400,
    };
    w.ctl.frame_changed(&second, there);
    assert_eq!(w.ctl.dock_edge(), None, "floating on the second monitor");
    w.ctl.dock_to(Some(DockEdge::Right));
    assert_eq!(w.ctl.dock_edge(), Some(DockEdge::Right));
    assert_eq!(
        w.ctl.restore_frame(&display()),
        place(open_rect(DockEdge::Left)),
        "back on the first it is still on the left"
    );
    assert_eq!(w.ctl.dock_edge(), Some(DockEdge::Left));
    let edges: Vec<_> = w
        .ctl
        .settings()
        .panel_geometry
        .iter()
        .map(|g| g.edge)
        .collect();
    assert_eq!(edges, [Some(DockEdge::Left), Some(DockEdge::Right)]);
}

#[test]
fn the_view_says_which_way_the_handle_runs() {
    for (edge, vertical) in [
        (DockEdge::Left, true),
        (DockEdge::Right, true),
        (DockEdge::Top, false),
        (DockEdge::Bottom, false),
    ] {
        let w = docked(DockTrigger::Click, edge);
        let view = w.ctl.view();
        assert_eq!(view.dock_vertical, vertical, "{edge:?}");
        assert!(view.docked && !view.dock_collapsed);
    }
}

#[test]
fn the_tick_interval_is_fifty_milliseconds() {
    assert_eq!(TICK, std::time::Duration::from_millis(50));
}

#[test]
fn dock_settings_changes_are_written_with_the_other_settings() {
    let mut w = floating(DockTrigger::Off);
    w.ctl.update_settings(|s| {
        s.dock = DockSettings {
            trigger: DockTrigger::Both,
            hover_delay_ms: 9999,
            leave_delay_ms: 0,
        };
    });
    assert_eq!(w.ctl.settings().dock.trigger, DockTrigger::Both);
    assert_eq!(w.ctl.settings().dock.hover_delay_ms, 2000, "clamped");
}
