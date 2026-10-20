//! The real `PanelWindow` bound to a `Controller` over the demo library and the platform
//! fakes, driven by key events through Slint's testing backend. Time is mocked, never the real
//! clock. One process for this file (the testing platform is process-wide).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use std::{cell::RefCell, rc::Rc, time::Duration};

use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::{ComponentHandle, Model, platform::Key};
use snipdeck_app::{
    binding::Binding,
    controller::{Controller, Effect, HelpTopic, Services, TOAST_ENTER, TOAST_EXIT, TOAST_HOLD},
    demo,
    frame::{Edge, Monitor, Rect},
    host::{Clock, FakeTargets, FixedClock},
    settings::{DockEdge, DockTrigger, HotkeyAction, Settings, Theme},
    settings_store::{FileSink, SettingsSaver},
};
use snipdeck_core::{Entitlements, ItemId, Tier, Timestamp};
use snipdeck_platform::{
    keys::{Key as PlatformKey, KeyEvent},
    payload::Data,
    route::{Focus, Target},
    system_look::{HighContrast, SystemLook},
    testing::{FakeClipboard, FakeKeyboard, FakeTiming},
    tray::TrayEvent,
};
use snipdeck_store::{ItemRepo, Store};
use snipdeck_ui::PanelWindow;

#[path = "support/mod.rs"]
mod support;
use support::{chord, key, on_ui_thread, press, type_text};

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

struct Rig {
    ui: PanelWindow,
    ctl: Rc<RefCell<Ctl>>,
    binding: Binding,
    targets: FakeTargets,
    clock: FixedClock,
    effects: Rc<RefCell<Vec<Effect>>>,
}

fn now() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

fn app(exe: &str, title: &str) -> Target {
    Target {
        app: exe.into(),
        title: title.into(),
        elevated: false,
        focus: Focus::Caret,
    }
}

/// The Panel as it runs on Windows: a tray icon is showing, so closing hides.
fn rig() -> Rig {
    rig_with_tray(true)
}

fn rig_with_tray(tray: bool) -> Rig {
    rig_on(demo::store(now()).expect("demo store"), tray, Vec::new())
}

/// A first run on a real (empty) library, as `main` opens it: the starter board is made first.
fn first_run_rig() -> Rig {
    let mut store = Store::open_in_memory().expect("store");
    assert!(snipdeck_app::starter::ensure_board(&mut store, now()).expect("starter"));
    let copied =
        snipdeck_platform::payload::Payload::new(vec![Data::Text("Copied earlier".into())])
            .expect("payload");
    rig_on(store, true, vec![copied])
}

fn rig_on(store: Store, tray: bool, clipboard: Vec<snipdeck_platform::payload::Payload>) -> Rig {
    let targets = FakeTargets::new(Some(demo::target()));
    let services = Services {
        clipboard: FakeClipboard {
            asks_after_polls: Some(1),
            written: clipboard,
            ..FakeClipboard::default()
        },
        keyboard: FakeKeyboard::default(),
        timing: FakeTiming::default(),
    };
    let clock = FixedClock::new(now());
    let ctl = Controller::new(
        store,
        services,
        Box::new(targets.clone()),
        Box::new(clock.clone()),
        Settings::default(),
        Entitlements::new(Tier::Pro),
    )
    .expect("controller");
    let ctl = Rc::new(RefCell::new(ctl));
    let _ = ctl.borrow_mut().on_tray(TrayEvent::IconPresent(tray));
    let ui = PanelWindow::new().expect("panel");
    let effects: Rc<RefCell<Vec<Effect>>> = Rc::default();
    let log = Rc::clone(&effects);
    let binding = Controller::bind(&ctl, &ui, move |e| log.borrow_mut().push(*e));
    ui.show().expect("show");
    slint::platform::update_timers_and_animations();
    Rig {
        ui,
        ctl,
        binding,
        targets,
        clock,
        effects,
    }
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

    fn uses(&self, row: usize) -> u32 {
        let id: ItemId = self.ctl.borrow().model().rows()[row].id;
        self.ctl.borrow().store().item(id).expect("item").use_count
    }

    fn button(&self, label: &str) -> ElementHandle {
        ElementHandle::find_by_accessible_label(&self.ui, label)
            .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .unwrap_or_else(|| panic!("no button labelled {label:?}"))
    }
}

/// A real left click (pointer move, press, release) at the centre of `element`, the way the
/// winit backend delivers one, then a timer tick. Unlike `invoke_accessible_default_action` this
/// goes through Slint's hit testing, so a control covered by something else is not clicked.
fn click(window: &slint::Window, element: &ElementHandle) {
    let at = element.absolute_position();
    let size = element.size();
    let position = slint::LogicalPosition::new(at.x + size.width / 2.0, at.y + size.height / 2.0);
    let button = slint::platform::PointerEventButton::Left;
    window.dispatch_event(slint::platform::WindowEvent::PointerMoved { position });
    window.dispatch_event(slint::platform::WindowEvent::PointerPressed { position, button });
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(50));
    window.dispatch_event(slint::platform::WindowEvent::PointerReleased { position, button });
    // Move away so the next press is not a double click.
    i_slint_backend_testing::mock_elapsed_time(Duration::from_secs(1));
    slint::platform::update_timers_and_animations();
}

fn element(ui: &PanelWindow, label: &str, role: AccessibleRole) -> ElementHandle {
    ElementHandle::find_by_accessible_label(ui, label)
        .find(|e| e.accessible_role() == Some(role))
        .unwrap_or_else(|| panic!("no {role:?} labelled {label:?}"))
}

#[test]
fn pointer_clicks_on_every_header_control_reach_the_controller() {
    on_ui_thread(|| {
        let r = rig();
        r.window().set_size(slint::LogicalSize::new(320.0, 480.0));
        slint::platform::update_timers_and_animations();

        click(r.window(), &r.button("Keep panel on top"));
        assert!(
            !r.ctl.borrow().view().on_top,
            "the pin click reached the controller"
        );

        click(r.window(), &r.button("Board: Work"));
        assert!(
            r.ui.get_board_menu_open(),
            "the board switcher click opened the menu"
        );
        click(
            r.window(),
            &element(&r.ui, "Personal", AccessibleRole::ListItem),
        );
        assert!(!r.ui.get_board_menu_open());
        assert_eq!(
            r.ui.get_board_name(),
            "Personal",
            "the menu item click switched"
        );
        assert_eq!(
            *r.effects.borrow(),
            [],
            "no click on a control starts a window drag"
        );

        click(r.window(), &r.button("Close panel (Esc)"));
        assert!(!r.window().is_visible());
    });
}

