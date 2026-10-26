//! Behaviour of the Picker window through Slint's testing backend: keyboard flows, quick-use, scopes, the
//! search callback, empty states, accessibility roles and labels, and hit targets. Time is mocked, never the
//! real clock.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use std::{cell::RefCell, rc::Rc};

use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementQuery, ElementRoot};
use slint::{ComponentHandle, platform::Key};
use snipdeck_ui::{PickerScope, PickerWindow};

#[path = "support/mod.rs"]
mod support;
use support::{chord, commit, compose, key, on_ui_thread, press, type_text};

#[path = "support/picker_fixture.rs"]
mod fixture;

const MIN_HIT: f32 = 24.0;

type Log = Rc<RefCell<Vec<String>>>;

/// A populated Picker whose every callback appends a line to the returned log.
fn start() -> (PickerWindow, Log) {
    let ui = PickerWindow::new().expect("picker");
    fixture::populate(&ui);
    ui.show().expect("show");
    slint::platform::update_timers_and_animations();
    let log: Log = Rc::default();
    macro_rules! hook {
        ($on:ident, || $line:expr) => {{
            let log = log.clone();
            ui.$on(move || log.borrow_mut().push($line));
        }};
        ($on:ident, |$($arg:ident),+| $line:expr) => {{
            let log = log.clone();
            ui.$on(move |$($arg),*| log.borrow_mut().push($line));
        }};
    }
    hook!(on_query_changed, |q| format!("query:{q}"));
    hook!(on_scope_changed, |s| format!("scope:{s:?}"));
    hook!(on_row_activated, |i| format!("use:{i}"));
    hook!(on_row_copied, |i| format!("copy:{i}"));
    hook!(on_quick_use, |n| format!("quick:{n}"));
    hook!(on_selection_moved, |d| format!("move:{d}"));
    hook!(on_edit_requested, |i| format!("edit:{i}"));
    hook!(on_close_requested, || "close".to_string());
    hook!(on_dismissed, || "dismissed".to_string());
    (ui, log)
}

fn take(log: &Log) -> Vec<String> {
    std::mem::take(&mut *log.borrow_mut())
}

