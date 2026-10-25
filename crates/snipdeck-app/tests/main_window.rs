//! The real `MainWindow`, `ListEditor` and `BoardEditor` bound to a `Controller` over the demo
//! library and the platform fakes, driven by the window's callbacks, key events and pointer
//! clicks through Slint's testing backend. Time is mocked, never the real clock. One process
//! for this file (the testing platform is process-wide).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::{ComponentHandle, Model, platform::Key};
use snipdeck_app::{
    binding::Binding,
    controller::{Controller, Effect, Services, TOAST_ENTER, TOAST_EXIT, TOAST_UNDO_HOLD},
    demo,
    host::{FakeTargets, FixedClock},
    main_binding::{MainBinding, MainUi},
    native_window::{self, Surface},
    onboarding_binding::OnboardingUi,
    settings::{Hotkeys, Settings},
    settings_binding::SettingsHost,
};
use snipdeck_app::{commands::Command, host::HotkeyProblem};
use snipdeck_core::{Entitlements, Tier, Timestamp};
use snipdeck_platform::{
    testing::{FakeClipboard, FakeKeyboard, FakeTiming},
    tray::TrayEvent,
};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, Store, TagRepo, TrashRepo};
use snipdeck_ui::{ItemAction, MainSection, MainView, MainWindow, PanelWindow, SelectMode};

#[path = "support/mod.rs"]
mod support;
use support::{chord, key, on_ui_thread, press, type_text};

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;
type Bound = MainBinding<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

struct Rig {
    ui: MainWindow,
    ctl: Rc<RefCell<Ctl>>,
    binding: Bound,
}

/// A row index the window reported as an `int`.
fn ix(i: i32) -> usize {
    usize::try_from(i).expect("a row index")
}

fn now() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

fn controller(tier: Tier) -> Rc<RefCell<Ctl>> {
    controller_on(demo::store(now()).expect("demo store"), tier)
}

fn controller_on(store: Store, tier: Tier) -> Rc<RefCell<Ctl>> {
    Rc::new(RefCell::new(
        Controller::new(
            store,
            Services {
                clipboard: FakeClipboard {
                    asks_after_polls: Some(1),
                    ..FakeClipboard::default()
                },
                keyboard: FakeKeyboard::default(),
                timing: FakeTiming::default(),
            },
            Box::new(FakeTargets::new(Some(demo::target()))),
            Box::new(FixedClock::new(now())),
            Settings::default(),
            Entitlements::new(tier),
        )
        .expect("controller"),
    ))
}

fn rig() -> Rig {
    rig_tier(Tier::Pro)
}

fn rig_tier(tier: Tier) -> Rig {
    rig_with(controller(tier))
}

/// A rig over the demo library with the tags `invoices` and `billing` added.
fn rig_with_tags() -> Rig {
    let mut store = demo::store(now()).expect("demo store");
    for name in ["invoices", "billing"] {
        store
            .insert_tag(&snipdeck_core::Tag::new(
                snipdeck_core::Name::new(name).unwrap(),
            ))
            .unwrap();
    }
    rig_with(controller_on(store, Tier::Pro))
}

fn rig_with(ctl: Rc<RefCell<Ctl>>) -> Rig {
    let ui = MainWindow::new().expect("main window");
    ui.window().set_size(slint::LogicalSize::new(1100.0, 720.0));
    let binding = Controller::bind_main(&ctl, &ui);
    ui.show().expect("show");
    slint::platform::update_timers_and_animations();
    Rig { ui, ctl, binding }
}

impl Rig {
    fn window(&self) -> &slint::Window {
        self.ui.window()
    }

    fn titles(&self) -> Vec<String> {
        self.ui
            .get_rows()
            .iter()
            .map(|r| r.title.to_string())
            .collect()
    }

    fn list_names(&self) -> Vec<String> {
        self.ui
            .get_lists()
            .iter()
            .map(|l| l.label.to_string())
            .collect()
    }

    /// Opens a list of the sidebar the way the window does: it sets its own section, then asks.
    fn open_list(&self, index: i32) {
        self.ui.set_section(MainSection::List);
        self.ui.set_section_index(index);
        self.ui.invoke_section_selected(MainSection::List, index);
    }

    fn open(&self, section: MainSection) {
        self.ui.set_section(section);
        self.ui.invoke_section_selected(section, 0);
    }

    fn row(&self, title: &str) -> i32 {
        i32::try_from(
            self.titles()
                .iter()
                .position(|t| t == title)
                .unwrap_or_else(|| panic!("no row {title}")),
        )
        .unwrap()
    }

    fn button(&self, label: &str) -> ElementHandle {
        element(&self.ui, label, AccessibleRole::Button)
    }

    fn trashed(&self) -> usize {
        self.ctl.borrow().store().trashed().unwrap().len()
    }
}

/// A real left click at the centre of `element`, through Slint's hit testing, then a timer tick.
fn click(window: &slint::Window, element: &ElementHandle) {
    let at = element.absolute_position();
    let size = element.size();
    let position = slint::LogicalPosition::new(at.x + size.width / 2.0, at.y + size.height / 2.0);
    let button = slint::platform::PointerEventButton::Left;
    window.dispatch_event(slint::platform::WindowEvent::PointerMoved { position });
    window.dispatch_event(slint::platform::WindowEvent::PointerPressed { position, button });
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(50));
    window.dispatch_event(slint::platform::WindowEvent::PointerReleased { position, button });
    i_slint_backend_testing::mock_elapsed_time(Duration::from_secs(1));
    slint::platform::update_timers_and_animations();
}

fn element<C: ComponentHandle>(ui: &C, label: &str, role: AccessibleRole) -> ElementHandle {
    ElementHandle::find_by_accessible_label(ui, label)
        .find(|e| e.accessible_role() == Some(role))
        .unwrap_or_else(|| panic!("no {role:?} labelled {label:?}"))
}

// ---- Opening ---------------------------------------------------------------------------------

#[test]
fn the_panel_list_menu_opens_the_list_editor_without_the_main_window() {
    on_ui_thread(|| {
        let ctl = controller(Tier::Pro);
        let main_ui = MainUi::new(&ctl);
        assert!(main_ui.list_editor().is_none());
        let tab = ctl
            .borrow_mut()
            .view()
            .tabs
            .iter()
            .position(|t| t.name == "Code")
            .expect("Code tab");
        ctl.borrow_mut().open_list_menu(tab);
        let effects = ctl.borrow_mut().choose_list_menu(0);
        assert_eq!(effects, [Effect::OpenListEditor]);
        main_ui.bind().expect("bind");
        let editor = main_ui.list_editor().expect("list editor window");
        assert!(editor.window().is_visible());
        assert!(
            !main_ui.window().expect("bound").window().is_visible(),
            "the Main window itself stays hidden"
        );
        // Opening Main afterwards still works.
        main_ui.open().expect("open");
        assert!(main_ui.window().expect("main").window().is_visible());
    });
}

