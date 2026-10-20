//! The real `PickerWindow` (and the toast window) bound to a `Controller` over the demo library
//! and the platform fakes, opened by the Picker hotkey and driven by key events through Slint's
//! testing backend. Time is mocked, never the real clock. One process for this file (the
//! testing platform is process-wide).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use std::{cell::RefCell, rc::Rc};

use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::{ComponentHandle, Model, platform::Key};
use snipdeck_app::{
    binding::Binding,
    controller::{Controller, Effect, HelpTopic, Services},
    demo,
    fill_binding::FillUi,
    frame::{Monitor, Rect},
    host::{FakeTargets, FixedClock},
    picker_binding::{AnchorMemory, PickerUi},
    placement::FakePopups,
    settings::{HotkeyAction, Settings},
    toast_binding::ToastUi,
};
use snipdeck_core::{Entitlements, ItemId, Tier, Timestamp};
use snipdeck_platform::{
    keys::{Key as PlatformKey, KeyEvent},
    payload::Data,
    route::ScreenRect,
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
    panel: PanelWindow,
    ctl: Rc<RefCell<Ctl>>,
    binding: Binding,
    effects: Rc<RefCell<Vec<Effect>>>,
    targets: FakeTargets,
    popups: Rc<FakePopups>,
    picker: PickerUi,
    toasts: ToastUi,
    #[allow(dead_code, reason = "bound for the life of the rig")]
    fill: FillUi,
}

fn now() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

fn monitor(scale: f32) -> Monitor {
    Monitor {
        taskbar: None,
        id: Monitor::id_of("DISPLAY1", 1920, 1080),
        work_area: Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        },
        scale,
        taskbar: None,
    }
}

/// The Panel is hidden (the Picker and the capture hotkey are used from other apps), a caret
/// is at (400, 200) 20 px high, and the monitor is 1920 x 1040 at 100 %.
fn rig() -> Rig {
    rig_at(1.0)
}

fn rig_at(scale: f32) -> Rig {
    let store = demo::store(now()).expect("demo store");
    let targets = FakeTargets::new(Some(demo::target()));
    targets.set_caret(Some(ScreenRect {
        x: 400,
        y: 200,
        w: 2,
        h: 20,
    }));
    let services = Services {
        clipboard: FakeClipboard {
            asks_after_polls: Some(1),
            written: vec![
                snipdeck_platform::payload::Payload::new(vec![Data::Text(
                    "Remember the milk".into(),
                )])
                .expect("payload"),
            ],
            ..FakeClipboard::default()
        },
        keyboard: FakeKeyboard::default(),
        timing: FakeTiming::default(),
    };
    let ctl = Controller::new(
        store,
        services,
        Box::new(targets.clone()),
        Box::new(FixedClock::new(now())),
        Settings::default(),
        Entitlements::new(Tier::Pro),
    )
    .expect("controller");
    let ctl = Rc::new(RefCell::new(ctl));
    let _ = ctl.borrow_mut().on_tray(TrayEvent::IconPresent(true));
    let panel = PanelWindow::new().expect("panel");
    let effects: Rc<RefCell<Vec<Effect>>> = Rc::default();
    let log = Rc::clone(&effects);
    let binding = Controller::bind(
        &ctl,
        &panel,
        std::rc::Rc::new(snipdeck_app::placement::NoPopups),
        move |e| log.borrow_mut().push(*e),
    );
    let popups = Rc::new(FakePopups::one(monitor(scale)));
    popups.cursor.set(Some((900, 600)));
    let anchor = AnchorMemory::default();
    let picker = PickerUi::new(&ctl, popups.clone(), Rc::clone(&anchor));
    let fill = FillUi::new(&ctl, popups.clone(), anchor);
    let toasts = ToastUi::new(&ctl, popups.clone(), Rc::new(|| false));
    // The Panel starts hidden, as when the Picker hotkey is used from another app.
    let hidden = ctl.borrow_mut().hide();
    assert_eq!(hidden, [Effect::HidePanel]);
    Rig {
        panel,
        ctl,
        binding,
        effects,
        targets,
        popups,
        picker,
        toasts,
        fill,
    }
}

fn titles(window: &snipdeck_ui::PickerWindow) -> Vec<String> {
    window
        .get_rows()
        .iter()
        .map(|r| r.title.to_string())
        .collect()
}

impl Rig {
    fn open(&self) -> snipdeck_ui::PickerWindow {
        self.binding.hotkey(HotkeyAction::Picker);
        slint::platform::update_timers_and_animations();
        self.picker.window().expect("the Picker window was made")
    }

    fn uses(&self, id: ItemId) -> u32 {
        self.ctl.borrow().store().item(id).expect("item").use_count
    }