#[test]
fn pointer_clicks_on_tabs_pinned_chips_and_rows_reach_the_controller() {
    on_ui_thread(|| {
        let r = rig();
        r.window().set_size(slint::LogicalSize::new(320.0, 480.0));
        slint::platform::update_timers_and_animations();

        click(
            r.window(),
            &element(&r.ui, "Addresses", AccessibleRole::Tab),
        );
        assert_eq!(
            r.titles(),
            ["Home address", "Office"],
            "the tab click switched lists"
        );

        let chip = r.ui.get_pinned().row_data(0).expect("a pinned chip").title;
        click(r.window(), &r.button(chip.as_str()));
        assert!(
            !r.ctl.borrow().services().clipboard.log.is_empty(),
            "the pinned chip click used the item"
        );
        assert_eq!(*r.effects.borrow(), []);
    });
}

#[test]
fn a_first_run_on_an_empty_library_has_a_board_and_working_controls() {
    on_ui_thread(|| {
        let r = first_run_rig();
        r.window().set_size(slint::LogicalSize::new(320.0, 480.0));
        slint::platform::update_timers_and_animations();
        assert_eq!(r.ui.get_board_name(), snipdeck_app::starter::STARTER_BOARD);

        let switcher = format!("Board: {}", snipdeck_app::starter::STARTER_BOARD);
        click(r.window(), &r.button(&switcher));
        assert_eq!(
            r.ui.get_board_menu_entries().row_count(),
            1,
            "the board menu lists the starter board"
        );
        click(
            r.window(),
            &element(
                &r.ui,
                snipdeck_app::starter::STARTER_BOARD,
                AccessibleRole::ListItem,
            ),
        );
        assert!(!r.ui.get_board_menu_open());

        // The empty list's "Save from clipboard" saves what the clipboard holds.
        click(r.window(), &r.button("Save from clipboard"));
        assert_eq!(r.titles(), ["Copied earlier"]);

        // The + tab adds a list and shows it.
        click(r.window(), &r.button("Add list"));
        assert_eq!(r.ui.get_tabs().row_count(), 2);
        assert_eq!(r.ui.get_current_tab(), 1);
        assert_eq!(*r.effects.borrow(), []);
    });
}

#[test]
fn the_panel_shows_the_demo_library() {
    on_ui_thread(|| {
        let r = rig();
        assert_eq!(r.ui.get_board_name(), "Work");
        assert_eq!(r.ui.get_tabs().row_count(), 3);
        assert_eq!(r.ui.get_pinned().row_count(), 3);
        assert_eq!(r.ui.get_rows().row_count(), 6);
        assert_eq!(r.titles()[0], "Thanks, received");
        assert_eq!(r.ui.get_target_name(), "Outlook");
        assert_eq!(r.ui.get_target_title(), "New message");
        assert_eq!(r.ui.get_toast_text(), "");
        let sensitive = r.ui.get_rows().row_data(5).expect("row");
        assert_eq!(sensitive.title, "Server password");
        assert_eq!(
            sensitive.preview, "",
            "a sensitive preview never reaches the window"
        );
    });
}

#[test]
fn holding_a_sensitive_row_reveals_its_text_and_releasing_hides_it() {
    on_ui_thread(|| {
        let r = rig();
        assert_eq!(r.ui.get_rows().row_data(5).unwrap().preview, "");
        r.ui.invoke_reveal_requested(5, true);
        assert_eq!(r.ui.get_revealed_index(), 5);
        assert_eq!(
            r.ui.get_rows().row_data(5).unwrap().preview,
            "hunter2-correct-horse"
        );
        r.ui.invoke_reveal_requested(5, false);
        assert_eq!(r.ui.get_revealed_index(), -1);
        assert_eq!(r.ui.get_rows().row_data(5).unwrap().preview, "");
        r.ui.invoke_reveal_requested(0, true);
        assert_eq!(r.ui.get_revealed_index(), -1, "only sensitive rows reveal");
    });
}

#[test]
fn the_stack_strip_shows_the_marks_and_its_buttons_start_and_end_the_stack() {
    on_ui_thread(|| {
        let r = rig();
        assert!(!r.ui.get_stack_visible());
        let id = r.ctl.borrow().view().rows[0].id;
        r.ctl.borrow_mut().toggle_stack_mark(id);
        r.ui.invoke_query_changed("".into());
        assert!(r.ui.get_stack_visible());
        assert_eq!(r.ui.get_stack_count(), 1);
        assert_eq!(
            r.ui.get_rows().row_data(0).expect("row").stack_position,
            1,
            "the marked card wears its place in the stack"
        );
        assert_eq!(r.ui.get_rows().row_data(1).expect("row").stack_position, 0);
        assert!(!r.ui.get_stack_running());
        r.ui.invoke_stack_started();
        assert!(r.ui.get_stack_running());
        r.ui.invoke_stack_ended();
        assert!(!r.ui.get_stack_visible());
    });
}

#[test]
fn a_context_menu_request_opens_the_item_menu_and_dismissing_closes_it() {
    on_ui_thread(|| {
        let r = rig();
        assert!(!r.ui.get_item_menu_open());
        r.ui.invoke_context_menu_requested(1, 10.0, 10.0);
        assert!(r.ui.get_item_menu_open());
        assert!(r.ui.get_item_menu_entries().row_count() >= 5);
        r.ui.invoke_item_menu_dismissed();
        assert!(!r.ui.get_item_menu_open());
        r.ui.invoke_pinned_context_menu_requested(0, 10.0, 10.0);
        assert!(r.ui.get_item_menu_open());
    });
}

#[test]
fn a_list_menu_request_opens_the_list_menu_and_dismissing_closes_it() {
    on_ui_thread(|| {
        let r = rig();
        assert!(!r.ui.get_list_menu_open());
        r.ui.invoke_list_menu_requested(1);
        assert!(r.ui.get_list_menu_open());
        r.ui.invoke_list_menu_dismissed();
        assert!(!r.ui.get_list_menu_open());
        r.ui.invoke_list_menu_requested(1);
        r.ui.invoke_list_menu_activated(0);
        assert!(!r.ui.get_list_menu_open(), "a choice closes the menu");
    });
}