fn settle() {
    slint::platform::update_timers_and_animations();
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

fn missing(root: &impl ElementRoot, label: &str, role: AccessibleRole) -> bool {
    all(root, label, role).is_empty()
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

fn rows(ui: &PickerWindow) -> Vec<ElementHandle> {
    by_role(ui, AccessibleRole::ListItem)
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
    // ---- Search --------------------------------------------------------------------------------

    fn keyboard_focus_starts_in_the_search_field_and_typing_reports_each_edit() {
        let (ui, log) = start();
        assert!(ui.get_search_focused(), "search holds focus on start");
        type_text(ui.window(), "inv");
        settle();
        assert_eq!(ui.get_query(), "inv");
        assert_eq!(take(&log), ["query:i", "query:in", "query:inv"]);
        assert_eq!(ui.get_selected_index(), 0, "a new query returns the selection to the first row");
    }

    fn a_new_query_returns_the_selection_to_the_first_row() {
        let (ui, log) = start();
        press(ui.window(), &key(Key::DownArrow));
        press(ui.window(), &key(Key::DownArrow));
        assert_eq!(ui.get_selected_index(), 2);
        take(&log);
        type_text(ui.window(), "r");
        assert_eq!(ui.get_selected_index(), 0);
    }

    fn escape_asks_to_close_whatever_the_search_holds() {
        let (ui, log) = start();
        press(ui.window(), &key(Key::Escape));
        assert_eq!(take(&log), ["close"]);
        type_text(ui.window(), "ab");
        take(&log);
        press(ui.window(), &key(Key::Escape));
        assert_eq!(take(&log), ["close"], "Esc dismisses; it does not clear first");
        assert_eq!(ui.get_query(), "ab", "the host decides what a re-summon shows");
    }

    fn losing_the_keyboard_to_another_window_dismisses_without_the_escape_close() {
        let (ui, log) = start();
        // Start from a window that has no keyboard, then give it the keyboard as Windows does.
        ui.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(false));
        settle();
        ui.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        ui.invoke_focus_search();
        settle();
        assert!(ui.get_search_focused());
        take(&log);
        ui.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(false));
        settle();
        assert!(!ui.get_search_focused(), "an inactive window holds no keyboard focus");
        assert_eq!(take(&log), ["dismissed"], "click-away must not ask for a refocusing close");
    }

    fn reset_gives_a_fresh_picker_for_the_next_summon() {
        let (ui, _log) = start();
        type_text(ui.window(), "ab");
        press(ui.window(), &key(Key::Tab));
        press(ui.window(), &key(Key::DownArrow));
        ui.invoke_reset();
        settle();
        assert_eq!(ui.get_query(), "");
        assert_eq!(ui.get_scope(), PickerScope::All);
        assert_eq!(ui.get_selected_index(), 0);
        assert!(ui.get_search_focused());
    }

    fn an_ime_composition_keeps_enter_and_digits_for_the_composition() {
        let (ui, log) = start();
        compose(ui.window(), "nihon");
        press(ui.window(), &key(Key::Return));
        press(ui.window(), &key(Key::DownArrow));
        assert!(
            take(&log).iter().all(|l| !l.starts_with("use:") && !l.starts_with("move:")),
            "Enter and arrows belong to the input method while composing"
        );
        commit(ui.window(), "x");
        take(&log);
        press(ui.window(), &key(Key::Return));
        assert_eq!(take(&log), ["use:0"], "after the commit Enter uses the row");
    }

    // ---- Selection and use ---------------------------------------------------------------------

    fn arrows_move_the_selection_and_stop_at_the_ends() {
        let (ui, log) = start();
        assert_eq!(ui.get_selected_index(), 0);
        press(ui.window(), &key(Key::UpArrow));
        assert!(take(&log).is_empty(), "already at the top");
        press(ui.window(), &key(Key::DownArrow));
        assert_eq!(ui.get_selected_index(), 1);
        assert_eq!(take(&log), ["move:1"]);
        press(ui.window(), &key(Key::UpArrow));
        assert_eq!(take(&log), ["move:-1"]);
        for _ in 0..10 {
            press(ui.window(), &key(Key::DownArrow));
        }
        assert_eq!(ui.get_selected_index(), 6, "seven rows, last index 6");
        assert_eq!(take(&log).len(), 6, "no event once at the end");
    }

    fn page_keys_move_by_a_page_of_visible_rows() {
        let (ui, log) = start();
        fixture::set_rows(&ui, fixture::numbered_rows(30));
        settle();
        press(ui.window(), &key(Key::PageDown));
        assert_eq!(ui.get_selected_index(), 8, "nine visible rows: a page is eight steps");
        assert_eq!(take(&log), ["move:8"]);
        press(ui.window(), &key(Key::PageUp));
        assert_eq!(ui.get_selected_index(), 0);
    }

    fn the_selected_row_is_exposed_as_selected_to_assistive_technology() {
        let (ui, _log) = start();
        press(ui.window(), &key(Key::DownArrow));
        settle();
        let items = rows(&ui);
        assert_eq!(items.len(), 7);
        let selected: Vec<_> = items
            .iter()
            .filter(|r| r.accessible_item_selected() == Some(true))
            .collect();
        assert_eq!(selected.len(), 1);
        assert!(selected[0]
            .accessible_label()
            .is_some_and(|l| l.starts_with("Invoice reminder")));
    }

    fn a_row_says_which_board_and_list_it_lives_in() {
        let (ui, _log) = start();
        let design = rows(&ui)
            .into_iter()
            .find(|r| r.accessible_label().is_some_and(|l| l.starts_with("Design spec")))
            .expect("Design spec row");
        assert_eq!(
            design.accessible_description().map(|d| d.to_string()).as_deref(),
            Some("Work / Links")
        );
    }

    fn enter_uses_the_selected_row_and_shift_enter_only_copies() {
        let (ui, log) = start();
        press(ui.window(), &key(Key::DownArrow));
        take(&log);
        press(ui.window(), &key(Key::Return));
        assert_eq!(take(&log), ["use:1"]);
        chord(ui.window(), &[Key::Shift], &key(Key::Return));
        assert_eq!(take(&log), ["copy:1"]);
    }

    fn enter_with_no_rows_does_nothing() {
        let (ui, log) = start();
        fixture::set_rows(&ui, Vec::new());
        settle();
        press(ui.window(), &key(Key::Return));
        press(ui.window(), &key(Key::DownArrow));
        chord(ui.window(), &[Key::Control], "e");
        assert!(take(&log).is_empty(), "no callbacks fired");
    }

    fn alt_digits_use_the_nth_row_and_bare_digits_are_always_search_text() {
        let (ui, log) = start();
        chord(ui.window(), &[Key::Alt], "3");
        assert_eq!(take(&log), ["quick:3"]);
        assert_eq!(ui.get_selected_index(), 2, "the used row becomes the selection");
        assert_eq!(ui.get_query(), "", "Alt+digit is not search text");

        chord(ui.window(), &[Key::Alt], "9");
        assert!(take(&log).is_empty(), "seven rows: there is no ninth");

        // A bare digit, even with an empty search, filters: typing "2024" must never paste row 2.
        type_text(ui.window(), "2024");
        assert_eq!(ui.get_query(), "2024", "digits are text");
        assert_eq!(take(&log), ["query:2", "query:20", "query:202", "query:2024"]);
    }

    fn alt_digit_uses_the_nth_row_even_with_a_query() {
        let (ui, log) = start();
        type_text(ui.window(), "re");
        take(&log);
        chord(ui.window(), &[Key::Alt], "2");
        assert_eq!(take(&log), ["quick:2"]);
        assert_eq!(ui.get_query(), "re", "Alt+digit is not search text");
        assert_eq!(ui.get_selected_index(), 1);
    }

    fn ctrl_e_asks_to_edit_the_selected_row() {
        let (ui, log) = start();
        press(ui.window(), &key(Key::DownArrow));
        take(&log);
        chord(ui.window(), &[Key::Control], "e");
        assert_eq!(take(&log), ["edit:1"]);
        assert_eq!(ui.get_query(), "", "the shortcut is not search text");
    }

    fn a_click_on_a_row_uses_it() {
        let (ui, log) = start();
        let design = rows(&ui)
            .into_iter()
            .find(|r| r.accessible_label().is_some_and(|l| l.starts_with("Design spec")))
            .expect("Design spec row");
        design.mock_single_click(slint::platform::PointerEventButton::Left);
        assert_eq!(take(&log), ["use:3"]);
        assert_eq!(ui.get_selected_index(), 3);
    }

    fn the_selection_scrolls_into_view_and_the_first_row_scrolls_back_to_the_top() {
        let (ui, _log) = start();
        fixture::set_rows(&ui, fixture::numbered_rows(30));
        let visible = rows(&ui).len();
        assert!((9..30).contains(&visible), "virtualised: {visible} of 30 rows exist");
        assert!(ui.get_scroll_y().abs() < f32::EPSILON);

        ui.set_selected_index(25);
        let _ = rows(&ui);
        settle();
        // Rows are 28px: row 25 ends at 26 * 28 = 728; the nine-row window shows 252 of it.
        assert!(
            (ui.get_scroll_y() + (26.0 * 28.0 - 9.0 * 28.0)).abs() < 1.0,
            "row 26 is the last visible one: {}",
            ui.get_scroll_y()
        );

        ui.set_selected_index(0);
        settle();
        assert!(ui.get_scroll_y().abs() < f32::EPSILON, "back at the top");
    }

    // ---- Scopes --------------------------------------------------------------------------------

    fn tab_cycles_the_scope_all_board_pinned_and_back() {
        let (ui, log) = start();
        assert_eq!(ui.get_scope(), PickerScope::All);
        press(ui.window(), &key(Key::Tab));
        assert_eq!(ui.get_scope(), PickerScope::Board);
        press(ui.window(), &key(Key::Tab));
        assert_eq!(ui.get_scope(), PickerScope::Pinned);
        press(ui.window(), &key(Key::Tab));
        assert_eq!(ui.get_scope(), PickerScope::All, "wraps around");
        assert_eq!(take(&log), ["scope:Board", "scope:Pinned", "scope:All"]);
        assert!(ui.get_search_focused(), "focus never leaves the search");
    }

    fn shift_tab_goes_back_through_the_scopes() {
        let (ui, log) = start();
        chord(ui.window(), &[Key::Shift], &key(Key::Backtab));
        assert_eq!(ui.get_scope(), PickerScope::Pinned);
        chord(ui.window(), &[Key::Shift], &key(Key::Tab));
        assert_eq!(ui.get_scope(), PickerScope::Board);
        assert_eq!(take(&log), ["scope:Pinned", "scope:Board"]);
    }

    fn changing_the_scope_returns_the_selection_to_the_first_row_and_keeps_the_query() {
        let (ui, _log) = start();
        type_text(ui.window(), "re");
        press(ui.window(), &key(Key::DownArrow));
        press(ui.window(), &key(Key::DownArrow));
        press(ui.window(), &key(Key::Tab));
        assert_eq!(ui.get_selected_index(), 0);
        assert_eq!(ui.get_query(), "re", "the scope narrows the same search");
    }

    fn clicking_a_scope_chip_selects_it_and_keeps_the_keyboard_in_the_search() {
        let (ui, log) = start();
        let board = find(&ui, "This board", AccessibleRole::Button);
        assert_eq!(board.accessible_checked(), Some(false));
        board.invoke_accessible_default_action();
        settle();
        assert_eq!(ui.get_scope(), PickerScope::Board);
        assert_eq!(take(&log), ["scope:Board"]);
        assert!(ui.get_search_focused());
        assert_eq!(find(&ui, "This board", AccessibleRole::Button).accessible_checked(), Some(true));
        assert_eq!(find(&ui, "All", AccessibleRole::Button).accessible_checked(), Some(false));
        // Choosing the scope already chosen is not a change.
        find(&ui, "This board", AccessibleRole::Button).invoke_accessible_default_action();
        assert!(take(&log).is_empty(), "no callback fired");
    }

    // ---- States --------------------------------------------------------------------------------

    fn an_empty_picker_teaches_and_a_search_with_no_results_says_so() {
        let (ui, log) = start();
        fixture::set_rows(&ui, Vec::new());
        settle();
        assert!(!all(&ui, "No snippets yet", AccessibleRole::Text).is_empty());
        assert_eq!(rows(&ui).len(), 0);

        type_text(ui.window(), "zz");
        settle();
        assert!(!all(&ui, "No matches", AccessibleRole::Text).is_empty());
        assert!(!all(&ui, "Nothing matches “zz”.", AccessibleRole::Text).is_empty());
        assert!(missing(&ui, "No snippets yet", AccessibleRole::Text));

        take(&log);
        press(ui.window(), &key(Key::Tab));
        press(ui.window(), &key(Key::Tab));
        settle();
        assert!(!all(&ui, "Nothing pinned matches “zz”.", AccessibleRole::Text).is_empty());
    }

    fn the_window_height_follows_the_results_up_to_nine_rows() {
        let (ui, _log) = start();
        settle();
        let seven = ui.get_content_height();
        fixture::set_rows(&ui, fixture::numbered_rows(3));
        settle();
        assert!((seven - ui.get_content_height() - 4.0 * 28.0).abs() < 0.5, "four rows less");
        fixture::set_rows(&ui, fixture::numbered_rows(40));
        settle();
        let nine = ui.get_content_height();
        assert!((nine - seven - 2.0 * 28.0).abs() < 0.5, "nine rows is the cap: {nine} vs {seven}");
        fixture::set_rows(&ui, Vec::new());
        settle();
        assert!(
            (ui.get_content_height() - (44.0 + 32.0 + 192.0 + 32.0)).abs() < 0.5,
            "the empty state has a fixed 192px body: {}",
            ui.get_content_height()
        );
    }

    // ---- Accessibility -------------------------------------------------------------------------

    fn the_controls_have_roles_and_labels() {
        let (ui, _log) = start();
        let search = find(&ui, "Search all snippets", AccessibleRole::TextInput);
        assert_eq!(search.accessible_placeholder_text().map(|p| p.to_string()).as_deref(), Some("Search all snippets"));
        assert!(
            ElementHandle::find_by_accessible_label(&ui, "Results").count() > 0,
            "the results list is labelled"
        );
        assert!(!all(&ui, "Search in", AccessibleRole::Groupbox).is_empty(), "the scopes are a labelled group");
        for chip in ["All", "This board", "Pinned"] {
            let c = find(&ui, chip, AccessibleRole::Button);
            assert_eq!(c.accessible_checkable(), Some(true), "{chip} reports a checked state");
        }
        assert_eq!(find(&ui, "All", AccessibleRole::Button).accessible_checked(), Some(true));
        assert!(!all(&ui, "Paste", AccessibleRole::Text).is_empty(), "the footer names the use");
        assert!(!all(&ui, "Enter", AccessibleRole::Text).is_empty(), "and the key that does it");
        assert!(!all(&ui, "Shift Enter", AccessibleRole::Text).is_empty());
        assert!(!all(&ui, "Escape", AccessibleRole::Text).is_empty());
        // Digits need Alt: the footer says so, and the number badges read as the key.
        assert!(!all(&ui, "Alt 1 to 9", AccessibleRole::Text).is_empty(), "the footer names Alt+N");
        assert!(!all(&ui, "Alt+1", AccessibleRole::Text).is_empty(), "the first badge reads as Alt+1");
    }

    fn the_footer_names_the_copy_route_when_there_is_no_target() {
        let (ui, _log) = start();
        ui.set_target_name("".into());
        settle();
        assert!(!all(&ui, "Copy", AccessibleRole::Text).is_empty());
        assert!(missing(&ui, "Paste", AccessibleRole::Text));
        assert!(missing(&ui, "Shift Enter", AccessibleRole::Text), "Enter already copies: no second route to hint");
    }

    fn the_sensitive_row_does_not_speak_its_secret() {
        let (ui, _log) = start();
        let secret = rows(&ui)
            .into_iter()
            .find(|r| r.accessible_label().is_some_and(|l| l.starts_with("Server password")))
            .expect("sensitive row");
        let label = secret.accessible_label().unwrap().to_string();
        assert!(!label.contains("hunter2"), "masked: {label}");
    }

    fn every_target_is_at_least_24px_in_both_directions() {
        let (ui, _log) = start();
        fixture::set_rows(&ui, fixture::numbered_rows(12));
        type_text(ui.window(), "a");
        settle();
        let mut checked = 0;
        for role in [AccessibleRole::Button, AccessibleRole::TextInput, AccessibleRole::ListItem] {
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
        assert!(checked >= 12, "checked {checked} targets");
    }
}
