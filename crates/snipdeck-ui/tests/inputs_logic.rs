//! Behaviour of the input and navigation components (group B), driven through Slint's testing backend:
//! accessibility tree, keyboard, IME composition, state machines and minimum hit targets.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementRoot};
use slint::{ComponentHandle, platform::Key};
use snipdeck_ui::{
    DialogGallery, DialogVariant, EmptyStateGallery, FormGallery, HotkeyState, InputsGallery,
    TabsGallery,
};

#[path = "support/mod.rs"]
mod support;
use support::{chord, commit, compose, key, on_ui_thread, press, type_text};

const MIN_HIT: f32 = 24.0;

fn start<C: ComponentHandle>(create: impl FnOnce() -> Result<C, slint::PlatformError>) -> C {
    let ui = create().expect("gallery");
    ui.show().expect("show");
    ui
}

fn settle() {
    slint::platform::update_timers_and_animations();
}

fn find(root: &impl ElementRoot, label: &str, role: AccessibleRole) -> ElementHandle {
    all(root, label, role)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no {role:?} labelled {label:?}"))
}

fn all(root: &impl ElementRoot, label: &str, role: AccessibleRole) -> Vec<ElementHandle> {
    ElementHandle::find_by_accessible_label(root, label)
        .filter(|e| e.accessible_role() == Some(role))
        .collect()
}

fn missing(root: &impl ElementRoot, label: &str, role: AccessibleRole) -> bool {
    all(root, label, role).is_empty()
}

/// Slint's testing platform is process-wide and bound to one thread, so every test body runs on the same
/// dedicated UI thread (see `support::on_ui_thread`); a panic there fails the calling test only.
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

// ---------------------------------------------------------------------------------------------
// TextField / SearchField
// ---------------------------------------------------------------------------------------------

fn text_field_exposes_role_label_placeholder_value_and_error() {
    let ui = start(InputsGallery::new);
    let name = find(&ui, "Name", AccessibleRole::TextInput);
    assert_eq!(
        name.accessible_placeholder_text().as_deref(),
        Some("Snippet name")
    );
    assert_eq!(name.accessible_value().as_deref(), Some(""));
    assert_eq!(name.accessible_enabled(), Some(true));

    let error = find(&ui, "Error field", AccessibleRole::TextInput);
    assert_eq!(
        error.accessible_description().as_deref(),
        Some("A name is required")
    );
    let disabled = find(&ui, "Disabled field", AccessibleRole::TextInput);
    assert_eq!(disabled.accessible_enabled(), Some(false));
    assert_eq!(disabled.accessible_value().as_deref(), Some("Quarterly report"));
}

fn accessibility_set_value_edits_the_text() {
    let ui = start(InputsGallery::new);
    let name = find(&ui, "Name", AccessibleRole::TextInput);
    name.set_accessible_value("Invoice");
    assert_eq!(ui.get_name_text(), "Invoice");
    assert_eq!(ui.get_edits(), 1);
    assert_eq!(name.accessible_value().as_deref(), Some("Invoice"));
}

fn typing_edits_and_enter_accepts() {
    let ui = start(InputsGallery::new);
    ui.invoke_focus_name();
    assert!(ui.get_name_has_focus());
    type_text(ui.window(), "abc");
    assert_eq!(ui.get_name_text(), "abc");
    assert_eq!(ui.get_edits(), 3, "one edited per keystroke");
    assert_eq!(ui.get_accepts(), 0);
    press(ui.window(), "\n");
    assert_eq!(ui.get_accepts(), 1);
    press(ui.window(), &key(Key::Backspace));
    assert_eq!(ui.get_name_text(), "ab");
}

fn a_disabled_field_ignores_typing() {
    let ui = start(InputsGallery::new);
    // Focus cannot reach it, so the keys land on nothing.
    type_text(ui.window(), "zzz");
    assert_eq!(ui.get_name_text(), "");
    assert_eq!(ui.get_edits(), 0);
}

