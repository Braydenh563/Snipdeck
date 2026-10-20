//! The tray, the foreground, blur hiding and closing windows.

use super::*;

// ---- The tray ----------------------------------------------------------------------------

pub(super) fn with_tray(w: &mut World) {
    w.ctl.on_tray(TrayEvent::IconPresent(true));
}

#[test]
fn the_tray_toggles_the_panel_and_open_panel_always_shows_it() {
    let mut w = world();
    assert_eq!(w.ctl.on_tray(TrayEvent::TogglePanel), [Effect::HidePanel]);
    assert!(!w.ctl.visible());
    w.targets.set(None);
    assert_eq!(w.ctl.on_tray(TrayEvent::TogglePanel), [Effect::ShowPanel]);
    assert_eq!(
        w.ctl.view().footer,
        Footer::Copies,
        "the Target is read when the tray shows the Panel"
    );
    assert_eq!(w.ctl.on_tray(TrayEvent::OpenPanel), [Effect::ShowPanel]);
    assert!(w.ctl.visible());
}

#[test]
fn the_tray_menu_opens_snipdeck_and_quits() {
    let mut w = world();
    assert_eq!(
        w.ctl.on_tray(TrayEvent::OpenSnipdeck),
        [Effect::OpenMainWindow]
    );
    assert_eq!(w.ctl.on_tray(TrayEvent::Quit), [Effect::Quit]);
    assert!(w.ctl.visible(), "quitting does not pretend to hide first");
}

#[test]
fn pause_hotkeys_asks_the_host_and_silences_hotkeys_that_were_already_queued() {
    let mut w = world();
    assert_eq!(
        w.ctl.on_tray(TrayEvent::HotkeysPaused(true)),
        [Effect::SetHotkeysPaused(true)]
    );
    assert!(w.ctl.hotkeys_paused());
    assert_eq!(w.ctl.on_hotkey(HotkeyAction::TogglePanel), Vec::new());
    assert_eq!(w.ctl.on_hotkey(HotkeyAction::Picker), Vec::new());
    assert!(w.ctl.visible());
    assert_eq!(
        w.ctl.on_tray(TrayEvent::HotkeysPaused(false)),
        [Effect::SetHotkeysPaused(false)]
    );
    assert_eq!(
        w.ctl.on_hotkey(HotkeyAction::TogglePanel),
        [Effect::HidePanel]
    );
}

#[test]
fn a_second_launch_opens_the_main_window_when_the_panel_is_already_up() {
    let mut w = world();
    assert_eq!(w.ctl.second_launch(), [Effect::OpenMainWindow]);
    w.ctl.hide();
    assert_eq!(w.ctl.second_launch(), [Effect::ShowPanel]);
    assert!(w.ctl.visible());
}

#[test]
fn closing_the_panel_hides_it_when_a_tray_icon_can_bring_it_back() {
    let mut w = world();
    with_tray(&mut w);
    assert_eq!(w.ctl.close(), [Effect::HidePanel]);
    assert!(!w.ctl.visible());
    assert_eq!(w.ctl.on_tray(TrayEvent::TogglePanel), [Effect::ShowPanel]);
}

#[test]
fn closing_the_panel_quits_without_a_tray_icon_or_with_close_to_tray_off() {
    let mut w = world();
    assert_eq!(w.ctl.close(), [Effect::Quit], "no tray icon to return from");
    assert!(w.ctl.visible());

    let mut w = world_with(
        None,
        Settings {
            close_to_tray: false,
            ..Settings::default()
        },
    );
    with_tray(&mut w);
    assert_eq!(w.ctl.close(), [Effect::Quit]);
}

/// An unpinned Panel that has been up for a second, with the default settings.
fn unpinned_world(settings: Settings) -> World {
    let mut w = world_with(Some(outlook()), settings);
    w.ctl.set_on_top(false);
    w.clock.set(at("2026-10-17T10:30:01Z"));
    w
}

#[test]
fn an_unpinned_panel_hides_when_another_app_takes_the_foreground() {
    let mut w = unpinned_world(Settings::default());
    w.targets.set(Some(outlook()));
    assert_eq!(w.ctl.on_foreground_changed(), [Effect::HidePanel]);
    assert!(!w.ctl.visible());
    // Hidden already: a later change has nothing to hide.
    assert_eq!(w.ctl.on_foreground_changed(), []);
}

