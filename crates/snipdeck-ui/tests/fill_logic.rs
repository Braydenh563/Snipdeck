//! Behaviour of the Template fill form through Slint's testing backend: keyboard flows, validation, tab order,
//! answers reported to the host, accessibility roles and labels, and hit targets. Time is mocked, never the real
//! clock.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use std::{cell::RefCell, rc::Rc};

use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementQuery, ElementRoot};
use slint::{ComponentHandle, Model, platform::Key};
use snipdeck_ui::{FormField, TemplateFillWindow};

#[path = "support/mod.rs"]
mod support;
use support::{chord, key, on_ui_thread, press, type_text};

#[path = "support/fill_fixture.rs"]
mod fixture;

const MIN_HIT: f32 = 24.0;

type Log = Rc<RefCell<Vec<String>>>;

/// A populated form whose every callback appends a line to the returned log.
fn start_with(fields: Vec<FormField>) -> (TemplateFillWindow, Log) {
    let ui = TemplateFillWindow::new().expect("form");
    fixture::populate(&ui);
    fixture::set_fields(&ui, fields);
    ui.show().expect("show");
    slint::platform::update_timers_and_animations();
    let log: Log = Rc::default();
    {
        let log = log.clone();
        ui.on_answer_changed(move |i, v| log.borrow_mut().push(format!("answer:{i}:{v}")));
    }
    {
        let log = log.clone();
        ui.on_submitted(move |fields| {
            let values: Vec<String> = fields.iter().map(|f| f.value.to_string()).collect();
            log.borrow_mut()
                .push(format!("submit:{}", values.join("|")));
        });
    }
    {
        let log = log.clone();
        ui.on_cancelled(move || log.borrow_mut().push("cancel".to_string()));
    }
    {
        let log = log.clone();
        ui.on_drag_requested(move || log.borrow_mut().push("drag".to_string()));
    }
    (ui, log)
}

fn start() -> (TemplateFillWindow, Log) {
    start_with(fixture::fields())
}

/// Name (required, empty) then Company (optional): two single-line fields.
fn two_fields() -> Vec<FormField> {
    vec![
        fixture::text("Name", "", true),
        fixture::text("Company", "", false),
    ]
}

fn take(log: &Log) -> Vec<String> {
    std::mem::take(&mut *log.borrow_mut())
}

fn settle() {
    slint::platform::update_timers_and_animations();
}

fn value(ui: &TemplateFillWindow, i: usize) -> String {
    ui.get_fields()
        .row_data(i)
        .expect("field")
        .value
        .to_string()
}

fn all(root: &impl ElementRoot, label: &str, role: AccessibleRole) -> Vec<ElementHandle> {
    ElementHandle::find_by_accessible_label(root, label)
        .filter(|e| e.accessible_role() == Some(role))
        .collect()
}

fn find(root: &impl ElementRoot, label: &str, role: AccessibleRole) -> ElementHandle {
    all(root, label, role)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no {role:?} labelled {label:?}"))
}

/// Every element with `role`, de-duplicated by geometry (the query reports repeated elements more than once).
fn by_role(root: &impl ElementRoot, role: AccessibleRole) -> Vec<ElementHandle> {
    let mut seen = Vec::new();
    ElementQuery::from_root(root)
        .match_descendants()
        .match_accessible_role(role)
        .find_all()
        .into_iter()
        .filter(|e| {
            let (p, s) = (e.absolute_position(), e.size());
            let k = (
                p.x.to_bits(),
                p.y.to_bits(),
                s.width.to_bits(),
                s.height.to_bits(),
                e.accessible_label().map(|l| l.to_string()),
            );
            let fresh = !seen.contains(&k);
            seen.push(k);
            fresh
        })
        .collect()
}

macro_rules! ui_tests {
    ($(fn $name:ident() $body:block)*) => {
        $(
            #[test]
            fn $name() {
                on_ui_thread(|| $body);
            }
        )*
    };
}