#[test]
fn open_snipdeck_from_the_tray_creates_the_window_once_and_shows_it_again_after_it_is_closed() {
    on_ui_thread(|| {
        let ctl = controller(Tier::Pro);
        let main_ui = MainUi::new(&ctl);
        assert!(
            main_ui.window().is_none(),
            "nothing is created before it is asked for"
        );

        let effects = ctl.borrow_mut().on_tray(TrayEvent::OpenSnipdeck);
        assert_eq!(effects, [Effect::OpenMainWindow]);
        main_ui.open().expect("open");
        let ui = main_ui.window().expect("created");
        assert!(ui.window().is_visible());
        assert_eq!(ui.get_count_all(), 11);
        assert_eq!(ui.get_board_name(), "Work");

        // A mark on the first window proves the second open reuses it.
        ui.set_dark(true);
        ui.window().hide().unwrap();
        assert!(!ui.window().is_visible());
        ctl.borrow_mut().on_tray(TrayEvent::OpenSnipdeck);
        main_ui.open().expect("open again");
        let again = main_ui.window().expect("same window");
        assert!(again.window().is_visible());
        assert!(again.get_dark(), "the same window, not a new one");
    });
}

#[test]
fn the_window_shows_the_demo_library_sections_lists_and_counts() {
    on_ui_thread(|| {
        let r = rig();
        assert_eq!(r.ui.get_board_name(), "Work");
        assert_eq!(
            (
                r.ui.get_count_all(),
                r.ui.get_count_pinned(),
                r.ui.get_count_recent(),
                r.ui.get_count_trash()
            ),
            (11, 3, 0, 0)
        );
        assert_eq!(r.list_names(), ["Email replies", "Addresses", "Code"]);
        assert_eq!(r.ui.get_lists().row_data(0).unwrap().count, 6);
        assert_eq!(r.ui.get_rows().row_count(), 11);
        assert_eq!(r.ui.get_section(), MainSection::All);
        assert!(!r.ui.get_rows().row_data(0).unwrap().list_name.is_empty());
    });
}

#[test]
fn choosing_a_system_view_or_a_list_changes_the_rows() {
    on_ui_thread(|| {
        let r = rig();
        r.open(MainSection::Pinned);
        assert_eq!(
            r.titles(),
            ["Thanks, received", "Home address", "Retry helper"]
        );
        r.open_list(1);
        assert_eq!(r.titles(), ["Home address", "Office"]);
        assert_eq!(r.ui.get_section_index(), 1);
        r.open(MainSection::Recent);
        assert_eq!(r.titles().len(), 0);
    });
}

#[test]
fn a_click_on_a_sidebar_list_opens_it() {
    on_ui_thread(|| {
        let r = rig();
        let addresses = element(&r.ui, "Addresses", AccessibleRole::ListItem);
        click(r.window(), &addresses);
        assert_eq!(r.ui.get_section(), MainSection::List);
        assert_eq!(r.titles(), ["Home address", "Office"]);
    });
}

#[test]
fn typing_in_the_search_box_searches_every_board() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.invoke_focus_search();
        type_text(r.window(), "milk");
        assert_eq!(r.ui.get_query(), "milk");
        assert_eq!(r.titles(), ["Shopping list"], "found on the other board");
        assert_eq!(r.ui.get_rows().row_data(0).unwrap().list_name, "Errands");
        r.open_list(0);
        assert_eq!(r.ui.get_query(), "");
        assert_eq!(r.ui.get_rows().row_count(), 6);
    });
}

// ---- Selection and item actions --------------------------------------------------------------

#[test]
fn a_selection_change_updates_the_rows_in_place_instead_of_rebuilding_the_model() {
    on_ui_thread(|| {
        let r = rig();
        // A handle to the model the list shows now: it only sees the change if the model is kept.
        let held = r.ui.get_rows();
        let count = held.row_count();
        assert!(count > 4);
        r.ui.invoke_select_requested(1, SelectMode::Replace);
        r.ui.invoke_select_requested(3, SelectMode::Extend);
        let selected: Vec<bool> = held.iter().map(|row| row.selected).collect();
        assert_eq!(&selected[..5], [false, true, true, true, false]);
        assert_eq!(held.row_count(), count);
        r.ui.invoke_clear_selection_requested();
        assert!(held.iter().all(|row| !row.selected));
    });
}

#[test]
fn clicks_and_keys_build_the_selection() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.invoke_select_requested(1, SelectMode::Replace);
        r.ui.invoke_select_requested(3, SelectMode::Extend);
        assert_eq!(r.ui.get_selected_count(), 3);
        let selected: Vec<bool> = r.ui.get_rows().iter().map(|row| row.selected).collect();
        assert_eq!(&selected[..5], [false, true, true, true, false]);
        r.ui.invoke_clear_selection_requested();
        assert_eq!(r.ui.get_selected_count(), 0);

        r.ui.invoke_focus_items();
        chord(r.window(), &[Key::Control], "a");
        assert_eq!(r.ui.get_selected_count(), 11);
        press(r.window(), &key(Key::Escape));
        assert_eq!(r.ui.get_selected_count(), 0);
        // The cursor stays where the last click left it (row 3); arrows move and select.
        press(r.window(), &key(Key::DownArrow));
        press(r.window(), &key(Key::DownArrow));
        assert_eq!(r.ui.get_cursor(), 5);
        assert_eq!(r.ui.get_selected_count(), 1);
    });
}

#[test]
fn ctrl_p_pins_the_row_at_the_cursor() {
    on_ui_thread(|| {
        let r = rig();
        let at = r.row("Out of office");
        r.ui.invoke_select_requested(at, SelectMode::Replace);
        r.ui.invoke_focus_items();
        chord(r.window(), &[Key::Control], "p");
        assert_eq!(r.ui.get_count_pinned(), 4);
        assert!(r.ui.get_rows().row_data(ix(at)).unwrap().pinned);
        chord(r.window(), &[Key::Control], "p");
        assert_eq!(r.ui.get_count_pinned(), 3);
    });
}

