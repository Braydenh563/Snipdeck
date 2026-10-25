//! Behaviour of the Panel window through Slint's testing backend: keyboard flows, quick-use, the search
//! callback, empty states, the board menu, the toast, accessibility roles and labels, and hit targets.
//! Time is mocked, never the real clock.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]
#![allow(
    clippy::cast_precision_loss,
    reason = "pixel sizes and chip counts are far below f32 precision"
)]

use std::{cell::RefCell, rc::Rc, time::Duration};

use i_slint_backend_testing::{
    AccessibleRole, ElementHandle, ElementQuery, ElementRoot, mock_elapsed_time,
};
use slint::{
    ComponentHandle, LogicalPosition, ModelRc, VecModel,
    platform::{Key, PointerEventButton, WindowEvent},
};
use snipdeck_ui::{
    ChoiceEntry, PanelLayout, PanelWindow, PinnedItem, RecentRow, ResizeEdge, ToastKind,
};

#[path = "support/mod.rs"]
mod support;
use support::{chord, key, on_ui_thread, press, type_text};

#[path = "support/panel_fixture.rs"]
mod fixture;

const MIN_HIT: f32 = 24.0;

type Log = Rc<RefCell<Vec<String>>>;

/// A populated Panel whose every callback appends a line to the returned log.
fn start() -> (PanelWindow, Log) {
    let ui = PanelWindow::new().expect("panel");
    fixture::populate(&ui);
    ui.show().expect("show");
    // The design size: tall enough for the chrome (header, tabs, pins, footer) and all six fixture rows.
    ui.window().set_size(slint::LogicalSize::new(320.0, 560.0));
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
    hook!(on_row_activated, |i| format!("use:{i}"));
    hook!(on_row_copied, |i| format!("copy:{i}"));
    hook!(on_quick_use, |n| format!("quick:{n}"));
    hook!(on_selection_moved, |d| format!("move:{d}"));
    hook!(on_tab_selected, |i| format!("tab:{i}"));
    hook!(on_list_menu_requested, |i| format!("list-menu:{i}"));
    hook!(on_list_edit_requested, |i| format!("list-edit:{i}"));
    hook!(on_reveal_requested, |i, on| format!("reveal:{i}:{on}"));
    hook!(on_close_requested, || "close".to_string());
    hook!(on_open_requested, || "open".to_string());
    hook!(on_help_requested, || "help".to_string());
    hook!(on_commands_requested, || "commands".to_string());
    hook!(on_shortcuts_requested, || "shortcuts".to_string());
    hook!(on_drag_requested, || "drag".to_string());
    hook!(on_pin_toggled, |on| format!("pin:{on}"));
    hook!(on_add_list_requested, || "add-list".to_string());
    hook!(on_add_from_clipboard, || "add-from-clipboard".to_string());
    hook!(on_board_menu_requested, || "board-menu".to_string());
    hook!(on_board_menu_activated, |i| format!("board:{i}"));
    hook!(on_board_menu_dismissed, || "board-dismissed".to_string());
    hook!(on_pinned_used, |i| format!("pinned:{i}"));
    hook!(on_toast_undo, || "undo".to_string());
    hook!(on_undo_requested, || "ctrl-z".to_string());
    hook!(on_new_item_requested, || "new".to_string());
    hook!(on_row_picked, |i, shift, ctrl| format!(
        "picked:{i}:{shift}:{ctrl}"
    ));
    hook!(on_select_all_requested, || "select-all".to_string());
    hook!(on_selection_extended, |d| format!("extend:{d}"));
    hook!(on_selection_cleared, || "clear-selection".to_string());
    hook!(on_bulk_move_requested, || "bulk-move".to_string());
    hook!(on_bulk_colour_requested, || "bulk-colour".to_string());
    hook!(on_bulk_delete_requested, || "bulk-delete".to_string());
    hook!(on_bulk_copy_requested, || "bulk-copy".to_string());
    hook!(on_quick_add_requested, |t, k, s| format!(
        "quick-add:{t}:{k}:{s}"
    ));
    hook!(on_quick_add_full, |t| format!("quick-add-full:{t}"));
    ui.on_detect_kind(|t| i32::from(t.starts_with("http")));
    hook!(on_settings_requested, || "settings".to_string());
    hook!(on_toast_hover_changed, |h| format!("hover:{h}"));
    hook!(on_app_menu_requested, || "app-menu".to_string());
    hook!(on_app_menu_activated, |i| format!("app:{i}"));
    hook!(on_app_menu_dismissed, || "app-dismissed".to_string());
    hook!(on_edit_requested, |i| format!("edit:{i}"));
    hook!(on_rename_requested, |i| format!("rename:{i}"));
    hook!(on_duplicate_requested, |i| format!("duplicate:{i}"));
    hook!(on_pin_requested, |i| format!("pin:{i}"));
    hook!(on_delete_requested, |i| format!("delete:{i}"));
    hook!(on_resize_requested, |edge| format!("resize:{edge:?}"));
    hook!(on_pinned_copied, |i| format!("pinned-copy:{i}"));
    hook!(on_handle_hovered, |on| format!("handle-hover:{on}"));
    hook!(on_handle_clicked, || "handle-click".to_string());
    hook!(on_item_reorder_requested, |d| format!("item-reorder:{d}"));
    hook!(on_row_dropped, |from, onto| format!(
        "row-drop:{from}:{onto}"
    ));
    hook!(on_row_dropped_on_tab, |row, tab| format!(
        "row-on-tab:{row}:{tab}"
    ));
    // The host's payload rides as user data; the tests use plain text of the same shape.
    ui.on_row_payload(|i| slint::DataTransfer::from(slint::SharedString::from(format!("row:{i}"))));
    ui.on_payload_row(|d| {
        d.plain_text()
            .ok()
            .and_then(|t| t.strip_prefix("row:").and_then(|n| n.parse::<i32>().ok()))
            .unwrap_or(-1)
    });
    {
        let log = log.clone();
        ui.on_context_menu_requested(move |i, _, _| log.borrow_mut().push(format!("menu:{i}")));
    }
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

/// The list items (rows) in tree order.
fn rows(ui: &PanelWindow) -> Vec<ElementHandle> {
    by_role(ui, AccessibleRole::ListItem)
}

/// A Panel in the quiet List layout (Panel v2 row): comfortable rows, 320 wide.
fn start_quiet() -> (PanelWindow, Log) {
    let (ui, log) = start();
    ui.set_layout(PanelLayout::List);
    settle();
    (ui, log)
}

/// The visible button `label` inside row `row`, if there is one.
fn row_button(ui: &PanelWindow, row: usize, label: &str) -> Option<ElementHandle> {
    let r = &rows(ui)[row];
    let (rp, rs) = (r.absolute_position(), r.size());
    all(ui, label, AccessibleRole::Button)
        .into_iter()
        .find(|b| {
            let (p, s) = (b.absolute_position(), b.size());
            p.y >= rp.y - 0.5 && p.y + s.height <= rp.y + rs.height + 0.5 && p.x >= rp.x - 0.5
        })
}

fn click_at(ui: &PanelWindow, at: LogicalPosition) {
    press_at(ui, at);
    settle();
}

// ---- Pointer helpers ---------------------------------------------------------------------------

fn centre(e: &ElementHandle) -> LogicalPosition {
    let (p, s) = (e.absolute_position(), e.size());
    LogicalPosition::new(p.x + s.width / 2.0, p.y + s.height / 2.0)
}

fn move_to(ui: &PanelWindow, at: LogicalPosition) {
    ui.window()
        .dispatch_event(WindowEvent::PointerMoved { position: at });
}

fn press_at(ui: &PanelWindow, at: LogicalPosition) {
    move_to(ui, at);
    ui.window().dispatch_event(WindowEvent::PointerPressed {
        position: at,
        button: PointerEventButton::Left,
    });
    ui.window().dispatch_event(WindowEvent::PointerReleased {
        position: at,
        button: PointerEventButton::Left,
    });
}

fn click_row_with(ui: &PanelWindow, row: usize, modifiers: &[Key]) {
    let at = centre(&rows(ui)[row]);
    for m in modifiers {
        ui.window().dispatch_event(WindowEvent::KeyPressed {
            text: char::from(*m).to_string().into(),
        });
    }
    press_at(ui, at);
    for m in modifiers.iter().rev() {
        ui.window().dispatch_event(WindowEvent::KeyReleased {
            text: char::from(*m).to_string().into(),
        });
    }
}

fn wait(ms: u64) {
    mock_elapsed_time(Duration::from_millis(ms));
    settle();
}

/// An empty part of the body (no control under it, no tooltip anchor): the bottom, clear of the footer text.
fn away(_ui: &PanelWindow) -> LogicalPosition {
    // Empty header space between the board picker and the search button: a drag region, no tooltip, no hover.
    LogicalPosition::new(92.0, 20.0)
}

fn set_pinned(ui: &PanelWindow, items: Vec<PinnedItem>) {
    ui.set_pinned(ModelRc::new(VecModel::from(items)));
    settle();
}

/// Same shape as `inputs_logic`: Slint's testing platform is process-wide and bound to one thread.
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

    fn typing_another_key_while_another_control_has_focus_moves_it_into_the_search() {
        let (ui, log) = start();
        press(ui.window(), &key(Key::Tab));
        settle();
        assert!(!ui.get_search_focused(), "Tab left the search field");
        take(&log);

        press(ui.window(), "x");
        settle();
        assert!(ui.get_search_focused(), "the character pulled focus back");
        assert_eq!(ui.get_query(), "x");
        assert_eq!(take(&log), ["query:x"]);
        // Further typing continues at the end of the text.
        type_text(ui.window(), "yz");
        assert_eq!(ui.get_query(), "xyz");
    }

    fn space_on_a_focused_control_is_not_redirected_into_the_search() {
        let (ui, log) = start();
        press(ui.window(), &key(Key::Tab));
        take(&log);
        press(ui.window(), " ");
        assert_eq!(ui.get_query(), "");
        assert!(
            take(&log).iter().all(|l| !l.starts_with("query:")),
            "the space went to the focused control, not into the search"
        );
    }

    fn holding_space_on_the_selected_sensitive_row_reveals_it_and_releasing_hides_it() {
        let (ui, log) = start();
        ui.set_selected_index(4);
        settle();
        take(&log);
        ui.window().dispatch_event(WindowEvent::KeyPressed { text: " ".into() });
        ui.window().dispatch_event(WindowEvent::KeyPressed { text: " ".into() });
        assert_eq!(take(&log), ["reveal:4:true"], "a key repeat does not ask twice");
        assert_eq!(ui.get_query(), "", "the space is not typed into the search");
        ui.window().dispatch_event(WindowEvent::KeyReleased { text: " ".into() });
        assert_eq!(take(&log), ["reveal:4:false"]);
    }

    fn a_reveal_the_host_ended_without_a_key_release_can_be_started_again() {
        // Alt+Tab while Space is held: the release never arrives, the host sets the index to -1.
        let (ui, log) = start();
        ui.set_selected_index(4);
        settle();
        take(&log);
        ui.window().dispatch_event(WindowEvent::KeyPressed { text: " ".into() });
        assert_eq!(take(&log), ["reveal:4:true"]);
        ui.set_revealed_index(4);
        settle();
        ui.set_revealed_index(-1);
        settle();
        ui.window().dispatch_event(WindowEvent::KeyPressed { text: " ".into() });
        assert_eq!(take(&log), ["reveal:4:true"], "held-reveal was reset, so Space asks again");
    }

    fn space_on_a_plain_selected_row_does_not_reveal() {
        let (ui, log) = start();
        ui.set_selected_index(0);
        settle();
        take(&log);
        press(ui.window(), " ");
        // Not a reveal, and the collapsed search does not start with a blank: Space does nothing at rest.
        assert_eq!(take(&log), Vec::<String>::new(), "no reveal for a plain row; no search opened by a space");
        assert!(!ui.get_search_focused() || ui.get_query() == "");
    }

    fn escape_clears_the_search_first_and_then_asks_to_close() {
        let (ui, log) = start();
        type_text(ui.window(), "ab");
        take(&log);
        press(ui.window(), &key(Key::Escape));
        assert_eq!(ui.get_query(), "");
        assert_eq!(take(&log), ["query:"], "first Esc restores the list");
        press(ui.window(), &key(Key::Escape));
        assert_eq!(take(&log), ["close"], "second Esc hides the Panel");
    }

    fn ctrl_f_focuses_and_selects_the_search_text() {
        let (ui, _log) = start();
        type_text(ui.window(), "abc");
        press(ui.window(), &key(Key::Tab));
        assert!(!ui.get_search_focused());
        chord(ui.window(), &[Key::Control], "f");
        assert!(ui.get_search_focused());
        // Typing replaces the selected text.
        type_text(ui.window(), "z");
        assert_eq!(ui.get_query(), "z");
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
        assert_eq!(ui.get_selected_index(), 5, "six rows, last index 5");
        assert_eq!(take(&log).len(), 5, "no event once at the end");
    }

    fn the_selected_row_is_exposed_as_selected_to_assistive_technology() {
        let (ui, _log) = start();
        press(ui.window(), &key(Key::DownArrow));
        settle();
        let items = rows(&ui);
        assert_eq!(items.len(), 6);
        let selected: Vec<_> = items
            .iter()
            .filter(|r| r.accessible_item_selected() == Some(true))
            .collect();
        assert_eq!(selected.len(), 1);
        assert!(selected[0]
            .accessible_label()
            .is_some_and(|l| l.starts_with("Invoice reminder")));
    }

    fn f1_asks_for_the_user_guide() {
        let (ui, log) = start();
        take(&log);
        press(ui.window(), &key(Key::F1));
        assert_eq!(take(&log), ["help"]);
    }

    fn ctrl_k_asks_for_the_commands_and_ctrl_slash_for_the_shortcuts() {
        let (ui, log) = start();
        take(&log);
        chord(ui.window(), &[Key::Control], "k");
        assert_eq!(take(&log), ["commands"]);
        chord(ui.window(), &[Key::Control], "/");
        assert_eq!(take(&log), ["shortcuts"]);
    }

    fn the_new_list_close_settings_and_undo_shortcuts_report_what_was_asked() {
        let (ui, log) = start();
        take(&log);
        chord(ui.window(), &[Key::Control, Key::Shift], "N");
        assert_eq!(take(&log), ["quick-add-full:"], "the full editor, prefilled with the box");
        chord(ui.window(), &[Key::Control], ",");
        assert_eq!(take(&log), ["settings"]);
        chord(ui.window(), &[Key::Control], "w");
        assert_eq!(take(&log), ["close"]);
        chord(ui.window(), &[Key::Control], "z");
        assert_eq!(take(&log), ["ctrl-z"]);
        type_text(ui.window(), "inv");
        settle();
        take(&log);
        chord(ui.window(), &[Key::Control], "z");
        assert!(!take(&log).contains(&"ctrl-z".to_string()), "typed text undoes itself");
    }

    // ---- Selection, bulk bar, quick add (O45) ------------------------------------------------------

    fn ctrl_click_on_a_row_picks_it_instead_of_using_it() {
        let (ui, log) = start();
        take(&log);
        click_row_with(&ui, 1, &[Key::Control]);
        assert_eq!(take(&log), ["picked:1:false:true"]);
    }

    fn shift_click_still_copies_until_a_selection_exists_then_it_extends() {
        let (ui, log) = start();
        take(&log);
        click_row_with(&ui, 1, &[Key::Shift]);
        assert_eq!(take(&log), ["copy:1"]);
        ui.set_marked_count(1);
        settle();
        click_row_with(&ui, 2, &[Key::Shift]);
        assert_eq!(take(&log), ["picked:2:true:false"]);
    }

    fn a_plain_click_picks_while_rows_are_selected_and_uses_nothing() {
        let (ui, log) = start();
        ui.set_marked_count(2);
        settle();
        take(&log);
        click_row_with(&ui, 0, &[]);
        assert_eq!(take(&log), ["picked:0:false:false"]);
    }

    fn ctrl_a_selects_all_but_not_while_a_query_is_typed() {
        let (ui, log) = start();
        take(&log);
        chord(ui.window(), &[Key::Control], "a");
        assert_eq!(take(&log), ["select-all"]);
        ui.set_query("inv".into());
        settle();
        chord(ui.window(), &[Key::Control], "a");
        assert!(!take(&log).contains(&"select-all".to_string()), "Ctrl+A is the search field's with a query");
    }

    fn shift_arrows_extend_the_selection() {
        let (ui, log) = start();
        take(&log);
        chord(ui.window(), &[Key::Shift], &key(Key::DownArrow));
        assert_eq!(take(&log), ["extend:1"]);
        chord(ui.window(), &[Key::Shift], &key(Key::UpArrow));
        assert_eq!(take(&log), ["extend:-1"]);
    }

    fn escape_clears_a_selection_and_delete_removes_all_of_it() {
        let (ui, log) = start();
        ui.set_marked_count(2);
        settle();
        take(&log);
        press(ui.window(), &key(Key::Delete));
        assert_eq!(take(&log), ["bulk-delete"], "one call for the whole selection, no confirm");
        press(ui.window(), &key(Key::Escape));
        assert_eq!(take(&log), ["clear-selection"]);
    }

    fn the_bulk_bar_names_the_count_and_each_button_reports_its_action() {
        let (ui, log) = start();
        assert!(missing(&ui, "2 selected", AccessibleRole::Groupbox), "no bar without a selection");
        ui.set_marked_count(2);
        settle();
        assert!(!missing(&ui, "2 selected", AccessibleRole::Groupbox));
        take(&log);
        for (label, line) in [
            ("Move to…", "bulk-move"),
            ("Colour", "bulk-colour"),
            ("Copy all", "bulk-copy"),
            ("Delete", "bulk-delete"),
            ("Clear selection", "clear-selection"),
        ] {
            find(&ui, label, AccessibleRole::Button).invoke_accessible_default_action();
            assert_eq!(take(&log), [line], "{label}");
        }
    }

    fn the_selected_row_keeps_its_more_button_without_hover() {
        let (ui, _log) = start();
        // The kebab plus the ⋯ of the selected row only: the quiet rows draw their actions on the selected or
        // hovered row (spec 2.5), so the others have none.
        assert_eq!(all(&ui, "More", AccessibleRole::Button).len(), 2);
    }

    fn the_new_item_row_opens_a_box_that_enter_saves_and_shift_enter_extends() {
        let (ui, log) = start();
        ui.set_add_list_name("Work".into());
        settle();
        assert!(!missing(&ui, "New item in Work", AccessibleRole::Button), "a labelled row under the items");
        chord(ui.window(), &[Key::Control], "n");
        settle();
        take(&log);
        type_text(ui.window(), "hello");
        chord(ui.window(), &[Key::Shift], &key(Key::Return));
        type_text(ui.window(), "world");
        press(ui.window(), &key(Key::Return));
        assert_eq!(take(&log), ["quick-add:hello\nworld:0:false"]);
    }

    fn quick_add_preselects_link_for_a_url_and_create_and_escape_work() {
        let (ui, log) = start();
        ui.set_add_list_name("Work".into());
        chord(ui.window(), &[Key::Control], "n");
        settle();
        type_text(ui.window(), "https://example.com");
        take(&log);
        find(&ui, "Create item", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["quick-add:https://example.com:1:false"]);
        type_text(ui.window(), "x");
        press(ui.window(), &key(Key::Escape));
        settle();
        assert!(!missing(&ui, "New item in Work", AccessibleRole::Button), "Esc folds the box back to the row");
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
        assert!(take(&log).is_empty(), "no callbacks fired");
    }

    fn alt_digits_use_the_nth_row_and_bare_digits_are_always_search_text() {
        let (ui, log) = start();
        chord(ui.window(), &[Key::Alt], "3");
        assert_eq!(take(&log), ["quick:3"]);
        assert_eq!(ui.get_selected_index(), 2, "the used row becomes the selection");
        assert_eq!(ui.get_query(), "", "Alt+digit is not search text");

        chord(ui.window(), &[Key::Alt], "9");
        assert!(take(&log).is_empty(), "six rows: there is no ninth");

        // A bare digit, even with an empty search, filters: typing "2024" must never paste row 2.
        type_text(ui.window(), "2024");
        assert_eq!(ui.get_query(), "2024", "digits are text");
        let seen = take(&log);
        assert_eq!(seen, ["query:2", "query:20", "query:202", "query:2024"]);

        // Alt+digit still works with a query.
        chord(ui.window(), &[Key::Alt], "1");
        assert_eq!(take(&log), ["quick:1"]);
        assert_eq!(ui.get_query(), "2024");
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

    fn the_menu_key_asks_for_the_selected_rows_context_menu() {
        let (ui, log) = start();
        press(ui.window(), &key(Key::DownArrow));
        take(&log);
        press(ui.window(), &key(Key::Menu));
        assert_eq!(take(&log), ["menu:1"]);
        chord(ui.window(), &[Key::Shift], &key(Key::F10));
        assert_eq!(take(&log), ["menu:1"]);
    }

    fn the_selection_scrolls_into_view_and_the_first_row_scrolls_back_to_the_top() {
        let (ui, _log) = start();
        fixture::set_rows(&ui, fixture::numbered_rows(30));
        // A query instantiates the visible rows (the ListView creates only what it shows).
        let visible = rows(&ui).len();
        assert!((6..30).contains(&visible), "virtualised: {visible} of 30 rows exist");
        assert!(ui.get_scroll_y().abs() < f32::EPSILON);

        ui.set_selected_index(25);
        let _ = rows(&ui);
        settle();
        let _ = rows(&ui);
        assert!(ui.get_scroll_y() < -300.0, "scrolled down: {}", ui.get_scroll_y());
        let labels: Vec<String> = rows(&ui)
            .iter()
            .filter_map(|r| r.accessible_label().map(|l| l.to_string()))
            .collect();
        assert!(
            labels.iter().any(|l| l.starts_with("Snippet 26,")),
            "the selected row exists after the jump: {labels:?}"
        );

        ui.set_selected_index(0);
        let _ = rows(&ui);
        settle();
        assert!(ui.get_scroll_y().abs() < f32::EPSILON, "back at the top");
    }

    // ---- Lists ---------------------------------------------------------------------------------

    fn ctrl_tab_cycles_the_lists_and_leaves_the_search() {
        let (ui, log) = start();
        type_text(ui.window(), "a");
        take(&log);
        chord(ui.window(), &[Key::Control], &key(Key::Tab));
        assert_eq!(ui.get_current_tab(), 1);
        assert_eq!(take(&log), ["query:", "tab:1"], "switching list leaves the board-wide search");
        assert_eq!(ui.get_query(), "");
        chord(ui.window(), &[Key::Control], &key(Key::Tab));
        chord(ui.window(), &[Key::Control], &key(Key::Tab));
        assert_eq!(ui.get_current_tab(), 0, "wraps around");
        take(&log);
        chord(ui.window(), &[Key::Control, Key::Shift], &key(Key::Backtab));
        assert_eq!(ui.get_current_tab(), 2, "Ctrl+Shift+Tab goes back");
        assert_eq!(take(&log), ["tab:2"]);
        chord(ui.window(), &[Key::Control, Key::Shift], &key(Key::Tab));
        assert_eq!(ui.get_current_tab(), 1);
    }

    fn clicking_a_tab_selects_it() {
        let (ui, log) = start();
        let tab = find(&ui, "Code", AccessibleRole::Tab);
        tab.invoke_accessible_default_action();
        assert_eq!(ui.get_current_tab(), 2);
        assert_eq!(take(&log), ["tab:2"]);
    }

    fn dragging_a_row_onto_a_tab_reports_the_drop_and_a_plain_click_still_uses_the_row() {
        use slint::platform::{PointerEventButton, WindowEvent};
        let (ui, log) = start();
        let _ = take(&log);
        let row = by_role(&ui, AccessibleRole::ListItem)
            .into_iter()
            .find(|e| e.accessible_label().is_some_and(|l| l.starts_with("Thanks, received")))
            .map(|e| centre(&e))
            .expect("the first row");
        let tab = centre(&find(&ui, "Code", AccessibleRole::Tab));
        let w = ui.window();
        w.dispatch_event(WindowEvent::PointerMoved { position: row });
        w.dispatch_event(WindowEvent::PointerPressed { position: row, button: PointerEventButton::Left });
        w.dispatch_event(WindowEvent::PointerMoved { position: slint::LogicalPosition::new(row.x, row.y + 12.0) });
        w.dispatch_event(WindowEvent::PointerMoved { position: tab });
        w.dispatch_event(WindowEvent::PointerReleased { position: tab, button: PointerEventButton::Left });
        settle();
        assert_eq!(take(&log), ["row-on-tab:0:2"]);
        press_at(&ui, row);
        assert_eq!(take(&log), ["use:0"], "a click still uses the row");
    }

    fn dragging_a_row_onto_another_row_reports_the_drop_only_while_rows_can_be_arranged() {
        use slint::platform::{PointerEventButton, WindowEvent};
        let (ui, log) = start();
        let rows: Vec<_> = by_role(&ui, AccessibleRole::ListItem)
            .into_iter()
            .map(|e| centre(&e))
            .collect();
        assert!(rows.len() >= 3, "the fixture shows rows");
        let (from, onto) = (rows[0], rows[2]);
        let drag = |ui: &PanelWindow| {
            let w = ui.window();
            w.dispatch_event(WindowEvent::PointerMoved { position: from });
            w.dispatch_event(WindowEvent::PointerPressed { position: from, button: PointerEventButton::Left });
            w.dispatch_event(WindowEvent::PointerMoved { position: slint::LogicalPosition::new(from.x, from.y + 12.0) });
            w.dispatch_event(WindowEvent::PointerMoved { position: onto });
            w.dispatch_event(WindowEvent::PointerReleased { position: onto, button: PointerEventButton::Left });
            settle();
        };
        let _ = take(&log);
        ui.set_rows_reorderable(false);
        drag(&ui);
        assert!(take(&log).is_empty(), "a sorted list or a search takes no drop");
        ui.set_rows_reorderable(true);
        drag(&ui);
        assert_eq!(take(&log), ["row-drop:0:2"]);
    }

    fn alt_up_and_alt_down_ask_to_move_the_selected_row_and_plain_arrows_only_select() {
        let (ui, log) = start();
        let _ = take(&log);
        ui.set_rows_reorderable(false);
        chord(ui.window(), &[Key::Alt], &key(Key::DownArrow));
        assert!(take(&log).is_empty(), "a sorted list or a search leaves the key alone");
        ui.set_rows_reorderable(true);
        chord(ui.window(), &[Key::Alt], &key(Key::DownArrow));
        chord(ui.window(), &[Key::Alt], &key(Key::UpArrow));
        assert_eq!(take(&log), ["item-reorder:1", "item-reorder:-1"]);
        press(ui.window(), &key(Key::DownArrow));
        assert!(take(&log).iter().all(|l| !l.starts_with("item-reorder")));
    }

    fn a_list_icon_on_a_tab_does_not_change_its_name() {
        use slint::{Global, Model};
        let (ui, _log) = start();
        let icons = snipdeck_ui::ListIcons::get(&ui).get_all();
        assert_eq!(icons.row_count(), 24);
        let mut tabs: Vec<snipdeck_ui::TabItem> = ui.get_tabs().iter().collect();
        tabs[2].icon = icons.row_data(12).expect("the code icon");
        ui.set_tabs(slint::ModelRc::new(slint::VecModel::from(tabs)));
        slint::platform::update_timers_and_animations();
        let tab = find(&ui, "Code", AccessibleRole::Tab);
        assert_eq!(tab.accessible_label().as_deref(), Some("Code"));
        assert!(tab.size().height >= 24.0);
    }

    fn right_clicking_a_tab_asks_for_its_list_menu_and_the_tab_says_it_has_one() {
        let (ui, log) = start();
        let tab = find(&ui, "Code", AccessibleRole::Tab);
        assert_eq!(
            tab.accessible_description().as_deref(),
            Some("Right-click or press Menu for list actions")
        );
        let at = centre(&tab);
        move_to(&ui, at);
        ui.window().dispatch_event(WindowEvent::PointerPressed {
            position: at,
            button: PointerEventButton::Right,
        });
        ui.window().dispatch_event(WindowEvent::PointerReleased {
            position: at,
            button: PointerEventButton::Right,
        });
        assert_eq!(take(&log), ["list-menu:2"], "no selection change either");
        assert_eq!(ui.get_current_tab(), 0);
    }

    // ---- Header --------------------------------------------------------------------------------

    fn pressing_the_header_starts_a_window_drag_but_pressing_a_button_does_not() {
        use slint::{LogicalPosition, platform::{PointerEventButton, WindowEvent}};
        let (ui, log) = start();
        let w = ui.window();
        // Empty header space between the board switcher and the buttons.
        let at = LogicalPosition::new(92.0, 10.0);
        w.dispatch_event(WindowEvent::PointerPressed { position: at, button: PointerEventButton::Left });
        w.dispatch_event(WindowEvent::PointerReleased { position: at, button: PointerEventButton::Left });
        assert_eq!(take(&log), ["drag"]);

        find(&ui, "Close panel (Esc)", AccessibleRole::Button).mock_single_click(PointerEventButton::Left);
        assert_eq!(take(&log), ["close"], "the close button is not a drag handle");
    }

    fn the_pin_button_toggles_keep_on_top() {
        let (ui, log) = start();
        let pin = find(&ui, "Keep panel on top", AccessibleRole::Button);
        assert_eq!(pin.accessible_checkable(), Some(true));
        assert_eq!(pin.accessible_checked(), Some(true), "the Panel is on top by default");
        pin.invoke_accessible_default_action();
        assert!(!ui.get_on_top());
        assert_eq!(take(&log), ["pin:false"]);
        assert_eq!(find(&ui, "Keep panel on top", AccessibleRole::Button).accessible_checked(), Some(false));
    }

    fn ctrl_shift_p_toggles_keep_on_top_like_the_pin_button_and_does_not_pin_the_row() {
        let (ui, log) = start();
        press(ui.window(), &key(Key::DownArrow));
        take(&log);
        assert!(ui.get_on_top(), "the Panel is on top by default");
        chord(ui.window(), &[Key::Control, Key::Shift], "P");
        assert!(!ui.get_on_top());
        assert_eq!(take(&log), ["pin:false"], "the pin button's callback, not the row's");
        assert_eq!(
            find(&ui, "Keep panel on top", AccessibleRole::Button).accessible_checked(),
            Some(false),
            "the button shows the new state"
        );
        chord(ui.window(), &[Key::Control, Key::Shift], "P");
        assert!(ui.get_on_top());
        assert_eq!(take(&log), ["pin:true"]);
        assert_eq!(ui.get_query(), "", "the key does not type");
        // Ctrl+P alone still pins the selected row and leaves the keep-on-top state alone.
        chord(ui.window(), &[Key::Control], "p");
        assert_eq!(take(&log), ["pin:1"]);
        assert!(ui.get_on_top());
    }

    fn the_board_menu_opens_activates_and_closes_with_escape_without_closing_the_panel() {
        let (ui, log) = start();
        let switcher = find(&ui, "Board: Work", AccessibleRole::Button);
        assert_eq!(switcher.accessible_expandable(), Some(true));
        switcher.invoke_accessible_default_action();
        assert_eq!(take(&log), ["board-menu"]);

        ui.set_board_menu_open(true);
        settle();
        assert_eq!(
            find(&ui, "Board: Work", AccessibleRole::Button).accessible_expanded(),
            Some(true)
        );
        let _menu = find(&ui, "Boards", AccessibleRole::List);
        find(&ui, "Personal", AccessibleRole::ListItem).invoke_accessible_default_action();
        assert_eq!(take(&log), ["board:1"]);

        press(ui.window(), &key(Key::Escape));
        assert_eq!(take(&log), ["board-dismissed"], "Esc closes the menu, not the Panel");
        // Keys the menu owns do not move the list selection.
        press(ui.window(), &key(Key::DownArrow));
        assert_eq!(ui.get_selected_index(), 0);
    }

    // ---- Footer and use label ------------------------------------------------------------------

    fn the_footer_says_where_a_click_goes() {
        let (ui, _log) = start();
        assert!(!missing(&ui, "Pastes into Outlook · New message", AccessibleRole::Text));
        ui.set_target_title("".into());
        settle();
        assert!(!missing(&ui, "Pastes into Outlook", AccessibleRole::Text));
        ui.set_target_name("".into());
        settle();
        assert!(!missing(&ui, "No target: click copies", AccessibleRole::Text));
        assert!(missing(&ui, "Pastes into Outlook", AccessibleRole::Text));
        // A search with no match: the footer says Enter creates the item from the query.
        fixture::set_rows(&ui, Vec::new());
        ui.set_query("invoice no. 7".into());
        settle();
        assert!(!missing(&ui, "Enter creates \u{201c}invoice no. 7\u{201d}", AccessibleRole::Text));
    }

    fn the_footer_sits_fully_inside_the_window_at_the_default_size_for_every_scale() {
        use slint::platform::WindowEvent;
        let (ui, _log) = start();
        for (scale, w_px, h_px) in [(1.0_f32, 320_u32, 480_u32), (1.25, 400, 600), (1.5, 480, 720)] {
            for text in [1.0_f32, 1.5, 2.25] {
                ui.window()
                    .dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: scale });
                ui.global::<snipdeck_ui::SystemLook<'_>>().set_text_scale(text);
                settle();
                ui.window().set_size(slint::PhysicalSize::new(w_px, h_px));
                settle();
                // The testing backend reports element boxes in the window's own (physical) pixels.
                let phys = ui.window().size();
                let px = |n: u32| f32::from(u16::try_from(n).unwrap_or(u16::MAX));
                let win = slint::LogicalSize::new(px(phys.width), px(phys.height));
                let footer = find(&ui, "Pastes into Outlook · New message", AccessibleRole::Text);
                let (pos, size) = (footer.absolute_position(), footer.size());
                let ctx = format!("scale {scale}, text {text}");
                assert!(pos.x >= -0.5 && pos.x + size.width <= win.width + 0.5, "{ctx}: footer inside horizontally {pos:?} {size:?} {win:?}");
                assert!(pos.y >= 0.0 && pos.y + size.height <= win.height + 0.5, "{ctx}: footer inside vertically");
                let open = find(&ui, "Open Snipdeck", AccessibleRole::Button);
                let (p, z) = (open.absolute_position(), open.size());
                assert!(p.x + z.width <= win.width + 0.5, "{ctx}: Open Snipdeck not clipped");
                assert!(p.y + z.height <= win.height + 0.5, "{ctx}: Open Snipdeck inside vertically");
            }
        }
        ui.global::<snipdeck_ui::SystemLook<'_>>().set_text_scale(1.0);
    }

    fn no_row_has_a_paste_or_copy_use_button_the_footer_says_where_a_click_goes() {
        let (ui, _log) = start();
        let use_buttons = |ui: &PanelWindow| {
            by_role(ui, AccessibleRole::Button)
                .into_iter()
                .filter(|e| {
                    e.accessible_label().is_some_and(|l| {
                        (l.starts_with("Paste ") || l.starts_with("Copy ")) && l != "Copy without pasting"
                    })
                })
                .count()
        };
        assert_eq!(use_buttons(&ui), 0, "the row click is Paste: no ghost button (spec 1.3)");
        ui.set_target_name("".into());
        settle();
        assert_eq!(use_buttons(&ui), 0);
        assert!(!missing(&ui, "No target: click copies", AccessibleRole::Text));
    }

    // ---- CJK (measured, never a golden: system fallback fonts differ per machine) -----------------

    fn cjk_rows_stay_hittable_inside_the_panel_in_every_layout() {
        let (ui, _log) = start();
        fixture::set_rows(&ui, fixture::cjk_rows());
        for view in [PanelLayout::List, PanelLayout::Compact, PanelLayout::Cards] {
            ui.set_layout(view);
            settle();
            let list = rows(&ui);
            assert_eq!(list.len(), 3, "{view:?}: three CJK rows");
            let panel_w = ui.window().size().to_logical(ui.window().scale_factor()).width;
            for (row, title) in list.iter().zip(["請求書のお願い", "短い", "会議のフォローアップ"]) {
                let (p, s) = (row.absolute_position(), row.size());
                assert!(s.height >= MIN_HIT, "{view:?} {title}: {} tall", s.height);
                assert!(
                    p.x >= 0.0 && p.x + s.width <= panel_w + 0.5,
                    "{view:?} {title}: x {} w {} in a {panel_w} panel",
                    p.x,
                    s.width
                );
                assert!(
                    row.accessible_label().is_some_and(|l| l.starts_with(title)),
                    "{view:?}: label {:?} starts with {title}",
                    row.accessible_label()
                );
            }
        }
    }

    // ---- The quiet List row (Panel v2 spec 2.5) -----------------------------------------------------------------

    fn row_click_pastes_once_and_shift_click_copies() {
        let (ui, log) = start_quiet();
        take(&log);
        click_row_with(&ui, 2, &[]);
        assert_eq!(take(&log), ["use:2"], "one click, one paste, no expand state");
        click_row_with(&ui, 2, &[Key::Shift]);
        assert_eq!(take(&log), ["copy:2"]);
    }

    fn action_click_does_not_paste() {
        let (ui, log) = start_quiet();
        // Row 0 is selected, so its actions show without a hover.
        take(&log);
        let copy = row_button(&ui, 0, "Copy").expect("Copy on the selected row");
        click_at(&ui, centre(&copy));
        assert_eq!(take(&log), ["copy:0"], "Copy copies and never pastes");
        let edit = row_button(&ui, 0, "Edit").expect("Edit on the selected row");
        click_at(&ui, centre(&edit));
        assert_eq!(take(&log), ["edit:0"], "Edit only edits");
        let more = row_button(&ui, 0, "More").expect("More on the selected row");
        click_at(&ui, centre(&more));
        assert_eq!(take(&log), ["menu:0"], "More only opens the row menu");
    }

    fn actions_show_on_hover_and_on_the_selected_row_only() {
        let (ui, _log) = start_quiet();
        assert!(row_button(&ui, 0, "Edit").is_some(), "the selected row");
        assert!(row_button(&ui, 3, "Edit").is_none(), "an idle row has no actions");
        move_to(&ui, centre(&rows(&ui)[3]));
        settle();
        for label in ["Copy", "Edit", "More"] {
            assert!(row_button(&ui, 3, label).is_some(), "{label} on the hovered row");
        }
    }

    fn the_title_keeps_its_width_when_the_actions_show() {
        let (ui, _log) = start_quiet();
        let title = |ui: &PanelWindow| {
            let r = &rows(ui)[3];
            let t = all(ui, "Design spec", AccessibleRole::Text)
                .into_iter()
                .find(|t| t.absolute_position().y >= r.absolute_position().y && t.absolute_position().y < r.absolute_position().y + r.size().height)
                .expect("the title text of row 3");
            (t.size().width, t.absolute_position().x)
        };
        let idle = title(&ui);
        move_to(&ui, centre(&rows(&ui)[3]));
        settle();
        assert_eq!(title(&ui), idle, "hovering the row does not move or crush the title");
    }

    fn actions_sit_on_the_preview_line_and_never_on_the_title_line() {
        let (ui, _log) = start_quiet();
        let r = &rows(&ui)[0];
        let (top, h) = (r.absolute_position().y, r.size().height);
        assert!((h - 56.0).abs() < 0.5, "a List row is 56 tall, got {h}");
        // Title line: y 4 to 24 inside the row. The preview line's hit row: 24 to 56.
        for label in ["Copy", "Edit", "More"] {
            let b = row_button(&ui, 0, label).expect(label);
            let (y, bh) = (b.absolute_position().y - top, b.size().height);
            assert!(y >= 24.0 - 0.5 && y + bh <= 56.0 + 0.5, "{label}: y {y}..{} not inside the preview line 24..56", y + bh);
            assert!(bh >= 32.0 - 0.5 && b.size().width >= 32.0 - 0.5, "{label} has a 32px hit box");
        }
    }

    fn copy_action_is_hidden_without_a_target() {
        let (ui, _log) = start_quiet();
        assert!(row_button(&ui, 0, "Copy").is_some());
        ui.set_target_name("".into());
        settle();
        assert!(row_button(&ui, 0, "Copy").is_none(), "the click already copies");
        assert!(row_button(&ui, 0, "Edit").is_some() && row_button(&ui, 0, "More").is_some());
    }

    fn the_sensitive_eye_is_always_visible_and_holds_to_reveal_without_using_the_row() {
        let (ui, log) = start_quiet();
        // Row 4 is the sensitive one: neither selected nor hovered.
        let eye = row_button(&ui, 4, "Hold to reveal").expect("the eye is always there");
        assert!(eye.size().width >= 32.0 - 0.5 && eye.size().height >= 32.0 - 0.5);
        take(&log);
        let at = centre(&eye);
        move_to(&ui, at);
        ui.window().dispatch_event(WindowEvent::PointerPressed { position: at, button: PointerEventButton::Left });
        settle();
        assert_eq!(take(&log), ["reveal:4:true"]);
        ui.window().dispatch_event(WindowEvent::PointerReleased { position: at, button: PointerEventButton::Left });
        settle();
        assert_eq!(take(&log), ["reveal:4:false"], "released: hidden again, and the row was not used");
        // Selected: the actions sit left of the eye, not on top of it.
        ui.set_selected_index(4);
        settle();
        let eye = row_button(&ui, 4, "Hold to reveal").expect("eye");
        let more = row_button(&ui, 4, "More").expect("More");
        assert!(more.absolute_position().x + more.size().width <= eye.absolute_position().x + 0.5);
    }

    fn a_masked_preview_has_no_trailing_characters() {
        let (ui, _log) = start_quiet();
        let r = &rows(&ui)[4];
        let label = r.accessible_label().expect("row label").to_string();
        assert!(!label.contains("7f3a"), "{label}");
        assert!(!label.contains("hunter2"), "the secret is not spoken: {label}");
        let dots = "••••••••••••";
        let found = all(&ui, dots, AccessibleRole::Text);
        assert!(!found.is_empty(), "twelve dots and nothing else");
        assert!(all(&ui, "•••• •••• 7f3a", AccessibleRole::Text).is_empty());
    }

    fn every_kind_has_a_named_type_tile_of_32_by_32() {
        let (ui, _log) = start_quiet();
        for (i, kind) in ["Text", "Template", "Code", "Link", "Sensitive", "File"].into_iter().enumerate() {
            let r = &rows(&ui)[i];
            let tile = all(&ui, kind, AccessibleRole::Image)
                .into_iter()
                .find(|t| {
                    let y = t.absolute_position().y;
                    y >= r.absolute_position().y && y < r.absolute_position().y + r.size().height
                })
                .unwrap_or_else(|| panic!("no tile named {kind} in row {i}"));
            assert_eq!((tile.size().width, tile.size().height), (32.0, 32.0), "{kind}");
            let dx = tile.absolute_position().x - r.absolute_position().x;
            assert!((dx - 28.0).abs() < 0.5, "{kind}: tile at x {dx}: 8 padding, 16 number, 4 gap");
        }
    }

    fn the_text_column_is_230_wide_at_320_and_gains_20_without_numbers() {
        let (ui, _log) = start_quiet();
        let r = &rows(&ui)[0];
        let rw = r.size().width;
        // List padding 8 on each side of the 320 window.
        assert!((rw - 304.0).abs() < 0.5, "row {rw} wide");
        let col = |ui: &PanelWindow| {
            let row_y = rows(ui)[0].absolute_position().y;
            // The pinned strip repeats the title above the list: take the one inside the row.
            let t = all(ui, "Thanks, received", AccessibleRole::Text)
                .into_iter()
                .find(|t| t.absolute_position().y >= row_y)
                .expect("title");
            (t.absolute_position().x - rows(ui)[0].absolute_position().x, t.size().width)
        };
        let (x, w) = col(&ui);
        assert!((x - 70.0).abs() < 0.5, "text column starts at x {x} (8 + 16 + 4 + 32 + 10)");
        assert!(w <= 230.5, "title width {w}");
        ui.set_show_numbers(false);
        settle();
        let (x2, _) = col(&ui);
        assert!((x2 - 50.0).abs() < 0.5, "without numbers the column starts at {x2}");
    }

    // ---- Empty states ----------------------------------------------------------------------------

    fn an_empty_list_teaches_how_to_fill_it_with_one_action() {
        let (ui, log) = start();
        fixture::set_rows(&ui, Vec::new());
        settle();
        assert!(!missing(&ui, "This list is empty", AccessibleRole::Text));
        find(&ui, "Save from clipboard", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["add-from-clipboard"]);
    }

    fn no_results_names_the_query_and_offers_no_action() {
        let (ui, _log) = start();
        fixture::set_rows(&ui, Vec::new());
        ui.set_query("zzz".into());
        settle();
        assert!(!missing(&ui, "Nothing matches \u{201c}zzz\u{201d}", AccessibleRole::Text));
        assert!(!missing(&ui, "Searched every board and list.", AccessibleRole::Text));
        assert!(missing(&ui, "Save from clipboard", AccessibleRole::Button));
        // The one action makes the query a note in the shown list; the quiet one leaves the search.
        assert!(!missing(&ui, "Create “zzz”", AccessibleRole::Button));
        assert!(!missing(&ui, "Clear search", AccessibleRole::Button));
    }

    fn the_empty_state_is_announced_when_a_search_comes_up_empty() {
        use i_slint_backend_testing::AccessibleLiveness;
        let (ui, _log) = start();
        fixture::set_rows(&ui, Vec::new());
        ui.set_query("zzz".into());
        settle();
        assert_eq!(
            find(&ui, "Nothing matches \u{201c}zzz\u{201d}", AccessibleRole::Text).accessible_live_region(),
            Some(AccessibleLiveness::Polite)
        );
    }

    fn a_board_with_no_lists_offers_a_new_list() {
        let (ui, log) = start();
        fixture::set_rows(&ui, Vec::new());
        ui.set_tabs(ModelRc::new(VecModel::default()));
        settle();
        assert!(!missing(&ui, "No lists yet", AccessibleRole::Text));
        find(&ui, "New list", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["add-list"]);
        find(&ui, "Add list", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["add-list"]);
    }

    fn loading_shows_a_skeleton_with_a_spoken_status_instead_of_the_empty_state() {
        let (ui, _log) = start();
        fixture::set_rows(&ui, Vec::new());
        ui.set_loading(true);
        settle();
        assert!(!missing(&ui, "Loading", AccessibleRole::ProgressIndicator));
        assert!(missing(&ui, "This list is empty", AccessibleRole::Text));
    }

    // ---- Toast -----------------------------------------------------------------------------------

    fn a_toast_is_announced_politely_and_can_offer_undo() {
        use i_slint_backend_testing::AccessibleLiveness;
        let (ui, log) = start();
        assert!(missing(&ui, "Copied", AccessibleRole::Text));
        ui.set_toast_text("Copied".into());
        ui.set_toast_kind(ToastKind::Success);
        ui.set_toast_undo_label("Undo".into());
        settle();
        let toast = find(&ui, "Copied", AccessibleRole::Text);
        assert_eq!(toast.accessible_live_region(), Some(AccessibleLiveness::Polite));
        find(&ui, "Undo", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["undo"]);
        ui.set_toast_text("".into());
        settle();
        assert!(missing(&ui, "Copied", AccessibleRole::Text));
    }

    fn the_pointer_on_a_toast_or_its_undo_button_reports_hover() {
        let (ui, log) = start();
        ui.set_toast_text("Deleted".into());
        ui.set_toast_undo_label("Undo".into());
        settle();
        take(&log);
        let centre = |e: &ElementHandle| {
            let (p, s) = (e.absolute_position(), e.size());
            slint::LogicalPosition::new(p.x + s.width / 2.0, p.y + s.height / 2.0)
        };
        let text_at = centre(&find(&ui, "Deleted", AccessibleRole::Text));
        let button_at = centre(&find(&ui, "Undo", AccessibleRole::Button));
        let away = slint::LogicalPosition::new(2.0, 2.0);
        let w = ui.window();
        w.dispatch_event(slint::platform::WindowEvent::PointerMoved { position: text_at });
        settle();
        assert_eq!(take(&log), ["hover:true"]);
        w.dispatch_event(slint::platform::WindowEvent::PointerMoved { position: button_at });
        settle();
        let seen = take(&log);
        assert!(!seen.contains(&"hover:false".to_string()), "the button is still the toast: {seen:?} {text_at:?} {button_at:?}");
        w.dispatch_event(slint::platform::WindowEvent::PointerMoved { position: away });
        settle();
        assert_eq!(take(&log).last().map(String::as_str), Some("hover:false"));
    }

    // ---- Accessibility and hit targets -----------------------------------------------------------

    fn every_control_has_a_role_and_a_label() {
        let (ui, _log) = start();
        let _ = find(&ui, "Close panel (Esc)", AccessibleRole::Button);
        let _ = find(&ui, "Keep panel on top", AccessibleRole::Button);
        let _ = find(&ui, "Board: Work", AccessibleRole::Button);
        let _ = find(&ui, "Search (Ctrl+F)", AccessibleRole::Button);
        let _ = find(&ui, "New", AccessibleRole::Button);
        let _ = find(&ui, "Open Snipdeck", AccessibleRole::Button);
        let _ = find(&ui, "Lists", AccessibleRole::TabList);
        for name in ["Email replies", "Addresses", "Code"] {
            let _ = find(&ui, name, AccessibleRole::Tab);
        }
        let _ = find(&ui, "Add list", AccessibleRole::Button);
        let _ = find(&ui, "Snippets", AccessibleRole::List);
        let _ = find(&ui, "Pinned snippets", AccessibleRole::List);
        for name in ["Thanks, received", "Home address", "+1 more"] {
            let _ = find(&ui, name, AccessibleRole::Button);
        }
        // Every button and row has a non-empty name.
        for role in [AccessibleRole::Button, AccessibleRole::Tab, AccessibleRole::ListItem] {
            for e in by_role(&ui, role) {
                assert!(
                    e.accessible_label().is_some_and(|l| !l.is_empty()),
                    "an unlabelled {role:?}"
                );
            }
        }
        // Rows read as "title, preview"; a sensitive row never reads its secret.
        let labels: Vec<String> = rows(&ui)
            .iter()
            .filter_map(|r| r.accessible_label().map(|l| l.to_string()))
            .collect();
        assert!(labels.iter().any(|l| l.starts_with("Thanks, received, Thanks for sending")));
        assert!(labels.iter().any(|l| l == "Server password"), "{labels:?}");
        assert!(labels.iter().all(|l| !l.contains("hunter2")));
        // The search field joins the tree once it is open, with the close button beside it.
        chord(ui.window(), &[Key::Control], "f");
        settle();
        let search = find(&ui, "Search snippets", AccessibleRole::TextInput);
        assert_eq!(search.accessible_placeholder_text().as_deref(), Some("Search snippets"));
        let _ = find(&ui, "Close search", AccessibleRole::Button);
    }

    fn close_search_clears_the_query_and_collapses_the_block() {
        let (ui, log) = start();
        assert!(missing(&ui, "Close search", AccessibleRole::Button));
        type_text(ui.window(), "a");
        settle();
        assert_eq!(ui.get_query(), "a", "a typed character opens the search and lands in it");
        assert!(missing(&ui, "Lists", AccessibleRole::TabList), "the block replaces the tabs");
        assert!(missing(&ui, "Pinned snippets", AccessibleRole::List), "and the pinned strip");
        take(&log);
        find(&ui, "Close search", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert_eq!(ui.get_query(), "");
        assert_eq!(take(&log), ["query:"]);
        assert!(missing(&ui, "Close search", AccessibleRole::Button));
        let _ = find(&ui, "Lists", AccessibleRole::TabList);
        let _ = find(&ui, "Pinned snippets", AccessibleRole::List);
        // The keyboard still types into the search afterwards.
        type_text(ui.window(), "b");
        assert_eq!(ui.get_query(), "b");
    }

    fn search_always_keeps_the_tabs_and_pins_and_has_no_close_button_only_a_clear_one() {
        let (ui, log) = start();
        ui.set_search_always(true);
        settle();
        assert!(missing(&ui, "Clear search", AccessibleRole::Button), "nothing to clear yet");
        let _ = find(&ui, "Search snippets", AccessibleRole::TextInput);
        type_text(ui.window(), "a");
        settle();
        assert!(missing(&ui, "Close search", AccessibleRole::Button), "the strip never goes away");
        let _ = find(&ui, "Lists", AccessibleRole::TabList);
        let _ = find(&ui, "Pinned snippets", AccessibleRole::List);
        take(&log);
        find(&ui, "Clear search", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(ui.get_query(), "");
        assert_eq!(take(&log), ["query:"]);
    }

    fn escape_leaves_a_menu_then_the_search_then_the_panel() {
        let (ui, log) = start();
        type_text(ui.window(), "ab");
        ui.set_app_menu_open(true);
        settle();
        take(&log);
        press(ui.window(), &key(Key::Escape));
        assert_eq!(take(&log), ["app-dismissed"], "the menu goes first and the query stays");
        assert_eq!(ui.get_query(), "ab");
        ui.set_app_menu_open(false);
        settle();
        press(ui.window(), &key(Key::Escape));
        assert_eq!(take(&log), ["query:"], "then the search");
        press(ui.window(), &key(Key::Escape));
        assert_eq!(take(&log), ["close"], "then the Panel");
        // An opened but empty search collapses on Esc as well.
        chord(ui.window(), &[Key::Control], "f");
        settle();
        let _ = find(&ui, "Close search", AccessibleRole::Button);
        take(&log);
        press(ui.window(), &key(Key::Escape));
        settle();
        assert!(missing(&ui, "Close search", AccessibleRole::Button));
        assert!(!take(&log).contains(&"close".to_string()), "that Esc only closed the search");
    }

    fn the_search_button_and_ctrl_f_open_the_search_with_its_text_selected() {
        let (ui, _log) = start();
        find(&ui, "Search (Ctrl+F)", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert!(ui.get_search_focused());
        let _ = find(&ui, "Close search", AccessibleRole::Button);
        type_text(ui.window(), "q");
        assert_eq!(ui.get_query(), "q");
    }

    fn every_target_is_at_least_24px_in_both_directions_in_every_state() {
        // Every clickable the accessibility tree names, in each state the goldens show (spec 6: hit targets).
        let walk = |ui: &PanelWindow, state: &str, row_min: f32| -> usize {
            let mut checked = 0;
            for role in [
                AccessibleRole::Button,
                AccessibleRole::Tab,
                AccessibleRole::TextInput,
                AccessibleRole::ListItem,
            ] {
                for e in by_role(ui, role) {
                    let s = e.size();
                    let min = if role == AccessibleRole::ListItem { row_min } else { MIN_HIT };
                    assert!(
                        s.width >= min && s.height >= min,
                        "{state}: {role:?} {:?} is {}x{}",
                        e.accessible_label(),
                        s.width,
                        s.height
                    );
                    checked += 1;
                }
            }
            checked
        };
        let (ui, _log) = start();
        assert!(walk(&ui, "rest", 32.0) >= 15, "rest");
        set_pinned(&ui, fixture::pinned_n(12));
        assert!(walk(&ui, "pinned overflow", 32.0) >= 15);
        fixture::set_rows(&ui, fixture::numbered_rows(12));
        type_text(ui.window(), "a");
        ui.set_toast_text("Copied".into());
        ui.set_toast_undo_label("Undo".into());
        settle();
        assert!(walk(&ui, "search with a toast", 32.0) >= 10);
        ui.set_toast_text("".into());
        ui.set_search_always(true);
        settle();
        walk(&ui, "search always", 32.0);
        // The empty states.
        fixture::set_rows(&ui, Vec::new());
        settle();
        assert!(walk(&ui, "no results", 32.0) >= 5);
        ui.set_query("".into());
        settle();
        walk(&ui, "empty list", 32.0);
        // A narrow window shows the icon-only New and Open Snipdeck.
        ui.window().set_size(slint::LogicalSize::new(280.0, 560.0));
        settle();
        walk(&ui, "280 wide", 32.0);
        // The menus: rows are 24 at the least.
        ui.window().set_size(slint::LogicalSize::new(320.0, 560.0));
        settle();
        find(&ui, "New", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        walk(&ui, "New menu", 24.0);
    }

    // ---- Chrome (Panel v2 spec 2.1-2.8) --------------------------------------------------------------

    fn the_chrome_at_rest_is_104_tall_without_pins_and_140_with() {
        let (ui, _log) = start();
        let height = ui.window().size().height as f32;
        let deck_top = |ui: &PanelWindow| {
            let tabs = find(ui, "Lists", AccessibleRole::TabList);
            let strip = by_role(ui, AccessibleRole::List)
                .into_iter()
                .find(|e| e.accessible_label().as_deref() == Some("Pinned snippets"));
            (tabs.absolute_position().y, strip.map(|e| (e.absolute_position().y, e.size().height)))
        };
        let footer = find(&ui, "Pastes into Outlook · New message", AccessibleRole::Text);
        assert!((footer.size().height - 32.0).abs() < 0.5, "footer 32 tall: {}", footer.size().height);
        let (tabs_y, strip) = deck_top(&ui);
        assert!((tabs_y - 40.0).abs() < 0.5, "header is 40 tall: tabs start at {tabs_y}");
        let (strip_y, strip_h) = strip.expect("pinned strip");
        assert!((strip_y - 72.0).abs() < 0.5 && (strip_h - 36.0).abs() < 0.5, "tabs 32 then pins 36: {strip_y} {strip_h}");
        let with_pins = 40.0 + 32.0 + strip_h + footer.size().height;
        assert!((with_pins - 140.0).abs() < 0.5);
        set_pinned(&ui, Vec::new());
        assert!(missing(&ui, "Pinned snippets", AccessibleRole::List));
        let list = find(&ui, "Snippets", AccessibleRole::List);
        // Header 40 + tabs 32 above the list; 32 of footer below it (the list ends above the footer).
        assert!(list.absolute_position().y >= 72.0 && list.absolute_position().y < 72.0 + 40.0, "list starts under the tabs: {}", list.absolute_position().y);
        assert!(footer.absolute_position().y + footer.size().height <= height + 0.5);
        assert!((40.0 + 32.0 + footer.size().height - 104.0).abs() < 0.5);
    }

    fn open_snipdeck_is_a_footer_button_that_goes_icon_only_below_300px() {
        let (ui, log) = start();
        let open = find(&ui, "Open Snipdeck", AccessibleRole::Button);
        assert!(open.size().width > 100.0, "words at 320: {}", open.size().width);
        open.invoke_accessible_default_action();
        assert_eq!(take(&log), ["open"]);
        ui.window().set_size(slint::LogicalSize::new(280.0, 560.0));
        settle();
        let open = find(&ui, "Open Snipdeck", AccessibleRole::Button);
        assert!(open.size().width <= 40.0, "icon only at 280: {}", open.size().width);
        assert!(open.size().width >= MIN_HIT && open.size().height >= MIN_HIT);
    }

    fn the_new_button_opens_a_menu_of_save_from_clipboard_new_note_and_new_list() {
        let (ui, log) = start();
        let entries = |ui: &PanelWindow| {
            ["Save from clipboard", "New note", "New list"]
                .map(|l| (l, all(ui, l, AccessibleRole::ListItem).len()))
        };
        assert_eq!(entries(&ui).map(|e| e.1), [0, 0, 0]);
        find(&ui, "New", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert_eq!(entries(&ui).map(|e| e.1), [1, 1, 1]);
        find(&ui, "Save from clipboard", AccessibleRole::ListItem).invoke_accessible_default_action();
        assert_eq!(take(&log), ["add-from-clipboard"]);
        assert_eq!(entries(&ui).map(|e| e.1), [0, 0, 0], "choosing closes the menu");
        find(&ui, "New", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        find(&ui, "New list", AccessibleRole::ListItem).invoke_accessible_default_action();
        assert_eq!(take(&log), ["add-list"]);
        // Esc closes the menu without closing the Panel.
        find(&ui, "New", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        press(ui.window(), &key(Key::Escape));
        assert_eq!(entries(&ui).map(|e| e.1), [0, 0, 0]);
        assert!(take(&log).is_empty());
    }

    fn enter_with_no_matches_creates_a_note_from_the_query() {
        let (ui, log) = start();
        fixture::set_rows(&ui, Vec::new());
        type_text(ui.window(), "zzz");
        settle();
        take(&log);
        press(ui.window(), &key(Key::Return));
        let lines = take(&log);
        assert!(lines.contains(&"quick-add:zzz:0:false".to_string()), "{lines:?}");
    }

    // ---- Reviewer notes on slices 1-3 -----------------------------------------------------------------

    fn the_sensitive_eye_can_be_activated_without_a_pointer() {
        let (ui, log) = start();
        // Assistive technology: the eye is a named button whose default action reveals; the keyboard's own way
        // is holding Space on the selected row (see `holding_space_on_the_selected_sensitive_row...`).
        take(&log);
        let eye = row_button(&ui, 4, "Hold to reveal").expect("the eye");
        eye.invoke_accessible_default_action();
        assert_eq!(take(&log), ["reveal:4:true"]);
    }

    fn a_tooltip_from_a_row_closes_when_the_rows_change_or_scroll() {
        let (ui, _log) = start();
        let row = rows(&ui)[2].clone();
        move_to(&ui, centre(&row));
        wait(900);
        let shown = ui.get_tooltip_shown().to_string();
        assert!(!shown.is_empty(), "the row's tooltip is up after a rest");
        // The hovered row goes (deleted): the pointer never leaves it, yet the tip must not outlive it.
        fixture::set_rows(&ui, fixture::numbered_rows(1));
        settle();
        assert_eq!(ui.get_tooltip_shown(), "", "closed when the row model changed");
        // And a header tooltip is not touched by a row change.
        move_to(&ui, centre(&find(&ui, "More", AccessibleRole::Button)));
        wait(600);
        assert_eq!(ui.get_tooltip_shown(), "More");
        fixture::set_rows(&ui, fixture::numbered_rows(2));
        settle();
        assert_eq!(ui.get_tooltip_shown(), "More");
    }

    // ---- Kebab menu ---------------------------------------------------------------------------------

    fn the_kebab_button_asks_the_host_for_the_app_menu_and_the_host_draws_it_under_the_header() {
        let (ui, log) = start();
        let kebab = find(&ui, "More", AccessibleRole::Button);
        assert!(kebab.size().width >= MIN_HIT && kebab.size().height >= MIN_HIT);
        // Between the pin and the close button.
        let pin = find(&ui, "Keep panel on top", AccessibleRole::Button).absolute_position().x;
        let close = find(&ui, "Close panel (Esc)", AccessibleRole::Button).absolute_position().x;
        assert!(pin < kebab.absolute_position().x && kebab.absolute_position().x < close);

        assert!(missing(&ui, "Panel menu", AccessibleRole::List), "closed by default");
        kebab.invoke_accessible_default_action();
        assert_eq!(take(&log), ["app-menu"], "the host decides; the Panel does not open it itself");
        assert!(!ui.get_app_menu_open());

        ui.set_app_menu_open(true);
        settle();
        let menu = find(&ui, "Panel menu", AccessibleRole::List);
        // Right-aligned under the button, clamped inside the window.
        let (mp, ms) = (menu.absolute_position(), menu.size());
        let kebab = find(&ui, "More", AccessibleRole::Button);
        let kebab_right = kebab.absolute_position().x + kebab.size().width;
        let window = ui.window().size();
        assert!(mp.x >= 0.0 && mp.x + ms.width <= window.width as f32, "inside the window: {mp:?} {ms:?}");
        assert!(mp.y >= kebab.absolute_position().y + kebab.size().height - 8.0, "below the button");
        assert!(
            (mp.x + ms.width - kebab_right).abs() <= 0.5,
            "right edge is the kebab's: menu {} kebab {kebab_right}",
            mp.x + ms.width
        );

        find(&ui, "Settings", AccessibleRole::ListItem).invoke_accessible_default_action();
        assert_eq!(take(&log), ["app:1"]);
    }

    fn the_app_menu_takes_the_keyboard_and_escape_closes_it_without_closing_the_panel() {
        let (ui, log) = start();
        ui.set_app_menu_open(true);
        settle();
        press(ui.window(), &key(Key::DownArrow));
        press(ui.window(), &key(Key::DownArrow));
        press(ui.window(), &key(Key::Return));
        assert_eq!(take(&log), ["app:1"], "focus is in the menu: two Downs and Enter pick the second row");
        assert_eq!(ui.get_selected_index(), 0, "the list selection did not move");
        press(ui.window(), &key(Key::Escape));
        assert_eq!(take(&log), ["app-dismissed"], "Esc closes the menu, not the Panel");

        ui.set_app_menu_open(false);
        settle();
        assert!(ui.get_search_focused(), "focus returns to the search field");
        type_text(ui.window(), "x");
        assert_eq!(ui.get_query(), "x", "typing works again");
    }

    fn clicking_outside_the_app_menu_dismisses_it_and_the_click_goes_no_further() {
        let (ui, log) = start();
        ui.set_app_menu_open(true);
        settle();
        // Right of the menu (it ends at the kebab), over a row.
        let y = centre(&rows(&ui)[0]).y;
        let w = ui.window().size().width as f32;
        press_at(&ui, LogicalPosition::new(w - 12.0, y));
        assert_eq!(take(&log), ["app-dismissed"], "the scrim eats the click: no use");
    }

    fn shift_f10_or_the_menu_key_on_the_focused_kebab_opens_the_app_menu() {
        for opener in [Key::Menu, Key::F10] {
            let (ui, log) = start();
            let mut opened = false;
            // Tab walks the header controls; the kebab is among the first few stops.
            for _ in 0..14 {
                press(ui.window(), &key(Key::Tab));
                if opener == Key::F10 {
                    chord(ui.window(), &[Key::Shift], &key(Key::F10));
                } else {
                    press(ui.window(), &key(Key::Menu));
                }
                let seen = take(&log);
                if seen.iter().any(|l| l == "app-menu") {
                    assert_eq!(seen, ["app-menu"], "only the app menu, not a row's menu");
                    opened = true;
                    break;
                }
            }
            assert!(opened, "{opener:?} on the kebab never asked for the menu");
        }
    }

    // ---- Tooltips ----------------------------------------------------------------------------------

    fn an_icon_button_shows_its_tooltip_after_500ms_of_hover_and_not_before() {
        let (ui, _log) = start();
        let kebab = find(&ui, "More", AccessibleRole::Button);
        assert_eq!(ui.get_tooltip_shown(), "");
        move_to(&ui, centre(&kebab));
        wait(499);
        assert_eq!(ui.get_tooltip_shown(), "", "not yet");
        wait(2);
        assert_eq!(ui.get_tooltip_shown(), "More");
        // The pointer leaves: it goes.
        move_to(&ui, away(&ui));
        settle();
        assert_eq!(ui.get_tooltip_shown(), "");
    }

    fn moving_from_one_icon_button_to_another_swaps_the_tooltip_instantly() {
        let (ui, _log) = start();
        let kebab = find(&ui, "More", AccessibleRole::Button);
        let pin = find(&ui, "Keep panel on top", AccessibleRole::Button);
        let close = find(&ui, "Close panel (Esc)", AccessibleRole::Button);
        move_to(&ui, centre(&kebab));
        wait(600);
        assert_eq!(ui.get_tooltip_shown(), "More");
        move_to(&ui, centre(&pin));
        settle();
        assert_eq!(ui.get_tooltip_shown(), "Keep panel on top", "no second 500ms wait");
        move_to(&ui, centre(&close));
        settle();
        assert_eq!(ui.get_tooltip_shown(), "Close panel (Esc)");
        // After a rest the next one waits again.
        move_to(&ui, away(&ui));
        wait(1000);
        move_to(&ui, centre(&kebab));
        wait(100);
        assert_eq!(ui.get_tooltip_shown(), "");
        wait(450);
        assert_eq!(ui.get_tooltip_shown(), "More");
    }

    fn every_icon_only_control_in_the_panel_has_a_tooltip() {
        let (ui, _log) = start();
        set_pinned(&ui, fixture::pinned_n(12));
        let hover = |label: &str| {
            move_to(&ui, away(&ui));
            wait(1000);
            let target = find(&ui, label, AccessibleRole::Button);
            move_to(&ui, centre(&target));
            wait(501);
            assert_eq!(ui.get_tooltip_shown(), label, "tooltip of {label:?}");
        };
        for label in ["More", "Keep panel on top", "Close panel (Esc)", "Add list", "Search (Ctrl+F)"] {
            hover(label);
        }
    }

    fn a_tooltip_hides_while_a_menu_is_open() {
        let (ui, _log) = start();
        let kebab = find(&ui, "More", AccessibleRole::Button);
        move_to(&ui, centre(&kebab));
        wait(600);
        assert_eq!(ui.get_tooltip_shown(), "More");
        ui.set_app_menu_open(true);
        settle();
        assert_eq!(ui.get_tooltip_shown(), "", "the menu is the label now");
    }

    // ---- Resize grips ------------------------------------------------------------------------------

    fn a_press_on_an_edge_or_corner_asks_the_host_to_resize_and_never_starts_a_drag() {
        let (ui, log) = start();
        let size = ui.window().size();
        let (w, h) = (size.width as f32, size.height as f32);
        let cases = [
            (2.0, h / 2.0, "Left"),
            (w - 2.0, h / 2.0, "Right"),
            (w / 2.0, 2.0, "Top"),
            (w / 2.0, h - 2.0, "Bottom"),
            (2.0, 2.0, "TopLeft"),
            (w - 2.0, 2.0, "TopRight"),
            (2.0, h - 2.0, "BottomLeft"),
            (w - 2.0, h - 2.0, "BottomRight"),
        ];
        for (x, y, edge) in cases {
            press_at(&ui, LogicalPosition::new(x, y));
            assert_eq!(take(&log), [format!("resize:{edge}")], "press at ({x}, {y})");
        }
        // The enum is the public contract.
        let _ = [
            ResizeEdge::Left,
            ResizeEdge::Right,
            ResizeEdge::Top,
            ResizeEdge::Bottom,
            ResizeEdge::TopLeft,
            ResizeEdge::TopRight,
            ResizeEdge::BottomLeft,
            ResizeEdge::BottomRight,
        ];
        // The header body still drags.
        press_at(&ui, LogicalPosition::new(92.0, 20.0));
        assert_eq!(take(&log), ["drag"]);
    }

    // ---- The edge dock ---------------------------------------------------------------------------

    fn the_open_panel_reports_the_pointer_over_any_part_of_it_once_and_swallows_no_click() {
        let (ui, log) = start();
        // Not hooked by `start`: every pointer move in the other tests would report it.
        let hovers: Log = Rc::default();
        {
            let hovers = hovers.clone();
            ui.on_panel_hovered(move |on| hovers.borrow_mut().push(format!("panel-hover:{on}")));
        }
        let row = rows(&ui).into_iter().next().expect("a row");
        let kebab = find(&ui, "More", AccessibleRole::Button);
        // A row, the header's kebab button, an edge grip, and the list again: always inside.
        let grip = LogicalPosition::new(2.0, ui.window().size().height as f32 / 2.0);
        for at in [centre(&row), centre(&kebab), grip, centre(&row)] {
            move_to(&ui, at);
        }
        assert_eq!(take(&hovers), ["panel-hover:true"], "reported on entering, not on every move");
        assert_eq!(take(&log), [] as [String; 0]);
        // Clicks still reach the controls under the hover area.
        press_at(&ui, centre(&row));
        assert_eq!(take(&log), ["use:0"]);
        ui.window().dispatch_event(WindowEvent::PointerExited);
        assert_eq!(take(&hovers), ["panel-hover:false"]);
    }

    fn moving_the_pointer_from_the_more_button_onto_its_open_menu_keeps_the_pointer_over_the_panel() {
        let (ui, log) = start();
        let hovers: Log = Rc::default();
        {
            let hovers = hovers.clone();
            ui.on_panel_hovered(move |on| hovers.borrow_mut().push(format!("panel-hover:{on}")));
        }
        let kebab = find(&ui, "More", AccessibleRole::Button);
        move_to(&ui, centre(&kebab));
        ui.set_app_menu_open(true);
        settle();
        let menu = find(&ui, "Panel menu", AccessibleRole::List);
        for item in all(&ui, "Settings", AccessibleRole::ListItem) {
            move_to(&ui, centre(&item));
        }
        move_to(&ui, centre(&menu));
        assert!(ui.get_app_menu_open(), "the menu stays open under the pointer");
        assert_eq!(take(&hovers), ["panel-hover:true"], "never reported as left");
        assert_eq!(take(&log), [] as [String; 0], "no dismissal from hovering");
    }

    fn a_collapsed_panel_is_one_named_handle_that_reports_hover_and_click() {
        let (ui, log) = start();
        let row_at = centre(&rows(&ui).into_iter().next().expect("a row"));
        ui.set_dock_collapsed(true);
        settle();
        let name = "Snipdeck handle, press the Panel hotkey to open";
        let handle = find(&ui, name, AccessibleRole::Button);
        let (size, at) = (handle.size(), handle.absolute_position());
        let window = ui.window().size().to_logical(ui.window().scale_factor());
        assert_eq!((at.x, at.y), (0.0, 0.0));
        assert_eq!((size.width, size.height), (window.width, window.height), "the whole window");

        move_to(&ui, centre(&handle));
        assert_eq!(take(&log), ["handle-hover:true"]);
        ui.window().dispatch_event(WindowEvent::PointerExited);
        assert_eq!(take(&log), ["handle-hover:false"]);

        press_at(&ui, centre(&handle));
        assert_eq!(take(&log), ["handle-hover:true", "handle-click"]);
        // What is under the handle is out of reach, pointer and keyboard.
        ui.window().dispatch_event(WindowEvent::PointerExited);
        take(&log);
        press_at(&ui, row_at);
        assert_eq!(take(&log), ["handle-hover:true", "handle-click"], "the row was not used");
        handle.invoke_accessible_default_action();
        assert_eq!(take(&log), ["handle-click"]);

        ui.set_dock_collapsed(false);
        settle();
        assert!(missing(&ui, name, AccessibleRole::Button), "the handle goes when the panel opens");
    }

    fn the_grips_are_above_the_header_but_do_not_steal_clicks_just_inside_the_edge() {
        let (ui, log) = start();
        // 10px in from the left edge is still the row (rows start 8px in).
        let row = rows(&ui)[0].clone();
        let y = centre(&row).y;
        press_at(&ui, LogicalPosition::new(10.0, y));
        assert_eq!(take(&log), ["use:0"], "a click 10px inside the edge reaches the row");
        let w = ui.window().size().width as f32;
        press_at(&ui, LogicalPosition::new(w - 10.0, y));
        assert_eq!(take(&log), ["use:0"], "and on the right side");
        // The header buttons next to the top-right corner still work.
        find(&ui, "Close panel (Esc)", AccessibleRole::Button).mock_single_click(PointerEventButton::Left);
        assert_eq!(take(&log), ["close"]);
        // A press on the top edge over the header is a resize, not a drag.
        press_at(&ui, LogicalPosition::new(120.0, 1.0));
        assert_eq!(take(&log), ["resize:Top"]);
    }

    fn a_resize_grip_works_while_a_menu_is_open() {
        let (ui, log) = start();
        ui.set_app_menu_open(true);
        settle();
        let h = ui.window().size().height as f32;
        press_at(&ui, LogicalPosition::new(2.0, h / 2.0));
        assert_eq!(take(&log), ["resize:Left"]);
    }

    // ---- Quick edit hooks ----------------------------------------------------------------------------

    fn ctrl_e_edits_f2_renames_and_delete_deletes_the_selected_row() {
        let (ui, log) = start();
        press(ui.window(), &key(Key::DownArrow));
        take(&log);
        chord(ui.window(), &[Key::Control], "e");
        assert_eq!(take(&log), ["edit:1"]);
        chord(ui.window(), &[Key::Control], "d");
        assert_eq!(take(&log), ["duplicate:1"]);
        chord(ui.window(), &[Key::Control], "p");
        assert_eq!(take(&log), ["pin:1"]);
        press(ui.window(), &key(Key::F2));
        assert_eq!(take(&log), ["rename:1"]);
        press(ui.window(), &key(Key::Delete));
        assert_eq!(take(&log), ["delete:1"]);
        assert_eq!(ui.get_query(), "", "none of them types");
    }

    fn delete_edits_the_search_text_once_there_is_a_query_and_the_hooks_need_a_row() {
        let (ui, log) = start();
        type_text(ui.window(), "th");
        take(&log);
        press(ui.window(), &key(Key::Delete));
        assert!(
            take(&log).iter().all(|l| !l.starts_with("delete")),
            "Delete is a text key while searching"
        );
        // Ctrl+E and F2 still work on the first match.
        chord(ui.window(), &[Key::Control], "e");
        assert_eq!(take(&log), ["edit:0"]);
        press(ui.window(), &key(Key::F2));
        assert_eq!(take(&log), ["rename:0"]);

        let (ui, log) = start();
        fixture::set_rows(&ui, Vec::new());
        settle();
        chord(ui.window(), &[Key::Control], "e");
        press(ui.window(), &key(Key::F2));
        press(ui.window(), &key(Key::Delete));
        assert_eq!(take(&log), Vec::<String>::new(), "no rows, nothing to edit");
    }

    fn a_double_click_on_a_row_never_asks_to_edit_it() {
        let (ui, log) = start();
        let at = centre(&rows(&ui)[2]);
        move_to(&ui, at);
        for _ in 0..2 {
            ui.window().dispatch_event(WindowEvent::PointerPressed {
                position: at,
                button: PointerEventButton::Left,
            });
            ui.window().dispatch_event(WindowEvent::PointerReleased {
                position: at,
                button: PointerEventButton::Left,
            });
        }
        // The first click used the row; the second adds nothing. Ctrl+E, F2 and the menu edit.
        let seen = take(&log);
        assert!(seen.iter().any(|l| l == "use:2"), "{seen:?}");
        assert!(seen.iter().all(|l| !l.starts_with("edit")), "{seen:?}");
    }

    // ---- Pinned strip scrolling ------------------------------------------------------------------------

    fn a_pinned_strip_that_fits_shows_no_more_button() {
        let (ui, _log) = start();
        set_pinned(&ui, fixture::pinned_n(2));
        let more = by_role(&ui, AccessibleRole::Button)
            .into_iter()
            .filter(|e| e.accessible_label().is_some_and(|l| l.ends_with(" more")))
            .count();
        assert_eq!(more, 0);
        let _ = find(&ui, "Pinned 01", AccessibleRole::Button);
        let _ = find(&ui, "Pinned 02", AccessibleRole::Button);
    }

    fn an_overflowing_strip_shows_two_chips_and_a_plus_n_more_menu_of_the_rest() {
        let (ui, log) = start();
        set_pinned(&ui, fixture::pinned_n(12));
        let _ = find(&ui, "Pinned 01", AccessibleRole::Button);
        let _ = find(&ui, "Pinned 02", AccessibleRole::Button);
        assert!(missing(&ui, "Pinned 03", AccessibleRole::Button), "two chips only");
        let more = find(&ui, "+10 more", AccessibleRole::Button);
        assert!(more.size().width >= MIN_HIT && more.size().height >= MIN_HIT);
        assert!(more.absolute_position().x + more.size().width <= ui.window().size().width as f32 - 4.0 + 0.5);
        ui.set_pinned_more_entries(ModelRc::new(VecModel::from(
            (3..=12)
                .map(|i| snipdeck_ui::MenuEntry {
                    text: format!("Pinned {i:02}").into(),
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        )));
        press_at(&ui, centre(&more));
        settle();
        assert_eq!(take(&log), Vec::<String>::new(), "opening the menu uses nothing");
        let menu = find(&ui, "More pinned snippets", AccessibleRole::List);
        assert!(menu.size().width > 0.0);
        let row = find(&ui, "Pinned 05", AccessibleRole::ListItem);
        row.invoke_accessible_default_action();
        assert_eq!(take(&log), ["pinned:4"], "row i of the menu is pinned item i + 2");
        assert!(missing(&ui, "More pinned snippets", AccessibleRole::List), "choosing closes it");
    }

    fn a_chip_in_an_overflowing_strip_still_uses_on_click() {
        let (ui, log) = start();
        set_pinned(&ui, fixture::pinned_n(12));
        press_at(&ui, centre(&find(&ui, "Pinned 02", AccessibleRole::Button)));
        assert_eq!(take(&log), ["pinned:1"]);
    }

    // ---- The Recent inbox -----------------------------------------------------------------------

    fn the_clock_tab_exists_only_while_history_is_on() {
        let (ui, _log) = start();
        assert!(missing(&ui, "Recent", AccessibleRole::Tab), "off by default: no clock tab");
        ui.set_recent_available(true);
        settle();
        let tab = find(&ui, "Recent", AccessibleRole::Tab);
        let first_list_tab = by_role(&ui, AccessibleRole::Tab)
            .into_iter()
            .filter(|t| t.accessible_label().as_deref() != Some("Recent"))
            .map(|t| t.absolute_position().x)
            .fold(f32::MAX, f32::min);
        assert!(tab.absolute_position().x < first_list_tab, "the clock is at the far left");
        assert!(tab.size().width >= MIN_HIT && tab.size().height >= MIN_HIT);
    }

    fn choosing_the_clock_tab_shows_the_entries_and_a_list_tab_goes_back() {
        let (ui, log) = start();
        let (rec, rec_log) = (ui.as_weak(), log.clone());
        ui.on_recent_toggled(move |open| {
            let _ = &rec;
            rec_log.borrow_mut().push(format!("recent:{open}"));
        });
        ui.set_recent_available(true);
        ui.set_recent_rows(ModelRc::new(VecModel::from(vec![
            RecentRow { id: 0, preview: "first copy".into(), app: "Notepad".into(), age_minutes: 3 },
            RecentRow { id: 2, preview: "second copy".into(), app: "".into(), age_minutes: 0 },
        ])));
        settle();
        let list_rows = rows(&ui).len();
        press_at(&ui, centre(&find(&ui, "Recent", AccessibleRole::Tab)));
        settle();
        assert!(ui.get_recent_open());
        assert_eq!(take(&log), ["recent:true"]);
        assert!(rows(&ui).len() != list_rows || list_rows == 0, "the entries replaced the list rows");
        assert!(!all(&ui, "Recent copies", AccessibleRole::List).is_empty(), "the Recent list is labelled");

        // A list tab leaves the inbox and reports the tab.
        let tabs = by_role(&ui, AccessibleRole::Tab);
        let list_tab = tabs
            .iter()
            .find(|t| t.accessible_label().as_deref() != Some("Recent"))
            .expect("a list tab");
        press_at(&ui, centre(list_tab));
        settle();
        assert!(!ui.get_recent_open());
        let seen = take(&log);
        assert!(seen.contains(&"recent:false".to_string()), "{seen:?}");
        assert!(seen.iter().any(|l| l.starts_with("tab:")), "{seen:?}");
    }

    fn the_recent_list_has_its_own_keys_and_none_of_the_item_shortcuts_fire() {
        let (ui, log) = start();
        let (copied, removed) = (log.clone(), log.clone());
        ui.on_recent_copied(move |i| copied.borrow_mut().push(format!("rcopy:{i}")));
        ui.on_recent_deleted(move |i| removed.borrow_mut().push(format!("rdel:{i}")));
        ui.set_recent_available(true);
        ui.set_recent_rows(ModelRc::new(VecModel::from(vec![
            RecentRow { id: 1, preview: "one".into(), app: "".into(), age_minutes: 1 },
            RecentRow { id: 2, preview: "two".into(), app: "".into(), age_minutes: 2 },
        ])));
        press_at(&ui, centre(&find(&ui, "Recent", AccessibleRole::Tab)));
        settle();
        take(&log);

        press(ui.window(), &key(Key::DownArrow));
        assert_eq!(ui.get_recent_selected(), 1);
        assert_eq!(ui.get_selected_index(), 0, "the item selection did not move");
        press(ui.window(), &key(Key::DownArrow));
        assert_eq!(ui.get_recent_selected(), 1, "clamped at the last entry");
        press(ui.window(), &key(Key::Return));
        press(ui.window(), &key(Key::Delete));
        press(ui.window(), &key(Key::F2));
        press(ui.window(), "1");
        chord(ui.window(), &[Key::Control], "e");
        chord(ui.window(), &[Key::Control], "d");
        chord(ui.window(), &[Key::Control], "p");
        settle();
        // The digit is search text only with a query; here it must not use an item either.
        let seen = take(&log);
        assert!(seen.contains(&"rcopy:2".to_string()), "{seen:?}");
        assert!(seen.contains(&"rdel:2".to_string()), "{seen:?}");
        for line in &seen {
            assert!(
                !line.starts_with("use:") && !line.starts_with("edit:") && !line.starts_with("duplicate:") && !line.starts_with("pin:") && !line.starts_with("delete:") && !line.starts_with("rename:") && !line.starts_with("quick:") && !line.starts_with("move:"),
                "an item shortcut fired in the Recent list: {line}"
            );
        }
    }

    fn the_empty_recent_list_says_why_and_the_menu_offers_save_to() {
        let (ui, _log) = start();
        ui.set_recent_available(true);
        ui.set_recent_targets(ModelRc::new(VecModel::from(vec![ChoiceEntry {
            label: "Replies".into(),
            count: 0,
            color: slint::Color::default(),
            icon: slint::Image::default(),
            checked: false,
        }])));
        press_at(&ui, centre(&find(&ui, "Recent", AccessibleRole::Tab)));
        settle();
        assert!(!all(&ui, "Nothing copied yet", AccessibleRole::Text).is_empty()
            || ElementHandle::find_by_accessible_label(&ui, "Nothing copied yet").next().is_some());
        ui.set_recent_paused(true);
        settle();
        assert!(ElementHandle::find_by_accessible_label(&ui, "History is paused").next().is_some());
    }
}