fn search_clear_button_exists_only_while_there_is_text_and_clears() {
    let ui = start(InputsGallery::new);
    assert!(missing(&ui, "Clear search", AccessibleRole::Button));

    ui.invoke_focus_search();
    type_text(ui.window(), "inv");
    settle();
    assert_eq!(ui.get_search_text(), "inv");
    let clear = find(&ui, "Clear search", AccessibleRole::Button);

    assert_eq!(ui.get_search_edits(), 3);
    clear.invoke_accessible_default_action();
    settle();
    assert_eq!(ui.get_search_text(), "");
    assert_eq!(ui.get_clears(), 1);
    assert_eq!(ui.get_search_edits(), 4, "clearing is an edit");
    assert!(missing(&ui, "Clear search", AccessibleRole::Button));
    assert!(
        ui.get_search_has_focus(),
        "clearing keeps the focus in the field"
    );
}

fn escape_clears_a_non_empty_search_and_leaves_an_empty_one_alone() {
    let ui = start(InputsGallery::new);
    ui.invoke_focus_search();
    type_text(ui.window(), "inv");
    press(ui.window(), &key(Key::Escape));
    assert_eq!(ui.get_search_text(), "");
    assert_eq!(ui.get_clears(), 1);
    // Empty now: Esc is not consumed, so it can close the panel.
    press(ui.window(), &key(Key::Escape));
    assert_eq!(ui.get_clears(), 1);
}

fn escape_does_not_clear_while_composing() {
    let ui = start(InputsGallery::new);
    ui.invoke_focus_search();
    type_text(ui.window(), "inv");
    compose(ui.window(), "nihon");
    assert!(ui.get_search_has_focus());
    press(ui.window(), &key(Key::Escape));
    assert_eq!(ui.get_clears(), 0, "Esc belongs to the IME while composing");
    assert_eq!(ui.get_search_text(), "inv");
}

fn search_field_role_label_and_placeholder() {
    let ui = start(InputsGallery::new);
    let search = find(&ui, "Search", AccessibleRole::TextInput);
    assert_eq!(
        search.accessible_placeholder_text().as_deref(),
        Some("Search snippets")
    );
    let filled = find(&ui, "Search filled", AccessibleRole::TextInput);
    assert_eq!(filled.accessible_value().as_deref(), Some("invoice"));
    assert!(!missing(&ui, "Clear filled", AccessibleRole::Button));
}

fn text_entry_hit_targets_are_at_least_24px() {
    let ui = start(InputsGallery::new);
    let search = find(&ui, "Search filled", AccessibleRole::TextInput);
    let clear = find(&ui, "Clear filled", AccessibleRole::Button);
    let field = find(&ui, "Name", AccessibleRole::TextInput);
    assert!(field.size().height >= MIN_HIT, "field {:?}", field.size());
    assert!(search.size().height >= MIN_HIT);
    let s = clear.size();
    assert!(
        s.width >= MIN_HIT && s.height >= MIN_HIT,
        "clear button {s:?}"
    );
    // The default field is 28px, like Button md.
    assert!((ui.get_name_box_height() - 28.0).abs() < 0.5);
}

fn multi_line_field_is_between_min_and_max_lines_tall() {
    let ui = start(InputsGallery::new);
    // 3 lines of text: min 3 lines (60) + vertical padding (8).
    assert!((ui.get_notes_box_height() - 68.0).abs() < 0.5);
    ui.set_notes_text("a\nb\nc\nd\ne\nf\ng\nh\ni\nj".into());
    settle();
    // Capped at 5 lines (100) + padding (8); the rest scrolls.
    assert!((ui.get_notes_box_height() - 108.0).abs() < 0.5);
    ui.set_notes_text("".into());
    settle();
    assert!((ui.get_notes_box_height() - 68.0).abs() < 0.5);
}