    /// The item called `title`. Opens and closes the Picker in the controller to read it, so
    /// call it before the test counts focus changes.
    fn id_of(&self, title: &str) -> ItemId {
        let mut ctl = self.ctl.borrow_mut();
        ctl.open_picker();
        let id = ctl
            .picker()
            .expect("picker")
            .rows()
            .into_iter()
            .find(|r| r.title == title)
            .map(|r| r.id)
            .expect("row");
        ctl.close_picker();
        id
    }
}

fn element(ui: &impl ComponentHandle, label: &str, role: AccessibleRole) -> ElementHandle {
    ElementHandle::find_by_accessible_label(ui, label)
        .find(|e| e.accessible_role() == Some(role))
        .unwrap_or_else(|| panic!("no {role:?} labelled {label:?}"))
}

#[test]
fn the_hotkey_opens_the_picker_below_the_caret_and_the_window_is_registered() {
    on_ui_thread(|| {
        let r = rig();
        assert!(r.picker.window().is_none(), "no window until it is needed");
        let w = r.open();
        assert!(w.window().is_visible());
        assert!(!r.panel.window().is_visible() || !r.ctl.borrow().visible());
        // 560 wide at 100 %, 4 px under the caret's bottom (200 + 20).
        let at = r.picker.placed().expect("it was placed");
        assert_eq!((at.x, at.y, at.w), (400, 224, 560), "below the caret");
        assert_eq!(
            *r.popups.calls.borrow(),
            ["attach", "activate"],
            "registered with the tracker, and given the keyboard to type into"
        );
        assert_eq!(titles(&w).len(), 11, "every item of every board");
    });
}

#[test]
fn the_picker_flips_above_a_caret_near_the_bottom_and_falls_back_to_the_mouse() {
    on_ui_thread(|| {
        let r = rig();
        r.targets.set_caret(Some(ScreenRect {
            x: 400,
            y: 980,
            w: 2,
            h: 20,
        }));
        let w = r.open();
        let at = r.picker.placed().expect("it was placed");
        assert_eq!(at.x, 400);
        assert_eq!(
            i64::from(at.y) + i64::from(at.h),
            980 - 4,
            "its bottom edge is 4 px above the caret"
        );
        press(w.window(), &key(Key::Escape));

        r.targets.set_caret(None);
        let _ = r.open();
        let at = r.picker.placed().expect("it was placed");
        assert_eq!(
            (at.x, at.y),
            (900, 604),
            "no caret: the mouse at (900, 600)"
        );
    });
}

#[test]
fn at_150_percent_the_window_and_its_gap_scale() {
    on_ui_thread(|| {
        let r = rig_at(1.5);
        let _ = r.open();
        let at = r.picker.placed().expect("it was placed");
        assert_eq!((at.x, at.y), (400, 226), "gap 6 px under 220");
        assert_eq!(at.w, 840, "560 logical x 1.5");
    });
}

#[test]
fn typing_filters_across_the_boards_and_enter_pastes_into_the_target() {
    on_ui_thread(|| {
        let r = rig();
        let retry = r.id_of("Retry helper");
        let base = r.targets.refocus_count();
        let w = r.open();
        type_text(w.window(), "retry");
        assert_eq!(r.ctl.borrow().picker().expect("open").query(), "retry");
        assert_eq!(titles(&w)[0], "Retry helper", "search spans every list");
        assert_eq!(w.get_rows().row_data(0).expect("row").list_name, "Code");

        press(w.window(), &key(Key::Return));
        assert!(!w.window().is_visible(), "the Picker is off the screen");
        {
            let ctl = r.ctl.borrow();
            let services = ctl.services();
            assert!(
                services.keyboard.sent[0].contains(&KeyEvent {
                    key: PlatformKey::V,
                    down: true
                }),
                "Ctrl+V reached the fake keyboard"
            );
            let Some(Data::Text(pasted)) = services
                .clipboard
                .written
                .last()
                .and_then(|p| p.data().first())
            else {
                panic!("text was put on the clipboard")
            };
            assert_ne!(pasted, "Remember the milk", "the item, not the seed");
        }
        assert_eq!(r.uses(retry), 1);
        assert_eq!(
            r.targets.refocus_count(),
            base + 1,
            "the Target had the focus back first"
        );
        assert!(r.ctl.borrow().picker().is_none());
    });
}

#[test]
fn shift_enter_copies_without_pasting() {
    on_ui_thread(|| {
        let r = rig();
        let office = r.id_of("Out of office");
        let w = r.open();
        type_text(w.window(), "out of office");
        chord(w.window(), &[Key::Shift], &key(Key::Return));
        let ctl = r.ctl.borrow();
        assert!(
            ctl.services().keyboard.sent.is_empty(),
            "no keys for a copy"
        );
        assert_eq!(ctl.services().clipboard.log, ["persistent:1"]);
        drop(ctl);
        assert_eq!(r.uses(office), 1);
        assert!(!w.window().is_visible());
    });
}