#[test]
fn delete_on_the_list_menu_shows_only_the_panel_confirmation() {
    // O52: Delete asked inside the list editor, which then appeared behind the confirmation.
    on_ui_thread(|| {
        let r = rig();
        let lists = r.ui.get_tabs().row_count();
        r.ui.invoke_list_menu_requested(1);
        r.ui.invoke_list_menu_activated(1);
        assert!(!r.ui.get_list_menu_open());
        assert!(r.ui.get_list_delete_open(), "the Panel asks");
        assert!(!r.ui.get_list_delete_name().is_empty());
        assert!(
            r.ctl.borrow_mut().main_screen().editor.is_none(),
            "no list editor opens"
        );
        r.ui.invoke_list_delete_dismissed();
        assert!(!r.ui.get_list_delete_open());
        assert_eq!(r.ui.get_tabs().row_count(), lists, "Cancel keeps the list");

        r.ui.invoke_list_menu_requested(1);
        r.ui.invoke_list_menu_activated(1);
        r.ui.invoke_list_delete_confirmed();
        assert!(!r.ui.get_list_delete_open());
        assert_eq!(r.ui.get_tabs().row_count(), lists - 1);
        assert!(r.ctl.borrow_mut().main_screen().editor.is_none());
    });
}

#[test]
fn typing_in_the_search_filters_the_rows() {
    on_ui_thread(|| {
        let r = rig();
        type_text(r.window(), "retry");
        assert_eq!(r.ui.get_query(), "retry");
        assert_eq!(r.titles()[0], "Retry helper", "search spans lists");
        assert_eq!(r.ui.get_rows().row_data(0).expect("row").list_name, "Code");
        press(r.window(), &key(Key::Escape));
        assert_eq!(r.ui.get_query(), "", "Esc clears the search first");
        assert_eq!(r.ui.get_rows().row_count(), 6);
        assert!(r.window().is_visible(), "and does not hide the Panel");
    });
}

#[test]
fn pressing_3_pastes_the_third_item_and_counts_the_use() {
    on_ui_thread(|| {
        let r = rig();
        assert_eq!(r.uses(2), 0);
        press(r.window(), "3");
        {
            let ctl = r.ctl.borrow();
            let services = ctl.services();
            assert_eq!(
                services.clipboard.written[0].data(),
                [Data::Text(
                    "I'm away until Monday with limited email. For anything urgent, please call the office."
                        .into()
                )]
            );
            assert!(
                services.keyboard.sent[0].contains(&KeyEvent {
                    key: PlatformKey::V,
                    down: true
                }),
                "Ctrl+V reached the fake keyboard"
            );
        }
        assert_eq!(r.uses(2), 1);
        assert_eq!(r.ui.get_toast_text(), "Pasted into Outlook");
        assert_eq!(r.targets.refocus_count(), 1);
    });
}

#[test]
fn enter_pastes_the_selected_row_and_shift_enter_only_copies() {
    on_ui_thread(|| {
        let r = rig();
        // Row 2 is the template, which asks for answers; rows 1 and 3 are plain text.
        press(r.window(), &key(Key::DownArrow));
        press(r.window(), &key(Key::DownArrow));
        chord(r.window(), &[Key::Shift], &key(Key::Return));
        {
            let ctl = r.ctl.borrow();
            assert!(
                ctl.services().keyboard.sent.is_empty(),
                "no keys for a copy"
            );
            assert_eq!(ctl.services().clipboard.log, ["persistent:1"]);
        }
        assert_eq!(r.uses(2), 1);
        assert_eq!(r.ui.get_toast_text(), "Copied");
        press(r.window(), &key(Key::Return));
        assert_eq!(
            r.ctl.borrow().services().keyboard.sent.len(),
            1,
            "plain Enter pastes"
        );
    });
}

#[test]
fn escape_on_an_empty_search_hides_the_panel() {
    on_ui_thread(|| {
        let r = rig();
        press(r.window(), &key(Key::Escape));
        assert!(!r.window().is_visible());
        assert!(!r.ctl.borrow().visible());
        assert_eq!(*r.effects.borrow(), [Effect::HidePanel]);
    });
}

#[test]
fn the_close_button_hides_and_the_pin_button_reaches_the_controller() {
    on_ui_thread(|| {
        let r = rig();
        assert!(r.ctl.borrow().view().on_top);
        r.button("Keep panel on top")
            .invoke_accessible_default_action();
        assert!(!r.ctl.borrow().view().on_top);
        assert!(!r.ui.get_on_top());
        r.button("Close panel (Esc)")
            .invoke_accessible_default_action();
        assert!(!r.window().is_visible());
    });
}

#[test]
fn the_toggle_hotkey_hides_and_shows_the_panel() {
    on_ui_thread(|| {
        let r = rig();
        r.binding.hotkey(HotkeyAction::TogglePanel);
        assert!(!r.window().is_visible());
        r.targets.set(None);
        r.binding.hotkey(HotkeyAction::TogglePanel);
        assert!(r.window().is_visible());
        assert_eq!(
            r.ui.get_target_name(),
            "",
            "the Target is read when the Panel shows"
        );
        // The other hotkeys are handled inside the controller: no effect for the host.
        r.binding.hotkey(HotkeyAction::Picker);
        r.binding.hotkey(HotkeyAction::Capture);
        r.binding.hotkey(HotkeyAction::PasteNext);
        assert_eq!(*r.effects.borrow(), [Effect::HidePanel, Effect::ShowPanel]);
        assert!(r.ctl.borrow().picker().is_some());
    });
}

#[test]
fn a_second_launch_shows_a_hidden_panel() {
    on_ui_thread(|| {
        let r = rig();
        r.binding.hotkey(HotkeyAction::TogglePanel);
        assert!(!r.window().is_visible());
        r.binding.show();
        assert!(r.window().is_visible());
        // Run again while it is up: the Main window opens instead.
        r.binding.show();
        assert_eq!(r.effects.borrow().last(), Some(&Effect::OpenMainWindow));
    });
}

#[test]
fn a_target_change_updates_the_footer() {
    on_ui_thread(|| {
        let r = rig();
        r.targets.set(Some(app("notepad++.exe", "notes.txt")));
        r.binding.target_changed();
        assert_eq!(r.ui.get_target_name(), "Notepad++");
        assert_eq!(r.ui.get_target_title(), "notes.txt");
        r.targets.set(None);
        r.binding.target_changed();
        assert_eq!(r.ui.get_target_name(), "", "empty means a click copies");
    });
}