fn a_long_composition_keeps_its_end_visible() {
    let ui = start(InputsGallery::new);
    ui.invoke_focus_narrow();
    let w = ui.get_narrow_visible_width();
    assert!(w > 0.0 && w < 160.0, "narrow viewport is {w}");

    // 1. Caret at the end of existing text.
    ui.set_narrow_text("invoice-2026".into());
    ui.invoke_set_narrow_cursor(12);
    settle();
    compose(ui.window(), "nihongo-tokyo-tokkyo-kyokakyoku-and-more");
    settle();
    assert!(ui.get_narrow_composing());
    let end = ui.get_narrow_end_x();
    assert!(
        end > 0.0 && end <= w,
        "composition end at {end} must lie inside the {w}px viewport"
    );
    assert!(
        ui.get_narrow_scroll_x() < 0.0,
        "the text scrolled to follow the composition"
    );

    // 2. It follows the composition while it grows, never falling behind.
    compose(ui.window(), "nihongo-tokyo-tokkyo-kyokakyoku-and-more-and-even-more");
    settle();
    let end2 = ui.get_narrow_end_x();
    assert!(end2 > 0.0 && end2 <= w, "end at {end2}, viewport {w}");

    // 3. Caret in the middle of the text.
    compose(ui.window(), "");
    ui.set_narrow_text("0123456789abcdefghijklmnopqrstuvwxyz".into());
    ui.invoke_set_narrow_cursor(4);
    settle();
    compose(ui.window(), "nihongo-tokyo-tokkyo-kyokakyoku");
    settle();
    let end3 = ui.get_narrow_end_x();
    assert!(end3 > 0.0 && end3 <= w, "mid-text end at {end3}, viewport {w}");

    // 4. CJK composition (the path real IMEs take): same guarantee.
    compose(ui.window(), "");
    ui.set_narrow_text("".into());
    ui.invoke_set_narrow_cursor(0);
    settle();
    compose(ui.window(), "にほんごにほんごにほんごにほんごにほんご");
    settle();
    let end4 = ui.get_narrow_end_x();
    assert!(end4 > 0.0 && end4 <= w, "CJK end at {end4}, viewport {w}");
}

fn committing_a_composition_inserts_the_text_and_ends_composing() {
    let ui = start(InputsGallery::new);
    ui.invoke_focus_narrow();
    compose(ui.window(), "nihon");
    assert!(ui.get_narrow_composing());
    assert_eq!(ui.get_narrow_text(), "", "a composition is not text yet");
    commit(ui.window(), "日本");
    settle();
    assert!(!ui.get_narrow_composing());
    assert_eq!(ui.get_narrow_text(), "日本");
}

// ---------------------------------------------------------------------------------------------
// Tab / ListTabBar
// ---------------------------------------------------------------------------------------------

fn tab_bar_roles_labels_and_selected_state() {
    let ui = start(TabsGallery::new);
    let bar = find(&ui, "Lists", AccessibleRole::TabList);
    assert_eq!(bar.accessible_item_count(), Some(4));

    // Each bar repeats the names; the first match of each is in the first bar.
    let selected: Vec<_> = ["Work", "Personal", "Code", "Mail"]
        .iter()
        .map(|n| {
            find(&ui, n, AccessibleRole::Tab)
                .accessible_item_selected()
                .unwrap_or_else(|| panic!("{n} has no selected state"))
        })
        .collect();
    assert_eq!(selected, [false, true, false, false]);
    let personal = find(&ui, "Personal", AccessibleRole::Tab);
    assert_eq!(personal.accessible_item_selectable(), Some(true));
    assert_eq!(personal.accessible_item_index(), Some(1));
    assert_eq!(personal.accessible_item_count(), Some(4));
}

fn clicking_or_activating_a_tab_selects_it() {
    let ui = start(TabsGallery::new);
    assert_eq!(ui.get_current(), 1);
    find(&ui, "Code", AccessibleRole::Tab).invoke_accessible_default_action();
    assert_eq!((ui.get_current(), ui.get_picked()), (2, 2));
    // A real pointer click does the same.
    find(&ui, "Work", AccessibleRole::Tab).mock_single_click(slint::platform::PointerEventButton::Left);
    assert_eq!((ui.get_current(), ui.get_picked()), (0, 0));
    assert!(
        find(&ui, "Work", AccessibleRole::Tab)
            .accessible_item_selected()
            .unwrap()
    );
}