#[test]
fn a_pinned_panel_is_topmost_unless_a_snipdeck_window_is_active() {
    let mut w = unpinned_world(Settings::default());
    assert!(!w.ctl.view().topmost, "unpinned: never topmost");
    w.ctl.set_on_top(true);
    assert!(w.ctl.view().topmost);
    w.ctl.set_own_window_active(true);
    assert!(!w.ctl.view().topmost, "Main can cover the Panel");
    assert!(w.ctl.view().on_top, "the pin button stays pressed");
    w.ctl.set_own_window_active(false);
    assert!(
        w.ctl.view().topmost,
        "back on top once focus is in another app"
    );
    w.ctl.set_on_top(false);
    w.ctl.set_own_window_active(true);
    w.ctl.set_own_window_active(false);
    assert!(!w.ctl.view().topmost);
}

fn reveal_first_row(w: &mut World) {
    let id = w.ids[0];
    make_sensitive(w, id);
    w.ctl.refresh_panel().expect("refresh");
    w.ctl.reveal_row(0, true);
    assert_eq!(w.ctl.view().revealed, Some(0), "held: the row is revealed");
}

#[test]
fn a_held_reveal_ends_when_the_foreground_changes() {
    // Alt+Tab away from a pinned Panel: the Space release goes to the other window.
    let mut w = unpinned_world(Settings::default());
    w.ctl.set_on_top(true);
    reveal_first_row(&mut w);
    assert_eq!(w.ctl.on_foreground_changed(), []);
    assert_eq!(w.ctl.view().revealed, None);
    assert!(w.ctl.visible(), "pinned: the Panel stays, hidden text");
}

#[test]
fn a_held_reveal_ends_when_a_snipdeck_window_takes_the_focus() {
    let mut w = unpinned_world(Settings::default());
    w.ctl.set_on_top(true);
    reveal_first_row(&mut w);
    w.ctl.set_own_window_active(true);
    assert_eq!(w.ctl.view().revealed, None);
}

#[test]
fn a_held_reveal_ends_when_the_panel_hides() {
    let mut w = unpinned_world(Settings::default());
    reveal_first_row(&mut w);
    w.ctl.hide();
    assert_eq!(w.ctl.view().revealed, None);
}

#[test]
fn a_pinned_panel_stays() {
    let mut w = unpinned_world(Settings::default());
    w.ctl.set_on_top(true);
    assert_eq!(w.ctl.on_foreground_changed(), []);
    assert!(w.ctl.visible());
}

#[test]
fn with_the_setting_off_the_panel_stays() {
    let mut w = unpinned_world(Settings {
        panel_hide_on_blur: false,
        ..Settings::default()
    });
    assert_eq!(w.ctl.on_foreground_changed(), []);
    assert!(w.ctl.visible());
}

#[test]
fn the_tray_or_desktop_taking_the_foreground_does_not_hide_the_panel() {
    let mut w = unpinned_world(Settings::default());
    // The tracker reports no Target for the tray, the taskbar or the desktop.
    w.targets.set(None);
    assert_eq!(w.ctl.on_foreground_changed(), []);
    assert!(w.ctl.visible());
}

#[test]
fn a_foreground_change_right_after_show_is_ignored() {
    let mut w = unpinned_world(Settings::default());
    w.ctl.hide();
    w.clock.set(at("2026-10-17T10:31:00Z"));
    assert_eq!(w.ctl.show(), [Effect::ShowPanel]);
    w.clock.set(at("2026-10-17T10:31:00.400Z"));
    assert_eq!(
        w.ctl.on_foreground_changed(),
        [],
        "400 ms is still the grace"
    );
    w.clock.set(at("2026-10-17T10:31:00.401Z"));
    assert_eq!(w.ctl.on_foreground_changed(), [Effect::HidePanel]);
}

#[test]
fn the_picker_open_blocks_the_hide() {
    let mut w = unpinned_world(Settings::default());
    w.ctl.open_picker();
    assert_eq!(w.ctl.on_foreground_changed(), []);
    assert!(w.ctl.visible());
}

#[test]
fn a_foreground_change_still_updates_the_footer() {
    let mut w = unpinned_world(Settings {
        panel_hide_on_blur: false,
        ..Settings::default()
    });
    w.targets.set(None);
    let _ = w.ctl.on_foreground_changed();
    assert_eq!(w.ctl.view().footer, Footer::Copies);
}

#[test]
fn closing_the_main_window_quits_when_keep_running_is_off() {
    let mut w = world_with(
        Some(outlook()),
        Settings {
            close_to_tray: false,
            ..Settings::default()
        },
    );
    with_tray(&mut w);
    assert!(w.ctl.main_window_closed());
}