#[test]
fn use_pastes_the_item_through_the_controller_and_counts_it() {
    on_ui_thread(|| {
        let r = rig();
        let at = r.row("Out of office");
        r.ui.invoke_item_action(ItemAction::Use, at);
        let id = match r.ctl.borrow_mut().main_screen().rows[ix(at)].key {
            snipdeck_app::main_window::Key::Item(id) => id,
            other => panic!("{other:?}"),
        };
        assert_eq!(r.ctl.borrow().store().item(id).unwrap().use_count, 1);
        assert_eq!(r.ui.get_toast_text(), "Pasted into Outlook");
    });
}

#[test]
fn move_asks_for_a_list_and_sends_the_selected_rows_there() {
    on_ui_thread(|| {
        let r = rig();
        let a = r.row("Out of office");
        r.ui.invoke_select_requested(a, SelectMode::Replace);
        r.ui.invoke_select_requested(a + 1, SelectMode::Extend);
        r.ui.invoke_item_action(ItemAction::Move, a);
        r.ui.invoke_move_chosen(2); // Code
        assert_eq!(r.ui.get_toast_text(), "Moved 2 items to Code");
        assert_eq!(r.ui.get_lists().row_data(2).unwrap().count, 4);
        r.open_list(2);
        assert_eq!(
            r.titles(),
            [
                "Retry helper",
                "Recent customers",
                "Out of office",
                "Meeting follow-up"
            ]
        );
    });
}

#[test]
fn holding_a_sensitive_row_reveals_its_text_and_releasing_hides_it() {
    on_ui_thread(|| {
        let r = rig();
        let at = r.row("Server password");
        assert_eq!(r.ui.get_rows().row_data(ix(at)).unwrap().preview, "");
        r.ui.invoke_reveal_requested(at, true);
        assert_eq!(r.ui.get_revealed_index(), at);
        assert_eq!(
            r.ui.get_rows().row_data(ix(at)).unwrap().preview,
            "hunter2-correct-horse"
        );
        r.ui.invoke_reveal_requested(at, false);
        assert_eq!(r.ui.get_revealed_index(), -1);
        assert_eq!(r.ui.get_rows().row_data(ix(at)).unwrap().preview, "");
    });
}

// ---- Delete, Trash, Undo ---------------------------------------------------------------------

#[test]
fn delete_trashes_at_once_and_the_toasts_undo_button_brings_the_item_back() {
    on_ui_thread(|| {
        let r = rig();
        let at = r.row("Out of office");
        r.ui.invoke_select_requested(at, SelectMode::Replace);
        r.ui.invoke_focus_items();
        press(r.window(), &key(Key::Delete));
        assert!(!r.titles().contains(&"Out of office".to_owned()));
        assert_eq!(r.ui.get_count_all(), 10);
        assert_eq!(r.ui.get_count_trash(), 1);
        assert_eq!(r.ui.get_toast_text(), "Moved 1 item to the Trash");
        assert_eq!(r.ui.get_toast_undo_label(), "Undo");

        // A real pointer click on the toast's Undo button.
        i_slint_backend_testing::mock_elapsed_time(TOAST_ENTER);
        click(r.window(), &r.button("Undo"));
        assert!(r.titles().contains(&"Out of office".to_owned()));
        assert_eq!(r.ui.get_count_trash(), 0);
        assert_eq!(r.ui.get_toast_text(), "Restored 1 item");
        assert_eq!(r.ui.get_toast_undo_label(), "");
    });
}

#[test]
fn the_undo_toast_stays_five_seconds_then_the_item_waits_in_the_trash() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.invoke_item_action(ItemAction::Delete, 0);
        assert_eq!(r.ui.get_toast_phase(), snipdeck_ui::ToastPhase::Enter);
        i_slint_backend_testing::mock_elapsed_time(TOAST_ENTER);
        assert_eq!(r.ui.get_toast_phase(), snipdeck_ui::ToastPhase::Hold);
        // Past the 2.5 s of an ordinary toast, still undoable.
        i_slint_backend_testing::mock_elapsed_time(Duration::from_secs(3));
        assert_eq!(r.ui.get_toast_phase(), snipdeck_ui::ToastPhase::Hold);
        assert_eq!(r.ui.get_toast_undo_label(), "Undo");
        i_slint_backend_testing::mock_elapsed_time(
            TOAST_UNDO_HOLD.saturating_sub(Duration::from_secs(3)),
        );
        assert_eq!(r.ui.get_toast_phase(), snipdeck_ui::ToastPhase::Exit);
        i_slint_backend_testing::mock_elapsed_time(TOAST_EXIT);
        assert_eq!(r.ui.get_toast_text(), "");
        r.ui.invoke_toast_undo();
        assert_eq!(r.ui.get_count_trash(), 1, "too late for Undo");
    });
}

#[test]
fn the_panel_and_the_main_window_share_one_toast_and_one_timer() {
    on_ui_thread(|| {
        let ctl = controller(Tier::Pro);
        let panel = PanelWindow::new().expect("panel");
        let _panel_binding: Binding = Controller::bind(
            &ctl,
            &panel,
            std::rc::Rc::new(snipdeck_app::placement::NoPopups),
            |_| {},
        );
        panel.show().unwrap();
        let ui = MainWindow::new().expect("main");
        let _binding = Controller::bind_main(&ctl, &ui);
        ui.show().unwrap();

        ui.invoke_item_action(ItemAction::Delete, 0);
        assert_eq!(panel.get_toast_text(), "Moved 1 item to the Trash");
        assert_eq!(panel.get_rows().row_count(), 5, "the Panel list follows");
        // One timer chain: after 2.6 s an undo toast is still up in both windows (two chains
        // would have ticked it into its fade-out at 2.6 s).
        i_slint_backend_testing::mock_elapsed_time(TOAST_ENTER);
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(2480));
        assert_eq!(ui.get_toast_undo_label(), "Undo");
        assert_eq!(ui.get_toast_phase(), snipdeck_ui::ToastPhase::Hold);
        assert_eq!(panel.get_toast_text(), "Moved 1 item to the Trash");
        assert_eq!(panel.get_toast_phase(), snipdeck_ui::ToastPhase::Hold);
        i_slint_backend_testing::mock_elapsed_time(
            TOAST_UNDO_HOLD.saturating_sub(Duration::from_millis(2480)),
        );
        assert_eq!(ui.get_toast_phase(), snipdeck_ui::ToastPhase::Exit);
        assert_eq!(panel.get_toast_phase(), snipdeck_ui::ToastPhase::Exit);

        // A change made in the Panel shows in the Main window too.
        panel.invoke_add_list_requested();
        assert_eq!(ui.get_lists().row_count(), 4);
    });
}