fn keyboard_moves_between_tabs_and_clamps_at_the_ends() {
    let ui = start(TabsGallery::new);
    ui.invoke_focus_bar();
    assert!(ui.get_bar_has_focus());
    assert_eq!(ui.get_current(), 1);

    press(ui.window(), &key(Key::RightArrow));
    assert_eq!(ui.get_current(), 2);
    press(ui.window(), &key(Key::End));
    assert_eq!(ui.get_current(), 3);
    press(ui.window(), &key(Key::RightArrow));
    assert_eq!(ui.get_current(), 3, "no wrap past the last tab");
    press(ui.window(), &key(Key::LeftArrow));
    assert_eq!(ui.get_current(), 2);
    press(ui.window(), &key(Key::Home));
    assert_eq!(ui.get_current(), 0);
    press(ui.window(), &key(Key::LeftArrow));
    assert_eq!(ui.get_current(), 0, "no wrap before the first tab");
    assert_eq!(ui.get_picked(), 0);
    // The accessibility tree follows.
    assert!(
        find(&ui, "Work", AccessibleRole::Tab)
            .accessible_item_selected()
            .unwrap()
    );
}

fn the_add_button_is_named_and_reports_clicks() {
    let ui = start(TabsGallery::new);
    let add = find(&ui, "Add list", AccessibleRole::Button);
    add.invoke_accessible_default_action();
    assert_eq!(ui.get_adds(), 1);
}

fn tabs_and_the_add_button_meet_the_minimum_hit_target() {
    let ui = start(TabsGallery::new);
    for name in ["Work", "Personal", "Code", "Mail"] {
        let s = find(&ui, name, AccessibleRole::Tab).size();
        assert!(s.width >= MIN_HIT && s.height >= MIN_HIT, "{name}: {s:?}");
    }
    let s = find(&ui, "Add list", AccessibleRole::Button).size();
    assert!(s.width >= MIN_HIT && s.height >= MIN_HIT, "add: {s:?}");
    assert!((find(&ui, "Personal", AccessibleRole::Tab).size().height - 32.0).abs() < 0.5);
}

fn an_overflowing_bar_scrolls_to_reveal_the_selected_tab() {
    let ui = start(TabsGallery::new);
    settle();
    assert!(ui.get_overflow_scroll_x().abs() < 0.5, "first tab selected: no scroll");
    assert!(
        ui.get_end_scroll_x() < -1.0,
        "last tab selected at start-up must be scrolled into view, scroll-x = {}",
        ui.get_end_scroll_x()
    );
    ui.invoke_select_last_overflow();
    settle();
    assert!(ui.get_overflow_scroll_x() < -1.0);
}

// ---------------------------------------------------------------------------------------------
// HotkeyRecorder
// ---------------------------------------------------------------------------------------------

fn hotkey_recorder_role_label_and_value() {
    let ui = start(FormGallery::new);
    let hk = find(&ui, "Open panel", AccessibleRole::Button);
    assert_eq!(hk.accessible_value().as_deref(), Some("Ctrl+Alt+V"));
    assert_eq!(ui.get_hk_state(), HotkeyState::Idle);
    assert!(hk.size().height >= MIN_HIT);
    let clear = find(&ui, "Clear shortcut", AccessibleRole::Button);
    assert!(clear.size().width >= MIN_HIT && clear.size().height >= MIN_HIT);
}

fn hotkey_recording_state_machine() {
    let ui = start(FormGallery::new);
    let hk = find(&ui, "Open panel", AccessibleRole::Button);

    // idle -> recording
    hk.invoke_accessible_default_action();
    assert!(ui.get_hk_recording());
    assert_eq!(ui.get_hk_state(), HotkeyState::Recording);
    assert_eq!(
        hk.accessible_description().as_deref(),
        Some("Press the new shortcut")
    );

    // Modifiers alone and keys without a modifier keep listening.
    chord(ui.window(), &[Key::Control], &key(Key::Alt));
    assert!(ui.get_hk_recording(), "a modifier alone is not a shortcut");
    press(ui.window(), "k");
    assert!(ui.get_hk_recording(), "a key without a modifier is ignored");
    assert_eq!(ui.get_hk_recorded(), 0);

    // recording -> idle with the combination.
    chord(ui.window(), &[Key::Control, Key::Alt], "k");
    assert_eq!(ui.get_hk_last(), "Ctrl+Alt+K");
    assert_eq!(ui.get_hk_recorded(), 1);
    assert!(!ui.get_hk_recording());
    assert_eq!(ui.get_hk_state(), HotkeyState::Idle);
    assert_eq!(ui.get_hk_cancels(), 0);
}

