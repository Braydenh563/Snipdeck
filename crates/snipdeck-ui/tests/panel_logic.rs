//! Behaviour of the Panel window through Slint's testing backend: keyboard flows, quick-use, the search
//! callback, empty states, the board menu, the toast, accessibility roles and labels, and hit targets.
//! Time is mocked, never the real clock.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use std::{cell::RefCell, rc::Rc};

use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementQuery, ElementRoot};
use slint::{ComponentHandle, ModelRc, VecModel, platform::Key};
use snipdeck_ui::{PanelView, PanelWindow, ToastKind};

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
    hook!(on_close_requested, || "close".to_string());
    hook!(on_drag_requested, || "drag".to_string());
    hook!(on_pin_toggled, |on| format!("pin:{on}"));
    hook!(on_add_list_requested, || "add-list".to_string());
    hook!(on_add_from_clipboard, || "add-from-clipboard".to_string());
    hook!(on_board_menu_requested, || "board-menu".to_string());
    hook!(on_board_menu_activated, |i| format!("board:{i}"));
    hook!(on_board_menu_dismissed, || "board-dismissed".to_string());
    hook!(on_pinned_used, |i| format!("pinned:{i}"));
    hook!(on_toast_undo, || "undo".to_string());
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
        assert!(take(&log).is_empty(), "no callbacks fired");
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

    fn digits_use_the_nth_row_only_while_the_search_is_empty() {
        let (ui, log) = start();
        press(ui.window(), "3");
        assert_eq!(take(&log), ["quick:3"]);
        assert_eq!(ui.get_selected_index(), 2, "the used row becomes the selection");
        assert_eq!(ui.get_query(), "", "the digit is not search text");

        press(ui.window(), "9");
        assert!(take(&log).is_empty(), "six rows: there is no ninth");

        type_text(ui.window(), "a3");
        assert_eq!(ui.get_query(), "a3", "with a query, digits are text");
        assert_eq!(take(&log), ["query:a", "query:a3"]);
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
        assert!((10..30).contains(&visible), "virtualised: {visible} of 30 rows exist");
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

    // ---- Header --------------------------------------------------------------------------------

    fn pressing_the_header_starts_a_window_drag_but_pressing_a_button_does_not() {
        use slint::{LogicalPosition, platform::{PointerEventButton, WindowEvent}};
        let (ui, log) = start();
        let w = ui.window();
        // Empty header space between the board switcher and the buttons.
        let at = LogicalPosition::new(170.0, 10.0);
        w.dispatch_event(WindowEvent::PointerPressed { position: at, button: PointerEventButton::Left });
        w.dispatch_event(WindowEvent::PointerReleased { position: at, button: PointerEventButton::Left });
        assert_eq!(take(&log), ["drag"]);

        find(&ui, "Close panel", AccessibleRole::Button).mock_single_click(PointerEventButton::Left);
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
        assert!(!missing(&ui, "Copies to clipboard", AccessibleRole::Text));
        assert!(missing(&ui, "Pastes into Outlook", AccessibleRole::Text));
    }

    fn the_selected_row_shows_paste_or_copy_and_no_other_row_does() {
        let (ui, _log) = start();
        let shown = |label: &str| {
            ElementHandle::find_by_accessible_label(&ui, label)
                .filter(|e| e.computed_opacity() > 0.9)
                .count()
        };
        assert_eq!(shown("Paste"), 1);
        assert_eq!(shown("Copy"), 0);
        ui.set_target_name("".into());
        settle();
        assert_eq!(shown("Copy"), 1);
        assert_eq!(shown("Paste"), 0);
    }

    // ---- CJK (measured, never a golden: system fallback fonts differ per machine) -----------------

    fn cjk_rows_stay_hittable_inside_the_panel_in_both_views() {
        let (ui, _log) = start();
        fixture::set_rows(&ui, fixture::cjk_rows());
        for view in [PanelView::Compact, PanelView::Comfortable] {
            ui.set_view(view);
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
        assert!(!missing(&ui, "No matches", AccessibleRole::Text));
        assert!(!missing(&ui, "Nothing on this board matches “zzz”.", AccessibleRole::Text));
        assert!(missing(&ui, "Save from clipboard", AccessibleRole::Button));
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

    // ---- Accessibility and hit targets -----------------------------------------------------------

    fn every_control_has_a_role_and_a_label() {
        let (ui, _log) = start();
        let _ = find(&ui, "Close panel", AccessibleRole::Button);
        let _ = find(&ui, "Keep panel on top", AccessibleRole::Button);
        let _ = find(&ui, "Board: Work", AccessibleRole::Button);
        let search = find(&ui, "Search snippets", AccessibleRole::TextInput);
        assert_eq!(search.accessible_placeholder_text().as_deref(), Some("Search snippets"));
        let _ = find(&ui, "Lists", AccessibleRole::TabList);
        for name in ["Email replies", "Addresses", "Code"] {
            let _ = find(&ui, name, AccessibleRole::Tab);
        }
        let _ = find(&ui, "Add list", AccessibleRole::Button);
        let _ = find(&ui, "Snippets", AccessibleRole::List);
        let _ = find(&ui, "Pinned snippets", AccessibleRole::List);
        for name in ["Thanks, received", "Home address", "Retry helper"] {
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
    }

    fn the_clear_button_appears_with_text_and_is_labelled() {
        let (ui, log) = start();
        assert!(missing(&ui, "Clear search", AccessibleRole::Button));
        type_text(ui.window(), "a");
        settle();
        take(&log);
        find(&ui, "Clear search", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(ui.get_query(), "");
        assert_eq!(take(&log), ["query:"]);
    }

    fn every_target_is_at_least_24px_in_both_directions() {
        let (ui, _log) = start();
        fixture::set_rows(&ui, fixture::numbered_rows(12));
        type_text(ui.window(), "a");
        ui.set_toast_text("Copied".into());
        ui.set_toast_undo_label("Undo".into());
        settle();
        let mut checked = 0;
        for role in [
            AccessibleRole::Button,
            AccessibleRole::Tab,
            AccessibleRole::TextInput,
            AccessibleRole::ListItem,
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
        assert!(checked >= 15, "checked {checked} targets");
    }
}