#[test]
fn the_trash_view_restores_deletes_for_good_and_empties() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.invoke_select_requested(0, SelectMode::Replace);
        r.ui.invoke_select_requested(3, SelectMode::Extend);
        r.ui.invoke_item_action(ItemAction::Delete, 0);
        assert_eq!(r.ui.get_count_trash(), 4);
        r.open(MainSection::Trash);
        assert_eq!(r.ui.get_rows().row_count(), 4);

        r.ui.invoke_item_action(ItemAction::Restore, 0);
        assert_eq!((r.ui.get_count_all(), r.ui.get_count_trash()), (8, 3));
        r.ui.invoke_item_action(ItemAction::DeleteForever, 0);
        assert_eq!(r.ui.get_count_trash(), 2);
        assert_eq!(r.ui.get_toast_text(), "Deleted for good");
        assert_eq!(r.trashed(), 2);

        r.ui.invoke_empty_trash_confirmed();
        assert_eq!(r.ui.get_count_trash(), 0);
        assert_eq!(r.ui.get_rows().row_count(), 0);
        assert_eq!(r.trashed(), 0);
        assert_eq!(r.ui.get_toast_text(), "The Trash is empty");
    });
}

// ---- Lists -----------------------------------------------------------------------------------

#[test]
fn add_list_opens_the_editor_and_saving_adds_the_list() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.invoke_add_list_requested();
        let editor = r.binding.list_editor().expect("list editor");
        assert!(editor.window().is_visible());
        assert!(editor.get_is_new());
        assert!(!editor.get_can_delete());
        assert_eq!(
            editor.get_name(),
            "New list 1",
            "a numbered name to type over"
        );

        // An empty name is refused with the reason, and the editor stays.
        editor.set_name("".into());
        editor.invoke_edited();
        editor.invoke_save_requested();
        assert!(editor.window().is_visible());
        assert_eq!(editor.get_name_error(), "Give it a name.");
        editor.set_name("Snippets 2".into());
        editor.set_accent(6);
        editor.set_sort_index(1);
        editor.invoke_edited();
        assert_eq!(editor.get_name_error(), "", "editing clears the refusal");
        editor.invoke_save_requested();

        assert!(!editor.window().is_visible());
        assert_eq!(
            r.list_names(),
            ["Email replies", "Addresses", "Code", "Snippets 2"]
        );
        assert_eq!(r.ui.get_section(), MainSection::List);
        assert_eq!(r.ui.get_section_index(), 3);
    });
}

#[test]
fn add_list_works_again_after_the_editor_and_the_main_window_were_closed() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.invoke_add_list_requested();
        let editor = r.binding.list_editor().expect("list editor");
        assert!(editor.window().is_visible());
        editor.set_name("Half typed".into());

        // The window's X, not the Cancel button: the headless close path.
        editor
            .window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        assert!(!editor.window().is_visible());
        // Then the Main window closes too, and is opened again the way the tray does.
        r.ui.window().hide().unwrap();
        r.ui.window().show().unwrap();

        r.ui.invoke_add_list_requested();
        let editor = r.binding.list_editor().expect("list editor");
        assert!(editor.window().is_visible(), "the + works a second time");
        assert!(editor.get_is_new());
        assert_eq!(
            editor.get_name(),
            "New list 1",
            "a fresh form, not the half typed one"
        );
    });
}

#[test]
fn add_list_sizes_the_editor_to_its_content() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.invoke_add_list_requested();
        let editor = r.binding.list_editor().expect("list editor");
        let wanted = editor.get_wanted_height();
        let size = editor
            .window()
            .size()
            .to_logical(editor.window().scale_factor());
        assert!(wanted > 150.0 && wanted < 440.0, "{wanted}");
        assert!(
            (size.height - wanted).abs() < 1.0,
            "{} vs {wanted}",
            size.height
        );
        assert!((size.width - 460.0).abs() < 1.0, "{}", size.width);
    });
}

// ---- Tags ------------------------------------------------------------------------------------

fn tag_labels(r: &Rig) -> Vec<String> {
    r.ui.get_tags()
        .iter()
        .map(|t| t.label.to_string())
        .collect()
}

#[test]
fn the_tags_gear_opens_the_tag_manager_with_the_tags_and_counts() {
    on_ui_thread(|| {
        let r = rig_with_tags();
        assert!(
            r.binding.tag_manager().is_none(),
            "nothing before it is asked for"
        );
        r.ui.invoke_manage_tags_requested();
        let manager = r.binding.tag_manager().expect("tag manager");
        assert!(manager.window().is_visible());
        assert_eq!(manager.get_tags().row_count(), r.ui.get_tags().row_count());
        assert_eq!(manager.get_tags().row_count(), 2);
        let names: Vec<String> = manager
            .get_tags()
            .iter()
            .map(|t| t.label.to_string())
            .collect();
        assert_eq!(names, tag_labels(&r));

        // Closed with its X and opened again from the menu entry: the same window comes back.
        manager
            .window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        assert!(!manager.window().is_visible());
        r.ui.invoke_manage_tags_requested();
        assert!(manager.window().is_visible());
    });
}

#[test]
fn renaming_in_the_tag_manager_updates_the_sidebar() {
    on_ui_thread(|| {
        let r = rig_with_tags();
        r.ui.invoke_manage_tags_requested();
        let manager = r.binding.tag_manager().unwrap();
        manager.set_editing(0);
        manager.invoke_rename_requested(0, "Receipts".into());
        assert!(
            tag_labels(&r).contains(&"Receipts".to_owned()),
            "{:?}",
            tag_labels(&r)
        );
        assert_eq!(manager.get_rename_error(), "");
        assert_eq!(manager.get_editing(), -1, "the field closes");
        let in_manager: Vec<String> = manager
            .get_tags()
            .iter()
            .map(|t| t.label.to_string())
            .collect();
        assert_eq!(in_manager, tag_labels(&r));
    });
}