#[test]
fn picking_a_list_tab_shows_that_list() {
    on_ui_thread(|| {
        let r = rig();
        chord(r.window(), &[Key::Control], &key(Key::Tab));
        assert_eq!(r.ui.get_current_tab(), 1);
        assert_eq!(r.titles(), ["Home address", "Office"]);
        assert_eq!(
            r.ctl
                .borrow()
                .model()
                .list()
                .map(|l| l.name.as_str().to_owned()),
            Some("Addresses".into())
        );
    });
}

#[test]
fn the_board_menu_lists_boards_and_switches() {
    on_ui_thread(|| {
        let r = rig();
        r.button("Board: Work").invoke_accessible_default_action();
        assert!(r.ui.get_board_menu_open());
        assert_eq!(r.ui.get_board_menu_entries().row_count(), 2);
        r.ui.invoke_focus_search();
        r.ui.invoke_board_menu_activated(1);
        slint::platform::update_timers_and_animations();
        assert!(!r.ui.get_board_menu_open());
        assert_eq!(r.ui.get_board_name(), "Personal");
        assert_eq!(r.titles(), ["Shopping list"]);
    });
}

#[test]
fn a_template_that_needs_answers_opens_the_fill_form_and_changes_nothing() {
    on_ui_thread(|| {
        let r = rig();
        press(r.window(), "2");
        assert_eq!(r.ui.get_toast_text(), "");
        assert_eq!(r.uses(1), 0);
        let ctl = r.ctl.borrow();
        assert_eq!(ctl.services().clipboard.log, Vec::<String>::new());
        assert_eq!(ctl.fill().map(|f| f.fields().len()), Some(2));
    });
}

#[test]
fn the_toast_walks_through_its_phases_on_mock_time_and_then_goes() {
    on_ui_thread(|| {
        let r = rig();
        press(r.window(), "1");
        assert_eq!(r.ui.get_toast_text(), "Pasted into Outlook");
        assert_eq!(r.ui.get_toast_phase(), snipdeck_ui::ToastPhase::Enter);
        i_slint_backend_testing::mock_elapsed_time(TOAST_ENTER);
        assert_eq!(r.ui.get_toast_phase(), snipdeck_ui::ToastPhase::Hold);
        i_slint_backend_testing::mock_elapsed_time(TOAST_HOLD);
        assert_eq!(r.ui.get_toast_phase(), snipdeck_ui::ToastPhase::Exit);
        i_slint_backend_testing::mock_elapsed_time(TOAST_EXIT);
        assert_eq!(r.ui.get_toast_text(), "");
    });
}

#[test]
fn the_tray_icon_toggles_the_panel_opens_the_menu_actions_and_quits() {
    on_ui_thread(|| {
        let r = rig();
        r.binding.tray(TrayEvent::TogglePanel);
        assert!(!r.window().is_visible());
        r.binding.tray(TrayEvent::TogglePanel);
        assert!(r.window().is_visible());
        r.binding.tray(TrayEvent::HotkeysPaused(true));
        r.binding.hotkey(HotkeyAction::TogglePanel);
        assert!(r.window().is_visible(), "paused hotkeys do nothing");
        r.binding.tray(TrayEvent::OpenSnipdeck);
        r.binding.tray(TrayEvent::Quit);
        assert_eq!(
            *r.effects.borrow(),
            [
                Effect::HidePanel,
                Effect::ShowPanel,
                Effect::SetHotkeysPaused(true),
                Effect::OpenMainWindow,
                Effect::Quit
            ]
        );
    });
}

/// The owner's report: close the Panel, then "Open Panel" in the tray menu does nothing. The
/// close button and the tray event go through the same binding the real tray thread feeds.
#[test]
fn open_panel_in_the_tray_menu_brings_back_a_panel_closed_with_its_button() {
    on_ui_thread(|| {
        let r = rig();
        r.button("Close panel (Esc)")
            .invoke_accessible_default_action();
        assert!(!r.window().is_visible(), "closed to the tray");
        r.binding.tray(TrayEvent::OpenPanel);
        assert!(r.window().is_visible(), "Open Panel shows it again");
        assert!(
            r.ctl.borrow().view().on_top,
            "and it is still the same Panel"
        );
        // The window host raises the Panel on every ShowPanel, also when it counts as shown
        // already but is buried under other windows: Open Panel must always say so.
        r.binding.tray(TrayEvent::OpenPanel);
        assert!(r.window().is_visible());
        // The tray's left click brings it back from hidden in the same way.
        r.button("Close panel (Esc)")
            .invoke_accessible_default_action();
        r.binding.tray(TrayEvent::TogglePanel);
        assert!(r.window().is_visible());
        assert_eq!(
            *r.effects.borrow(),
            [
                Effect::HidePanel,
                Effect::ShowPanel,
                Effect::ShowPanel,
                Effect::HidePanel,
                Effect::ShowPanel,
            ]
        );
    });
}

#[test]
fn without_a_tray_icon_the_close_button_quits_instead_of_hiding() {
    on_ui_thread(|| {
        let r = rig_with_tray(false);
        r.button("Close panel (Esc)")
            .invoke_accessible_default_action();
        assert!(
            r.window().is_visible(),
            "the host quits; the window is not hidden first"
        );
        assert_eq!(*r.effects.borrow(), [Effect::Quit]);
    });
}

#[test]
fn windows_dark_mode_and_high_contrast_reach_the_window() {
    on_ui_thread(|| {
        let r = rig();
        let look = r.ui.global::<snipdeck_ui::SystemLook<'_>>();
        assert!(!r.ui.get_dark());
        assert!(!look.get_high_contrast());

        r.binding.system_look(SystemLook {
            dark: true,
            high_contrast: None,
            accent: None,
            reduce_motion: false,
        });
        assert!(r.ui.get_dark(), "theme = system follows Windows");

        let hc = HighContrast::default();
        r.binding.system_look(SystemLook {
            dark: false,
            high_contrast: Some(HighContrast {
                highlight: snipdeck_platform::system_look::Rgb { r: 1, g: 2, b: 3 },
                ..hc
            }),
            accent: None,
            reduce_motion: false,
        });
        assert!(look.get_high_contrast());
        assert_eq!(look.get_hc_highlight(), slint::Color::from_rgb_u8(1, 2, 3));
        assert_eq!(look.get_hc_window(), slint::Color::from_rgb_u8(0, 0, 0));

        r.binding.system_look(SystemLook::default());
        assert!(!look.get_high_contrast());
        assert!(!r.ui.get_dark());
    });
}