#[test]
fn alt_digit_uses_the_nth_row_and_arrows_move_the_selection() {
    on_ui_thread(|| {
        let r = rig();
        let w = r.open();
        press(w.window(), &key(Key::DownArrow));
        assert_eq!(w.get_selected_index(), 1);
        assert_eq!(r.ctl.borrow().picker().expect("open").selected(), 1);
        // Alt+digit uses the Nth row whatever the search holds: here the first of two matches.
        type_text(w.window(), "office");
        assert_eq!(
            w.get_selected_index(),
            0,
            "typing puts the selection back on the first row"
        );
        let first = r
            .ctl
            .borrow()
            .picker()
            .expect("open")
            .item_at(0)
            .expect("row");
        chord(w.window(), &[Key::Alt], "1");
        assert!(!w.window().is_visible());
        assert_eq!(r.ctl.borrow().services().keyboard.sent.len(), 1);
        assert_eq!(r.uses(first), 1);
    });
}

#[test]
fn escape_closes_the_picker_and_the_target_gets_the_focus_back() {
    on_ui_thread(|| {
        let r = rig();
        let w = r.open();
        assert_eq!(r.targets.refocus_count(), 0);
        press(w.window(), &key(Key::Escape));
        assert!(!w.window().is_visible());
        assert!(r.ctl.borrow().picker().is_none());
        assert_eq!(
            r.targets.refocus_count(),
            1,
            "the Target is the foreground again"
        );
        assert_eq!(r.ctl.borrow().services().keyboard.sent.len(), 0);
        // The hotkey again opens it, with the search empty.
        let w = r.open();
        assert!(w.window().is_visible());
        assert_eq!(w.get_query(), "");
        // And pressed a second time while open it closes it.
        r.binding.hotkey(HotkeyAction::Picker);
        assert!(!w.window().is_visible());
    });
}

#[test]
fn f1_opens_the_user_guide_and_leaves_the_picker_open() {
    on_ui_thread(|| {
        let r = rig();
        let w = r.open();
        type_text(w.window(), "out");
        press(w.window(), &key(Key::F1));
        assert_eq!(*r.effects.borrow(), [Effect::OpenHelp(HelpTopic::Guide)]);
        assert!(w.window().is_visible(), "help does not close the Picker");
        assert_eq!(w.get_query(), "out", "F1 is not search text");
    });
}

#[test]
fn ctrl_e_opens_the_editor_on_the_selected_item_and_closes_the_picker() {
    on_ui_thread(|| {
        let r = rig();
        let w = r.open();
        type_text(w.window(), "out of office");
        let id = r
            .ctl
            .borrow()
            .picker()
            .expect("open")
            .item_at(0)
            .expect("row");
        let title = r.ctl.borrow().store().item(id).expect("item").title;
        chord(w.window(), &[Key::Control], "e");
        let ctl = r.ctl.borrow();
        let editor = ctl.item_editor_view().expect("the editor opened");
        assert!(!editor.is_new, "it edits an existing item");
        assert_eq!(editor.fields.title, title, "it edits the selected item");
        drop(ctl);
        assert!(!w.window().is_visible());
        assert!(
            r.ctl.borrow().services().keyboard.sent.is_empty(),
            "nothing pasted"
        );
    });
}

#[test]
fn tab_cycles_the_scope() {
    on_ui_thread(|| {
        let r = rig();
        let w = r.open();
        press(w.window(), &key(Key::Tab));
        assert_eq!(w.get_scope(), snipdeck_ui::PickerScope::Board);
        assert_eq!(
            r.ctl.borrow().picker().expect("open").scope(),
            snipdeck_app::picker::Scope::Board
        );
        press(w.window(), &key(Key::Tab));
        assert_eq!(w.get_scope(), snipdeck_ui::PickerScope::Pinned);
    });
}

#[test]
fn a_sensitive_row_shows_the_mask_and_never_its_preview() {
    on_ui_thread(|| {
        let r = rig();
        let w = r.open();
        let rows: Vec<_> = w.get_rows().iter().collect();
        let secret = rows
            .iter()
            .find(|row| row.title == "Server password")
            .expect("the sensitive row is listed");
        assert_eq!(secret.preview, "");
        assert_eq!(secret.masked_preview, "••••••••");
        assert_eq!(secret.kind, snipdeck_ui::ItemKind::Sensitive);
        // Nothing in the window carries the secret.
        assert!(
            rows.iter()
                .all(|row| !row.preview.contains("hunter2") && !row.title.contains("hunter2")),
            "the secret is nowhere in the rows"
        );
        let plain = rows
            .iter()
            .find(|row| row.title == "Out of office")
            .expect("row");
        assert_eq!(plain.masked_preview, "", "ordinary rows are not masked");
        assert!(!plain.preview.is_empty());
    });
}