#[test]
fn a_clashing_rename_shows_the_error_and_keeps_the_editor() {
    on_ui_thread(|| {
        let r = rig_with_tags();
        let before = tag_labels(&r);
        r.ui.invoke_manage_tags_requested();
        let manager = r.binding.tag_manager().unwrap();
        manager.set_editing(0);
        manager.invoke_rename_requested(0, before[1].to_uppercase().into());
        assert_eq!(
            manager.get_rename_error(),
            "A tag with that name already exists."
        );
        assert_eq!(manager.get_editing(), 0, "the field stays open");
        assert_eq!(tag_labels(&r), before);

        manager.invoke_rename_requested(0, "".into());
        assert_eq!(manager.get_rename_error(), "Give it a name.");
        // A good name clears the message and closes the field.
        manager.invoke_rename_requested(0, "Receipts".into());
        assert_eq!(manager.get_rename_error(), "");
        assert_eq!(manager.get_editing(), -1);
    });
}

#[test]
fn deleting_in_the_tag_manager_asks_then_removes_the_tag() {
    on_ui_thread(|| {
        let r = rig_with_tags();
        let before = tag_labels(&r);
        r.ui.invoke_manage_tags_requested();
        let manager = r.binding.tag_manager().unwrap();
        // The row's Delete only asks; the removal comes with the confirmation.
        manager.set_delete_index(0);
        assert_eq!(tag_labels(&r), before);
        manager.invoke_delete_confirmed(0);
        let after = tag_labels(&r);
        assert_eq!(after.len(), 1);
        assert!(!after.contains(&before[0]));
        assert_eq!(manager.get_tags().row_count(), 1, "the manager follows");
    });
}

#[test]
fn cancel_closes_the_editor_without_changes() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.invoke_add_list_requested();
        let editor = r.binding.list_editor().unwrap();
        editor.set_name("Nope".into());
        editor.invoke_cancel_requested();
        assert!(!editor.window().is_visible());
        assert_eq!(r.list_names().len(), 3);
    });
}

#[test]
fn edit_list_shows_its_fields_and_saving_renames_it() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.invoke_list_edit_requested(1);
        let editor = r.binding.list_editor().unwrap();
        assert!(editor.window().is_visible());
        assert!(!editor.get_is_new());
        assert_eq!(editor.get_name(), "Addresses");
        assert_eq!(editor.get_accent(), 3, "amber");
        assert_eq!(editor.get_sort_index(), 0);
        assert_eq!(editor.get_item_count(), 2);
        assert!(editor.get_can_delete());
        editor.set_name("Places".into());
        editor.invoke_save_requested();
        assert_eq!(r.list_names()[1], "Places");
        assert!(!editor.window().is_visible());
    });
}

#[test]
fn deleting_a_list_names_the_item_count_trashes_it_and_offers_undo() {
    on_ui_thread(|| {
        let r = rig();
        assert_eq!(r.ui.get_lists().row_data(0).unwrap().count, 6);
        r.ui.invoke_list_delete_confirmed(0);
        assert_eq!(r.list_names(), ["Addresses", "Code"]);
        assert_eq!(r.ui.get_count_all(), 5);
        assert_eq!(
            r.ui.get_toast_text(),
            "Deleted list \u{201c}Email replies\u{201d}"
        );
        assert_eq!(r.ui.get_toast_undo_label(), "Undo");
        r.ui.invoke_toast_undo();
        assert_eq!(r.list_names(), ["Email replies", "Addresses", "Code"]);
        assert_eq!(r.ui.get_count_all(), 11);
    });
}

#[test]
fn the_editors_delete_goes_through_its_own_confirmation_with_the_item_count() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.invoke_list_edit_requested(2);
        let editor = r.binding.list_editor().unwrap();
        assert_eq!(editor.get_item_count(), 2);
        editor.invoke_delete_confirmed();
        assert!(!editor.window().is_visible());
        assert_eq!(r.list_names(), ["Email replies", "Addresses"]);
        assert_eq!(r.ui.get_toast_undo_label(), "Undo");
    });
}

// ---- Boards ----------------------------------------------------------------------------------

#[test]
fn the_board_menu_lists_boards_and_actions_and_switches_boards() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.invoke_board_menu_requested();
        assert!(r.ui.get_board_menu_open());
        let entries: Vec<String> =
            r.ui.get_board_menu_entries()
                .iter()
                .map(|e| e.text.to_string())
                .collect();
        assert_eq!(
            entries,
            [
                "Work",
                "Personal",
                "",
                "New board\u{2026}",
                "Rename board\u{2026}",
                "Move board up",
                "Move board down",
                "Delete board\u{2026}"
            ]
        );
        assert!(r.ui.get_board_menu_entries().row_data(2).unwrap().separator);
        assert!(
            r.ui.get_board_menu_entries().row_data(5).unwrap().disabled,
            "first board"
        );
        assert!(
            r.ui.get_board_menu_entries()
                .row_data(7)
                .unwrap()
                .destructive
        );
        r.ui.invoke_board_menu_activated(1);
        assert!(!r.ui.get_board_menu_open());
        assert_eq!(r.ui.get_board_name(), "Personal");
        assert_eq!(r.list_names(), ["Errands"]);
        r.ui.invoke_board_menu_requested();
        r.ui.invoke_board_menu_dismissed();
        assert!(!r.ui.get_board_menu_open());
    });
}

#[test]
fn new_board_rename_and_delete_use_the_board_editor() {
    on_ui_thread(|| {
        let r = rig();
        // New board.
        r.ui.invoke_board_menu_requested();
        r.ui.invoke_board_menu_activated(3);
        let editor = r.binding.board_editor().expect("board editor");
        assert!(editor.window().is_visible());
        assert!(editor.get_is_new());
        editor.set_name("Side projects".into());
        editor.invoke_save_requested();
        assert!(!editor.window().is_visible());
        assert_eq!(r.ui.get_board_name(), "Side projects");
        assert_eq!(r.ctl.borrow().store().boards().unwrap().len(), 3);

        // Rename it.
        r.ui.invoke_board_menu_requested();
        r.ui.invoke_board_menu_activated(4 + 1);
        assert!(editor.window().is_visible());
        assert_eq!(editor.get_name(), "Side projects");
        editor.set_name("Projects".into());
        editor.invoke_save_requested();
        assert_eq!(r.ui.get_board_name(), "Projects");

        // Delete it: only the Main window's confirmation asks, naming the item count; the
        // editor stays shut (O52).
        r.ui.invoke_board_menu_requested();
        r.ui.invoke_board_menu_activated(4 + 4);
        assert!(!editor.window().is_visible());
        assert!(r.ui.get_board_delete_open());
        assert_eq!(r.ui.get_board_delete_count(), 0);
        r.ui.invoke_board_delete_confirmed();
        assert!(!r.ui.get_board_delete_open());
        assert!(!editor.window().is_visible());
        assert_eq!(r.ctl.borrow().store().boards().unwrap().len(), 2);
        assert_eq!(
            r.ui.get_toast_text(),
            "Deleted board \u{201c}Projects\u{201d}"
        );
        r.ui.invoke_toast_undo();
        assert_eq!(r.ctl.borrow().store().boards().unwrap().len(), 3);
    });
}