fn hotkey_combos_name_modifiers_and_special_keys() {
    let ui = start(FormGallery::new);
    let hk = find(&ui, "Open panel", AccessibleRole::Button);
    for (mods, k, expected) in [
        (vec![Key::Shift, Key::Control], key(Key::F5), "Ctrl+Shift+F5"),
        (vec![Key::Meta], " ".to_string(), "Win+Space"),
        (vec![Key::Alt], key(Key::LeftArrow), "Alt+Left"),
        (vec![Key::Control, Key::Alt, Key::Shift], "1".to_string(), "Ctrl+Alt+Shift+1"),
    ] {
        hk.invoke_accessible_default_action();
        assert!(ui.get_hk_recording(), "recording before {expected}");
        chord(ui.window(), &mods, &k);
        assert_eq!(ui.get_hk_last(), expected);
    }
}

fn escape_cancels_recording_without_changing_anything() {
    let ui = start(FormGallery::new);
    let hk = find(&ui, "Open panel", AccessibleRole::Button);
    hk.invoke_accessible_default_action();
    press(ui.window(), &key(Key::Escape));
    assert!(!ui.get_hk_recording());
    assert_eq!(ui.get_hk_cancels(), 1);
    assert_eq!(ui.get_hk_recorded(), 0);
    assert_eq!(ui.get_hk_last(), "");
    assert_eq!(ui.get_hk_state(), HotkeyState::Idle);
    // The keys are untouched.
    assert_eq!(hk.accessible_value().as_deref(), Some("Ctrl+Alt+V"));
}

fn losing_focus_cancels_recording() {
    let ui = start(FormGallery::new);
    find(&ui, "Open panel", AccessibleRole::Button).invoke_accessible_default_action();
    assert!(ui.get_hk_recording());
    // Change handlers compare against the last value they saw; the real event loop runs them between events.
    settle();
    assert!(ui.get_hk_has_focus());
    ui.invoke_focus_choice();
    settle();
    assert!(!ui.get_hk_has_focus());
    assert!(!ui.get_hk_recording());
    assert_eq!(ui.get_hk_cancels(), 1);
}

fn enter_and_space_start_recording_from_the_keyboard() {
    let ui = start(FormGallery::new);
    ui.invoke_focus_hk();
    press(ui.window(), "\n");
    assert!(ui.get_hk_recording());
    press(ui.window(), &key(Key::Escape));
    assert!(!ui.get_hk_recording());
    press(ui.window(), " ");
    assert!(ui.get_hk_recording());
}

fn a_conflict_is_reported_by_the_caller_and_shown_until_cleared() {
    let ui = start(FormGallery::new);
    let hk = find(&ui, "Open panel", AccessibleRole::Button);
    hk.invoke_accessible_default_action();
    chord(ui.window(), &[Key::Control, Key::Alt], "p");
    // The caller checks `recorded` and rejects it.
    ui.set_hk_conflict("Already used by Snipping Tool".into());
    settle();
    assert_eq!(ui.get_hk_state(), HotkeyState::Conflict);
    assert_eq!(
        hk.accessible_description().as_deref(),
        Some("Already used by Snipping Tool")
    );

    // Recording again takes precedence over the old message.
    hk.invoke_accessible_default_action();
    assert_eq!(ui.get_hk_state(), HotkeyState::Recording);
    press(ui.window(), &key(Key::Escape));
    assert_eq!(ui.get_hk_state(), HotkeyState::Conflict);

    ui.set_hk_conflict("".into());
    assert_eq!(ui.get_hk_state(), HotkeyState::Idle);
}

