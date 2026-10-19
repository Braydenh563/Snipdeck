//! The real `TemplateFillWindow` bound to a `Controller` over the demo library: a prompted
//! template chosen in the Picker (or from the Panel) opens the form, the answers are typed
//! with key events, and Enter pastes the filled text. Time is mocked, never the real clock.
//! One process for this file (the testing platform is process-wide).

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
    controller::{Controller, Effect, Services},
    demo,
    fill_binding::FillUi,
    frame::{Monitor, Rect},
    host::{FakeTargets, FixedClock},
    picker_binding::{AnchorMemory, PickerUi},
    placement::FakePopups,
    settings::{HotkeyAction, Settings},
    toast_binding::ToastUi,
};
use snipdeck_core::{Entitlements, Tier, Timestamp};
use snipdeck_platform::{
    payload::Data,
    route::ScreenRect,
    testing::{FakeClipboard, FakeKeyboard, FakeTiming},
    tray::TrayEvent,
};
use snipdeck_store::{ItemRepo, Store};
use snipdeck_ui::{PanelWindow, PickerWindow, TemplateFillWindow};

#[path = "support/mod.rs"]
mod support;
use support::{chord, key, on_ui_thread, press, type_text};

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

struct Rig {
    ctl: Rc<RefCell<Ctl>>,
    binding: Binding,
    targets: FakeTargets,
    popups: Rc<FakePopups>,
    picker: PickerUi,
    fill: FillUi,
    #[allow(dead_code, reason = "bound for the life of the rig")]
    panel: PanelWindow,
    #[allow(dead_code, reason = "bound for the life of the rig")]
    toasts: ToastUi,
    #[allow(dead_code, reason = "recorded for the Panel binding")]
    effects: Rc<RefCell<Vec<Effect>>>,
}

fn now() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

fn rig() -> Rig {
    rig_on(demo::store(now()).expect("demo store"))
}

fn rig_on(store: Store) -> Rig {
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
            ..FakeClipboard::default()
        },
        keyboard: FakeKeyboard::default(),
        timing: FakeTiming::default(),
    };
    let ctl = Rc::new(RefCell::new(
        Controller::new(
            store,
            services,
            Box::new(targets.clone()),
            Box::new(FixedClock::new(now())),
            Settings::default(),
            Entitlements::new(Tier::Pro),
        )
        .expect("controller"),
    ));
    let _ = ctl.borrow_mut().on_tray(TrayEvent::IconPresent(true));
    let panel = PanelWindow::new().expect("panel");
    let effects: Rc<RefCell<Vec<Effect>>> = Rc::default();
    let log = Rc::clone(&effects);
    let binding = Controller::bind(&ctl, &panel, move |e| log.borrow_mut().push(*e));
    let popups = Rc::new(FakePopups::one(Monitor {
        id: Monitor::id_of("DISPLAY1", 1920, 1080),
        work_area: Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        },
        scale: 1.0,
    }));
    popups.cursor.set(Some((900, 600)));
    let anchor = AnchorMemory::default();
    let picker = PickerUi::new(&ctl, popups.clone(), Rc::clone(&anchor));
    let fill = FillUi::new(&ctl, popups.clone(), anchor);
    let toasts = ToastUi::new(&ctl, popups.clone(), Rc::new(|| false));
    let _ = ctl.borrow_mut().hide();
    Rig {
        ctl,
        binding,
        targets,
        popups,
        picker,
        fill,
        panel,
        toasts,
        effects,
    }
}

impl Rig {
    /// Opens the Picker by its hotkey and chooses the invoice template; returns the form.
    fn open_form(&self, copy: bool) -> (PickerWindow, TemplateFillWindow) {
        self.binding.hotkey(HotkeyAction::Picker);
        slint::platform::update_timers_and_animations();
        let picker = self.picker.window().expect("picker");
        type_text(picker.window(), "invoice");
        assert_eq!(
            picker.get_rows().row_data(0).expect("row").title,
            "Invoice reminder"
        );
        if copy {
            chord(picker.window(), &[Key::Shift], &key(Key::Return));
        } else {
            press(picker.window(), &key(Key::Return));
        }
        slint::platform::update_timers_and_animations();
        (
            picker,
            self.fill.window().expect("the form's window was made"),
        )
    }
}

