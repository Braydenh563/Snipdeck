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
    controller::{Controller, Effect, Services, TOAST_ENTER, TOAST_EXIT, TOAST_HOLD},
    demo,
    host::{FakeTargets, FixedClock},
    settings::{HotkeyAction, Settings, Theme},
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

        click(r.window(), &r.button("Close panel"));
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
        r.button("Close panel").invoke_accessible_default_action();
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
        r.button("Close panel").invoke_accessible_default_action();
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
        r.button("Close panel").invoke_accessible_default_action();
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
        r.button("Close panel").invoke_accessible_default_action();
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
        });
        assert!(r.ui.get_dark(), "theme = system follows Windows");

        let hc = HighContrast::default();
        r.binding.system_look(SystemLook {
            dark: false,
            high_contrast: Some(HighContrast {
                highlight: snipdeck_platform::system_look::Rgb { r: 1, g: 2, b: 3 },
                ..hc
            }),
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
fn an_explicit_theme_ignores_windows_dark_mode() {
    on_ui_thread(|| {
        let r = rig();
        r.ctl.borrow_mut().set_theme(Theme::Light);
        r.binding.system_look(SystemLook {
            dark: true,
            high_contrast: None,
        });
        assert!(!r.ui.get_dark());
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
            snipdeck_app::paths::load_settings(temp.path()).theme,
            Theme::Dark
        );
        assert_eq!(r.ctl.borrow().settings_wait(), None);
    });
}

fn at(s: &str) -> Timestamp {
    s.parse().expect("timestamp")
}
