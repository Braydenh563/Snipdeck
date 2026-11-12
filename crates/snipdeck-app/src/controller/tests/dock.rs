//! The edge dock through the controller: the hover and click reveal, the collapse on leaving,
//! on a paste and on the hotkey, the placements it asks the window for, and the edge each
//! monitor remembers. Time is the controller's injected clock, set by hand.

use snipdeck_core::{UseMode, UseOutcome};

use super::*;
use crate::{
    dock::{DockEvent, TICK},
    frame,
    settings::{DockEdge, DockSettings, DockTrigger},
    toast::ToastMessage,
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
    // The pointer has been on it since: a Panel docked from a menu waits for that.
    w.ctl.dock_hover(DockEvent::PanelHover(true));
    w
}

fn open_rect(edge: DockEdge) -> Rect {
    open_rect_from(edge, FLOATING)
}

/// The open dock for a Panel dropped at `dropped`: its top (or its left) is where the dock starts.
fn open_rect_from(edge: DockEdge, dropped: Rect) -> Rect {
    frame::docked_rect(
        edge,
        display().work_area,
        (dropped.w, dropped.h),
        1.0,
        Some(dropped.along(edge).start),
        false,
    )
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

/// Pointer down on the Panel's header with the mouse at `cursor` and the window at `window`.
fn press(w: &mut World, cursor: (i32, i32), window: Rect) {
    w.targets.set_cursor(Some(cursor));
    w.ctl.drag_begin(cursor, window);
    assert!(w.ctl.dragging());
}

/// The mouse moves to `cursor` with the button held.
fn pull(w: &mut World, cursor: (i32, i32)) -> Vec<Effect> {
    w.targets.set_cursor(Some(cursor));
    w.ctl.drag_move(None)
}

/// A second monitor to the right of `display()`, at 150 %.
fn second_display() -> frame::Monitor {
    frame::Monitor {
        id: frame::Monitor::id_of("DISPLAY2", 2560, 1440),
        work_area: Rect {
            x: 1920,
            y: 0,
            w: 2560,
            h: 1380,
        },
        taskbar: None,
        scale: 1.5,
    }
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
    assert_eq!(Rect::of(&stored[0]), FLOATING, "the floating frame is kept");
    assert_eq!(stored[0].docked, Some((FLOATING.w, FLOATING.h)));
    assert_eq!(
        w.ctl.placement(&display()),
        Some(open_rect(DockEdge::Right)),
        "shown again, it goes back to the edge"
    );
}

#[test]
fn floating_again_clears_the_edge_and_goes_back_to_where_it_floated() {
    let mut w = docked(DockTrigger::Hover, DockEdge::Left);
    w.ctl.dock_to(None);
    assert_eq!(w.ctl.take_pending(), place(FLOATING));
    assert_eq!(w.ctl.dock_edge(), None);
    assert!(w.ctl.view().dock_edge.is_none());
}

#[test]
fn dragging_a_docked_panel_off_its_edge_gives_the_floating_size_back_and_keeps_the_docked_one() {
    let mut w = docked(DockTrigger::Click, DockEdge::Left);
    // Made wider while docked: that is the docked size only.
    let wide = Rect {
        w: 400,
        ..open_rect(DockEdge::Left)
    };
    assert_eq!(w.ctl.frame_changed(&display(), wide), Vec::new());
    press(&mut w, (200, 110), wide);
    // Pulled 30 px off the edge: it floats again, at its floating size, still under the cursor.
    let off = Rect {
        x: 70,
        y: 100,
        w: FLOATING.w,
        h: FLOATING.h,
    };
    assert_eq!(pull(&mut w, (230, 110)), place(off));
    assert!(w.ctl.view().dock_edge.is_none(), "shown floating at once");
    assert_eq!(pull(&mut w, (700, 400)), [Effect::MoveWindow(540, 390)]);
    let dropped = Rect {
        x: 540,
        y: 390,
        ..off
    };
    assert_eq!(w.ctl.drag_end(), place(dropped));
    assert!(!w.ctl.dragging());
    assert_eq!(w.ctl.dock_edge(), None);
    assert_eq!(w.ctl.placement(&display()), Some(dropped));
    // Docked again, it has the width it was given while docked; floating again, its own.
    w.ctl.dock_to(Some(DockEdge::Right));
    let pending = w.ctl.take_pending();
    let [Effect::PlaceWindow(rect)] = pending.as_slice() else {
        panic!("one placement, got {pending:?}");
    };
    assert_eq!((rect.w, rect.h), (400, FLOATING.h));
    w.ctl.dock_to(None);
    assert_eq!(w.ctl.take_pending(), place(dropped));
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
    w.ctl.set_panel_focused(true);
    let refocused = w.targets.refocus_count();
    assert_eq!(
        w.ctl.on_hotkey(HotkeyAction::TogglePanel),
        place(handle(DockEdge::Left))
    );
    assert!(w.ctl.visible(), "still on screen, as a handle");
    assert_eq!(
        w.targets.refocus_count(),
        refocused + 1,
        "the keyboard goes back to the Target"
    );
    // Tucked, the hotkey shows it like any Panel: it comes out at its docked frame and takes
    // the keyboard.
    assert_eq!(
        w.ctl.on_hotkey(HotkeyAction::TogglePanel),
        [
            Effect::PlaceWindow(open_rect(DockEdge::Left)),
            Effect::ShowPanel
        ]
    );
    assert!(w.ctl.visible());
    assert!(!w.ctl.view().dock_collapsed);
    assert_eq!(w.ctl.placement(&display()), Some(open_rect(DockEdge::Left)));
}

#[test]
fn closing_a_docked_panel_tucks_it_and_losing_the_foreground_does_not_hide_it() {
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    with_tray(&mut w);
    w.ctl.set_on_top(false);
    advance(&w, 5_000);
    assert_eq!(w.ctl.on_foreground_changed(), Vec::new());
    assert!(w.ctl.visible(), "its tab is how it comes back");
    assert!(
        w.ctl.view().topmost,
        "a docked Panel stays above other windows"
    );
    assert_eq!(w.ctl.close(), place(handle(DockEdge::Right)));
    assert!(w.ctl.visible());
    assert!(w.ctl.view().dock_collapsed);
    assert_eq!(w.ctl.close(), Vec::new(), "already tucked");
    // Floating, the same Panel hides as before.
    let mut loose = floating(DockTrigger::Both);
    with_tray(&mut loose);
    loose.ctl.set_on_top(false);
    advance(&loose, 5_000);
    assert_eq!(loose.ctl.on_foreground_changed(), [Effect::HidePanel]);
}

#[test]
fn the_pointer_near_the_tab_reveals_the_panel_and_leaving_tucks_it() {
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    assert!(w.ctl.dock_live());
    // Away from the open Panel: the leave delay runs from the first look.
    w.targets.set_cursor(Some((1000, 300)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 399);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 400);
    assert_eq!(w.ctl.dock_tick(), place(handle(DockEdge::Right)));
    // In front of the tab is another window's scrollbar, not the tab: nothing, however long.
    for x in [1894, 1905, 1913] {
        w.targets.set_cursor(Some((x, 300)));
        advance(&w, 5_000);
        assert_eq!(w.ctl.dock_tick(), Vec::new(), "{x}");
        assert!(w.ctl.view().dock_collapsed);
    }
    // On it: the hover delay runs, then the Panel is out.
    w.targets.set_cursor(Some((1914, 300)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 5_149);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 5_150);
    assert_eq!(w.ctl.dock_tick(), place(open_rect(DockEdge::Right)));
    // On the Panel, or a few pixels beside it, it stays out.
    for cursor in [(1700, 300), (1595, 300)] {
        w.targets.set_cursor(Some(cursor));
        advance(&w, 60_000);
        assert_eq!(w.ctl.dock_tick(), Vec::new(), "{cursor:?}");
        assert!(!w.ctl.view().dock_collapsed);
    }
}

#[test]
fn brushing_past_the_tab_reveals_nothing() {
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    w.ctl.dock_event(DockEvent::Hotkey);
    w.targets.set_cursor(Some((1915, 300)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 100);
    w.targets.set_cursor(Some((1000, 300)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 1_000);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    assert!(w.ctl.view().dock_collapsed);
}

#[test]
fn a_panel_in_use_stays_out_whatever_the_pointer_does() {
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    w.targets.set_cursor(Some((500, 500)));
    // It has the keyboard (the search box is being typed in).
    w.ctl.set_panel_focused(true);
    advance(&w, 10_000);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 20_000);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    // A menu is open too.
    w.ctl.open_app_menu();
    advance(&w, 30_000);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 40_000);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    assert!(w.ctl.view().app_menu.is_some());
    w.ctl.close_app_menu();
    w.ctl.set_panel_focused(false);
    // It is being dragged along its edge.
    press(&mut w, (1700, 110), open_rect(DockEdge::Right));
    advance(&w, 50_000);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 60_000);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    w.ctl.drag_end();
    // Let go of, with the pointer away: the leave delay starts only now.
    w.targets.set_cursor(Some((500, 500)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 60_400);
    assert_eq!(w.ctl.dock_tick(), place(handle(DockEdge::Right)));
}

#[test]
fn a_floating_panel_and_a_stay_open_dock_are_never_polled() {
    let mut loose = floating(DockTrigger::Both);
    loose.targets.set_cursor(Some((0, 0)));
    assert!(!loose.ctl.dock_live());
    advance(&loose, 60_000);
    assert_eq!(loose.ctl.dock_tick(), Vec::new());
    let mut stays = docked(DockTrigger::Off, DockEdge::Left);
    stays.targets.set_cursor(Some((1000, 1000)));
    assert!(!stays.ctl.dock_live());
    advance(&stays, 60_000);
    assert_eq!(stays.ctl.dock_tick(), Vec::new());
    assert!(!stays.ctl.view().dock_collapsed);
}

#[test]
fn where_the_cursor_cannot_be_read_the_windows_own_hover_still_works() {
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    w.targets.set_cursor(None);
    assert_eq!(w.ctl.dock_hover(DockEvent::PanelHover(false)), Vec::new());
    advance(&w, 400);
    assert_eq!(w.ctl.dock_tick(), place(handle(DockEdge::Right)));
    // Where it can be read, the window's enter and leave are not listened to.
    let mut polled = docked(DockTrigger::Both, DockEdge::Right);
    polled.targets.set_cursor(Some((1700, 300)));
    assert_eq!(
        polled.ctl.dock_hover(DockEvent::PanelHover(false)),
        Vec::new()
    );
    advance(&polled, 5_000);
    assert_eq!(polled.ctl.dock_tick(), Vec::new());
    assert!(!polled.ctl.view().dock_collapsed);
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
    assert_eq!(
        w.ctl.show(),
        [
            Effect::PlaceWindow(open_rect(DockEdge::Right)),
            Effect::ShowPanel
        ]
    );
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

    // A paste whose clipboard restore failed still pasted: the Panel collapses.
    let mut w = docked(DockTrigger::Hover, DockEdge::Right);
    assert_eq!(
        w.ctl
            .dock_after_use(&UseResult::Done(UseOutcome::PastedNotRestored)),
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
fn a_window_that_reports_a_leave_under_an_open_menu_neither_collapses_nor_closes_it() {
    // Where the cursor cannot be read the window says when the pointer leaves, and it may say
    // so when the pointer only moved onto its own menu.
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    w.targets.set_cursor(None);
    w.ctl.open_app_menu();
    assert_eq!(w.ctl.dock_hover(DockEvent::PanelHover(false)), Vec::new());
    assert!(!w.ctl.dock_needs_tick(), "no leave delay runs under a menu");
    advance(&w, 5_000);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    assert!(w.ctl.view().app_menu.is_some());
    assert!(!w.ctl.view().dock_collapsed);
}

#[test]
fn an_open_menu_does_not_hold_the_panel_out_once_the_pointer_and_the_keyboard_are_gone() {
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    w.ctl.open_app_menu();
    // On the menu (it is drawn inside the Panel): the Panel stays.
    w.targets.set_cursor(Some((1700, 300)));
    advance(&w, 10_000);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    assert!(w.ctl.view().app_menu.is_some());
    // The pointer went to another window and the Panel has not the keyboard: the leave delay
    // runs like any other, and the menu goes with the Panel.
    w.targets.set_cursor(Some((500, 500)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 10_399);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 10_400);
    assert_eq!(w.ctl.dock_tick(), place(handle(DockEdge::Right)));
    assert_eq!(w.ctl.view().app_menu, None);
}

#[test]
fn a_tucked_panel_stays_tucked_under_a_pointer_that_never_left_the_tab() {
    // The close button of a Panel docked to the right is by the screen edge: the pointer that
    // clicked it is on the tab the moment the Panel tucks.
    for tuck in [0, 1, 2] {
        let mut w = docked(DockTrigger::Both, DockEdge::Right);
        w.ctl.set_panel_focused(true);
        w.targets.set_cursor(Some((1916, 110)));
        let tucked = match tuck {
            0 => w.ctl.close(),
            1 => w.ctl.on_hotkey(HotkeyAction::TogglePanel),
            _ => w.ctl.dock_event(DockEvent::Pasted),
        };
        assert_eq!(tucked, place(handle(DockEdge::Right)), "{tuck}");
        w.ctl.set_panel_focused(false);
        for step in 1..=40 {
            advance(&w, step * 50);
            assert_eq!(w.ctl.dock_tick(), Vec::new(), "{tuck} at step {step}");
        }
        assert!(w.ctl.view().dock_collapsed);
        // Off the tab and back on it: that is asking for the Panel.
        w.targets.set_cursor(Some((1500, 110)));
        assert_eq!(w.ctl.dock_tick(), Vec::new());
        w.targets.set_cursor(Some((1916, 110)));
        advance(&w, 3_000);
        assert_eq!(w.ctl.dock_tick(), Vec::new());
        advance(&w, 3_150);
        assert_eq!(
            w.ctl.dock_tick(),
            place(open_rect(DockEdge::Right)),
            "{tuck}"
        );
    }
    // A click on the tab opens it without leaving first.
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    w.targets.set_cursor(Some((1916, 110)));
    w.ctl.close();
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    assert_eq!(
        w.ctl.dock_event(DockEvent::HandleClick),
        place(open_rect(DockEdge::Right))
    );
}

#[test]
fn the_pointer_at_the_edge_beside_the_tab_opens_the_panel_once_and_it_stays() {
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    // Tucked by leaving it.
    w.targets.set_cursor(Some((500, 500)));
    w.ctl.dock_tick();
    advance(&w, 400);
    assert_eq!(w.ctl.dock_tick(), place(handle(DockEdge::Right)));
    w.ctl.dock_tick();
    // At the screen edge ten pixels above the tab (it spans 100 to 600): near enough.
    w.targets.set_cursor(Some((1917, 90)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 550);
    assert_eq!(w.ctl.dock_tick(), place(open_rect(DockEdge::Right)));
    // The pointer has not moved, and it is as near the open Panel as it was to the tab: the
    // Panel must not tuck again (and then open again, for ever).
    for step in 1..=100 {
        advance(&w, 550 + step * 50);
        assert_eq!(w.ctl.dock_tick(), Vec::new(), "step {step}");
        assert!(!w.ctl.view().dock_collapsed, "step {step}");
    }
}

#[test]
fn the_hotkey_gives_a_panel_that_only_slid_out_the_keyboard_and_tucks_one_that_has_it() {
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    // Slid out under the pointer: the Target still has the keyboard.
    w.targets.set_cursor(Some((1700, 300)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    // The hotkey there means "I want to type in it", not "go away".
    assert_eq!(
        w.ctl.on_hotkey(HotkeyAction::TogglePanel),
        [Effect::ShowPanel]
    );
    assert!(!w.ctl.view().dock_collapsed);
    // The window took the keyboard; the hotkey again tucks it.
    w.ctl.set_panel_focused(true);
    assert_eq!(
        w.ctl.on_hotkey(HotkeyAction::TogglePanel),
        place(handle(DockEdge::Right))
    );
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
    // Made narrower from its inner side: the outer side stays on the edge.
    let narrower = Rect {
        x: 1620,
        w: 300,
        ..open_rect(DockEdge::Right)
    };
    assert_eq!(w.ctl.frame_changed(&display(), narrower), Vec::new());
    let g = &w.ctl.settings().panel_geometry[0];
    assert_eq!(g.docked, Some((300, FLOATING.h)));
    assert_eq!(
        Rect::of(g),
        FLOATING,
        "the floating frame is not the docked one"
    );
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
fn a_tall_panel_dragged_through_the_middle_of_the_screen_never_docks() {
    // As tall as the work area and a few pixels from the left edge: the old dock took this
    // for "at an edge" on every drag.
    let tall = Rect {
        x: 10,
        y: 0,
        w: 320,
        h: 1040,
    };
    let mut w = floating(DockTrigger::Both);
    w.ctl.frame_changed(&display(), tall);
    press(&mut w, (150, 10), tall);
    for (ms, cursor) in [
        (0, (160, 12)),
        (300, (400, 20)),
        (900, (960, 30)),
        (5_000, (960, 30)),
    ] {
        advance(&w, ms);
        pull(&mut w, cursor);
        assert_eq!(w.ctl.view().dock_preview, None, "{cursor:?}");
        assert!(w.ctl.view().dragging);
    }
    let dropped = Rect {
        x: 820,
        y: 0,
        ..tall
    };
    assert_eq!(w.ctl.drag_end(), place(dropped));
    assert_eq!(w.ctl.dock_edge(), None);
    assert_eq!(w.ctl.placement(&display()), Some(dropped));
    assert!(!w.ctl.view().dragging);
}

#[test]
fn a_drag_that_stays_on_the_edge_slides_the_dock_along_it() {
    let mut w = docked(DockTrigger::Click, DockEdge::Left);
    press(&mut w, (100, 110), open_rect(DockEdge::Left));
    let slid = Rect {
        y: 180,
        ..open_rect(DockEdge::Left)
    };
    // Ten pixels of wobble across the edge is not a pull off it.
    assert_eq!(pull(&mut w, (110, 190)), place(slid));
    assert_eq!(w.ctl.drag_end(), place(slid));
    assert_eq!(w.ctl.dock_edge(), Some(DockEdge::Left));
    let g = &w.ctl.settings().panel_geometry[0];
    assert_eq!((g.along, Rect::of(g)), (Some(180), FLOATING));
    assert_eq!(w.ctl.placement(&display()), Some(slid));
}

#[test]
fn a_press_on_a_floating_header_changes_nothing_about_docking() {
    // Against the left edge, pressed and let go without a move, however long it is held.
    let at_edge = Rect { x: 0, ..FLOATING };
    let mut w = floating(DockTrigger::Click);
    w.ctl.frame_changed(&display(), at_edge);
    press(&mut w, (2, 110), at_edge);
    for ms in [0, 200, 5_000] {
        advance(&w, ms);
        assert_eq!(pull(&mut w, (2, 110)), Vec::new(), "{ms}");
        assert_eq!(w.ctl.view().dock_preview, None, "{ms}");
    }
    assert_eq!(w.ctl.drag_end(), place(at_edge));
    assert_eq!(w.ctl.dock_edge(), None);
    // No drag at all: moves and releases do nothing.
    assert_eq!(w.ctl.drag_move(Some((5, 5))), Vec::new());
    assert_eq!(w.ctl.drag_end(), Vec::new());
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
    assert_eq!(w.ctl.take_pending(), place(FLOATING));
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
    assert_eq!(
        w.ctl.choose_app_menu(row),
        place(open_rect_from(DockEdge::Left, near_left))
    );
    assert_eq!(w.ctl.dock_edge(), Some(DockEdge::Left));
    assert!(w.ctl.view().dock_edge.is_some());
    assert_eq!(w.ctl.view().app_menu, None);
    // Undocking puts it back where it floated, at its own size.
    assert_eq!(w.ctl.choose_app_menu(row), place(near_left));
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
        taskbar: None,
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
        assert!(view.dock_edge.is_some() && !view.dock_collapsed);
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
            full_edge: false,
            ..DockSettings::default()
        };
    });
    assert_eq!(w.ctl.settings().dock.trigger, DockTrigger::Both);
    assert_eq!(w.ctl.settings().dock.hover_delay_ms, 2000, "clamped");
}

/// `display()` with the taskbar on `edge`.
fn taskbar_display(edge: DockEdge) -> frame::Monitor {
    frame::Monitor {
        taskbar: Some(edge),
        ..display()
    }
}

#[test]
fn the_kebab_names_the_nearest_edge_and_never_the_taskbars() {
    let mut w = floating(DockTrigger::Both);
    // O32: lower middle, a little left of centre, over a bottom taskbar. The bottom is the
    // nearest edge but the taskbar's: the row offers the left edge and docks there, once.
    let lower_middle = Rect {
        x: 200,
        y: 740,
        w: 320,
        h: 300,
    };
    w.ctl
        .frame_changed(&taskbar_display(DockEdge::Bottom), lower_middle);
    assert_eq!(w.ctl.dock_target(), Some(DockEdge::Left));
    assert_eq!(w.ctl.view().dock_target, Some(DockEdge::Left));
    let row = APP_MENU
        .iter()
        .position(|e| *e == AppMenuEntry::Dock)
        .expect("in menu");
    w.ctl.open_app_menu();
    assert_eq!(
        w.ctl.choose_app_menu(row),
        place(open_rect_from(DockEdge::Left, lower_middle))
    );
    assert_eq!(w.ctl.dock_edge(), Some(DockEdge::Left));
    assert_eq!(w.ctl.view().dock_target, None, "docked: the row undocks");
    // Without a taskbar the same place docks to the bottom.
    w.ctl.dock_to(None);
    w.ctl.take_pending();
    w.ctl.frame_changed(&display(), lower_middle);
    assert_eq!(w.ctl.dock_target(), Some(DockEdge::Bottom));
}

#[test]
fn docking_to_the_taskbar_edge_is_refused_with_a_toast() {
    let mut w = floating(DockTrigger::Both);
    w.ctl
        .frame_changed(&taskbar_display(DockEdge::Bottom), FLOATING);
    w.ctl.dock_to(Some(DockEdge::Bottom));
    assert_eq!(w.ctl.dock_edge(), None);
    assert_eq!(w.ctl.take_pending(), Vec::new());
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::TaskbarEdge
    );
    w.ctl.dock_to(Some(DockEdge::Top));
    assert_eq!(w.ctl.dock_edge(), Some(DockEdge::Top));
}

#[test]
fn a_drag_held_at_an_allowed_edge_docks_there_and_at_the_taskbar_does_not() {
    let mut w = floating(DockTrigger::Both);
    let m = taskbar_display(DockEdge::Bottom);
    w.ctl.frame_changed(&m, FLOATING);
    // A move that is not a drag (a restore) previews and docks nothing.
    let near_right = Rect {
        x: 1590,
        ..FLOATING
    };
    w.ctl.frame_moved(&m, near_right);
    assert_eq!(w.ctl.view().dock_preview, None);
    assert_eq!(w.ctl.dock_edge(), None);
    w.ctl.frame_moved(&m, FLOATING);

    // A drag: the cursor reaches the right edge and stays there.
    press(&mut w, (700, 110), FLOATING);
    pull(&mut w, (1200, 200));
    assert_eq!(w.ctl.view().dock_preview, None, "away from every edge");
    advance(&w, 1_000);
    assert_eq!(pull(&mut w, (1919, 300)), [Effect::MoveWindow(1819, 290)]);
    assert_eq!(w.ctl.view().dock_preview, None, "only just arrived");
    advance(&w, 1_199);
    assert_eq!(pull(&mut w, (1919, 300)), Vec::new());
    advance(&w, 1_200);
    let at_right = Rect {
        x: 1600,
        y: 290,
        ..FLOATING
    };
    assert_eq!(pull(&mut w, (1919, 300)), place(at_right));
    assert_eq!(w.ctl.view().dock_preview, Some(DockEdge::Right));
    assert_eq!(
        w.ctl.dock_edge(),
        None,
        "nothing is docked before the release"
    );
    assert_eq!(w.ctl.drag_end(), place(at_right));
    assert_eq!(w.ctl.dock_edge(), Some(DockEdge::Right));
    assert_eq!(w.ctl.view().dock_preview, None);
    assert_eq!(Rect::of(&w.ctl.settings().panel_geometry[0]).w, FLOATING.w);

    // Pulled off again and held at the bottom, the taskbar's edge: it stays floating.
    press(&mut w, (1800, 300), at_right);
    pull(&mut w, (1700, 300));
    assert!(w.ctl.view().dock_edge.is_none());
    advance(&w, 2_000);
    pull(&mut w, (900, 1039));
    advance(&w, 3_000);
    pull(&mut w, (900, 1039));
    assert_eq!(w.ctl.view().dock_preview, None);
    w.ctl.drag_end();
    assert_eq!(w.ctl.dock_edge(), None);
}

#[test]
fn pulling_back_from_the_edge_before_the_release_leaves_the_panel_floating() {
    let mut w = floating(DockTrigger::Both);
    press(&mut w, (700, 110), FLOATING);
    pull(&mut w, (0, 300));
    advance(&w, 200);
    let at_left = Rect {
        x: 0,
        y: 290,
        ..FLOATING
    };
    assert_eq!(pull(&mut w, (0, 300)), place(at_left));
    assert_eq!(w.ctl.view().dock_preview, Some(DockEdge::Left));
    // Twelve pixels are still the edge; the thirteenth is the way out.
    assert_eq!(pull(&mut w, (12, 300)), Vec::new());
    assert_eq!(w.ctl.view().dock_preview, Some(DockEdge::Left));
    let out = Rect {
        x: -87,
        y: 290,
        ..FLOATING
    };
    assert_eq!(pull(&mut w, (13, 300)), place(out));
    assert_eq!(w.ctl.view().dock_preview, None);
    // Dropped there, it floats, pulled inside the work area.
    assert_eq!(w.ctl.drag_end(), place(Rect { x: 0, ..out }));
    assert_eq!(w.ctl.dock_edge(), None);
}

#[test]
fn crossing_to_another_monitor_docks_nothing_and_the_panel_floats_there() {
    let mut w = floating(DockTrigger::Both);
    w.targets.add_monitor(display());
    w.targets.add_monitor(second_display());
    press(&mut w, (700, 110), FLOATING);
    // Over the seam between the two monitors in well under the arm delay.
    pull(&mut w, (1919, 300));
    advance(&w, 100);
    pull(&mut w, (1925, 300));
    advance(&w, 150);
    pull(&mut w, (2600, 400));
    advance(&w, 2_000);
    pull(&mut w, (2600, 400));
    assert_eq!(w.ctl.view().dock_preview, None);
    // The same logical size at 150 %, under the cursor as it was held: 100 of 320 across the
    // header is 150 of 480, and 10 down it is 15.
    let there = Rect {
        x: 2450,
        y: 385,
        w: 480,
        h: 750,
    };
    assert_eq!(w.ctl.drag_end(), place(there));
    assert_eq!(w.ctl.dock_edge(), None);
    assert_eq!(w.ctl.placement(&second_display()), Some(there));
    assert_eq!(
        w.ctl.placement(&display()),
        Some(FLOATING),
        "what the first monitor remembers is left alone"
    );
}

#[test]
fn a_docked_panel_dragged_to_another_monitor_is_no_longer_docked_on_the_first() {
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    w.targets.add_monitor(display());
    w.targets.add_monitor(second_display());
    press(&mut w, (1700, 110), open_rect(DockEdge::Right));
    pull(&mut w, (1600, 110));
    pull(&mut w, (3000, 500));
    w.ctl.drag_end();
    assert_eq!(w.ctl.dock_edge(), None);
    let edges: Vec<_> = w
        .ctl
        .settings()
        .panel_geometry
        .iter()
        .map(|g| g.edge)
        .collect();
    assert_eq!(edges, [None, None]);
}

#[test]
fn reset_size_and_position_floats_the_panel_clear_of_every_edge() {
    let mut w = docked(DockTrigger::Both, DockEdge::Left);
    let effects = w.ctl.reset_panel_geometry();
    let [Effect::PlaceWindow(rect)] = effects.as_slice() else {
        panic!("one placement, got {effects:?}");
    };
    let area = display().work_area;
    assert_eq!(w.ctl.dock_edge(), None, "the reset forgets the edge");
    let inside = |start: i32, len: u32, low: i32, span: u32| {
        start > low && i64::from(start) + i64::from(len) < i64::from(low) + i64::from(span)
    };
    assert!(
        inside(rect.x, rect.w, area.x, area.w) && inside(rect.y, rect.h, area.y, area.h),
        "{rect:?} must touch no edge of {area:?}"
    );
}

#[test]
fn dropping_at_y_300_docks_at_y_300_and_remembers_it() {
    let mut w = floating(DockTrigger::Off);
    let dropped = Rect { y: 300, ..FLOATING };
    w.ctl.frame_changed(&display(), dropped);
    w.ctl.dock_to(Some(DockEdge::Left));
    let expected = Rect {
        x: 0,
        y: 300,
        w: FLOATING.w,
        h: FLOATING.h,
    };
    assert_eq!(w.ctl.take_pending(), place(expected));
    let g = &w.ctl.settings().panel_geometry[0];
    assert_eq!(
        (g.edge, g.along, g.docked, Rect::of(g)),
        (
            Some(DockEdge::Left),
            Some(300),
            Some((FLOATING.w, FLOATING.h)),
            dropped
        )
    );
    // Shown again, it comes back at the same height.
    assert_eq!(w.ctl.placement(&display()), Some(expected));
}

#[test]
fn undocking_gives_the_floating_frame_back_and_keeps_the_docked_size() {
    let mut w = floating(DockTrigger::Off);
    w.ctl.dock_to(Some(DockEdge::Right));
    w.ctl.take_pending();
    w.ctl.frame_changed(
        &display(),
        Rect {
            x: 1520,
            w: 400,
            ..open_rect(DockEdge::Right)
        },
    );
    w.ctl.dock_to(None);
    assert_eq!(w.ctl.take_pending(), place(FLOATING));
    let g = &w.ctl.settings().panel_geometry[0];
    assert_eq!((g.edge, g.along), (None, None));
    assert_eq!(Rect::of(g), FLOATING);
    assert_eq!(
        g.docked,
        Some((400, FLOATING.h)),
        "for the next time it docks"
    );
}

#[test]
fn a_resize_while_floating_changes_only_the_floating_size() {
    let mut w = docked(DockTrigger::Off, DockEdge::Left);
    w.ctl.dock_to(None);
    w.ctl.take_pending();
    let bigger = Rect {
        w: 440,
        h: 700,
        ..FLOATING
    };
    assert_eq!(w.ctl.frame_changed(&display(), bigger), Vec::new());
    let g = &w.ctl.settings().panel_geometry[0];
    assert_eq!(
        (Rect::of(g), g.docked),
        (bigger, Some((FLOATING.w, FLOATING.h)))
    );
    w.ctl.dock_to(Some(DockEdge::Left));
    assert_eq!(w.ctl.take_pending(), place(open_rect(DockEdge::Left)));
}

#[test]
fn full_edge_gives_the_side_dock_the_work_area_height_and_back() {
    let mut w = docked(DockTrigger::Off, DockEdge::Left);
    w.ctl.set_dock_full_edge(true);
    let full = display().work_area;
    assert_eq!(
        w.ctl.take_pending(),
        place(Rect {
            x: 0,
            y: full.y,
            w: FLOATING.w,
            h: full.h
        })
    );
    w.ctl.set_dock_full_edge(false);
    assert_eq!(w.ctl.take_pending(), place(open_rect(DockEdge::Left)));
}

#[test]
fn sliding_clamps_to_the_work_area() {
    let mut w = docked(DockTrigger::Off, DockEdge::Left);
    let start = open_rect(DockEdge::Left);
    let moved = Rect {
        y: start.y + 50,
        ..start
    };
    press(&mut w, (100, 110), start);
    assert_eq!(pull(&mut w, (100, 160)), place(moved));
    assert_eq!(w.ctl.drag_end(), place(moved));
    assert_eq!(w.ctl.settings().panel_geometry[0].along, Some(start.y + 50));
    // Far past the bottom: pulled inside; the top: stops at the work area.
    let work = display().work_area;
    let bottom = Rect {
        y: i32::try_from(work.h - start.h).unwrap(),
        ..start
    };
    press(&mut w, (100, 160), moved);
    assert_eq!(pull(&mut w, (100, 5160)), place(bottom));
    assert_eq!(pull(&mut w, (100, 6000)), Vec::new(), "already at the end");
    assert_eq!(pull(&mut w, (100, -9000)), place(Rect { y: 0, ..start }));
    w.ctl.drag_end();
    let g = &w.ctl.settings().panel_geometry[0];
    assert_eq!((g.edge, g.along), (Some(DockEdge::Left), Some(0)));
}

#[test]
fn a_docked_panel_refuses_a_resize_from_its_outer_edge() {
    use crate::frame::Edge;
    let mut w = docked(DockTrigger::Off, DockEdge::Left);
    assert_eq!(w.ctl.resize(Edge::Left), Vec::new());
    assert_eq!(w.ctl.resize(Edge::TopLeft), Vec::new());
    assert_eq!(
        w.ctl.resize(Edge::Right),
        vec![Effect::ResizeWindow(Edge::Right)]
    );
    assert_eq!(
        w.ctl.resize(Edge::Bottom),
        vec![Effect::ResizeWindow(Edge::Bottom)]
    );
    w.ctl.dock_to(None);
    assert_eq!(
        w.ctl.resize(Edge::Left),
        vec![Effect::ResizeWindow(Edge::Left)]
    );
}

#[test]
fn the_full_edge_keeps_the_panels_own_height_for_when_it_is_turned_off() {
    let mut w = floating(DockTrigger::Off);
    w.ctl.set_dock_full_edge(true);
    w.ctl.dock_to(Some(DockEdge::Left));
    let work = display().work_area;
    let full = Rect {
        x: 0,
        y: work.y,
        w: FLOATING.w,
        h: work.h,
    };
    assert_eq!(w.ctl.take_pending(), place(full));
    // The window reports the full frame back; the remembered height is still the Panel's own.
    w.ctl.frame_changed(&display(), full);
    let g = &w.ctl.settings().panel_geometry[0];
    assert_eq!((g.h, g.along), (FLOATING.h, Some(FLOATING.y)));
    w.ctl.set_dock_full_edge(false);
    assert_eq!(
        w.ctl.take_pending(),
        place(open_rect_from(DockEdge::Left, FLOATING))
    );
}

#[test]
fn a_slide_counts_from_the_press_so_repeated_moves_do_not_pile_up() {
    let mut w = docked(DockTrigger::Off, DockEdge::Left);
    let start = open_rect(DockEdge::Left);
    press(&mut w, (100, 110), start);
    // Three moves, the window not having caught up between them.
    for y in [120, 140, 130] {
        pull(&mut w, (100, y));
    }
    // The same place again changes nothing (no placement).
    assert_eq!(pull(&mut w, (100, 130)), Vec::new());
    w.ctl.drag_end();
    assert_eq!(w.ctl.settings().panel_geometry[0].along, Some(start.y + 20));
    // A new press starts from where the Panel now is.
    press(
        &mut w,
        (100, 130),
        Rect {
            y: start.y + 20,
            ..start
        },
    );
    pull(&mut w, (100, 135));
    w.ctl.drag_end();
    assert_eq!(w.ctl.settings().panel_geometry[0].along, Some(start.y + 25));
}

#[test]
fn the_full_edge_does_not_slide_or_resize_a_side_dock_but_a_top_dock_still_does() {
    use crate::frame::Edge;
    let mut w = docked(DockTrigger::Off, DockEdge::Left);
    w.ctl.set_dock_full_edge(true);
    w.ctl.take_pending();
    let before = w.ctl.settings().clone();
    let work = display().work_area;
    let full = Rect {
        x: 0,
        y: work.y,
        w: FLOATING.w,
        h: work.h,
    };
    press(&mut w, (100, 110), full);
    assert_eq!(pull(&mut w, (100, 190)), Vec::new());
    assert_eq!(w.ctl.drag_end(), place(full));
    assert_eq!(*w.ctl.settings(), before, "no stored change");
    assert!(w.ctl.view().dock_full_edge);
    for e in [Edge::Top, Edge::Bottom, Edge::TopRight, Edge::BottomRight] {
        assert_eq!(w.ctl.resize(e), Vec::new(), "{e:?}");
    }
    assert_eq!(
        w.ctl.resize(Edge::Right),
        vec![Effect::ResizeWindow(Edge::Right)]
    );
    // Top docks are not side docks.
    w.ctl.dock_to(Some(DockEdge::Top));
    let pending = w.ctl.take_pending();
    let [Effect::PlaceWindow(top)] = pending.as_slice() else {
        panic!("one placement, got {pending:?}");
    };
    press(&mut w, (top.x + 100, 10), *top);
    assert_eq!(
        pull(&mut w, (top.x + 150, 10)),
        place(Rect {
            x: top.x + 50,
            ..*top
        })
    );
}

#[test]
fn each_edge_remembers_where_the_panel_sat_along_it() {
    let mut w = floating(DockTrigger::Off);
    w.ctl.frame_changed(&display(), Rect { y: 300, ..FLOATING });
    w.ctl.dock_to(Some(DockEdge::Left));
    let left = w.ctl.take_pending();
    let [Effect::PlaceWindow(at)] = left.as_slice() else {
        panic!("one placement")
    };
    press(&mut w, (100, 310), *at);
    pull(&mut w, (100, 410));
    w.ctl.drag_end();
    // To the top edge: no memory there, so it is centred; the left's place is kept.
    w.ctl.dock_to(Some(DockEdge::Top));
    let top = w.ctl.take_pending();
    let [Effect::PlaceWindow(r)] = top.as_slice() else {
        panic!("one placement")
    };
    assert_eq!(r.y, 0);
    // Back to the left: the place it had there.
    w.ctl.dock_to(Some(DockEdge::Left));
    let left = w.ctl.take_pending();
    let [Effect::PlaceWindow(r)] = left.as_slice() else {
        panic!("one placement")
    };
    assert_eq!(r.y, 400);
    // And it survives a settings round trip.
    let back = Settings::from_json(&w.ctl.settings().to_json());
    assert_eq!(back.panel_geometry, w.ctl.settings().panel_geometry);
}

#[test]
fn moving_a_side_dock_to_the_bottom_does_not_bring_the_full_height_along() {
    // What an older version stored for a side dock: the work area's full height.
    let mut w = floating(DockTrigger::Off);
    let work = display().work_area;
    w.ctl.update_settings(|s| {
        s.remember_panel_geometry(crate::settings::PanelGeometry {
            pre_dock: Some((FLOATING.w, FLOATING.h)),
            ..Rect {
                x: 0,
                y: 0,
                w: FLOATING.w,
                h: work.h,
            }
            .geometry(&display(), Some(DockEdge::Left))
        });
    });
    assert_eq!(
        w.ctl.settings().panel_geometry[0].docked,
        Some((FLOATING.w, work.h)),
        "a side dock is stored full height"
    );
    w.ctl.dock_to(Some(DockEdge::Bottom));
    let pending = w.ctl.take_pending();
    let [Effect::PlaceWindow(rect)] = pending.as_slice() else {
        panic!("one placement")
    };
    assert_eq!(rect.h, FLOATING.h, "the height it floats at");
    assert_eq!(
        i64::from(rect.y) + i64::from(rect.h),
        i64::from(work.y) + i64::from(work.h),
        "flush with the bottom of the work area"
    );
    // Shown again later, a stored frame of the same kind still gives the same height.
    assert_eq!(w.ctl.placement(&display()).map(|r| r.h), Some(FLOATING.h));
}

#[test]
fn a_stored_bottom_dock_that_was_saved_full_height_reopens_at_the_panel_height() {
    // What an older version could leave in settings.json: edge Bottom, full height, no pre_dock.
    let mut w = floating(DockTrigger::Off);
    let work = display().work_area;
    w.ctl.update_settings(|s| {
        s.remember_panel_geometry(
            Rect {
                x: 600,
                y: work.y,
                w: 320,
                h: work.h,
            }
            .geometry(&display(), Some(DockEdge::Bottom)),
        );
    });
    let rect = w.ctl.placement(&display()).expect("a stored frame");
    assert_eq!(rect.h, 480, "the default height, not the whole screen");
    assert_eq!(
        i64::from(rect.y) + i64::from(rect.h),
        i64::from(work.y) + i64::from(work.h)
    );
}

#[test]
fn an_old_full_height_side_dock_opens_compact_when_the_full_edge_is_off() {
    let mut w = floating(DockTrigger::Off);
    let work = display().work_area;
    w.ctl.update_settings(|s| {
        s.remember_panel_geometry(
            Rect {
                x: 0,
                y: 0,
                w: FLOATING.w,
                h: work.h,
            }
            .geometry(&display(), Some(DockEdge::Left)),
        );
        s.panel_geometry[0].along = None;
    });
    let placed = w.ctl.placement(&display()).expect("a place");
    assert_eq!((placed.w, placed.h), (FLOATING.w, 480));
    assert_eq!(
        placed.y,
        i32::try_from((work.h - 480) / 2).unwrap(),
        "centred"
    );
    w.ctl.set_dock_full_edge(true);
    assert_eq!(w.ctl.placement(&display()).map(|r| r.h), Some(work.h));
}

#[test]
fn a_side_dock_and_a_top_or_bottom_dock_each_keep_the_size_they_were_given() {
    let mut w = floating(DockTrigger::Off);
    let work = display().work_area;
    // Docked right and made tall and narrow there.
    w.ctl.dock_to(Some(DockEdge::Right));
    w.ctl.take_pending();
    let tall = Rect {
        x: 1620,
        y: 40,
        w: 300,
        h: 900,
    };
    assert_eq!(w.ctl.frame_changed(&display(), tall), Vec::new());
    // To the bottom: never sized there, so its floating size, not the tall side dock's.
    w.ctl.dock_to(Some(DockEdge::Bottom));
    let pending = w.ctl.take_pending();
    let [Effect::PlaceWindow(bottom)] = pending.as_slice() else {
        panic!("one placement")
    };
    assert_eq!((bottom.w, bottom.h), (FLOATING.w, FLOATING.h));
    // Made wide and low there.
    let low = Rect {
        x: 500,
        y: i32::try_from(work.h).unwrap() - 260,
        w: 440,
        h: 260,
    };
    assert_eq!(w.ctl.frame_changed(&display(), low), Vec::new());
    // Back to the right: the tall, narrow size, where it was along that edge.
    w.ctl.dock_to(Some(DockEdge::Right));
    assert_eq!(w.ctl.take_pending(), place(tall));
    // To the top: the same kind of edge as the bottom, so the size it has there.
    w.ctl.dock_to(Some(DockEdge::Top));
    let pending = w.ctl.take_pending();
    let [Effect::PlaceWindow(top)] = pending.as_slice() else {
        panic!("one placement")
    };
    assert_eq!((top.w, top.h, top.y), (440, 260, 0));
    // Both sizes are kept, apart.
    let g = &w.ctl.settings().panel_geometry[0];
    assert_eq!(
        (g.docked, g.docked_flat),
        (Some((300, 900)), Some((440, 260)))
    );
    assert_eq!((g.w, g.h), (FLOATING.w, FLOATING.h), "nor the floating one");
}

#[test]
fn a_drag_to_the_bottom_edge_never_shows_a_panel_as_tall_as_the_screen() {
    // A Panel that floats as tall as the screen, held at the bottom edge.
    let mut w = floating(DockTrigger::Off);
    let work = display().work_area;
    w.ctl.frame_changed(
        &display(),
        Rect {
            h: work.h,
            y: 0,
            ..FLOATING
        },
    );
    press(
        &mut w,
        (700, 10),
        Rect {
            y: 0,
            h: work.h,
            ..FLOATING
        },
    );
    pull(&mut w, (700, 600));
    let bottom = i32::try_from(work.h).unwrap() - 1;
    pull(&mut w, (700, bottom));
    advance(&w, 1000);
    let shown = pull(&mut w, (700, bottom));
    let [Effect::PlaceWindow(rect)] = shown.as_slice() else {
        panic!("the preview: {shown:?}")
    };
    assert_eq!(rect.h, 480, "the default height: it floats full height too");
    assert_eq!(i64::from(rect.y) + i64::from(rect.h), i64::from(work.h));
}

#[test]
fn a_drag_ends_when_the_button_is_seen_up_after_it_was_seen_down() {
    let mut w = floating(DockTrigger::Off);
    assert!(!w.ctl.drag_let_go(), "no drag");
    w.targets.set_primary_down(Some(true));
    press(&mut w, (700, 110), FLOATING);
    assert!(!w.ctl.drag_let_go());
    pull(&mut w, (900, 300));
    assert!(!w.ctl.drag_let_go());
    w.targets.set_primary_down(Some(false));
    assert!(
        w.ctl.drag_let_go(),
        "the window never said so, the host does"
    );
    assert!(w.ctl.dragging(), "asking ends nothing by itself");
    let dropped = Rect {
        x: 800,
        y: 290,
        ..FLOATING
    };
    assert_eq!(w.ctl.drag_end(), place(dropped));
    assert!(!w.ctl.drag_let_go(), "no drag any more");
}

#[test]
fn a_button_that_never_reads_as_down_does_not_end_the_drag() {
    // In front of a window of higher privilege Windows answers "up" for every button.
    let mut w = floating(DockTrigger::Off);
    w.targets.set_primary_down(Some(false));
    press(&mut w, (700, 110), FLOATING);
    for step in 1..=20 {
        assert!(!w.ctl.drag_let_go(), "step {step}");
        pull(&mut w, (700 + step * 10, 110));
    }
    assert!(w.ctl.dragging());
    // Nor where the button cannot be read at all.
    w.targets.set_primary_down(None);
    assert!(!w.ctl.drag_let_go());
    // The window's own pointer-up is what ends it.
    assert_eq!(w.ctl.drag_end().len(), 1);
    // A drag seen down and ended says nothing about the next one.
    w.targets.set_primary_down(Some(true));
    press(&mut w, (1000, 110), Rect { x: 900, ..FLOATING });
    assert!(!w.ctl.drag_let_go());
    w.ctl.drag_end();
    w.targets.set_primary_down(Some(false));
    press(&mut w, (1000, 110), Rect { x: 900, ..FLOATING });
    assert!(!w.ctl.drag_let_go(), "this drag never saw the button down");
}

#[test]
fn a_tucked_panel_the_system_moved_to_another_monitor_opens_where_it_is_remembered_there() {
    // Tucked on a monitor that went away: Windows moves the tab's window to the one left.
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    w.ctl.set_panel_focused(true);
    assert_eq!(w.ctl.close(), place(handle(DockEdge::Right)));
    w.ctl.set_panel_focused(false);
    let moved = Rect {
        x: 1920,
        y: 100,
        w: 6,
        h: 500,
    };
    let there = w.ctl.placement(&second_display()).expect("a place");
    assert_eq!(w.ctl.frame_moved(&second_display(), moved), place(there));
    assert!(second_display().holds(there.x, there.y), "{there:?}");
    assert!(
        there.w > 6,
        "the Panel, not its tab: it is not docked there"
    );
    assert!(!w.ctl.view().dock_collapsed);
    assert_eq!(w.ctl.dock_edge(), None);
    assert_eq!(
        w.ctl.settings().panel_geometry[0].edge,
        Some(DockEdge::Right),
        "the first monitor still remembers its dock"
    );
}

/// `display()` with its work area starting 200 px lower (a taskbar or an app bar took the top).
fn lowered_display() -> frame::Monitor {
    frame::Monitor {
        work_area: Rect {
            x: 0,
            y: 200,
            w: 1920,
            h: 840,
        },
        ..display()
    }
}

#[test]
fn a_tucked_panel_follows_a_work_area_that_changed_under_it() {
    let lowered = lowered_display();
    let open = frame::docked_rect(
        DockEdge::Right,
        lowered.work_area,
        (FLOATING.w, FLOATING.h),
        1.0,
        Some(FLOATING.y),
        false,
    );
    assert_eq!(open.y, 200, "pulled inside the new work area");
    let tab = frame::handle_rect(
        DockEdge::Right,
        lowered.work_area,
        open.along(DockEdge::Right),
        1.0,
    );
    // The window reports a move (Windows shifted it): the tab is placed on the new area.
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    w.ctl.dock_event(DockEvent::Hotkey);
    assert!(w.ctl.view().dock_collapsed);
    assert_eq!(
        w.ctl.frame_moved(&lowered, handle(DockEdge::Right)),
        place(tab)
    );
    assert_eq!(w.ctl.frame_moved(&lowered, tab), Vec::new(), "once");
    assert_eq!(w.ctl.dock_event(DockEvent::HandleClick), place(open));
    // No window is told when only the taskbar moves: the dock's timer sees it.
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    w.ctl.dock_event(DockEvent::Hotkey);
    w.targets.set_monitors(vec![display()]);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    w.targets.set_monitors(vec![lowered]);
    assert_eq!(w.ctl.dock_tick(), place(tab));
    assert_eq!(w.ctl.dock_tick(), Vec::new(), "once");
    assert!(w.ctl.view().dock_collapsed);
}

#[test]
fn a_resize_under_way_holds_the_docked_panel_out() {
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    w.targets.set_primary_down(Some(true));
    assert_eq!(
        w.ctl.resize(frame::Edge::Left),
        vec![Effect::ResizeWindow(frame::Edge::Left)]
    );
    // Past the widest the Panel goes, the pointer is well outside it.
    w.targets.set_cursor(Some((1300, 300)));
    for step in 1..=40 {
        advance(&w, step * 50);
        assert_eq!(w.ctl.dock_tick(), Vec::new(), "step {step}");
    }
    assert!(!w.ctl.view().dock_collapsed);
    // The button comes up out there: from then the leave delay runs as ever.
    w.targets.set_primary_down(Some(false));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 2_399);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 2_400);
    assert_eq!(w.ctl.dock_tick(), place(handle(DockEdge::Right)));
    // A button held for something else afterwards holds nothing.
    w.targets.set_cursor(Some((1000, 300)));
    w.targets.set_primary_down(Some(true));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    assert!(w.ctl.view().dock_collapsed);
}

#[test]
fn a_panel_docked_from_the_menu_stays_out_until_the_pointer_has_been_on_it() {
    let mut w = floating(DockTrigger::Both);
    // The pointer is where the menu row was, or over the Settings window.
    w.targets.set_cursor(Some((1000, 300)));
    w.ctl.dock_to(Some(DockEdge::Right));
    assert_eq!(w.ctl.take_pending(), place(open_rect(DockEdge::Right)));
    for step in 1..=100 {
        advance(&w, step * 50);
        assert_eq!(w.ctl.dock_tick(), Vec::new(), "step {step}");
    }
    assert!(!w.ctl.view().dock_collapsed, "its toast can be read");
    // On it, then off it: now it tucks as ever.
    w.targets.set_cursor(Some((1700, 300)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    w.targets.set_cursor(Some((1000, 300)));
    advance(&w, 6_000);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 6_400);
    assert_eq!(w.ctl.dock_tick(), place(handle(DockEdge::Right)));
}

#[test]
fn where_the_cursor_cannot_be_read_only_the_leave_of_the_docking_itself_is_ignored() {
    // The window went to its edge from under the pointer and says so at once: not a leave.
    let mut w = floating(DockTrigger::Both);
    w.targets.set_cursor(None);
    w.ctl.dock_to(Some(DockEdge::Right));
    assert_eq!(w.ctl.take_pending(), place(open_rect(DockEdge::Right)));
    advance(&w, 30);
    assert_eq!(w.ctl.dock_hover(DockEvent::PanelHover(false)), Vec::new());
    assert!(!w.ctl.dock_needs_tick(), "no leave delay runs");
    // The pointer comes to it and goes: now it tucks as ever.
    advance(&w, 5_000);
    assert_eq!(w.ctl.dock_hover(DockEvent::PanelHover(true)), Vec::new());
    assert_eq!(w.ctl.dock_hover(DockEvent::PanelHover(false)), Vec::new());
    advance(&w, 5_400);
    assert_eq!(w.ctl.dock_tick(), place(handle(DockEdge::Right)));

    // The Panel stayed under the pointer as it docked, so the window reported no enter: a
    // leave long after the docking is the pointer going, and the leave delay runs.
    let mut w = floating(DockTrigger::Both);
    w.targets.set_cursor(None);
    w.ctl.dock_to(Some(DockEdge::Right));
    w.ctl.take_pending();
    advance(&w, 1_000);
    assert_eq!(w.ctl.dock_hover(DockEvent::PanelHover(false)), Vec::new());
    assert!(w.ctl.dock_needs_tick(), "the leave delay is running");
    advance(&w, 1_400);
    assert_eq!(w.ctl.dock_tick(), place(handle(DockEdge::Right)));
}

#[test]
fn a_floating_panel_dropped_on_another_monitor_is_not_said_to_float_again() {
    let mut w = floating(DockTrigger::Both);
    w.targets.add_monitor(display());
    w.targets.add_monitor(second_display());
    press(&mut w, (700, 110), FLOATING);
    pull(&mut w, (2600, 400));
    w.ctl.drag_end();
    assert_eq!(w.ctl.dock_edge(), None);
    assert_eq!(
        w.ctl.view().toast,
        None,
        "it floated before and it floats now"
    );
    // A dock that was left behind is still news.
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    w.targets.add_monitor(display());
    w.targets.add_monitor(second_display());
    press(&mut w, (1700, 110), open_rect(DockEdge::Right));
    pull(&mut w, (1600, 110));
    pull(&mut w, (3000, 500));
    w.ctl.drag_end();
    assert_eq!(
        w.ctl.view().toast.map(|t| t.message),
        Some(ToastMessage::DockSet(None))
    );
}

#[test]
fn a_full_screen_app_in_front_hides_the_tab_and_the_pointer_does_not_reveal_it() {
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    assert_eq!(w.ctl.close(), place(handle(DockEdge::Right)));
    w.targets.set_cursor(Some((1000, 300)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    assert!(w.ctl.view().topmost, "the tab is above other windows");
    // A game takes the whole screen: the pointer at its edge is the game's.
    w.targets.set_fullscreen(Some(&display()));
    w.targets.set_cursor(Some((1919, 300)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    assert!(!w.ctl.view().topmost, "the game covers the tab");
    advance(&w, 5_000);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    assert!(w.ctl.view().dock_collapsed);
    // The game is gone: the tab is back, and the pointer resting on it opens the Panel.
    w.targets.set_fullscreen(None);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    assert!(w.ctl.view().topmost);
    advance(&w, 5_150);
    assert_eq!(w.ctl.dock_tick(), place(open_rect(DockEdge::Right)));
}

#[test]
fn the_hotkey_still_opens_the_dock_over_a_full_screen_app_and_it_tucks_when_left() {
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    assert_eq!(w.ctl.close(), place(handle(DockEdge::Right)));
    w.targets.set_fullscreen(Some(&display()));
    w.targets.set_cursor(Some((1000, 300)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    assert_eq!(
        w.ctl.on_hotkey(HotkeyAction::TogglePanel),
        [
            Effect::PlaceWindow(open_rect(DockEdge::Right)),
            Effect::ShowPanel
        ]
    );
    w.ctl.set_panel_focused(true);
    assert!(
        w.ctl.view().topmost,
        "called by the user, it is above the app"
    );
    advance(&w, 60_000);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    assert!(!w.ctl.view().dock_collapsed, "in use, it stays");
    // The keyboard goes back to the app: the Panel does not stay over it, pointer or not.
    w.ctl.set_panel_focused(false);
    w.targets.set_cursor(Some((1700, 300)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    assert!(!w.ctl.view().topmost);
    advance(&w, 60_400);
    assert_eq!(w.ctl.dock_tick(), place(handle(DockEdge::Right)));
}

#[test]
fn a_hidden_start_of_a_docked_panel_shows_its_tab() {
    let before = docked(DockTrigger::Both, DockEdge::Right);
    // As the app starts: no window yet, so nothing has said what monitor the Panel is on. The
    // host finds the one it was left on.
    let mut w = world_with(Some(outlook()), before.ctl.settings().clone());
    w.targets.add_monitor(display());
    assert_eq!(w.ctl.start_hidden(), place(handle(DockEdge::Right)));
    assert!(w.ctl.visible(), "on screen, as its tab");
    assert!(w.ctl.dock_live(), "and watching for the pointer");
    assert!(w.ctl.view().dock_collapsed);
    // The pointer comes to the tab: the Panel opens.
    w.targets.set_cursor(Some((1000, 300)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    w.targets.set_cursor(Some((1919, 300)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 150);
    assert_eq!(w.ctl.dock_tick(), place(open_rect(DockEdge::Right)));

    // A floating Panel and a dock that does not tuck start hidden, as asked.
    for before in [
        floating(DockTrigger::Both),
        docked(DockTrigger::Off, DockEdge::Right),
    ] {
        let mut w = world_with(Some(outlook()), before.ctl.settings().clone());
        w.targets.add_monitor(display());
        assert_eq!(w.ctl.start_hidden(), [Effect::HidePanel]);
        assert!(!w.ctl.visible());
        // Its first show (the tray, the hotkey) still has no window to ask: it goes where it
        // was left.
        let left = before.ctl.placement(&display()).expect("a stored place");
        w.ctl.show();
        assert_eq!(w.ctl.restore_anchor(), place(left));
        assert_eq!(w.ctl.restore_anchor(), Vec::new(), "once");
    }
    // So does a dock on a host that cannot say where its monitors are.
    let mut w = world_with(Some(outlook()), before.ctl.settings().clone());
    assert_eq!(w.ctl.start_hidden(), [Effect::HidePanel]);
}

#[test]
fn a_hidden_start_counts_the_monitor_the_panel_was_last_left_on() {
    // Floating on the first monitor, then taken to the second and docked there.
    let mut before = floating(DockTrigger::Both);
    let there = Rect {
        x: 2400,
        y: 200,
        w: 480,
        h: 750,
    };
    assert_eq!(
        before.ctl.frame_changed(&second_display(), there),
        Vec::new()
    );
    before.ctl.dock_to(Some(DockEdge::Right));
    let open = before.ctl.placement(&second_display()).expect("docked");
    let tab = frame::handle_rect(
        DockEdge::Right,
        second_display().work_area,
        open.along(DockEdge::Right),
        1.5,
    );
    let mut w = world_with(Some(outlook()), before.ctl.settings().clone());
    w.targets.add_monitor(display());
    w.targets.add_monitor(second_display());
    assert_eq!(
        w.ctl.start_hidden(),
        place(tab),
        "its tab, where it is docked"
    );
    assert!(w.ctl.dock_live());

    // Docked on the second monitor, then taken back to the first and left floating there: it
    // was last a floating Panel, and that starts hidden.
    before.ctl.take_pending();
    assert_eq!(before.ctl.frame_changed(&display(), FLOATING), Vec::new());
    let mut w = world_with(Some(outlook()), before.ctl.settings().clone());
    w.targets.add_monitor(display());
    w.targets.add_monitor(second_display());
    assert_eq!(w.ctl.start_hidden(), [Effect::HidePanel]);

    // The monitor it was docked on is gone: nothing of it is left to show a tab on.
    let gone = docked(DockTrigger::Both, DockEdge::Right);
    let mut w = world_with(Some(outlook()), gone.ctl.settings().clone());
    w.targets.add_monitor(second_display());
    assert_eq!(w.ctl.start_hidden(), [Effect::HidePanel]);
}

#[test]
fn the_first_show_puts_the_panel_where_it_was_left_before_its_window_exists() {
    // The window is made only once the event loop runs: the first show cannot ask it what
    // monitor it is on, and must not leave it wherever the system puts a new window.
    let before = docked(DockTrigger::Both, DockEdge::Right);
    let mut w = world_with(Some(outlook()), before.ctl.settings().clone());
    w.targets.add_monitor(display());
    assert_eq!(w.ctl.restore_anchor(), place(open_rect(DockEdge::Right)));
    assert_eq!(w.ctl.dock_edge(), Some(DockEdge::Right));
    assert_eq!(
        w.ctl.restore_anchor(),
        Vec::new(),
        "once: its monitor is known now"
    );

    let before = floating(DockTrigger::Both);
    let mut w = world_with(Some(outlook()), before.ctl.settings().clone());
    w.targets.add_monitor(display());
    assert_eq!(w.ctl.restore_anchor(), place(FLOATING));
    // The window's first report of that frame changes nothing.
    assert_eq!(w.ctl.frame_changed(&display(), FLOATING), Vec::new());
    assert_eq!(
        w.ctl.settings().panel_geometry,
        before.ctl.settings().panel_geometry
    );
}

#[test]
fn the_first_show_after_a_hidden_start_asks_again_where_the_monitors_are() {
    // Left floating on the second monitor, with an older place on the first.
    let mut before = floating(DockTrigger::Both);
    let there = Rect {
        x: 2400,
        y: 200,
        w: 480,
        h: 750,
    };
    assert_eq!(
        before.ctl.frame_changed(&second_display(), there),
        Vec::new()
    );
    let mut w = world_with(Some(outlook()), before.ctl.settings().clone());
    w.targets.add_monitor(display());
    w.targets.add_monitor(second_display());
    assert_eq!(w.ctl.start_hidden(), [Effect::HidePanel]);
    // The second monitor is unplugged while the Panel is hidden; then the hotkey.
    w.targets.set_monitors(vec![display()]);
    w.ctl.show();
    assert_eq!(
        w.ctl.restore_anchor(),
        place(FLOATING),
        "where it was left on the monitor that is still there"
    );

    // No monitor it was ever on is left: it opens on the one the pointer is on, at the size
    // it had, inside that monitor.
    let mut settings = before.ctl.settings().clone();
    settings
        .panel_geometry
        .retain(|g| g.monitor_id == second_display().id);
    let mut w = world_with(Some(outlook()), settings);
    w.targets.add_monitor(second_display());
    assert_eq!(w.ctl.start_hidden(), [Effect::HidePanel]);
    w.targets.set_monitors(vec![display()]);
    w.targets.set_cursor(Some((500, 300)));
    w.ctl.show();
    assert_eq!(
        w.ctl.restore_anchor(),
        place(frame::clamp(there, &display()))
    );
    assert!(
        display().holds(1440, 200),
        "the fixture is on the first monitor"
    );
}

#[test]
fn the_tray_click_tucks_an_open_dock_and_opens_a_tucked_one() {
    use snipdeck_platform::tray::TrayEvent;
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    assert_eq!(
        w.ctl.on_tray(TrayEvent::TogglePanel),
        place(handle(DockEdge::Right))
    );
    assert!(w.ctl.visible(), "the tab stays");
    assert!(w.ctl.dock_live());
    assert_eq!(
        w.ctl.on_tray(TrayEvent::TogglePanel),
        [
            Effect::PlaceWindow(open_rect(DockEdge::Right)),
            Effect::ShowPanel
        ]
    );
    assert!(!w.ctl.view().dock_collapsed);
}

#[test]
fn a_panel_docked_by_an_older_version_stays_open_until_it_is_docked_again() {
    // What an older version left: docked (its default, or its dock took the Panel), and the
    // trigger off because that was the default then.
    let mut settings = Settings::default();
    settings.dock.trigger = DockTrigger::Off;
    settings.dock.tucks_by_default = false;
    settings
        .panel_geometry
        .push(open_rect(DockEdge::Right).geometry(&display(), Some(DockEdge::Right)));
    let mut w = world_with(Some(outlook()), settings);
    assert_eq!(w.ctl.restore_frame(&display()).len(), 1);
    assert!(!w.ctl.dock_live(), "it stays open, as it did");
    w.targets.set_cursor(Some((1000, 300)));
    advance(&w, 60_000);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    assert!(!w.ctl.view().dock_collapsed);
    // Docked again by the user, it is this version's dock: it tucks.
    w.ctl.dock_to(Some(DockEdge::Left));
    assert_eq!(w.ctl.settings().dock.trigger, DockTrigger::Both);
    assert!(w.ctl.dock_live());
}

#[test]
fn a_full_screen_app_on_another_monitor_leaves_the_dock_alone() {
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    w.targets.add_monitor(display());
    w.targets.add_monitor(second_display());
    assert_eq!(w.ctl.close(), place(handle(DockEdge::Right)));
    // A video fills the second monitor while the user works on the first, where the dock is.
    w.targets.set_fullscreen(Some(&second_display()));
    w.targets.set_cursor(Some((1000, 300)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    assert!(w.ctl.view().topmost, "the tab stays above the windows here");
    w.targets.set_cursor(Some((1919, 300)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 150);
    assert_eq!(
        w.ctl.dock_tick(),
        place(open_rect(DockEdge::Right)),
        "and the pointer on it opens the Panel"
    );
    assert!(w.ctl.view().topmost);
}

#[test]
fn a_tab_on_an_edge_another_monitor_begins_at_reveals_from_just_past_it() {
    // Docked to the right edge of the first monitor, where the second begins: the pointer
    // does not stop at that edge, it runs on.
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    w.targets.add_monitor(display());
    w.targets.add_monitor(second_display());
    assert_eq!(w.ctl.close(), place(handle(DockEdge::Right)));
    w.targets.set_cursor(Some((1000, 300)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    // Stopped a few pixels onto the second monitor: as good as on the tab.
    w.targets.set_cursor(Some((1923, 300)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 150);
    assert_eq!(w.ctl.dock_tick(), place(open_rect(DockEdge::Right)));
    // Further in, it is on the other monitor's windows.
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    assert_eq!(w.ctl.close(), place(handle(DockEdge::Right)));
    w.targets.set_cursor(Some((1000, 300)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    w.targets.set_cursor(Some((1940, 300)));
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 5_000);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    assert!(w.ctl.view().dock_collapsed);
}

#[test]
fn a_dock_on_an_edge_another_monitor_begins_at_drags_straight_onto_that_monitor() {
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    w.targets.add_monitor(display());
    w.targets.add_monitor(second_display());
    press(&mut w, (1700, 110), open_rect(DockEdge::Right));
    // Straight to the right, over the edge it is docked to, with no pull back first.
    pull(&mut w, (1925, 110));
    assert_eq!(w.ctl.view().dock_edge, Some(DockEdge::Right), "a push");
    pull(&mut w, (2400, 300));
    assert!(w.ctl.view().dock_edge.is_none(), "it has left its edge");
    pull(&mut w, (3000, 500));
    let dropped = w.ctl.drag_end();
    let [Effect::PlaceWindow(at)] = dropped[..] else {
        panic!("one placement, got {dropped:?}");
    };
    assert!(
        second_display().holds(at.x, at.y),
        "it floats on the second monitor, at {at:?}"
    );
    assert_eq!(w.ctl.dock_edge(), None);
    assert_eq!(
        w.ctl
            .settings()
            .panel_geometry
            .iter()
            .find(|g| g.monitor_id == display().id)
            .and_then(|g| g.edge),
        None,
        "and is no longer docked where it came from"
    );
}

/// What an older version stored for a Panel docked to `edge`: the docked frame, as tall as
/// the work area, as the Panel's only size, and no place along the edge.
fn old_full_height_dock(edge: DockEdge) -> World {
    let mut settings = Settings::default();
    settings.dock.trigger = DockTrigger::Both;
    settings
        .panel_geometry
        .push(crate::settings::PanelGeometry {
            monitor_id: display().id,
            x: if edge == DockEdge::Left { 0 } else { 1600 },
            y: 0,
            w: 320,
            h: 1040,
            edge: Some(edge),
            ..crate::settings::PanelGeometry::default()
        });
    world_with(Some(outlook()), settings)
}

#[test]
fn undocking_a_dock_an_older_version_stored_full_height_gives_a_panel_of_ordinary_height() {
    // From the menu.
    let mut w = old_full_height_dock(DockEdge::Right);
    let opened = w.ctl.restore_frame(&display());
    let [Effect::PlaceWindow(open)] = opened[..] else {
        panic!("one placement, got {opened:?}");
    };
    assert_eq!((open.w, open.h), (320, 480), "it opens compact");
    w.ctl.dock_to(None);
    let floated = w.ctl.take_pending();
    let [Effect::PlaceWindow(loose)] = floated[..] else {
        panic!("one placement, got {floated:?}");
    };
    assert_eq!(
        (loose.w, loose.h),
        (320, 480),
        "and floats at the Panel's ordinary height, not as tall as the screen"
    );
    assert_eq!(w.ctl.settings().panel_geometry[0].h, 480);

    // By pulling it off its edge.
    let mut w = old_full_height_dock(DockEdge::Right);
    let opened = w.ctl.restore_frame(&display());
    let [Effect::PlaceWindow(open)] = opened[..] else {
        panic!("one placement, got {opened:?}");
    };
    press(&mut w, (1700, open.y + 10), open);
    let pulled = pull(&mut w, (1500, open.y + 10));
    let [Effect::PlaceWindow(loose)] = pulled[..] else {
        panic!("one placement, got {pulled:?}");
    };
    assert_eq!((loose.w, loose.h), (320, 480));
    w.ctl.drag_end();
    assert_eq!(w.ctl.settings().panel_geometry[0].h, 480);
}

#[test]
fn sliding_a_dock_an_older_version_stored_full_height_keeps_it_compact() {
    let mut w = old_full_height_dock(DockEdge::Left);
    let opened = w.ctl.restore_frame(&display());
    let [Effect::PlaceWindow(open)] = opened[..] else {
        panic!("one placement, got {opened:?}");
    };
    assert_eq!((open.w, open.h), (320, 480));
    press(&mut w, (100, open.y + 10), open);
    assert_eq!(
        pull(&mut w, (100, open.y + 60)),
        place(Rect {
            y: open.y + 50,
            ..open
        }),
        "it slides at the height it opened with"
    );
    w.ctl.drag_end();
    let stored = &w.ctl.settings().panel_geometry[0];
    assert_eq!(stored.docked, Some((320, 480)));
    assert_eq!(stored.along, Some(open.y + 50));
    assert_eq!(
        stored.h, 480,
        "and its floating height is the ordinary one too"
    );

    // The same from a file whose floating size was kept apart: only the docked size was full.
    let mut settings = Settings::default();
    settings.dock.trigger = DockTrigger::Both;
    settings
        .panel_geometry
        .push(crate::settings::PanelGeometry {
            docked: Some((340, 1040)),
            along: None,
            ..FLOATING.geometry(&display(), Some(DockEdge::Left))
        });
    let mut w = world_with(Some(outlook()), settings);
    let opened = w.ctl.restore_frame(&display());
    let [Effect::PlaceWindow(open)] = opened[..] else {
        panic!("one placement, got {opened:?}");
    };
    assert_eq!((open.w, open.h), (340, FLOATING.h));
    press(&mut w, (100, open.y + 10), open);
    assert_eq!(
        pull(&mut w, (100, open.y + 60)),
        place(Rect {
            y: open.y + 50,
            ..open
        })
    );
    w.ctl.drag_end();
    assert_eq!(
        w.ctl.settings().panel_geometry[0].docked,
        Some((340, FLOATING.h))
    );
}

#[test]
fn a_dock_an_older_version_stored_full_height_is_left_at_an_ordinary_height_when_dragged_away() {
    let mut w = old_full_height_dock(DockEdge::Right);
    w.targets.add_monitor(display());
    w.targets.add_monitor(second_display());
    let opened = w.ctl.restore_frame(&display());
    let [Effect::PlaceWindow(open)] = opened[..] else {
        panic!("one placement, got {opened:?}");
    };
    press(&mut w, (1700, open.y + 10), open);
    pull(&mut w, (2400, 300));
    pull(&mut w, (3000, 500));
    w.ctl.drag_end();
    let left = w
        .ctl
        .settings()
        .panel_geometry
        .iter()
        .find(|g| g.monitor_id == display().id)
        .expect("the place on the monitor it left");
    assert_eq!(left.edge, None);
    assert_eq!(
        left.h, 480,
        "back on that monitor it floats at an ordinary height, not as tall as the screen"
    );
}

#[test]
fn a_full_height_docked_size_an_older_version_stored_does_not_come_back_on_the_next_dock() {
    // The floating size was kept apart; only the docked size was as tall as the screen.
    let mut settings = Settings::default();
    settings.dock.trigger = DockTrigger::Both;
    settings
        .panel_geometry
        .push(crate::settings::PanelGeometry {
            docked: Some((340, 1040)),
            along: None,
            ..FLOATING.geometry(&display(), Some(DockEdge::Left))
        });
    let mut w = world_with(Some(outlook()), settings);
    w.ctl.restore_frame(&display());
    w.ctl.dock_to(None);
    assert_eq!(w.ctl.take_pending(), place(FLOATING));
    w.ctl.dock_to(Some(DockEdge::Left));
    let docked = w.ctl.take_pending();
    let [Effect::PlaceWindow(open)] = docked[..] else {
        panic!("one placement, got {docked:?}");
    };
    assert_eq!(
        (open.w, open.h),
        (340, FLOATING.h),
        "the full edge is off: the dock is as tall as the Panel floats"
    );
    assert_eq!(
        w.ctl.settings().panel_geometry[0].docked,
        Some((340, FLOATING.h))
    );
}

#[test]
fn a_resize_lasts_as_long_as_the_windows_own_size_loop() {
    // The button reads as up throughout (a window of higher privilege is in front), and the
    // grip is pulled past the widest the Panel goes: no size report comes. The window's own
    // size loop still says that the resize goes on.
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    w.targets.set_primary_down(Some(false));
    // The system starts the loop a moment after it is asked to.
    w.targets.set_size_loop(Some(false));
    assert_eq!(
        w.ctl.resize(frame::Edge::Left),
        vec![Effect::ResizeWindow(frame::Edge::Left)]
    );
    w.targets.set_cursor(Some((1300, 300)));
    advance(&w, 50);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    w.targets.set_size_loop(Some(true));
    for step in 2..=60_i64 {
        advance(&w, step * 50);
        assert_eq!(w.ctl.dock_tick(), Vec::new(), "step {step}");
    }
    assert!(
        !w.ctl.view().dock_collapsed,
        "three seconds without a size report, and still out"
    );
    // The grip is let go: the loop ends, the hold with it, and the leave delay runs as ever.
    w.targets.set_size_loop(Some(false));
    advance(&w, 3_050);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 3_050 + 399);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 3_050 + 400);
    assert_eq!(w.ctl.dock_tick().len(), 1, "it tucks");
    assert!(w.ctl.view().dock_collapsed);
}

#[test]
fn a_resize_holds_the_dock_out_where_the_button_always_reads_as_up() {
    // Under a window of higher privilege Windows reports every button as up, a held grip
    // too. The window's own reports of its new size then say that the resize goes on.
    let mut w = docked(DockTrigger::Both, DockEdge::Right);
    w.targets.set_primary_down(Some(false));
    assert_eq!(
        w.ctl.resize(frame::Edge::Left),
        vec![Effect::ResizeWindow(frame::Edge::Left)]
    );
    w.targets.set_cursor(Some((1300, 300)));
    let open = open_rect(DockEdge::Right);
    for step in 1..=40_u32 {
        advance(&w, i64::from(step) * 50);
        if step % 4 == 0 {
            let wider = Rect {
                x: open.x - i32::try_from(step).unwrap_or(0),
                w: open.w + step,
                ..open
            };
            assert_eq!(w.ctl.frame_changed(&display(), wider), Vec::new());
        }
        assert_eq!(w.ctl.dock_tick(), Vec::new(), "step {step}");
    }
    assert!(
        !w.ctl.view().dock_collapsed,
        "not tucked in the middle of a resize"
    );
    // The reports stop (the grip was let go): a moment later the hold is over, and the leave
    // delay runs as ever.
    advance(&w, 2_000 + 999);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    assert!(!w.ctl.dock_needs_tick(), "still held: no leave delay runs");
    advance(&w, 2_000 + 1_000);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 2_000 + 1_399);
    assert_eq!(w.ctl.dock_tick(), Vec::new());
    advance(&w, 2_000 + 1_400);
    assert_eq!(w.ctl.dock_tick().len(), 1, "it tucks");
    assert!(w.ctl.view().dock_collapsed);
}