#[test]
fn reduced_motion_reaches_the_window_from_windows_or_the_setting() {
    on_ui_thread(|| {
        let r = rig();
        let look = r.ui.global::<snipdeck_ui::SystemLook<'_>>();
        assert!(!look.get_reduced_motion());

        r.binding.system_look(SystemLook {
            reduce_motion: true,
            ..SystemLook::default()
        });
        assert!(look.get_reduced_motion(), "Windows animation effects off");

        r.binding.system_look(SystemLook::default());
        assert!(!look.get_reduced_motion());
        r.ctl
            .borrow_mut()
            .update_settings(|s| s.reduce_motion = true);
        r.binding.system_look(SystemLook::default());
        assert!(look.get_reduced_motion(), "the Snipdeck setting");
    });
}

#[test]
fn an_explicit_theme_ignores_windows_dark_mode() {
    on_ui_thread(|| {
        let r = rig();
        r.ctl.borrow_mut().set_theme(Theme::Light);
        r.binding.system_look(SystemLook {
            dark: true,
            high_contrast: None,
            accent: None,
            reduce_motion: false,
        });
        assert!(!r.ui.get_dark());
    });
}

#[test]
fn windows_are_painted_in_the_theme_window_colour_before_their_first_frame() {
    // O52: a window flashed white between showing and its first frame. The colour Windows
    // paints first is the theme's `bg-app`, and it follows a theme change.
    on_ui_thread(|| {
        let r = rig();
        let erase = snipdeck_app::native_window::erase_colour;
        r.ctl.borrow_mut().set_theme(Theme::Dark);
        r.binding.system_look(SystemLook::default());
        assert_eq!(
            erase(),
            Some((0x11, 0x0F, 0x0D)),
            "dark bg-app (neutral-900)"
        );
        r.ctl.borrow_mut().set_theme(Theme::Light);
        r.binding.system_look(SystemLook::default());
        let light = erase().expect("a colour");
        assert_ne!(light, (0xFF, 0xFF, 0xFF), "never the default white");
        assert_ne!(light, (0x11, 0x0F, 0x0D), "follows the theme");
    });
}

#[test]
fn a_changed_setting_is_written_when_the_quiet_timer_fires_on_the_injected_clock() {
    on_ui_thread(|| {
        let r = rig();
        let temp = tempfile::tempdir().expect("dir");
        let file = temp.path().join("settings.json");
        r.ctl.borrow_mut().set_settings_saver(SettingsSaver::new(
            Box::new(FileSink::new(temp.path())),
            Settings::default(),
        ));

        r.ctl.borrow_mut().set_theme(Theme::Dark);
        // Any redraw arms the timer for what is waiting.
        r.binding.system_look(SystemLook::default());
        assert!(r.ui.get_dark());

        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(400));
        r.clock.set(at("2026-10-17T10:30:00.4Z"));
        assert!(!file.exists(), "inside the quiet time");

        // Slint's mock time and the app's clock are both moved to the due moment.
        r.clock.set(at("2026-10-17T10:30:00.5Z"));
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(100));
        assert_eq!(
            snipdeck_app::paths::read_settings(temp.path())
                .settings
                .theme,
            Theme::Dark
        );
        assert_eq!(r.ctl.borrow().settings_wait(), None);
    });
}

// ---- The Panel menu (kebab), row actions and the window frame -------------------------------

fn centre(element: &ElementHandle) -> slint::LogicalPosition {
    let at = element.absolute_position();
    let size = element.size();
    slint::LogicalPosition::new(at.x + size.width / 2.0, at.y + size.height / 2.0)
}

/// Moves the pointer to `position` and lets the hover timers (the tooltip's delay) run.
fn move_to(window: &slint::Window, position: slint::LogicalPosition) {
    window.dispatch_event(slint::platform::WindowEvent::PointerMoved { position });
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(700));
    slint::platform::update_timers_and_animations();
}

/// A left click at `position`, the way the winit backend delivers one.
fn click_at(window: &slint::Window, position: slint::LogicalPosition) {
    let button = slint::platform::PointerEventButton::Left;
    window.dispatch_event(slint::platform::WindowEvent::PointerMoved { position });
    window.dispatch_event(slint::platform::WindowEvent::PointerPressed { position, button });
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(50));
    window.dispatch_event(slint::platform::WindowEvent::PointerReleased { position, button });
    i_slint_backend_testing::mock_elapsed_time(Duration::from_secs(1));
    slint::platform::update_timers_and_animations();
}

const MENU_ROWS: [&str; 11] = [
    "Open Snipdeck",
    "All commands\u{2026}",
    "New snippet",
    "Settings\u{2026}",
    "Dock to the nearest edge",
    "Pause hotkeys",
    "Troubleshoot\u{2026}",
    "User guide",
    "Keyboard shortcuts",
    "About Snipdeck",
    "Quit",
];

fn open_menu(r: &Rig) {
    r.window().set_size(slint::LogicalSize::new(320.0, 480.0));
    slint::platform::update_timers_and_animations();
    click(r.window(), &r.button("More"));
    assert!(r.ui.get_app_menu_open(), "the kebab click opened the menu");
}

fn menu_row(r: &Rig, label: &str) -> ElementHandle {
    element(&r.ui, label, AccessibleRole::ListItem)
}

#[test]
fn the_kebab_menu_is_a_list_of_the_expected_rows_with_a_name_and_positions() {
    on_ui_thread(|| {
        let r = rig();
        open_menu(&r);
        let menu = element(&r.ui, "Panel menu", AccessibleRole::List);
        assert!(menu.size().width > 0.0);
        for (i, label) in MENU_ROWS.iter().enumerate() {
            let row = menu_row(&r, label);
            assert_eq!(row.accessible_item_index(), Some(i), "{label}");
            assert_eq!(
                row.accessible_item_count(),
                Some(MENU_ROWS.len()),
                "{label}"
            );
            assert_eq!(row.accessible_enabled(), Some(true), "{label}");
        }
    });
}