fn element(ui: &impl ComponentHandle, label: &str, role: AccessibleRole) -> ElementHandle {
    ElementHandle::find_by_accessible_label(ui, label)
        .find(|e| e.accessible_role() == Some(role))
        .unwrap_or_else(|| panic!("no {role:?} labelled {label:?}"))
}

#[test]
fn a_prompted_template_chosen_in_the_picker_opens_the_form_where_the_picker_was() {
    on_ui_thread(|| {
        let r = rig();
        let (picker, form) = r.open_form(false);
        assert!(
            !picker.window().is_visible(),
            "the Picker is off the screen"
        );
        assert!(form.window().is_visible());
        assert!(r.ctl.borrow().fill().is_some());
        assert!(
            r.ctl.borrow().services().keyboard.sent.is_empty(),
            "nothing pasted yet"
        );
        assert_eq!(form.get_template_title(), "Invoice reminder");
        let labels: Vec<String> = form
            .get_fields()
            .iter()
            .map(|f| f.label.to_string())
            .collect();
        assert_eq!(labels, ["Name", "Number"]);
        assert!(form.get_fields().iter().all(|f| f.required));
        assert_eq!(form.get_submit_label(), "Insert");
        // Below the caret the Picker had; 440 logical wide.
        let at = r.fill.placed().expect("it was placed");
        assert_eq!((at.x, at.y, at.w), (400, 224, 440));
        assert_eq!(
            r.popups
                .calls
                .borrow()
                .iter()
                .filter(|c| **c == "attach")
                .count(),
            2,
            "the Picker's window and the form's window were each registered once"
        );
    });
}

#[test]
fn the_form_follows_the_picker_even_when_the_caret_can_no_longer_be_read() {
    on_ui_thread(|| {
        let r = rig();
        r.binding.hotkey(HotkeyAction::Picker);
        let picker = r.picker.window().expect("picker");
        // The Picker has the keyboard now, so the Target's caret is gone.
        r.targets.set_caret(None);
        type_text(picker.window(), "invoice");
        press(picker.window(), &key(Key::Return));
        let _ = r.fill.window().expect("form");
        let at = r.fill.placed().expect("it was placed");
        assert_eq!((at.x, at.y), (400, 224), "where the Picker was");
    });
}

#[test]
fn a_sensitive_template_previews_as_a_mask_and_never_its_text() {
    on_ui_thread(|| {
        let mut store = demo::store(now()).expect("demo store");
        let mut template = store
            .all_items()
            .expect("items")
            .into_iter()
            .find(|i| i.title == "Invoice reminder")
            .expect("the template");
        template.sensitive = true;
        template.preview.clear();
        store.update_item(&template, None).expect("mark sensitive");
        let r = rig_on(store);
        let (_, form) = r.open_form(false);
        assert_eq!(form.get_preview(), "\u{2022}".repeat(8));
        type_text(form.window(), "Alice");
        assert_eq!(
            form.get_preview(),
            "\u{2022}".repeat(8),
            "typing does not unmask it"
        );
    });
}