#[test]
fn the_toast_shows_in_its_own_window_while_the_panel_is_hidden() {
    on_ui_thread(|| {
        let r = rig();
        let w = r.open();
        type_text(w.window(), "out of office");
        press(w.window(), &key(Key::Return));
        let t = r.toasts.window().expect("a toast window was made");
        assert!(
            t.window().is_visible(),
            "the Panel is hidden, so the toast has its window"
        );
        assert_eq!(t.get_text(), "Pasted into Outlook");
        assert_eq!(t.get_kind(), snipdeck_ui::ToastKind::Success);
        let at = r.toasts.placed().expect("it was placed");
        assert!(
            (i64::from(at.x) * 2 + i64::from(at.w) - 1920).abs() <= 1,
            "centred across the work area (to a pixel)"
        );
        assert_eq!(
            i64::from(at.y) + i64::from(at.h),
            1040 - 24,
            "24 px above the bottom of the work area"
        );
        assert_eq!(
            element(&t, "Pasted into Outlook", AccessibleRole::Text)
                .accessible_label()
                .as_deref(),
            Some("Pasted into Outlook")
        );
        // It leaves with the toast.
        for step in [
            snipdeck_app::controller::TOAST_ENTER,
            snipdeck_app::controller::TOAST_HOLD,
            snipdeck_app::controller::TOAST_EXIT,
        ] {
            i_slint_backend_testing::mock_elapsed_time(step + std::time::Duration::from_millis(1));
            slint::platform::update_timers_and_animations();
        }
        assert!(!t.window().is_visible(), "gone when the toast is");
    });
}

#[test]
fn the_toast_window_stays_away_while_the_panel_shows_it() {
    on_ui_thread(|| {
        let r = rig();
        r.binding.show();
        let w = r.open();
        press(w.window(), &key(Key::Escape));
        r.ctl.borrow_mut().quick_capture();
        r.binding.hotkey_problems(&[]);
        assert!(r.ctl.borrow().visible());
        assert!(
            r.toasts.window().is_none_or(|t| !t.window().is_visible()),
            "the Panel draws its own toast"
        );
    });
}

#[test]
fn the_picker_exposes_roles_and_labels_to_assistive_technology() {
    on_ui_thread(|| {
        let r = rig();
        let w = r.open();
        let results = element(&w, "Results", AccessibleRole::List);
        assert!(
            results.accessible_item_count().unwrap_or(0) > 0
                || results.accessible_label().is_some()
        );
        let scopes = element(&w, "Search in", AccessibleRole::Groupbox);
        assert_eq!(scopes.accessible_label().as_deref(), Some("Search in"));
        // The search field takes the keyboard when the window opens.
        assert!(w.get_search_focused());
    });
}

#[test]
fn the_capture_and_paste_next_hotkeys_report_in_the_toast_window_while_the_panel_is_hidden() {
    on_ui_thread(|| {
        let r = rig();
        // Quick capture saves what is on the clipboard (the rig put some text there).
        r.binding.hotkey(HotkeyAction::Capture);
        let t = r.toasts.window().expect("a toast window was made");
        assert!(t.window().is_visible());
        assert!(
            t.get_text().starts_with("Saved to "),
            "the capture toast is visible without the Panel: {}",
            t.get_text()
        );
        assert_eq!(t.get_kind(), snipdeck_ui::ToastKind::Success);

        // Paste next pastes the first pinned item and says where it went.
        r.binding.hotkey(HotkeyAction::PasteNext);
        assert_eq!(t.get_text(), "Pasted into Outlook");
        assert_eq!(r.ctl.borrow().services().keyboard.sent.len(), 1);
    });
}

#[test]
fn a_toast_from_another_window_that_shows_toasts_is_not_doubled() {
    on_ui_thread(|| {
        let r = rig();
        // The Main window (or Settings) is open: it draws the toast itself.
        let showing = Rc::new(std::cell::Cell::new(true));
        let flag = Rc::clone(&showing);
        let quiet = ToastUi::new(&r.ctl, r.popups.clone(), Rc::new(move || flag.get()));
        r.binding.hotkey(HotkeyAction::Capture);
        assert!(
            quiet.window().is_none_or(|t| !t.window().is_visible()),
            "no second toast over the open window"
        );
        showing.set(false);
        r.binding.hotkey_problems(&[]);
        assert!(quiet.window().is_some_and(|t| t.window().is_visible()));
    });
}