#[test]
fn the_menu_is_still_open_after_the_pointer_moves_into_it_and_away_from_the_button() {
    // The owner's report: "I hover over or click the meatball button, the More dropdown
    // appears, and when I go to click it it disappears". What appeared was the tooltip.
    on_ui_thread(|| {
        let r = rig();
        r.window().set_size(slint::LogicalSize::new(320.0, 480.0));
        slint::platform::update_timers_and_animations();

        // Hovering shows the tooltip "More" (and nothing else).
        let kebab = r.button("More");
        move_to(r.window(), centre(&kebab));
        assert_eq!(r.ui.get_tooltip_shown(), "More");
        assert!(!r.ui.get_app_menu_open(), "hovering does not open the menu");

        click(r.window(), &kebab);
        assert!(r.ui.get_app_menu_open());
        assert_eq!(
            r.ui.get_tooltip_shown(),
            "",
            "the tooltip gives way to the menu"
        );

        // Down into the menu, row by row, then off to the pin button, then over the list.
        for label in ["Open Snipdeck", "Settings\u{2026}", "Quit"] {
            move_to(r.window(), centre(&menu_row(&r, label)));
            assert!(r.ui.get_app_menu_open(), "still open over {label}");
        }
        move_to(r.window(), centre(&r.button("Keep panel on top")));
        assert!(r.ui.get_app_menu_open(), "still open away from the button");
        assert_eq!(r.ui.get_tooltip_shown(), "", "no tooltip over an open menu");
        move_to(r.window(), slint::LogicalPosition::new(20.0, 300.0));
        assert!(r.ui.get_app_menu_open(), "moving never closes it");
        assert!(r.ctl.borrow().view().app_menu.is_some());
        assert_eq!(*r.effects.borrow(), []);

        // And it can still be used.
        click(r.window(), &menu_row(&r, "Settings\u{2026}"));
        assert!(!r.ui.get_app_menu_open());
        assert_eq!(*r.effects.borrow(), [Effect::OpenSettings]);
    });
}

#[test]
fn escape_closes_the_menu_without_closing_the_panel() {
    on_ui_thread(|| {
        let r = rig();
        open_menu(&r);
        press(r.window(), &key(Key::Escape));
        assert!(!r.ui.get_app_menu_open());
        assert!(r.ctl.borrow().view().app_menu.is_none());
        assert!(
            r.window().is_visible(),
            "Esc closed the menu, not the Panel"
        );
        assert_eq!(*r.effects.borrow(), []);
        assert!(r.ui.get_search_focused(), "typing goes to the search again");
    });
}

#[test]
fn a_click_outside_the_menu_closes_it_and_reaches_nothing_behind() {
    on_ui_thread(|| {
        let r = rig();
        open_menu(&r);
        let uses_before = r.ctl.borrow().services().clipboard.log.len();
        // Over the first row, left of the menu.
        let row = centre(&r.window_element_for_row());
        click_at(r.window(), slint::LogicalPosition::new(24.0, row.y));
        assert!(!r.ui.get_app_menu_open());
        assert!(r.window().is_visible());
        assert_eq!(
            r.ctl.borrow().services().clipboard.log.len(),
            uses_before,
            "the click that dismissed the menu did not use the row under it"
        );
        assert_eq!(*r.effects.borrow(), []);
    });
}

impl Rig {
    /// Some element of the first row, for a click that lands on the list.
    fn window_element_for_row(&self) -> ElementHandle {
        let title = self.titles().remove(0);
        ElementHandle::find_by_accessible_label(&self.ui, &title)
            .next()
            .expect("the first row")
    }
}

#[test]
fn quit_in_the_menu_yields_the_quit_effect() {
    on_ui_thread(|| {
        let r = rig();
        open_menu(&r);
        click(r.window(), &menu_row(&r, "Quit"));
        assert_eq!(*r.effects.borrow(), [Effect::Quit]);
        assert!(!r.ui.get_app_menu_open());
    });
}

#[test]
fn open_snipdeck_help_and_new_snippet_reach_the_host() {
    on_ui_thread(|| {
        let r = rig();
        for label in [
            "Open Snipdeck",
            "New snippet",
            "Troubleshoot\u{2026}",
            "User guide",
            "About Snipdeck",
        ] {
            open_menu(&r);
            click(r.window(), &menu_row(&r, label));
        }
        assert_eq!(
            *r.effects.borrow(),
            [
                Effect::OpenMainWindow,
                Effect::NewItem,
                Effect::OpenHelp(HelpTopic::Troubleshoot),
                Effect::OpenHelp(HelpTopic::Guide),
                Effect::OpenHelp(HelpTopic::About),
            ]
        );
    });
}

#[test]
fn pause_hotkeys_is_checked_while_paused() {
    on_ui_thread(|| {
        let r = rig();
        open_menu(&r);
        let checked = |r: &Rig| {
            r.ui.get_app_menu_entries()
                .row_data(5)
                .expect("the Pause row")
                .icon
                .size()
                .width
                > 0
        };
        assert!(!checked(&r), "off: no check mark");
        click(r.window(), &menu_row(&r, "Pause hotkeys"));
        assert_eq!(*r.effects.borrow(), [Effect::SetHotkeysPaused(true)]);
        assert!(r.ctl.borrow().hotkeys_paused());

        click(r.window(), &r.button("More"));
        assert!(checked(&r), "paused: checked");
        click(r.window(), &menu_row(&r, "Pause hotkeys"));
        assert_eq!(r.effects.borrow()[1], Effect::SetHotkeysPaused(false));
        click(r.window(), &r.button("More"));
        assert!(!checked(&r), "resumed: unchecked again");
    });
}

fn right_grip(r: &Rig) -> slint::LogicalPosition {
    let size = r.window().size().to_logical(r.window().scale_factor());
    slint::LogicalPosition::new(size.width - 2.0, size.height / 2.0)
}

fn display() -> Monitor {
    Monitor {
        taskbar: None,
        id: Monitor::id_of("DISPLAY1", 1920, 1080),
        work_area: Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        },
        scale: 1.0,
    }
}

#[test]
fn a_press_on_the_right_edge_asks_the_host_to_resize_from_that_edge() {
    on_ui_thread(|| {
        let r = rig();
        r.window().set_size(slint::LogicalSize::new(320.0, 480.0));
        slint::platform::update_timers_and_animations();
        let position = right_grip(&r);
        r.window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved { position });
        r.window()
            .dispatch_event(slint::platform::WindowEvent::PointerPressed {
                position,
                button: slint::platform::PointerEventButton::Left,
            });
        assert_eq!(*r.effects.borrow(), [Effect::ResizeWindow(Edge::Right)]);
    });
}

