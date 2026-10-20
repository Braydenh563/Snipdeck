//! The Panel menus, row actions and the window frame.

use super::*;

// ---- The Panel menu, row actions and window frame ----------------------------------------

fn app_menu_index(entry: AppMenuEntry) -> usize {
    APP_MENU.iter().position(|e| *e == entry).expect("in menu")
}

fn row_titles(w: &World) -> Vec<String> {
    w.ctl.view().rows.iter().map(|r| r.title.clone()).collect()
}

fn display() -> crate::frame::Monitor {
    crate::frame::Monitor {
        id: crate::frame::Monitor::id_of("DISPLAY1", 1920, 1080),
        work_area: Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        },
        taskbar: None,
        scale: 1.0,
    }
}

#[test]
fn the_app_menu_opens_and_closes_without_a_choice() {
    let mut w = world();
    assert_eq!(w.ctl.view().app_menu, None);
    w.ctl.open_app_menu();
    assert_eq!(w.ctl.view().app_menu, Some(APP_MENU.to_vec()));
    w.ctl.close_app_menu();
    assert_eq!(w.ctl.view().app_menu, None);
}

#[test]
fn opening_the_app_menu_closes_the_board_menu_and_hiding_closes_it() {
    let mut w = world();
    w.ctl.open_board_menu();
    w.ctl.open_app_menu();
    assert!(w.ctl.view().board_menu.is_none());
    assert!(w.ctl.view().app_menu.is_some());
    w.ctl.hide();
    assert!(w.ctl.view().app_menu.is_none());
}

#[test]
fn each_app_menu_entry_asks_the_host_for_its_thing_and_closes_the_menu() {
    let cases = [
        (AppMenuEntry::Open, Effect::OpenMainWindow),
        (AppMenuEntry::Commands, Effect::OpenCommands),
        (AppMenuEntry::NewSnippet, Effect::NewItem),
        (AppMenuEntry::Settings, Effect::OpenSettings),
        (
            AppMenuEntry::Troubleshoot,
            Effect::OpenHelp(HelpTopic::Troubleshoot),
        ),
        (AppMenuEntry::UserGuide, Effect::OpenHelp(HelpTopic::Guide)),
        (
            AppMenuEntry::Shortcuts,
            Effect::OpenHelp(HelpTopic::Shortcuts),
        ),
        (AppMenuEntry::About, Effect::OpenHelp(HelpTopic::About)),
        (AppMenuEntry::Quit, Effect::Quit),
    ];
    for (entry, effect) in cases {
        let mut w = world();
        w.ctl.open_app_menu();
        assert_eq!(
            w.ctl.choose_app_menu(app_menu_index(entry)),
            [effect],
            "{entry:?}"
        );
        assert!(w.ctl.view().app_menu.is_none(), "{entry:?} closes the menu");
    }
}

#[test]
fn pause_hotkeys_toggles_and_the_view_says_which_way_it_is() {
    let mut w = world();
    let pause = app_menu_index(AppMenuEntry::PauseHotkeys);
    assert!(!w.ctl.view().hotkeys_paused);
    assert_eq!(
        w.ctl.choose_app_menu(pause),
        [Effect::SetHotkeysPaused(true)]
    );
    assert!(w.ctl.view().hotkeys_paused);
    assert!(w.ctl.hotkeys_paused());
    assert_eq!(w.ctl.on_hotkey(HotkeyAction::TogglePanel), Vec::new());
    assert_eq!(
        w.ctl.choose_app_menu(pause),
        [Effect::SetHotkeysPaused(false)]
    );
    assert!(!w.ctl.view().hotkeys_paused);
}

#[test]
fn a_line_in_the_app_menu_does_nothing_and_a_missing_row_closes_it() {
    let mut w = world();
    w.ctl.open_app_menu();
    let line = app_menu_index(AppMenuEntry::Separator);
    assert_eq!(w.ctl.choose_app_menu(line), Vec::new());
    assert!(w.ctl.view().app_menu.is_some(), "a line is not a choice");
    assert_eq!(w.ctl.choose_app_menu(99), Vec::new());
    assert!(w.ctl.view().app_menu.is_none());
}

#[test]
fn edit_and_rename_ask_for_the_quick_editor_on_that_row() {
    let mut w = world();
    let id = w.ctl.model().rows()[1].id;
    assert_eq!(w.ctl.edit_row(1), [Effect::EditItem(id)]);
    assert_eq!(w.ctl.rename_row(1), [Effect::EditItem(id)]);
    assert_eq!(w.ctl.edit_row(99), Vec::new());
    assert_eq!(w.ctl.rename_row(99), Vec::new());
}

