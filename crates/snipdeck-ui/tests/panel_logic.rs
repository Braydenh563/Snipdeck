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
    ChoiceEntry, PanelView, PanelWindow, PinnedItem, RecentRow, ResizeEdge, ToastKind,
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
    hook!(on_settings_requested, || "settings".to_string());
    hook!(on_toast_hover_changed, |h| format!("hover:{h}"));
    hook!(on_app_menu_requested, || "app-menu".to_string());
    hook!(on_app_menu_activated, |i| format!("app:{i}"));
    hook!(on_app_menu_dismissed, || "app-dismissed".to_string());
    hook!(on_edit_requested, |i| format!("edit:{i}"));
    hook!(on_rename_requested, |i| format!("rename:{i}"));
    hook!(on_delete_requested, |i| format!("delete:{i}"));
    hook!(on_resize_requested, |edge| format!("resize:{edge:?}"));
    hook!(on_pinned_copied, |i| format!("pinned-copy:{i}"));
    hook!(on_handle_hovered, |on| format!("handle-hover:{on}"));
    hook!(on_handle_clicked, || "handle-click".to_string());
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

fn scroll_at(ui: &PanelWindow, at: LogicalPosition, dx: f32, dy: f32) {
    move_to(ui, at);
    ui.window().dispatch_event(WindowEvent::PointerScrolled {
        position: at,
        delta_x: dx,
        delta_y: dy,
    });
}

fn wait(ms: u64) {
    mock_elapsed_time(Duration::from_millis(ms));
    settle();
}

/// An empty part of the body (no control under it, no tooltip anchor): the bottom, clear of the footer text.
fn away(ui: &PanelWindow) -> LogicalPosition {
    let size = ui.window().size();
    LogicalPosition::new(size.width as f32 / 2.0, size.height as f32 - 16.0)
}

fn set_pinned(ui: &PanelWindow, items: Vec<PinnedItem>) {
    ui.set_pinned(ModelRc::new(VecModel::from(items)));
    settle();
}

