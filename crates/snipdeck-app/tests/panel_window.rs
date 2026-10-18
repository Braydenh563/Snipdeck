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
    let store = demo::store(now()).expect("demo store");
    let targets = FakeTargets::new(Some(demo::target()));
    let services = Services {
        clipboard: FakeClipboard {
            asks_after_polls: Some(1),
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
    ctl.borrow_mut().set_tray_available(tray);
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
        r.button("Keep on top").invoke_accessible_default_action();
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
        r.binding.hotkey(HotkeyAction::Picker);
        r.binding.hotkey(HotkeyAction::Capture);
        r.binding.hotkey(HotkeyAction::PasteNext);
        assert_eq!(
            *r.effects.borrow(),
            [
                Effect::HidePanel,
                Effect::ShowPanel,
                Effect::OpenPicker,
                Effect::StartCapture,
                Effect::PasteNext
            ]
        );
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
fn a_template_that_needs_answers_toasts_and_changes_nothing() {
    on_ui_thread(|| {
        let r = rig();
        press(r.window(), "2");
        assert_eq!(r.ui.get_toast_text(), "Fill in this template first.");
        assert_eq!(r.uses(1), 0);
        let ctl = r.ctl.borrow();
        assert_eq!(ctl.services().clipboard.log, Vec::<String>::new());
        assert_eq!(ctl.pending().map(|p| p.prompts.len()), Some(2));
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