#[test]
fn deleting_a_row_trashes_it_with_an_undo_toast_that_brings_it_back() {
    let mut w = world();
    assert_eq!(row_titles(&w), ["One", "Two", "Three", "Greeting"]);
    w.ctl.delete_row(1);
    assert_eq!(row_titles(&w), ["One", "Three", "Greeting"]);
    let toast = w.ctl.view().toast.expect("toast");
    assert_eq!(toast.message, ToastMessage::ItemsTrashed { count: 1 });
    assert!(toast.undo, "5 s to undo");
    assert_eq!(w.ctl.store().trashed().expect("trash").len(), 1);

    w.ctl.main_undo();
    assert_eq!(row_titles(&w), ["One", "Two", "Three", "Greeting"]);
    assert_eq!(w.ctl.store().trashed().expect("trash"), Vec::new());
    let toast = w.ctl.view().toast.expect("toast");
    assert_eq!(toast.message, ToastMessage::Restored { count: 1 });
}

#[test]
fn the_item_menu_opens_on_a_row_and_closes_without_a_choice() {
    let mut w = world();
    assert_eq!(w.ctl.view().item_menu, None);
    w.ctl.open_item_menu(1, false);
    let entries = w.ctl.view().item_menu.expect("menu");
    assert!(entries.contains(&ItemMenuEntry::Pin { pinned: false }));
    w.ctl.close_item_menu();
    assert_eq!(w.ctl.view().item_menu, None);
    w.ctl.open_item_menu(99, false);
    assert_eq!(w.ctl.view().item_menu, None, "no such row");
}

#[test]
fn the_item_menu_edits_pins_and_trashes_the_row_it_was_opened_on() {
    let mut w = world();
    let id = w.ctl.model().rows()[1].id;
    let index_of = |w: &World, entry: ItemMenuEntry| {
        w.ctl
            .view()
            .item_menu
            .expect("menu")
            .iter()
            .position(|e| *e == entry)
            .expect("entry")
    };
    w.ctl.open_item_menu(1, false);
    let edit = index_of(&w, ItemMenuEntry::Edit);
    assert_eq!(w.ctl.choose_item_menu(edit), [Effect::EditItem(id)]);
    assert_eq!(w.ctl.view().item_menu, None);

    w.ctl.open_item_menu(1, false);
    let pin = index_of(&w, ItemMenuEntry::Pin { pinned: false });
    assert_eq!(w.ctl.choose_item_menu(pin), Vec::new());
    assert!(w.ctl.store().item(id).expect("item").pinned);
    w.ctl.open_item_menu(1, false);
    assert!(
        w.ctl
            .view()
            .item_menu
            .expect("menu")
            .contains(&ItemMenuEntry::Pin { pinned: true })
    );
    w.ctl.close_item_menu();

    w.ctl.open_item_menu(1, false);
    let trash = index_of(&w, ItemMenuEntry::Trash);
    assert_eq!(w.ctl.choose_item_menu(trash), Vec::new());
    assert_eq!(row_titles(&w), ["One", "Three", "Greeting"]);
    assert!(w.ctl.view().toast.expect("toast").undo);
}

#[test]
fn duplicate_puts_a_copy_after_the_row_from_the_menu_and_from_ctrl_d() {
    let mut w = world();
    w.ctl.open_item_menu(0, false);
    let entries = w.ctl.view().item_menu.expect("menu");
    let duplicate = entries
        .iter()
        .position(|e| *e == ItemMenuEntry::Duplicate)
        .expect("entry");
    assert_eq!(w.ctl.choose_item_menu(duplicate), Vec::new());
    assert_eq!(row_titles(&w), ["One", "One", "Two", "Three", "Greeting"]);
    assert_eq!(w.ctl.view().item_menu, None);

    w.ctl.duplicate_row(2);
    assert_eq!(
        row_titles(&w),
        ["One", "One", "Two", "Two", "Three", "Greeting"]
    );
    w.ctl.duplicate_row(99);
    assert_eq!(row_titles(&w).len(), 6, "no such row");
}

#[test]
fn a_line_in_the_item_menu_does_nothing_and_hiding_closes_it() {
    let mut w = world();
    w.ctl.open_item_menu(0, false);
    let line = w
        .ctl
        .view()
        .item_menu
        .expect("menu")
        .iter()
        .position(|e| *e == ItemMenuEntry::Separator)
        .expect("line");
    assert_eq!(w.ctl.choose_item_menu(line), Vec::new());
    assert!(w.ctl.view().item_menu.is_some(), "a line is not a choice");
    w.ctl.hide();
    assert_eq!(w.ctl.view().item_menu, None);
}

#[test]
fn deleting_a_row_that_is_not_there_changes_nothing() {
    let mut w = world();
    w.ctl.delete_row(99);
    assert_eq!(row_titles(&w).len(), 4);
    assert!(w.ctl.view().toast.is_none());
}

#[test]
fn a_resize_grip_asks_the_host_to_start_the_os_resize() {
    let mut w = world();
    assert_eq!(
        w.ctl.resize(Edge::BottomRight),
        [Effect::ResizeWindow(Edge::BottomRight)]
    );
}