#[test]
fn typing_the_answers_updates_the_preview_and_enter_in_the_last_field_pastes() {
    on_ui_thread(|| {
        let r = rig();
        let (_, form) = r.open_form(false);
        type_text(form.window(), "Alice");
        assert!(
            form.get_preview()
                .contains("Hi Alice, a gentle reminder that invoice  is due"),
            "the preview follows what is typed: {}",
            form.get_preview()
        );
        press(form.window(), &key(Key::Return));
        type_text(form.window(), "1042");
        assert!(form.get_preview().contains("invoice 1042 is due"));
        assert_eq!(r.ctl.borrow().services().keyboard.sent.len(), 0);
        press(form.window(), &key(Key::Return));

        assert!(!form.window().is_visible(), "the form is off the screen");
        assert!(r.ctl.borrow().fill().is_none());
        let ctl = r.ctl.borrow();
        let Data::Text(text) = &ctl.services().clipboard.written[0].data()[0] else {
            panic!("text on the clipboard")
        };
        assert!(
            text.starts_with("Hi Alice, a gentle reminder that invoice 1042 is due on "),
            "the answers are in the pasted text: {text}"
        );
        assert_eq!(
            ctl.services().keyboard.sent.len(),
            1,
            "Ctrl+V went to the Target"
        );
        assert_eq!(
            r.targets.refocus_count(),
            2,
            "once when the Picker used the template, once for the paste"
        );
    });
}

#[test]
fn shift_enter_in_the_picker_makes_a_form_that_ends_in_a_copy() {
    on_ui_thread(|| {
        let r = rig();
        let (_, form) = r.open_form(true);
        assert_eq!(form.get_submit_label(), "Copy");
        type_text(form.window(), "Bo");
        press(form.window(), &key(Key::Return));
        type_text(form.window(), "7");
        press(form.window(), &key(Key::Return));
        let ctl = r.ctl.borrow();
        assert!(
            ctl.services().keyboard.sent.is_empty(),
            "a copy sends no keys"
        );
        assert_eq!(ctl.services().clipboard.log, ["persistent:1"]);
        assert!(!form.window().is_visible());
    });
}

#[test]
fn a_required_field_left_empty_keeps_the_form_open_and_flags_it() {
    on_ui_thread(|| {
        let r = rig();
        let (_, form) = r.open_form(false);
        type_text(form.window(), "Alice");
        press(form.window(), &key(Key::Return)); // to the Number field
        press(form.window(), &key(Key::Return)); // submit with Number empty
        assert!(form.window().is_visible(), "still open");
        assert!(form.get_showing_errors());
        assert!(r.ctl.borrow().fill().is_some());
        assert_eq!(r.ctl.borrow().services().keyboard.sent.len(), 0);
    });
}

#[test]
fn escape_closes_the_form_and_the_target_gets_the_focus_back() {
    on_ui_thread(|| {
        let r = rig();
        let (_, form) = r.open_form(false);
        // Using the template brought the Target forward once, before the form opened.
        let base = r.targets.refocus_count();
        press(form.window(), &key(Key::Escape));
        assert!(!form.window().is_visible());
        assert!(r.ctl.borrow().fill().is_none());
        assert_eq!(
            r.targets.refocus_count(),
            base + 1,
            "Esc hands the focus back"
        );
        assert_eq!(r.ctl.borrow().services().keyboard.sent.len(), 0);
        assert_eq!(r.ctl.borrow().services().clipboard.written.len(), 0);
    });
}

#[test]
fn a_second_form_starts_with_fresh_fields() {
    on_ui_thread(|| {
        let r = rig();
        let (_, form) = r.open_form(false);
        type_text(form.window(), "Alice");
        press(form.window(), &key(Key::Escape));
        let (_, form) = r.open_form(false);
        assert_eq!(form.get_fields().row_data(0).expect("field").value, "");
        assert!(!form.get_preview().contains("Alice"));
    });
}

#[test]
fn the_form_exposes_roles_and_labels_to_assistive_technology() {
    on_ui_thread(|| {
        let r = rig();
        let (_, form) = r.open_form(false);
        let fields = element(&form, "Fields", AccessibleRole::Groupbox);
        assert_eq!(fields.accessible_label().as_deref(), Some("Fields"));
        let preview = element(&form, "Preview", AccessibleRole::Groupbox);
        assert_eq!(preview.accessible_label().as_deref(), Some("Preview"));
        let insert = element(&form, "Insert", AccessibleRole::Button);
        assert_eq!(insert.accessible_label().as_deref(), Some("Insert"));
    });
}