#[test]
fn the_free_plan_stops_a_sixth_board_with_a_toast() {
    on_ui_thread(|| {
        let r = rig_tier(Tier::Free);
        let new_board_entry = |r: &Rig| {
            r.ui.get_board_menu_entries()
                .iter()
                .position(|e| e.text.starts_with("New board"))
                .unwrap()
        };
        // Two boards in the demo; add the third, fourth and fifth.
        for name in ["Third", "Fourth", "Fifth"] {
            r.ui.invoke_board_menu_requested();
            r.ui.invoke_board_menu_activated(i32::try_from(new_board_entry(&r)).unwrap());
            r.binding.board_editor().unwrap().set_name(name.into());
            r.binding.board_editor().unwrap().invoke_save_requested();
        }
        assert_eq!(r.ctl.borrow().store().boards().unwrap().len(), 5);

        r.ui.invoke_board_menu_requested();
        r.ui.invoke_board_menu_activated(i32::try_from(new_board_entry(&r)).unwrap());
        assert!(!r.binding.board_editor().unwrap().window().is_visible());
        assert_eq!(
            r.ui.get_toast_text(),
            "The Free plan includes 5 boards. Supporter adds unlimited boards."
        );
        assert_eq!(r.ctl.borrow().store().boards().unwrap().len(), 5);
    });
}

#[test]
fn the_last_board_cannot_be_deleted_and_its_menu_entry_is_off() {
    on_ui_thread(|| {
        let r = rig();
        // Leave one board.
        r.ui.invoke_board_menu_requested();
        r.ui.invoke_board_menu_activated(3 + 4);
        assert!(r.ui.get_board_delete_open());
        r.ui.invoke_board_delete_confirmed();
        assert_eq!(r.ctl.borrow().store().boards().unwrap().len(), 1);
        r.ui.invoke_board_menu_requested();
        let delete = r.ui.get_board_menu_entries().row_data(1 + 1 + 4).unwrap();
        assert!(delete.destructive && delete.disabled);
        r.ui.invoke_board_menu_activated(1 + 1 + 4);
        assert!(!r.ui.get_board_delete_open());
        assert!(
            r.binding
                .board_editor()
                .is_none_or(|e| !e.window().is_visible())
        );
        assert_eq!(r.ctl.borrow().store().boards().unwrap().len(), 1);
    });
}

#[test]
fn cancelling_the_board_delete_keeps_the_board() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.invoke_board_menu_requested();
        r.ui.invoke_board_menu_activated(3 + 4);
        r.ui.invoke_board_delete_dismissed();
        assert!(!r.ui.get_board_delete_open());
        assert_eq!(r.ctl.borrow().store().boards().unwrap().len(), 2);
    });
}

// ---- Reordering ------------------------------------------------------------------------------

#[test]
fn alt_down_and_alt_up_move_the_row_at_the_cursor_within_its_list() {
    on_ui_thread(|| {
        let r = rig();
        r.open_list(0);
        r.ui.invoke_select_requested(1, SelectMode::Replace);
        r.ui.invoke_focus_items();
        chord(r.window(), &[Key::Alt], &key(Key::DownArrow));
        assert_eq!(
            &r.titles()[..3],
            ["Thanks, received", "Out of office", "Invoice reminder"]
        );
        assert_eq!(r.ui.get_cursor(), 2);
        chord(r.window(), &[Key::Alt], &key(Key::UpArrow));
        chord(r.window(), &[Key::Alt], &key(Key::UpArrow));
        assert_eq!(r.titles()[0], "Invoice reminder");
        assert_eq!(r.ui.get_cursor(), 0);
        chord(r.window(), &[Key::Alt], &key(Key::UpArrow));
        assert_eq!(
            r.titles()[0],
            "Invoice reminder",
            "off the top does nothing"
        );
    });
}

#[test]
fn alt_arrows_on_a_sidebar_list_reorder_the_lists() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.set_section(MainSection::List);
        r.ui.invoke_list_reorder_requested(2, -1);
        assert_eq!(r.list_names(), ["Email replies", "Code", "Addresses"]);

        // The key itself: the sidebar's cursor on "Addresses" (index 6 of the flat list).
        let addresses = element(&r.ui, "Addresses", AccessibleRole::ListItem);
        click(r.window(), &addresses);
        // Tab lands on the sidebar with its cursor on the open list.
        for _ in 0..12 {
            if r.ui.get_sidebar_focused() {
                break;
            }
            press(r.window(), &key(Key::Tab));
        }
        assert!(r.ui.get_sidebar_focused());
        chord(r.window(), &[Key::Alt], &key(Key::UpArrow));
        assert_eq!(r.list_names(), ["Email replies", "Addresses", "Code"]);
        assert_eq!(r.ui.get_section_index(), 1, "the open list moved with it");
    });
}

#[test]
fn the_board_menu_moves_the_open_board_down_and_up() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.invoke_board_menu_requested();
        r.ui.invoke_board_menu_activated(3 + 3);
        let order = |r: &Rig| -> Vec<String> {
            r.ctl
                .borrow()
                .store()
                .boards()
                .unwrap()
                .iter()
                .map(|b| b.name.as_str().to_owned())
                .collect()
        };
        assert_eq!(order(&r), ["Personal", "Work"]);
        r.ui.invoke_board_menu_requested();
        assert!(
            r.ui.get_board_menu_entries().row_data(6).unwrap().disabled,
            "now last"
        );
        r.ui.invoke_board_menu_activated(3 + 2);
        assert_eq!(order(&r), ["Work", "Personal"]);
    });
}

// ---- The command palette ---------------------------------------------------------------------

/// A machine that does nothing: the tour window needs a host and these tests never reach it.
struct NoHost;

impl SettingsHost for NoHost {
    fn apply_hotkeys(&self, _hotkeys: &Hotkeys) -> Vec<HotkeyProblem> {
        Vec::new()
    }

    fn set_autostart(&self, _on: bool) -> Result<(), String> {
        Ok(())
    }