#[test]
fn a_frame_wider_than_480_is_cut_remembered_and_the_host_told() {
    let mut w = world();
    let m = display();
    let asked = Rect {
        x: 100,
        y: 100,
        w: 1000,
        h: 600,
    };
    let cut = Rect { w: 480, ..asked };
    assert_eq!(w.ctl.frame_changed(&m, asked), [Effect::PlaceWindow(cut)]);
    assert_eq!(w.ctl.placement(&m), Some(cut));
    let stored = &w.ctl.settings().panel_geometry;
    assert_eq!(stored.len(), 1);
    assert_eq!(
        (stored[0].monitor_id.as_str(), stored[0].w, stored[0].h),
        (m.id.as_str(), 480, 600)
    );
}

#[test]
fn a_move_across_a_monitor_edge_is_remembered_but_not_pulled_back() {
    let mut w = world();
    let m = display();
    let straddling = Rect {
        x: 1800,
        y: 100,
        w: 400,
        h: 600,
    };
    assert_eq!(w.ctl.frame_moved(&m, straddling), Vec::new());
    let stored = &w.ctl.settings().panel_geometry;
    assert_eq!((stored[0].x, stored[0].y), (1800, 100));
    // Restoring fits it inside the work area; a resize still clamps and answers.
    let fitted = Rect {
        x: 1520,
        ..straddling
    };
    assert_eq!(w.ctl.placement(&m), Some(fitted));
    assert_eq!(
        w.ctl.frame_changed(&m, straddling),
        [Effect::PlaceWindow(fitted)]
    );
}

#[test]
fn a_frame_inside_the_limits_is_remembered_as_it_is_with_no_effect() {
    let mut w = world();
    let m = display();
    let frame = Rect {
        x: 50,
        y: 60,
        w: 300,
        h: 500,
    };
    assert_eq!(w.ctl.frame_changed(&m, frame), Vec::new());
    assert_eq!(w.ctl.placement(&m), Some(frame));
    let frame = Rect { w: 400, ..frame };
    assert_eq!(w.ctl.frame_changed(&m, frame), Vec::new());
    assert_eq!(w.ctl.settings().panel_geometry.len(), 1, "one per monitor");
    assert_eq!(w.ctl.placement(&m), Some(frame));
}

#[test]
fn a_frame_with_no_size_is_not_a_place_the_user_chose() {
    let mut w = world();
    let nothing = Rect {
        x: -32000,
        y: -32000,
        w: 0,
        h: 0,
    };
    assert_eq!(w.ctl.frame_changed(&display(), nothing), Vec::new());
    assert_eq!(w.ctl.settings().panel_geometry, Vec::new());
}

#[test]
fn each_monitor_keeps_its_own_place_and_restoring_uses_the_one_it_is_on() {
    let mut w = world();
    let first = display();
    let second = crate::frame::Monitor {
        id: crate::frame::Monitor::id_of("DISPLAY2", 1280, 1024),
        work_area: Rect {
            x: 1920,
            y: 0,
            w: 1280,
            h: 984,
        },
        taskbar: None,
        scale: 1.0,
    };
    let on_first = Rect {
        x: 10,
        y: 20,
        w: 300,
        h: 500,
    };
    let on_second = Rect {
        x: 2000,
        y: 30,
        w: 450,
        h: 700,
    };
    w.ctl.frame_changed(&first, on_first);
    w.ctl.frame_changed(&second, on_second);
    assert_eq!(w.ctl.restore_frame(&first), [Effect::PlaceWindow(on_first)]);
    assert_eq!(
        w.ctl.restore_frame(&second),
        [Effect::PlaceWindow(on_second)]
    );
    let third = crate::frame::Monitor {
        id: "DISPLAY3 800x600".into(),
        ..first
    };
    assert_eq!(w.ctl.restore_frame(&third), Vec::new(), "nothing stored");
}

#[test]
fn a_stored_place_is_fitted_to_a_monitor_that_has_become_smaller() {
    let mut w = world();
    w.ctl.update_settings(|s| {
        s.remember_panel_geometry(crate::settings::PanelGeometry {
            monitor_id: display().id,
            x: 1700,
            y: 900,
            w: 480,
            h: 900,
            edge: None,
        });
    });
    assert_eq!(
        w.ctl.placement(&display()),
        Some(Rect {
            x: 1440,
            y: 140,
            w: 480,
            h: 900
        }),
        "pulled back inside the work area"
    );
}

#[test]
fn a_remembered_frame_is_written_after_the_quiet_time() {
    let temp = tempfile::tempdir().expect("dir");
    let mut w = saving_world(temp.path());
    let frame = Rect {
        x: 50,
        y: 60,
        w: 300,
        h: 500,
    };
    w.ctl.frame_changed(&display(), frame);
    w.clock.set(at("2026-10-17T10:30:01Z"));
    w.ctl.save_settings_if_due();
    let loaded = crate::paths::load_settings(temp.path());
    assert_eq!(loaded.panel_geometry.len(), 1);
    assert_eq!(Rect::of(&loaded.panel_geometry[0]), frame);
}