fn clearing_the_shortcut_reports_cleared() {
    let ui = start(FormGallery::new);
    find(&ui, "Clear shortcut", AccessibleRole::Button).invoke_accessible_default_action();
    assert_eq!(ui.get_hk_clears(), 1);
    // Delete does the same from the keyboard.
    ui.invoke_focus_hk();
    press(ui.window(), &key(Key::Delete));
    assert_eq!(ui.get_hk_clears(), 2);
    // With no keys there is nothing to clear.
    ui.set_hk_keys(slint::ModelRc::default());
    settle();
    assert!(missing(&ui, "Clear shortcut", AccessibleRole::Button));
}

// ---------------------------------------------------------------------------------------------
// TemplateField
// ---------------------------------------------------------------------------------------------

fn template_text_and_date_fields_are_labelled_with_their_required_state() {
    let ui = start(FormGallery::new);
    let recipient = find(&ui, "Recipient, Required", AccessibleRole::TextInput);
    assert_eq!(recipient.accessible_placeholder_text().as_deref(), Some("Full name"));
    recipient.set_accessible_value("Ada");
    assert_eq!(ui.get_tf_name(), "Ada");

    // Not required: the label is just the name.
    assert!(!missing(&ui, "Greeting", AccessibleRole::TextInput));

    let reference = find(&ui, "Reference, Required", AccessibleRole::TextInput);
    assert_eq!(
        reference.accessible_description().as_deref(),
        Some("Enter a reference number")
    );

    let due = find(&ui, "Due date", AccessibleRole::TextInput);
    assert_eq!(due.accessible_value().as_deref(), Some("2026-10-17"));
    assert_eq!(due.accessible_placeholder_text().as_deref(), Some("YYYY-MM-DD"));
    let sent = find(&ui, "Sent on", AccessibleRole::TextInput);
    assert_eq!(
        sent.accessible_description().as_deref(),
        Some("That date does not exist")
    );
}

fn template_choice_is_a_radio_group_with_one_checked_option() {
    let ui = start(FormGallery::new);
    let group = find(&ui, "Length, Required", AccessibleRole::RadioGroup);
    assert_eq!(group.accessible_item_count(), Some(3));
    let checked: Vec<_> = ["Short", "Medium", "Long"]
        .iter()
        .map(|n| {
            find(&ui, n, AccessibleRole::RadioButton)
                .accessible_checked()
                .unwrap_or_else(|| panic!("{n} has no checked state"))
        })
        .collect();
    assert_eq!(checked, [false, true, false]);

    find(&ui, "Long", AccessibleRole::RadioButton).invoke_accessible_default_action();
    assert_eq!(ui.get_tf_choice(), 2);
    assert_eq!(ui.get_tf_chosen(), 1);
    assert!(find(&ui, "Long", AccessibleRole::RadioButton).accessible_checked().unwrap());
    assert!(!find(&ui, "Medium", AccessibleRole::RadioButton).accessible_checked().unwrap());
    // Choosing the same option again is not a change.
    find(&ui, "Long", AccessibleRole::RadioButton).invoke_accessible_default_action();
    assert_eq!(ui.get_tf_chosen(), 1);

    let s = find(&ui, "Short", AccessibleRole::RadioButton).size();
    assert!(s.height >= MIN_HIT, "option row {s:?}");
}

fn template_choice_keyboard_navigation() {
    let ui = start(FormGallery::new);
    ui.invoke_focus_choice();
    assert_eq!(ui.get_tf_choice(), 1);
    press(ui.window(), &key(Key::DownArrow));
    assert_eq!(ui.get_tf_choice(), 2);
    press(ui.window(), &key(Key::DownArrow));
    assert_eq!(ui.get_tf_choice(), 2, "no wrap");
    press(ui.window(), &key(Key::Home));
    assert_eq!(ui.get_tf_choice(), 0);
    press(ui.window(), &key(Key::UpArrow));
    assert_eq!(ui.get_tf_choice(), 0);
    press(ui.window(), &key(Key::End));
    assert_eq!(ui.get_tf_choice(), 2);
    assert_eq!(ui.get_tf_chosen(), 3);
}