    fn set_panel_in_taskbar(&self, _on: bool) {}

    fn open_folder(&self, _folder: &Path) -> Result<(), String> {
        Ok(())
    }

    fn open_url(&self, _url: &str) -> Result<(), String> {
        Ok(())
    }

    fn documents_folder(&self) -> Option<PathBuf> {
        None
    }

    fn pick_import(
        &self,
        _kind: snipdeck_app::controller::ImportKind,
        _owner: Option<isize>,
    ) -> Option<PathBuf> {
        None
    }

    fn clipboard_history_supported(&self) -> bool {
        false
    }

    fn read_clipboard_history(&self) -> snipdeck_platform::clipboard_history::HistoryRead {
        snipdeck_platform::clipboard_history::HistoryRead::Unsupported
    }
}

#[test]
fn the_view_menu_and_the_palette_switch_the_theme_and_a_toast_says_so() {
    use snipdeck_app::settings::Theme;
    on_ui_thread(|| {
        let r = rig();
        assert_eq!(r.ui.get_theme_index(), 0, "the default follows Windows");

        r.button("View").invoke_accessible_default_action();
        slint::platform::update_timers_and_animations();
        element(&r.ui, "Theme: Dark", AccessibleRole::ListItem).invoke_accessible_default_action();
        slint::platform::update_timers_and_animations();
        assert_eq!(r.ctl.borrow().settings().theme, Theme::Dark);
        assert_eq!(r.ui.get_theme_index(), 2);
        assert!(r.ui.get_dark(), "the window follows the setting at once");
        assert_eq!(r.ui.get_toast_text(), "Theme: Dark");

        r.ui.invoke_command_requested(Command::ThemeLight.id());
        assert_eq!(r.ctl.borrow().settings().theme, Theme::Light);
        assert_eq!(r.ui.get_theme_index(), 1);
        assert!(!r.ui.get_dark());
        assert_eq!(r.ui.get_toast_text(), "Theme: Light");

        r.ui.invoke_command_requested(Command::ThemeSystem.id());
        assert_eq!(r.ctl.borrow().settings().theme, Theme::System);
        assert_eq!(r.ui.get_toast_text(), "Theme: System");
    });
}

#[test]
fn the_view_menu_and_the_palette_set_the_row_density_the_settings_page_shares() {
    use snipdeck_app::settings::RowDensity;
    on_ui_thread(|| {
        let r = rig();
        assert_eq!(r.ui.get_view(), MainView::Comfortable);

        r.button("View").invoke_accessible_default_action();
        slint::platform::update_timers_and_animations();
        element(
            &r.ui,
            "Main list density: Compact",
            AccessibleRole::ListItem,
        )
        .invoke_accessible_default_action();
        slint::platform::update_timers_and_animations();
        assert_eq!(r.ui.get_view(), MainView::Compact);
        assert_eq!(
            r.ctl.borrow().settings().row_density,
            RowDensity::Compact,
            "the View menu writes the setting"
        );

        r.ui.invoke_command_requested(Command::DensityComfortable.id());
        assert_eq!(r.ui.get_view(), MainView::Comfortable);
        assert_eq!(
            r.ctl.borrow().settings().row_density,
            RowDensity::Comfortable
        );

        // From tiles, the palette brings the list back at the chosen density.
        r.ui.invoke_command_requested(Command::ViewTile.id());
        assert_eq!(r.ui.get_view(), MainView::Tile);
        r.ui.invoke_command_requested(Command::DensityCompact.id());
        assert_eq!(r.ui.get_view(), MainView::Compact);
        assert_eq!(r.ctl.borrow().settings().row_density, RowDensity::Compact);
    });
}

#[test]
fn the_command_texts_have_one_entry_per_command() {
    on_ui_thread(|| {
        let r = rig();
        assert_eq!(
            snipdeck_app::command_binding::texts(&r.ui).len(),
            Command::ALL.len(),
            "`CommandTexts.all` (command_texts.slint) is in the order of `Command::ALL`"
        );
    });
}

#[test]
fn a_palette_id_runs_the_command_it_names() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.invoke_command_requested(Command::GoTrash.id());
        assert_eq!(r.ui.get_section(), MainSection::Trash);
        r.ui.invoke_command_requested(Command::GoAll.id());
        assert_eq!(r.ui.get_section(), MainSection::All);
        r.ui.invoke_command_requested(Command::DensityCompact.id());
        assert_eq!(r.ui.get_view(), MainView::Compact);
        r.ui.invoke_command_requested(Command::ViewTile.id());
        assert_eq!(r.ui.get_view(), MainView::Tile);
        r.ui.invoke_command_requested(Command::NewList.id());
        assert!(
            r.binding.list_editor().is_some(),
            "New list opens the list editor"
        );
    });
}

#[test]
fn an_unknown_palette_id_does_nothing() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.invoke_command_requested(-1);
        r.ui.invoke_command_requested(i32::try_from(Command::ALL.len()).unwrap());
        assert_eq!(r.ui.get_section(), MainSection::All);
        assert!(r.binding.list_editor().is_none());
    });
}

#[test]
fn the_search_command_moves_the_focus_to_the_search_field() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.invoke_focus_items();
        slint::platform::update_timers_and_animations();
        assert!(!r.ui.get_search_focused());
        r.ui.invoke_command_requested(Command::Search.id());
        slint::platform::update_timers_and_animations();
        assert!(r.ui.get_search_focused());
        assert!(
            r.ctl.borrow().tour_step().is_none(),
            "the controller was not asked to do anything else"
        );
    });
}

#[test]
fn the_tour_command_opens_the_welcome_tour_window() {
    on_ui_thread(|| {
        let r = rig();
        let tour = OnboardingUi::new(&r.ctl, Rc::new(NoHost));
        assert!(
            tour.window().is_none(),
            "not created before it is asked for"
        );
        assert!(r.ctl.borrow().tour_step().is_none());
        r.ui.invoke_command_requested(Command::Tour.id());
        assert!(r.ctl.borrow().tour_step().is_some(), "the tour runs");
        assert!(tour.window().expect("created").window().is_visible());
    });
}

#[test]
fn a_command_and_the_settings_shortcut_open_their_window_over_the_main_window() {
    on_ui_thread(|| {
        let r = rig();
        native_window::note_opener(Surface::Panel);
        r.ui.invoke_command_requested(Command::Settings.id());
        assert_eq!(native_window::last_opener(), Some(Surface::Main));

        native_window::note_opener(Surface::Panel);
        r.ui.invoke_settings_requested();
        assert_eq!(native_window::last_opener(), Some(Surface::Main));
    });
}