#[test]
fn a_resize_to_1000_wide_is_cut_to_480_placed_and_remembered_for_that_monitor() {
    on_ui_thread(|| {
        let r = rig();
        let temp = tempfile::tempdir().expect("dir");
        r.ctl.borrow_mut().set_settings_saver(SettingsSaver::new(
            Box::new(FileSink::new(temp.path())),
            Settings::default(),
        ));
        r.window().set_size(slint::LogicalSize::new(320.0, 480.0));

        // The OS resize loop ended with a frame 1000 wide (what a host fake reports).
        let m = display();
        r.binding.frame_changed(
            &m,
            Rect {
                x: 200,
                y: 100,
                w: 1000,
                h: 600,
            },
        );
        let cut = Rect {
            x: 200,
            y: 100,
            w: 480,
            h: 600,
        };
        assert_eq!(*r.effects.borrow(), [Effect::PlaceWindow(cut)]);
        assert_eq!(
            (r.window().size().width, r.window().size().height),
            (480, 600),
            "the window was put at the clamped size"
        );
        assert_eq!(r.ctl.borrow().placement(&m), Some(cut));

        // Written after the quiet time.
        r.clock.set(at("2026-10-17T10:30:00.5Z"));
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(600));
        slint::platform::update_timers_and_animations();
        let stored = snipdeck_app::paths::read_settings(temp.path())
            .settings
            .panel_geometry;
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].monitor_id, m.id);
        assert_eq!(Rect::of(&stored[0]), cut);
    });
}

#[test]
fn showing_the_panel_puts_it_where_the_user_left_it_on_that_monitor() {
    on_ui_thread(|| {
        let r = rig();
        let m = display();
        let kept = Rect {
            x: 300,
            y: 120,
            w: 410,
            h: 700,
        };
        r.binding.frame_changed(&m, kept);
        assert_eq!(*r.effects.borrow(), [], "inside the limits: nothing to fix");

        r.window().set_size(slint::LogicalSize::new(320.0, 480.0));
        r.binding.restore_frame(&m);
        assert_eq!(*r.effects.borrow(), [Effect::PlaceWindow(kept)]);
        assert_eq!(
            (r.window().size().width, r.window().size().height),
            (410, 700)
        );
        // The testing backend has no desktop, so it keeps no window position: the position
        // part of `PlaceWindow` is carried by the effect (asserted above) and only the real
        // window can show it.

        r.effects.borrow_mut().clear();
        let other = Monitor {
            id: Monitor::id_of("DISPLAY2", 1280, 1024),
            ..m
        };
        r.binding.restore_frame(&other);
        assert_eq!(
            *r.effects.borrow(),
            [],
            "nothing stored for another monitor"
        );
    });
}

#[test]
fn delete_trashes_the_selected_row_and_the_toasts_undo_brings_it_back() {
    on_ui_thread(|| {
        let r = rig();
        r.window().set_size(slint::LogicalSize::new(320.0, 480.0));
        slint::platform::update_timers_and_animations();
        let before = r.titles();
        let gone = before[0].clone();

        press(r.window(), &key(Key::Delete));
        assert_eq!(r.titles(), before[1..], "{gone:?} is out of the list");
        assert_eq!(r.ui.get_toast_undo_label(), "Undo");
        assert_eq!(*r.effects.borrow(), []);

        r.button("Undo").invoke_accessible_default_action();
        assert_eq!(r.titles(), before, "undo brought it back, in its place");
        assert_eq!(
            r.ui.get_toast_undo_label(),
            "",
            "a Restored toast has no Undo"
        );
    });
}

#[test]
fn edit_and_rename_on_the_selected_row_ask_for_the_quick_editor() {
    on_ui_thread(|| {
        let r = rig();
        r.window().set_size(slint::LogicalSize::new(320.0, 480.0));
        slint::platform::update_timers_and_animations();
        let id = r.ctl.borrow().model().rows()[0].id;
        chord(r.window(), &[Key::Control], "e");
        press(r.window(), &key(Key::F2));
        assert_eq!(
            *r.effects.borrow(),
            [Effect::EditItem(id), Effect::EditItem(id)]
        );
    });
}

// ---- The edge dock ----------------------------------------------------------------------------

const HANDLE_LABEL: &str = "Snipdeck handle, press the Panel hotkey to open";

/// The Panel on the first monitor, 40 px from its left edge, docked there through the kebab's
/// "Dock to the left edge" row, with the dock trigger set to `trigger`.
fn docked_rig(trigger: DockTrigger) -> Rig {
    let r = rig();
    r.ctl
        .borrow_mut()
        .update_settings(|s| s.dock.trigger = trigger);
    r.window().set_size(slint::LogicalSize::new(320.0, 480.0));
    r.binding.frame_changed(
        &display(),
        Rect {
            x: 40,
            y: 300,
            w: 320,
            h: 480,
        },
    );
    open_menu(&r);
    click(r.window(), &menu_row(&r, "Dock to the left edge"));
    assert_eq!(r.ctl.borrow().dock_edge(), Some(DockEdge::Left));
    r
}

const OPEN: Rect = Rect {
    x: 0,
    y: 0,
    w: 320,
    h: 1040,
};
const STRIP: Rect = Rect {
    x: 0,
    y: 0,
    w: 6,
    h: 1040,
};

fn size(r: &Rig) -> (u32, u32) {
    (r.window().size().width, r.window().size().height)
}

fn handle(r: &Rig) -> ElementHandle {
    element(&r.ui, HANDLE_LABEL, AccessibleRole::Button)
}

/// Lets `ms` pass for the controller's clock and for the window's timers.
fn pass(r: &Rig, ms: u64) {
    let then = r.clock.now().as_millisecond();
    r.clock.set(
        Timestamp::from_millisecond(then + i64::try_from(ms).expect("small")).expect("moment"),
    );
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(ms + 60));
    slint::platform::update_timers_and_animations();
}

#[test]
fn the_kebab_docks_the_panel_flush_with_the_nearest_edge_and_offers_to_undock() {
    on_ui_thread(|| {
        let r = docked_rig(DockTrigger::Both);
        assert_eq!(*r.effects.borrow(), [Effect::PlaceWindow(OPEN)]);
        assert_eq!(size(&r), (OPEN.w, OPEN.h));
        open_menu(&r);
        menu_row(&r, "Undock the panel");
    });
}