// ---------------------------------------------------------------------------------------------
// Dialog
// ---------------------------------------------------------------------------------------------

fn dialog_is_a_labelled_group_with_its_body_and_actions() {
    let ui = start(DialogGallery::new);
    let card = find(&ui, "Import snippets?", AccessibleRole::Groupbox);
    assert_eq!(
        card.accessible_description().as_deref(),
        Some("42 snippets from backup.snipdeck will be added to your boards.")
    );
    find(&ui, "Import", AccessibleRole::Button).invoke_accessible_default_action();
    find(&ui, "Cancel", AccessibleRole::Button).invoke_accessible_default_action();
    assert_eq!((ui.get_confirms(), ui.get_cancels()), (1, 1));
    for name in ["Import", "Cancel"] {
        let s = find(&ui, name, AccessibleRole::Button).size();
        assert!(s.width >= MIN_HIT && s.height >= MIN_HIT, "{name}: {s:?}");
    }
}

fn escape_dismisses_and_enter_accepts_the_default_dialog() {
    let ui = start(DialogGallery::new);
    press(ui.window(), "\n");
    assert_eq!((ui.get_accepts(), ui.get_dismissals()), (1, 0));
    press(ui.window(), &key(Key::Escape));
    assert_eq!((ui.get_accepts(), ui.get_dismissals()), (1, 1));
}

fn enter_never_triggers_the_destructive_default() {
    let ui = start(DialogGallery::new);
    ui.set_variant(DialogVariant::Destructive);
    settle();
    let card = find(&ui, "Delete list “Work”?", AccessibleRole::Groupbox);
    assert!(card.accessible_description().is_some());
    press(ui.window(), "\n");
    assert_eq!(ui.get_accepts(), 0, "Enter on a destructive dialog does nothing");
    press(ui.window(), &key(Key::Escape));
    assert_eq!(ui.get_dismissals(), 1, "Esc still dismisses");
    find(&ui, "Delete list", AccessibleRole::Button).invoke_accessible_default_action();
    assert_eq!(ui.get_confirms(), 1);
}

fn a_closed_dialog_is_gone_and_holds_no_keys() {
    let ui = start(DialogGallery::new);
    assert!(
        !find(&ui, "Behind button", AccessibleRole::Button)
            .accessible_enabled()
            .unwrap(),
        "the caller disables what is behind an open dialog"
    );
    ui.set_open(false);
    settle();
    assert!(missing(&ui, "Import snippets?", AccessibleRole::Groupbox));
    assert!(
        find(&ui, "Behind button", AccessibleRole::Button)
            .accessible_enabled()
            .unwrap()
    );
    press(ui.window(), &key(Key::Escape));
    press(ui.window(), "\n");
    assert_eq!((ui.get_dismissals(), ui.get_accepts()), (0, 0));
    ui.set_open(true);
    settle();
    press(ui.window(), &key(Key::Escape));
    assert_eq!(ui.get_dismissals(), 1, "reopening takes the focus again");
}

// ---------------------------------------------------------------------------------------------
// EmptyState
// ---------------------------------------------------------------------------------------------

fn empty_state_has_heading_sentence_and_one_primary_action() {
    let ui = start(EmptyStateGallery::new);
    for heading in ["No snippets yet", "No lists on this board", "Nothing found", "Inbox is empty"] {
        assert!(
            !missing(&ui, heading, AccessibleRole::Text),
            "heading {heading:?}"
        );
    }
    let action = find(&ui, "Save from clipboard", AccessibleRole::Button);
    assert!(action.size().height >= MIN_HIT);
    action.invoke_accessible_default_action();
    assert_eq!(ui.get_actions(), 1);
    // The search kind has no action.
    let buttons = ["Save from clipboard", "Add a list", "Capture a snippet"]
        .iter()
        .filter(|n| !missing(&ui, n, AccessibleRole::Button))
        .count();
    assert_eq!(buttons, 3);
}

}