#[test]
fn opening_the_palette_fills_every_command_and_never_refills_an_open_one() {
    on_ui_thread(|| {
        let r = rig();
        assert!(!r.ui.get_palette_open());
        r.ui.invoke_open_palette();
        assert!(r.ui.get_palette_open());
        let offered = r.ui.get_palette_rows().row_count();
        assert!(offered > 0, "the palette lists the commands");
        assert!(
            (0..offered).any(|i| r.ui.get_palette_rows().row_data(i).unwrap().label == "Settings"),
            "Settings is one of them"
        );

        // The palette is up with the user's query in its field: a second request (the Panel's
        // Ctrl+K, say) must not reset the rows under it.
        r.ui.set_palette_rows(slint::ModelRc::default());
        r.ui.invoke_open_palette();
        assert!(r.ui.get_palette_open());
        assert_eq!(
            r.ui.get_palette_rows().row_count(),
            0,
            "the rows stay with the field's text"
        );
    });
}

#[test]
fn the_palette_search_filters_the_real_commands_by_name_synonym_and_new_words() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.invoke_open_palette();
        let labels = |r: &Rig| -> Vec<String> {
            let rows = r.ui.get_palette_rows();
            (0..rows.row_count())
                .map(|i| rows.row_data(i).unwrap().label.to_string())
                .collect()
        };
        let all = labels(&r).len();
        for (query, expected) in [
            ("density", "Main list density: Compact"),
            ("compact", "Main list density: Compact"),
            ("dark", "Theme: Dark"),
            ("paste next", "Paste next item"),
            ("select all", "Select all items"),
        ] {
            r.ui.invoke_palette_query_changed(query.into());
            let found = labels(&r);
            assert!(
                found.iter().any(|l| l == expected),
                "{query:?} finds {expected:?}: {found:?}"
            );
            assert!(found.len() < all, "{query:?} narrows the list");
        }
        r.ui.invoke_palette_query_changed("zzzz".into());
        assert_eq!(labels(&r), Vec::<String>::new());
        r.ui.invoke_palette_query_changed("".into());
        assert_eq!(
            labels(&r).len(),
            all,
            "an empty query lists everything again"
        );
    });
}

#[test]
fn the_palette_does_not_open_over_a_menu() {
    on_ui_thread(|| {
        let r = rig();
        r.button("Help").invoke_accessible_default_action();
        slint::platform::update_timers_and_animations();
        assert!(r.ui.get_menu_open());
        r.ui.invoke_open_palette();
        assert!(
            !r.ui.get_palette_open(),
            "a menu is open: the same rule as Ctrl+K"
        );
    });
}

#[test]
fn the_recent_section_lists_what_was_copied_and_its_buttons_reach_the_controller() {
    on_ui_thread(|| {
        let r = rig();
        {
            let mut ctl = r.ctl.borrow_mut();
            // The inbox is sealed with the library key, so it needs key protection.
            ctl.set_key_provider(Box::new(
                snipdeck_store::crypto::MemoryKeyProvider::new().expect("provider"),
            ));
            ctl.update_settings(|s| s.recent_history = true);
            for text in ["first copy", "second copy"] {
                ctl.on_clipboard_change(&snipdeck_app::recent::ClipboardObservation {
                    text: Some(text.to_owned()),
                    source_app: Some("notepad.exe".to_owned()),
                    ..Default::default()
                });
            }
        }
        r.binding.redraw();
        assert_eq!(r.ui.get_count_recent(), 2, "the sidebar count is real");
        assert_eq!(
            r.ui.get_recent_rows().row_count(),
            0,
            "rows only in the section"
        );
        r.ui.invoke_section_selected(MainSection::Recent, 0);
        let rows = r.ui.get_recent_rows();
        let previews: Vec<_> = rows.iter().map(|e| e.preview.to_string()).collect();
        assert_eq!(previews, ["second copy", "first copy"]);
        assert_eq!(rows.row_data(0).unwrap().app, "Notepad");
        assert!(
            r.ui.get_recent_targets().row_count() > 0,
            "lists to save to"
        );

        let id_of = |i: usize| rows.row_data(i).unwrap().id;
        r.ui.invoke_recent_copied(id_of(1));
        assert_eq!(r.ctl.borrow().services().clipboard.written.len(), 1);
        let saved_before: usize = {
            let ctl = r.ctl.borrow();
            ctl.store()
                .boards()
                .unwrap()
                .iter()
                .flat_map(|b| ctl.store().lists(b.id).unwrap())
                .map(|l| ctl.store().items(l.id).unwrap().len())
                .sum()
        };
        r.ui.invoke_recent_saved(id_of(0), 0);
        assert!(!r.ui.get_toast_text().is_empty(), "saving says so");
        {
            let ctl = r.ctl.borrow();
            let all: Vec<String> = ctl
                .store()
                .boards()
                .unwrap()
                .iter()
                .flat_map(|b| ctl.store().lists(b.id).unwrap())
                .flat_map(|l| ctl.store().items(l.id).unwrap())
                .map(|i| i.preview.clone())
                .collect();
            assert_eq!(all.len(), saved_before + 1, "an item was created");
            assert!(
                all.iter().any(|p| p == "second copy"),
                "with the entry's text: {all:?}"
            );
        }
        r.ui.invoke_recent_deleted(id_of(0));
        assert_eq!(r.ui.get_recent_rows().row_count(), 1);
        assert_eq!(r.ui.get_count_recent(), 1);
        r.ui.invoke_recent_cleared();
        assert_eq!(r.ui.get_recent_rows().row_count(), 0);
        assert_eq!(r.ui.get_count_recent(), 0);
    });
}

#[test]
fn the_history_switch_in_the_welcome_tour_turns_the_inbox_off_and_on() {
    on_ui_thread(|| {
        let r = rig();
        let tour = OnboardingUi::new(&r.ctl, Rc::new(NoHost));
        r.ui.invoke_command_requested(Command::Tour.id());
        let window = tour.window().expect("created");
        assert!(window.get_recent_history(), "on for a new install");
        assert!(r.ctl.borrow().settings().recent_history);
        window.invoke_recent_history_toggled(false);
        assert!(!r.ctl.borrow().settings().recent_history);
        assert!(!window.get_recent_history());
        window.invoke_recent_history_toggled(true);
        assert!(r.ctl.borrow().settings().recent_history);
    });
}