#[test]
fn a_docked_panel_collapses_to_a_handle_on_the_hotkey_and_the_handle_is_named() {
    on_ui_thread(|| {
        let r = docked_rig(DockTrigger::Both);
        r.effects.borrow_mut().clear();
        r.binding.hotkey(HotkeyAction::TogglePanel);
        assert_eq!(*r.effects.borrow(), [Effect::PlaceWindow(STRIP)]);
        assert_eq!(size(&r), (STRIP.w, STRIP.h));
        assert!(r.ui.get_dock_collapsed());
        assert!(r.ui.window().is_visible(), "a handle, not a hidden window");
        let h = handle(&r);
        assert_eq!((h.size().width, h.size().height), (6.0, 1040.0));
        r.binding.hotkey(HotkeyAction::TogglePanel);
        assert_eq!(size(&r), (OPEN.w, OPEN.h));
        assert!(
            ElementHandle::find_by_accessible_label(&r.ui, HANDLE_LABEL)
                .next()
                .is_none()
        );
    });
}

#[test]
fn resting_the_pointer_on_the_handle_opens_the_panel_after_the_hover_delay() {
    on_ui_thread(|| {
        let r = docked_rig(DockTrigger::Hover);
        r.binding.hotkey(HotkeyAction::TogglePanel);
        r.effects.borrow_mut().clear();
        let at_handle = centre(&handle(&r));
        r.window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: at_handle,
            });
        slint::platform::update_timers_and_animations();
        assert!(r.ctl.borrow().dock_needs_tick(), "the delay is running");

        pass(&r, 100);
        assert_eq!(size(&r), (STRIP.w, STRIP.h), "not before 150 ms");
        pass(&r, 50);
        assert_eq!(*r.effects.borrow(), [Effect::PlaceWindow(OPEN)]);
        assert_eq!(size(&r), (OPEN.w, OPEN.h));
        assert!(!r.ui.get_dock_collapsed());
        assert!(
            !r.ctl.borrow().dock_needs_tick(),
            "the timer stops when idle"
        );
    });
}

#[test]
fn leaving_the_handle_before_the_delay_keeps_it_collapsed() {
    on_ui_thread(|| {
        let r = docked_rig(DockTrigger::Hover);
        r.binding.hotkey(HotkeyAction::TogglePanel);
        let at_handle = centre(&handle(&r));
        r.window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: at_handle,
            });
        slint::platform::update_timers_and_animations();
        r.window()
            .dispatch_event(slint::platform::WindowEvent::PointerExited);
        slint::platform::update_timers_and_animations();
        pass(&r, 500);
        assert_eq!(size(&r), (STRIP.w, STRIP.h));
        assert!(r.ui.get_dock_collapsed());
    });
}

#[test]
fn a_click_on_the_handle_opens_the_panel_when_the_trigger_has_click() {
    on_ui_thread(|| {
        let r = docked_rig(DockTrigger::Click);
        r.binding.hotkey(HotkeyAction::TogglePanel);
        // Resting on it does nothing with a click-only trigger.
        let at_handle = centre(&handle(&r));
        r.window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: at_handle,
            });
        pass(&r, 600);
        assert_eq!(size(&r), (STRIP.w, STRIP.h));
        click(r.window(), &handle(&r));
        assert_eq!(size(&r), (OPEN.w, OPEN.h));
    });
}

#[test]
fn the_open_panel_collapses_after_the_pointer_has_left_it_for_the_leave_delay() {
    on_ui_thread(|| {
        let r = docked_rig(DockTrigger::Both);
        // The pointer over a row: a child control. The Panel still counts as hovered.
        let row = ElementHandle::find_by_accessible_label(&r.ui, "Snippets")
            .next()
            .expect("the list");
        r.window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: centre(&row),
            });
        slint::platform::update_timers_and_animations();
        pass(&r, 1000);
        assert_eq!(size(&r), (OPEN.w, OPEN.h), "hovered, so it stays open");

        r.window()
            .dispatch_event(slint::platform::WindowEvent::PointerExited);
        slint::platform::update_timers_and_animations();
        assert!(
            r.ctl.borrow().dock_needs_tick(),
            "the leave delay is running"
        );
        pass(&r, 300);
        assert_eq!(size(&r), (OPEN.w, OPEN.h), "not before 400 ms");
        pass(&r, 100);
        assert_eq!(size(&r), (STRIP.w, STRIP.h));
        assert!(r.ui.get_dock_collapsed());
    });
}

#[test]
fn coming_back_to_the_panel_in_time_keeps_it_open() {
    on_ui_thread(|| {
        let r = docked_rig(DockTrigger::Both);
        let inside = slint::LogicalPosition::new(100.0, 200.0);
        r.window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved { position: inside });
        slint::platform::update_timers_and_animations();
        r.window()
            .dispatch_event(slint::platform::WindowEvent::PointerExited);
        pass(&r, 200);
        r.window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved { position: inside });
        pass(&r, 2000);
        assert_eq!(size(&r), (OPEN.w, OPEN.h));
    });
}

#[test]
fn a_paste_from_a_docked_panel_collapses_it() {
    on_ui_thread(|| {
        let r = docked_rig(DockTrigger::Both);
        r.effects.borrow_mut().clear();
        press(r.window(), "3");
        assert_eq!(r.uses(2), 1, "the third item was pasted");
        assert_eq!(*r.effects.borrow(), [Effect::PlaceWindow(STRIP)]);
        assert!(r.ui.get_dock_collapsed());
    });
}

#[test]
fn a_floating_panel_keeps_everything_as_before() {
    on_ui_thread(|| {
        let r = rig();
        r.ctl
            .borrow_mut()
            .update_settings(|s| s.dock.trigger = DockTrigger::Both);
        r.binding.frame_changed(
            &display(),
            Rect {
                x: 40,
                y: 300,
                w: 320,
                h: 480,
            },
        );
        press(r.window(), "3");
        assert!(!r.ui.get_dock_collapsed());
        assert_eq!(*r.effects.borrow(), []);
        r.binding.hotkey(HotkeyAction::TogglePanel);
        assert_eq!(*r.effects.borrow(), [Effect::HidePanel]);
    });
}

fn at(s: &str) -> Timestamp {
    s.parse().expect("timestamp")
}