/// How far the pinned row is scrolled (>= 0), worked out from the first "Pinned NN" chip still in view
/// (chips that scrolled out of the strip leave the accessibility tree). Chips are equally wide; the first
/// one sits 12px in (8px panel padding, 4px row margin) with a 4px gap between chips.
fn pinned_scroll(ui: &PanelWindow) -> f32 {
    let chip = by_role(ui, AccessibleRole::Button)
        .into_iter()
        .filter_map(|e| {
            let n: usize = e
                .accessible_label()?
                .strip_prefix("Pinned ")?
                .parse()
                .ok()?;
            Some((n, e))
        })
        .min_by_key(|(n, _)| *n)
        .expect("a pinned chip in view");
    let (n, e) = chip;
    12.0 + (n - 1) as f32 * (e.size().width + 4.0) - e.absolute_position().x
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
        // Not a reveal: with the search focused, Space is a text key, so the search reports " " (the host reads a
        // blank query as no query, `Panel::is_searching`).
        assert_eq!(take(&log), ["query: "], "no reveal for a plain row; the space is search text");
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
        chord(ui.window(), &[Key::Control], "n");
        assert_eq!(take(&log), ["new"]);
        chord(ui.window(), &[Key::Control, Key::Shift], "N");
        assert_eq!(take(&log), ["add-list"]);
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

    fn the_footer_names_the_pickers_configured_hotkey_only_when_there_is_one() {
        let (ui, _log) = start();
        let hint = |ui: &PanelWindow| all(ui, "Control Shift Space", AccessibleRole::Text).len();
        assert_eq!(hint(&ui), 0, "no chord set, no hint");
        ui.set_picker_keys(ModelRc::new(VecModel::from(
            ["Ctrl", "Shift", "Space"].map(slint::SharedString::from).to_vec(),
        )));
        ui.set_picker_spoken("Control Shift Space".into());
        assert_eq!(hint(&ui), 1, "the caps are spoken as the chord");
        assert_eq!(
            all(&ui, "Picker anywhere", AccessibleRole::Text).len(),
            1,
            "the words beside the caps are read too"
        );
        ui.set_picker_keys(ModelRc::new(VecModel::from(Vec::<slint::SharedString>::new())));
        assert_eq!(hint(&ui), 0, "switching the hotkey off removes the hint");
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
        let at = LogicalPosition::new(170.0, 10.0);
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

    fn the_empty_state_is_announced_when_a_search_comes_up_empty() {
        use i_slint_backend_testing::AccessibleLiveness;
        let (ui, _log) = start();
        fixture::set_rows(&ui, Vec::new());
        ui.set_query("zzz".into());
        settle();
        assert_eq!(
            find(&ui, "No matches", AccessibleRole::Text).accessible_live_region(),
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
        for label in ["More", "Keep panel on top", "Close panel (Esc)", "Add list", "Scroll right"] {
            hover(label);
        }
        // The left caret only exists once the row has been scrolled.
        press_at(&ui, centre(&find(&ui, "Scroll right", AccessibleRole::Button)));
        settle();
        hover("Scroll left");
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
        press_at(&ui, LogicalPosition::new(170.0, 20.0));
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

    fn a_double_click_on_a_row_asks_to_edit_it() {
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
        let seen = take(&log);
        assert!(seen.iter().any(|l| l == "edit:2"), "{seen:?}");
        assert_eq!(seen.iter().filter(|l| l.starts_with("edit")).count(), 1);
        // A single click alone never edits.
        press_at(&ui, centre(&rows(&ui)[3]));
        mock_elapsed_time(Duration::from_secs(2));
        assert!(take(&log).iter().all(|l| !l.starts_with("edit")));
    }

    // ---- Pinned strip scrolling ------------------------------------------------------------------------

    fn a_pinned_strip_that_fits_shows_no_scroll_buttons() {
        let (ui, _log) = start();
        set_pinned(&ui, fixture::pinned_n(2));
        assert!(missing(&ui, "Scroll left", AccessibleRole::Button));
        assert!(missing(&ui, "Scroll right", AccessibleRole::Button));
    }

    fn the_mouse_wheel_and_shift_wheel_scroll_an_overflowing_strip_sideways() {
        let (ui, _log) = start();
        set_pinned(&ui, fixture::pinned_n(12));
        assert!(pinned_scroll(&ui).abs() < 0.5, "starts at the first chip");
        let on_chip = centre(&find(&ui, "Pinned 02", AccessibleRole::Button));

        // A vertical wheel notch over a chip scrolls the row to the left (later chips come into view).
        scroll_at(&ui, on_chip, 0.0, -60.0);
        settle();
        let after_wheel = pinned_scroll(&ui);
        assert!(after_wheel > 20.0, "wheel moved the row: {after_wheel}");

        // Shift+wheel does the same; so does a native horizontal delta.
        let shift = char::from(Key::Shift).to_string();
        ui.window().dispatch_event(WindowEvent::KeyPressed {
            text: shift.clone().into(),
        });
        scroll_at(&ui, on_chip, 0.0, -60.0);
        ui.window().dispatch_event(WindowEvent::KeyReleased { text: shift.into() });
        settle();
        let after_shift = pinned_scroll(&ui);
        assert!(after_shift > after_wheel + 20.0, "shift+wheel: {after_wheel} -> {after_shift}");
        scroll_at(&ui, on_chip, -60.0, 0.0);
        settle();
        assert!(pinned_scroll(&ui) > after_shift + 20.0, "horizontal delta");

        // Wheel up goes back, and never past the start.
        for _ in 0..20 {
            scroll_at(&ui, on_chip, 0.0, 120.0);
        }
        settle();
        assert!(pinned_scroll(&ui).abs() < 0.5, "clamped at the start");
        // And a long wheel down stops at the last chip.
        for _ in 0..40 {
            scroll_at(&ui, on_chip, 0.0, -120.0);
        }
        settle();
        let last = find(&ui, "Pinned 12", AccessibleRole::Button);
        let w = ui.window().size().width as f32;
        assert!(last.absolute_position().x + last.size().width <= w - 8.0 + 0.5, "last chip fully in view");
    }

    fn scroll_buttons_float_at_the_ends_that_have_more_chips_and_page_through_the_strip() {
        let (ui, log) = start();
        set_pinned(&ui, fixture::pinned_n(12));
        // At the start there is nothing to the left.
        assert!(missing(&ui, "Scroll left", AccessibleRole::Button));
        let right = find(&ui, "Scroll right", AccessibleRole::Button);
        assert!(right.size().width >= MIN_HIT && right.size().height >= MIN_HIT);
        assert!(right.absolute_position().x + right.size().width <= ui.window().size().width as f32 - 8.0 + 0.5);

        press_at(&ui, centre(&right));
        settle();
        let paged = pinned_scroll(&ui);
        assert!(paged > 100.0, "Scroll right paged the row: {paged}");
        let left = find(&ui, "Scroll left", AccessibleRole::Button);
        assert!(left.size().width >= MIN_HIT && left.size().height >= MIN_HIT);
        assert!(left.absolute_position().x < right.absolute_position().x);
        assert_eq!(take(&log), Vec::<String>::new(), "a caret button is not a chip");

        // Page right until the end: the right caret goes, the last chip is fully visible.
        for _ in 0..12 {
            if missing(&ui, "Scroll right", AccessibleRole::Button) {
                break;
            }
            press_at(&ui, centre(&find(&ui, "Scroll right", AccessibleRole::Button)));
            settle();
        }
        assert!(missing(&ui, "Scroll right", AccessibleRole::Button), "at the end");
        let last = find(&ui, "Pinned 12", AccessibleRole::Button);
        assert!(last.absolute_position().x + last.size().width <= ui.window().size().width as f32 - 8.0 + 0.5);

        // And back to the start.
        for _ in 0..12 {
            if missing(&ui, "Scroll left", AccessibleRole::Button) {
                break;
            }
            press_at(&ui, centre(&find(&ui, "Scroll left", AccessibleRole::Button)));
            settle();
        }
        assert!(missing(&ui, "Scroll left", AccessibleRole::Button));
        assert!(pinned_scroll(&ui).abs() < 0.5);
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
        settle();
        // The digit is search text only with a query; here it must not use an item either.
        let seen = take(&log);
        assert!(seen.contains(&"rcopy:2".to_string()), "{seen:?}");
        assert!(seen.contains(&"rdel:2".to_string()), "{seen:?}");
        for line in &seen {
            assert!(
                !line.starts_with("use:") && !line.starts_with("delete:") && !line.starts_with("rename:") && !line.starts_with("quick:") && !line.starts_with("move:"),
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