ui_tests! {
    // ---- Filling -------------------------------------------------------------------------------

    fn the_first_field_has_keyboard_focus_when_the_form_opens() {
        let (ui, log) = start();
        type_text(ui.window(), "x");
        assert_eq!(value(&ui, 0), "x", "typing lands in the first field and replaces its selected default");
        assert_eq!(take(&log), ["answer:0:x"]);
    }

    fn typing_reports_each_edit_and_keeps_the_fields_in_step() {
        let (ui, log) = start_with(two_fields());
        type_text(ui.window(), "Al");
        assert_eq!(take(&log), ["answer:0:A", "answer:0:Al"]);
        assert_eq!(value(&ui, 0), "Al");
        assert_eq!(value(&ui, 1), "");
    }

    fn defaults_are_the_answers_before_anything_is_typed() {
        let (ui, log) = start();
        ui.invoke_submit();
        assert_eq!(take(&log), ["submit:Ada||Friendly||"], "the choice default is its first option");
    }

    fn enter_moves_to_the_next_field_and_enter_on_the_last_one_submits() {
        let (ui, log) = start_with(two_fields());
        type_text(ui.window(), "Al");
        take(&log);
        press(ui.window(), &key(Key::Return));
        assert!(take(&log).is_empty(), "Enter on the first field only moves on");
        type_text(ui.window(), "Co");
        assert_eq!(take(&log), ["answer:1:C", "answer:1:Co"]);
        press(ui.window(), &key(Key::Return));
        assert_eq!(take(&log), ["submit:Al|Co"], "Enter on the last field inserts");
    }

    fn enter_on_a_choice_moves_on_and_down_arrow_picks_the_next_option() {
        let (ui, log) = start();
        press(ui.window(), &key(Key::Return));
        press(ui.window(), &key(Key::Return));
        take(&log);
        // Name -> Company -> Tone (a choice).
        press(ui.window(), &key(Key::DownArrow));
        assert_eq!(take(&log), ["answer:2:Formal"]);
        assert_eq!(ui.get_fields().row_data(2).unwrap().choice_index, 1);
        press(ui.window(), &key(Key::Return));
        type_text(ui.window(), "2030");
        assert_eq!(value(&ui, 3), "2030", "Enter on the choice moved to the date");
    }

    fn enter_in_a_multiline_field_adds_a_line_and_ctrl_enter_submits() {
        let (ui, log) = start();
        ui.invoke_focus_field(4);
        settle();
        type_text(ui.window(), "a");
        press(ui.window(), &key(Key::Return));
        type_text(ui.window(), "b");
        assert_eq!(value(&ui, 4), "a\nb");
        take(&log);
        chord(ui.window(), &[Key::Control], &key(Key::Return));
        assert_eq!(take(&log), ["submit:Ada||Friendly||a\nb"]);
    }

    fn ctrl_enter_submits_from_any_field() {
        let (ui, log) = start();
        chord(ui.window(), &[Key::Control], &key(Key::Return));
        assert_eq!(take(&log), ["submit:Ada||Friendly||"]);
    }

    fn escape_cancels() {
        let (ui, log) = start();
        press(ui.window(), &key(Key::Escape));
        assert_eq!(take(&log), ["cancel"]);
    }

    fn the_header_close_button_and_the_cancel_button_cancel() {
        let (ui, log) = start();
        find(&ui, "Close", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["cancel"]);
        find(&ui, "Cancel", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["cancel"]);
    }

    fn the_insert_button_submits_and_takes_its_label_from_the_host() {
        let (ui, log) = start();
        find(&ui, "Insert", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["submit:Ada||Friendly||"]);
        ui.set_submit_label("Copy".into());
        settle();
        find(&ui, "Copy", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["submit:Ada||Friendly||"]);
    }

    fn picking_a_choice_with_the_pointer_reports_it_and_updates_the_answer() {
        let (ui, log) = start();
        find(&ui, "Brief", AccessibleRole::RadioButton).invoke_accessible_default_action();
        settle();
        assert_eq!(take(&log), ["answer:2:Brief"]);
        assert_eq!(value(&ui, 2), "Brief");
        assert_eq!(find(&ui, "Brief", AccessibleRole::RadioButton).accessible_checked(), Some(true));
        assert_eq!(find(&ui, "Friendly", AccessibleRole::RadioButton).accessible_checked(), Some(false));
    }

    fn dragging_the_header_asks_the_host_to_move_the_window() {
        let (ui, log) = start();
        // The title text sits on the drag region.
        let title = find(&ui, "Invoice reminder", AccessibleRole::Text);
        title.mock_single_click(slint::platform::PointerEventButton::Left);
        assert_eq!(take(&log), ["drag"]);
        let _ = ui;
    }

    // ---- Validation ----------------------------------------------------------------------------

    fn a_required_field_left_empty_blocks_submitting_flags_it_and_takes_focus() {
        let (ui, log) = start_with(two_fields());
        // Focus is in Name; move to Company so that the flag has to bring it back.
        press(ui.window(), &key(Key::Tab));
        assert!(!ui.get_showing_errors());
        chord(ui.window(), &[Key::Control], &key(Key::Return));
        assert!(take(&log).is_empty(), "nothing submitted");
        assert!(ui.get_showing_errors());
        settle();
        let name = find(&ui, "Name, Required", AccessibleRole::TextInput);
        assert_eq!(
            name.accessible_description().map(|d| d.to_string()).as_deref(),
            Some("This field is required"),
            "the field says what is wrong in text"
        );
        type_text(ui.window(), "Al");
        assert_eq!(value(&ui, 0), "Al", "the invalid field took focus");
        assert_eq!(value(&ui, 1), "");
        assert_eq!(take(&log), ["answer:0:A", "answer:0:Al"]);
    }

    fn the_flag_goes_away_as_soon_as_the_field_is_filled_and_the_form_then_submits() {
        let (ui, log) = start_with(two_fields());
        ui.invoke_submit();
        settle();
        let name = || find(&ui, "Name, Required", AccessibleRole::TextInput);
        assert_eq!(name().accessible_description().map(|d| d.to_string()).as_deref(), Some("This field is required"));
        type_text(ui.window(), "A");
        settle();
        assert_eq!(name().accessible_description().map(|d| d.to_string()).unwrap_or_default(), "");
        take(&log);
        press(ui.window(), &key(Key::Return));
        press(ui.window(), &key(Key::Return));
        assert_eq!(take(&log), ["submit:A|"]);
    }

    fn the_first_of_several_invalid_fields_gets_focus() {
        let fields = vec![
            fixture::text("A", "x", true),
            fixture::text("B", "", true),
            fixture::text("C", "", true),
        ];
        let (ui, log) = start_with(fields);
        ui.invoke_submit();
        settle();
        type_text(ui.window(), "b");
        assert_eq!(value(&ui, 1), "b", "B is the first empty required field");
        assert_eq!(value(&ui, 2), "");
        assert!(take(&log).iter().all(|l| l.starts_with("answer:1:")));
    }

    fn an_optional_empty_field_does_not_block() {
        let (ui, log) = start_with(vec![fixture::text("A", "", false)]);
        ui.invoke_submit();
        assert_eq!(take(&log), ["submit:"]);
        assert!(!ui.get_showing_errors());
    }

    fn a_required_choice_with_nothing_chosen_is_invalid() {
        let mut choice = fixture::choice("Tone", &["Friendly", "Formal"], -1);
        choice.required = true;
        let (ui, log) = start_with(vec![choice]);
        ui.invoke_submit();
        assert!(take(&log).is_empty(), "no callback fired");
        assert!(ui.get_showing_errors());
        find(&ui, "Formal", AccessibleRole::RadioButton).invoke_accessible_default_action();
        take(&log);
        ui.invoke_submit();
        assert_eq!(take(&log), ["submit:Formal"]);
    }

    fn an_error_from_the_host_shows_until_the_field_is_edited() {
        let mut date = fixture::date("Due date");
        date.error = "Use YYYY-MM-DD".into();
        let (ui, _log) = start_with(vec![date]);
        settle();
        let field = || find(&ui, "Due date", AccessibleRole::TextInput);
        assert_eq!(field().accessible_description().map(|d| d.to_string()).as_deref(), Some("Use YYYY-MM-DD"));
        type_text(ui.window(), "2");
        settle();
        assert_eq!(field().accessible_description().map(|d| d.to_string()).unwrap_or_default(), "");
    }

    // ---- Tab order -----------------------------------------------------------------------------

    fn tab_visits_the_fields_in_order_then_cancel_and_insert() {
        let (ui, log) = start();
        // Name -> Company
        press(ui.window(), &key(Key::Tab));
        type_text(ui.window(), "c");
        assert_eq!(value(&ui, 1), "c");
        // Company -> Tone (a choice is one Tab stop; Down picks)
        press(ui.window(), &key(Key::Tab));
        press(ui.window(), &key(Key::DownArrow));
        assert_eq!(ui.get_fields().row_data(2).unwrap().choice_index, 1);
        // Tone -> Due date
        press(ui.window(), &key(Key::Tab));
        type_text(ui.window(), "d");
        assert_eq!(value(&ui, 3), "d");
        // Due date -> Notes
        press(ui.window(), &key(Key::Tab));
        type_text(ui.window(), "n");
        assert_eq!(value(&ui, 4), "n");
        take(&log);
        // Notes -> the header close button is first in tree order, so wrap: the next stops are Cancel, Insert.
        press(ui.window(), &key(Key::Tab));
        press(ui.window(), &key(Key::Return));
        assert_eq!(take(&log), ["cancel"], "Cancel is the stop after the last field");
    }

    fn shift_tab_goes_back_one_field() {
        let (ui, _log) = start();
        press(ui.window(), &key(Key::Tab));
        chord(ui.window(), &[Key::Shift], &key(Key::Backtab));
        type_text(ui.window(), "z");
        assert_eq!(value(&ui, 0), "z", "Tab selects the text, so typing replaces the default");
    }

    // ---- Accessibility -------------------------------------------------------------------------

    fn the_controls_have_roles_and_labels() {
        let (ui, _log) = start();
        settle();
        for (label, role) in [
            ("Name, Required", AccessibleRole::TextInput),
            ("Company", AccessibleRole::TextInput),
            ("Due date", AccessibleRole::TextInput),
            ("Notes", AccessibleRole::TextInput),
            ("Tone", AccessibleRole::RadioGroup),
            ("Friendly", AccessibleRole::RadioButton),
            ("Cancel", AccessibleRole::Button),
            ("Insert", AccessibleRole::Button),
            ("Close", AccessibleRole::Button),
            ("Fields", AccessibleRole::Groupbox),
            ("Preview", AccessibleRole::Groupbox),
        ] {
            assert!(!all(&ui, label, role).is_empty(), "no {role:?} labelled {label:?}");
        }
        assert_eq!(find(&ui, "Friendly", AccessibleRole::RadioButton).accessible_checked(), Some(true));
        assert!(!all(&ui, "Control Enter", AccessibleRole::Text).is_empty(), "the shortcut is announced in words");
    }

    fn the_preview_text_follows_the_host() {
        let (ui, _log) = start();
        settle();
        let first_line = "Hello Ada,\n\nA gentle reminder that the invoice is due.\n\nThanks,\nSnipdeck";
        assert!(!all(&ui, first_line, AccessibleRole::Text).is_empty());
        ui.set_preview("Hello Grace".into());
        settle();
        assert!(!all(&ui, "Hello Grace", AccessibleRole::Text).is_empty());
        assert!(all(&ui, first_line, AccessibleRole::Text).is_empty(), "the old text is gone");
    }

    fn every_target_is_at_least_24px_in_both_directions() {
        let (ui, _log) = start();
        settle();
        let mut checked = 0;
        for role in [
            AccessibleRole::Button,
            AccessibleRole::TextInput,
            AccessibleRole::RadioButton,
        ] {
            for e in by_role(&ui, role) {
                let s = e.size();
                assert!(
                    s.width >= MIN_HIT && s.height >= MIN_HIT,
                    "{role:?} {:?} is {}x{}",
                    e.accessible_label(),
                    s.width,
                    s.height
                );
                checked += 1;
            }
        }
        assert!(checked >= 10, "checked {checked} targets");
    }
}