#[test]
fn closing_the_main_window_does_not_quit_with_a_tray_icon() {
    let mut w = world();
    with_tray(&mut w);
    w.ctl.hide();
    assert!(!w.ctl.main_window_closed());
}

#[test]
fn closing_the_main_window_quits_without_a_tray_when_the_panel_is_hidden() {
    let mut w = world();
    assert!(!w.ctl.main_window_closed(), "the Panel is still up");
    w.ctl.hide();
    assert!(
        w.ctl.main_window_closed(),
        "nothing could bring a window back"
    );
}

#[test]
fn real_tray_clicks_on_the_fake_reach_the_controller() {
    use snipdeck_platform::{testing::FakeTray, tray::Tray, tray::TrayEntry};
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let mut tray = FakeTray::new("Snipdeck", move |e| {
        let _ = tx.lock().expect("lock").send(e);
    });
    let mut w = world();

    // Drain what the user did to the icon into the controller, as the app's UI thread does.
    let pump = |w: &mut World| {
        rx.try_iter()
            .flat_map(|event| w.ctl.on_tray(event))
            .collect::<Vec<_>>()
    };
    tray.show().expect("show");
    assert_eq!(pump(&mut w), [], "the icon appearing needs no effect");
    assert_eq!(w.ctl.close(), [Effect::HidePanel], "but now closing hides");
    assert_eq!(w.ctl.show(), [Effect::ShowPanel]);
    tray.left_click();
    assert_eq!(pump(&mut w), [Effect::HidePanel]);
    tray.left_click();
    assert_eq!(pump(&mut w), [Effect::ShowPanel]);
    assert!(tray.right_click().is_some());
    tray.pick(TrayEntry::PauseHotkeys);
    assert_eq!(pump(&mut w), [Effect::SetHotkeysPaused(true)]);
    assert!(tray.hotkeys_paused(), "the menu check follows");
    tray.pick(TrayEntry::OpenSnipdeck);
    tray.pick(TrayEntry::Quit);
    assert_eq!(pump(&mut w), [Effect::OpenMainWindow, Effect::Quit]);
}

/// A controller wired to a `FakeTray` the way the app wires the real one: every event the
/// tray emits is queued and then handed to `on_tray`.
struct Wired {
    w: World,
    tray: snipdeck_platform::testing::FakeTray,
    rx: std::sync::mpsc::Receiver<TrayEvent>,
}

impl Wired {
    fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let tray = snipdeck_platform::testing::FakeTray::new("Snipdeck", move |e| {
            let _ = tx.lock().expect("lock").send(e);
        });
        Self {
            w: world(),
            tray,
            rx,
        }
    }

    fn pump(&mut self) {
        for event in self.rx.try_iter() {
            self.w.ctl.on_tray(event);
        }
    }

    fn close(&mut self) -> Vec<Effect> {
        self.pump();
        self.w.ctl.close()
    }
}

#[test]
fn a_failed_first_show_then_explorer_starting_lets_close_hide() {
    use snipdeck_platform::tray::Tray;
    let mut t = Wired::new();
    t.tray.set_fail_show(true);
    let mut tray = t.tray.clone();
    assert!(tray.show().is_err());
    assert_eq!(
        t.close(),
        [Effect::Quit],
        "no icon yet: nothing to return from"
    );

    t.tray.set_fail_show(false);
    t.tray.taskbar_created();
    assert!(t.tray.visible());
    assert_eq!(t.close(), [Effect::HidePanel], "the icon appeared later");
}

#[test]
fn explorer_restarting_without_taking_the_icon_back_makes_close_quit() {
    use snipdeck_platform::tray::Tray;
    let mut t = Wired::new();
    let mut tray = t.tray.clone();
    tray.show().expect("show");
    assert_eq!(t.close(), [Effect::HidePanel]);

    // Explorer restarts and the icon cannot be added again: no icon, so Esc must not hide
    // the Panel into nothing.
    t.tray.set_fail_show(true);
    t.tray.taskbar_created();
    assert!(!t.tray.visible());
    assert_eq!(t.close(), [Effect::Quit]);

    t.tray.set_fail_show(false);
    t.tray.taskbar_created();
    assert_eq!(t.close(), [Effect::HidePanel], "back again");
}

#[test]
fn hiding_the_icon_makes_close_quit() {
    use snipdeck_platform::tray::Tray;
    let mut t = Wired::new();
    let mut tray = t.tray.clone();
    tray.show().expect("show");
    tray.hide().expect("hide");
    assert_eq!(t.close(), [Effect::Quit]);
}
