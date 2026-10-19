//! Behaviour of the Main window through Slint's testing backend: keyboard flows in the sidebar and the item area,
//! selection (with the real `selection` binding), the context menu, the bulk-action bar, confirmations, search,
//! the icon rail, accessibility roles and labels, and hit targets. Time is mocked, never the real clock.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use std::{cell::RefCell, rc::Rc};

use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementQuery, ElementRoot};
use slint::{ComponentHandle, Model, VecModel, platform::Key};
use snipdeck_ui::{Fonts, MainSection, MainView, MainWindow, selection};

#[path = "support/mod.rs"]
mod support;
use support::{chord, key, on_ui_thread, press, type_text};

#[path = "support/main_fixture.rs"]
mod fixture;

const MIN_HIT: f32 = 24.0;

type Log = Rc<RefCell<Vec<String>>>;

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

/// The last match: an overlay is drawn after (so it comes after) the sidebar entry of the same name.
fn find_top(root: &impl ElementRoot, label: &str, role: AccessibleRole) -> ElementHandle {
    all(root, label, role)
        .into_iter()
        .last()
        .unwrap_or_else(|| panic!("no {role:?} labelled {label:?}"))
}

fn missing(root: &impl ElementRoot, label: &str, role: AccessibleRole) -> bool {
    all(root, label, role).is_empty()
}

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

fn press_key(ui: &MainWindow, k: Key) {
    press(ui.window(), &key(k));
    settle();
}

fn shift(ui: &MainWindow, k: Key) {
    chord(ui.window(), &[Key::Shift], &key(k));
    settle();
}

/// A populated window whose callbacks append a line to the returned log. Focus starts on the item area.
fn start() -> (MainWindow, Log) {
    let ui = MainWindow::new().expect("main window");
    fixture::populate(&ui);
    ui.window().set_size(slint::LogicalSize::new(1040.0, 640.0));
    ui.show().expect("show");
    settle();
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
    hook!(on_section_selected, |s, i| format!("section:{s:?}:{i}"));
    hook!(on_view_changed, |v| format!("view:{v:?}"));
    hook!(on_new_item_requested, || "new".to_string());
    hook!(on_select_requested, |i, m| format!("select:{i}:{m:?}"));
    hook!(on_select_all_requested, || "select-all".to_string());
    hook!(on_clear_selection_requested, || "clear".to_string());
    hook!(on_item_action, |a, i| format!("action:{a:?}:{i}"));
    hook!(on_move_chosen, |i| format!("move:{i}"));
    hook!(on_tag_toggled, |i, on| format!("tag:{i}:{on}"));
    hook!(on_reveal_requested, |i, on| format!("reveal:{i}:{on}"));
    hook!(on_settings_requested, || "settings".to_string());
    hook!(on_help_requested, |i| format!("help:{i}"));
    hook!(on_panel_requested, || "panel".to_string());
    hook!(on_list_edit_requested, |i| format!("list-edit:{i}"));
    hook!(on_list_delete_confirmed, |i| format!("list-delete:{i}"));
    hook!(on_tag_delete_confirmed, |i| format!("tag-delete:{i}"));
    hook!(on_empty_trash_confirmed, || "empty-trash".to_string());
    hook!(on_manage_tags_requested, || "manage-tags".to_string());
    ui.invoke_focus_items();
    settle();
    (ui, log)
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
    // ---- Item area: keyboard -------------------------------------------------------------------

    fn the_item_area_is_one_tab_stop_and_arrows_move_the_cursor_and_report_the_selection() {
        let (ui, log) = start();
        assert!(ui.get_items_focused());
        press_key(&ui, Key::DownArrow);
        assert_eq!(ui.get_cursor(), 1);
        press_key(&ui, Key::DownArrow);
        press_key(&ui, Key::UpArrow);
        assert_eq!(take(&log), ["select:1:Replace", "select:2:Replace", "select:1:Replace"]);

        shift(&ui, Key::DownArrow);
        assert_eq!(take(&log), ["select:2:Extend"]);
        chord(ui.window(), &[Key::Control], " ");
        assert_eq!(take(&log), ["select:2:Toggle"]);
        chord(ui.window(), &[Key::Control], "a");
        assert_eq!(take(&log), ["select-all"]);

        press_key(&ui, Key::End);
        assert_eq!(ui.get_cursor(), 7);
        press_key(&ui, Key::DownArrow);
        assert_eq!(ui.get_cursor(), 7, "the last row stops");
        press_key(&ui, Key::Home);
        assert_eq!(ui.get_cursor(), 0);
        press_key(&ui, Key::UpArrow);
        assert_eq!(ui.get_cursor(), 0, "the first row stops");
    }

    fn enter_uses_shift_enter_copies_and_shortcuts_edit_pin_and_delete() {
        let (ui, log) = start();
        press_key(&ui, Key::DownArrow);
        take(&log);
        press_key(&ui, Key::Return);
        shift(&ui, Key::Return);
        press_key(&ui, Key::F2);
        chord(ui.window(), &[Key::Control], "e");
        chord(ui.window(), &[Key::Control], "p");
        press_key(&ui, Key::Delete);
        assert_eq!(
            take(&log),
            [
                "action:Use:1",
                "action:Copy:1",
                "action:Edit:1",
                "action:Edit:1",
                "action:Pin:1",
                "action:Delete:1"
            ]
        );
    }

    fn escape_clears_a_selection_and_does_nothing_without_one() {
        let (ui, log) = start();
        press_key(&ui, Key::Escape);
        assert_eq!(take(&log), Vec::<String>::new());
        ui.set_selected_count(2);
        settle();
        press_key(&ui, Key::Escape);
        assert_eq!(take(&log), ["clear"]);
    }

    fn holding_space_on_a_sensitive_item_reveals_it_until_release() {
        let (ui, log) = start();
        for _ in 0..6 {
            press_key(&ui, Key::DownArrow);
        }
        take(&log);
        // `press` sends press and release; the pair is a reveal and a hide.
        press(ui.window(), " ");
        assert_eq!(take(&log), ["reveal:6:true", "reveal:6:false"]);
        press_key(&ui, Key::UpArrow);
        take(&log);
        press(ui.window(), " ");
        assert!(take(&log).is_empty(), "Space on an ordinary item does nothing");
    }

    fn the_trash_restores_with_enter_and_deletes_for_good_only_after_asking() {
        let (ui, log) = start();
        ui.set_section(MainSection::Trash);
        settle();
        press_key(&ui, Key::Return);
        assert_eq!(take(&log), ["action:Restore:0"]);

        press_key(&ui, Key::Delete);
        assert!(ui.get_confirm_open());
        assert!(take(&log).is_empty(), "nothing is deleted before the answer");
        let dialog = find(&ui, "Delete for good?", AccessibleRole::Groupbox);
        assert_eq!(
            dialog.accessible_description().as_deref(),
            Some("The selected items are deleted for good. This can't be undone.")
        );
        press_key(&ui, Key::Return);
        assert!(ui.get_confirm_open(), "Enter is not the destructive default");
        press_key(&ui, Key::Escape);
        assert!(!ui.get_confirm_open());
        assert_eq!(take(&log), Vec::<String>::new());

        press_key(&ui, Key::Delete);
        find(&ui, "Delete for good", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["action:DeleteForever:0"]);
        assert!(!ui.get_confirm_open());
        // Edit and Pin are not offered in the Trash.
        press_key(&ui, Key::F2);
        chord(ui.window(), &[Key::Control], "p");
        assert_eq!(take(&log), Vec::<String>::new());
    }

    // ---- Context menu, move and tag ------------------------------------------------------------

    fn the_menu_key_opens_the_context_menu_and_move_to_list_opens_a_chooser() {
        let (ui, log) = start();
        press_key(&ui, Key::DownArrow);
        take(&log);
        press_key(&ui, Key::Menu);
        assert!(ui.get_menu_open());
        for label in ["Use", "Copy", "Edit", "Pin", "Move to list…", "Tags…", "Move to Trash"] {
            let _ = find(&ui, label, AccessibleRole::ListItem);
        }
        press_key(&ui, Key::Escape);
        assert!(!ui.get_menu_open());
        assert!(ui.get_items_focused(), "focus returns to the items");

        press_key(&ui, Key::Menu);
        find(&ui, "Move to list…", AccessibleRole::ListItem).invoke_accessible_default_action();
        settle();
        assert_eq!(take(&log), ["action:Move:1"]);
        assert!(ui.get_menu_open(), "the chooser is open");
        let _ = find(&ui, "Move to list", AccessibleRole::List);
        find_top(&ui, "Addresses", AccessibleRole::ListItem).invoke_accessible_default_action();
        settle();
        assert_eq!(take(&log), ["move:1"]);
        assert!(!ui.get_menu_open(), "choosing a list closes it");
    }

    fn the_tag_chooser_stays_open_and_ticks_each_tag() {
        let (ui, log) = start();
        // The host fills the choices (with what the selection already has) before it opens the chooser.
        ui.set_tag_choices(slint::ModelRc::new(VecModel::from(fixture::tags())));
        press_key(&ui, Key::Menu);
        find(&ui, "Tags…", AccessibleRole::ListItem).invoke_accessible_default_action();
        settle();
        take(&log);
        let chooser = find(&ui, "Tags", AccessibleRole::List);
        assert!(chooser.size().height >= MIN_HIT);
        let billing = all(&ui, "billing", AccessibleRole::ListItem)
            .into_iter()
            .find(|e| e.accessible_checkable() == Some(true))
            .expect("the chooser lists billing as a checkable entry");
        billing.invoke_accessible_default_action();
        settle();
        assert_eq!(take(&log), ["tag:0:true"]);
        assert!(ui.get_menu_open(), "multi-select stays open");
        press_key(&ui, Key::Escape);
        assert!(!ui.get_menu_open());
    }

    fn pin_reads_unpin_for_a_pinned_item() {
        let (ui, _log) = start();
        for _ in 0..3 {
            press_key(&ui, Key::DownArrow);
        }
        press_key(&ui, Key::Menu);
        assert!(missing(&ui, "Pin", AccessibleRole::ListItem), "the Design spec row is pinned");
        let _ = find(&ui, "Unpin", AccessibleRole::ListItem);
    }

    // ---- Selection with the real binding ---------------------------------------------------------

    fn shift_arrows_select_a_range_and_the_bulk_bar_appears() {
        let (ui, _log) = start();
        let rows: Rc<VecModel<_>> = Rc::new(VecModel::from(fixture::sample_rows()));
        ui.set_rows(slint::ModelRc::from(rows.clone()));
        let sel = selection::bind(&ui, &rows);
        settle();

        assert!(missing(&ui, "2 selected", AccessibleRole::Groupbox));
        press_key(&ui, Key::DownArrow);
        shift(&ui, Key::DownArrow);
        shift(&ui, Key::DownArrow);
        assert_eq!(ui.get_selected_count(), 3);
        assert_eq!(sel.borrow().len(), 3);
        let flags: Vec<bool> = (0..rows.row_count()).map(|i| rows.row_data(i).unwrap().selected).collect();
        assert_eq!(flags, [false, true, true, true, false, false, false, false]);
        let _ = find(&ui, "3 selected", AccessibleRole::Groupbox);
        for label in ["Move", "Tag", "Pin", "Move to Trash", "Clear selection"] {
            let b = find(&ui, label, AccessibleRole::Button);
            assert!(b.size().height >= MIN_HIT && b.size().width >= MIN_HIT, "{label}");
        }
        // The context menu on several rows disables what only works on one.
        press_key(&ui, Key::Menu);
        assert_eq!(find(&ui, "Use", AccessibleRole::ListItem).accessible_enabled(), Some(false));
        assert_eq!(find(&ui, "Edit", AccessibleRole::ListItem).accessible_enabled(), Some(false));
        assert_eq!(find(&ui, "Move to Trash", AccessibleRole::ListItem).accessible_enabled(), Some(true));
        press_key(&ui, Key::Escape);

        chord(ui.window(), &[Key::Control], "a");
        assert_eq!(ui.get_selected_count(), 8);
        press_key(&ui, Key::Escape);
        assert_eq!(ui.get_selected_count(), 0);
        assert!(missing(&ui, "8 selected", AccessibleRole::Groupbox));
    }

    fn the_bulk_bar_acts_on_the_selection() {
        let (ui, log) = start();
        ui.set_selected_count(2);
        settle();
        take(&log);
        find(&ui, "Move to Trash", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["action:Delete:0"]);
        find(&ui, "Clear selection", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["clear"]);
        ui.set_section(MainSection::Trash);
        settle();
        assert!(missing(&ui, "Move", AccessibleRole::Button));
        let _ = find(&ui, "Restore", AccessibleRole::Button);
        find(&ui, "Delete for good…", AccessibleRole::Button).invoke_accessible_default_action();
        assert!(ui.get_confirm_open());
    }

    // ---- Sidebar -----------------------------------------------------------------------------------

    fn the_sidebar_is_one_tab_stop_with_arrows_home_end_and_enter() {
        let (ui, log) = start();
        for _ in 0..12 {
            if ui.get_sidebar_focused() {
                break;
            }
            press_key(&ui, Key::Backtab);
        }
        assert!(ui.get_sidebar_focused(), "Shift+Tab reaches the sidebar");
        take(&log);
        for _ in 0..4 {
            press_key(&ui, Key::DownArrow);
        }
        press_key(&ui, Key::Return);
        assert_eq!(take(&log), ["section:List:0"]);
        assert_eq!(ui.get_section(), MainSection::List);
        assert_eq!(ui.get_section_index(), 0);

        press_key(&ui, Key::End);
        press_key(&ui, Key::Return);
        assert_eq!(take(&log), ["section:Tag:2"]);
        press_key(&ui, Key::Home);
        press_key(&ui, Key::Return);
        assert_eq!(take(&log), ["section:All:0"]);
        press_key(&ui, Key::UpArrow);
        press_key(&ui, Key::Return);
        assert_eq!(take(&log), ["section:All:0"], "the first entry stops");
    }

    fn the_sidebar_names_every_entry_and_marks_the_open_view() {
        let (ui, _log) = start();
        let _ = find(&ui, "Navigation", AccessibleRole::List);
        for label in ["All items", "Pinned", "Recent", "Trash"] {
            let _ = find(&ui, label, AccessibleRole::ListItem);
        }
        for label in ["Email replies", "Addresses", "Code", "billing", "meeting", "sql"] {
            let _ = find(&ui, label, AccessibleRole::ListItem);
        }
        // Every entry sits below the board switcher, one under the other, none scrolled out of the pane.
        let ys: Vec<f32> = ["All items", "Pinned", "Recent", "Trash", "Email replies", "billing"]
            .iter()
            .map(|l| find(&ui, l, AccessibleRole::ListItem).absolute_position().y)
            .collect();
        assert!(ys[0] >= 44.0, "All items starts at y={ys:?}");
        assert!(ys.windows(2).all(|w| w[1] >= w[0] + 28.0), "sidebar rows overlap: {ys:?}");
        assert_eq!(find(&ui, "All items", AccessibleRole::ListItem).accessible_item_selected(), Some(true));
        ui.set_section(MainSection::Trash);
        settle();
        assert_eq!(find(&ui, "Trash", AccessibleRole::ListItem).accessible_item_selected(), Some(true));
        assert_eq!(find(&ui, "All items", AccessibleRole::ListItem).accessible_item_selected(), Some(false));
    }

    fn a_list_context_menu_edits_or_asks_before_deleting() {
        let (ui, log) = start();
        for _ in 0..12 {
            if ui.get_sidebar_focused() {
                break;
            }
            press_key(&ui, Key::Backtab);
        }
        for _ in 0..4 {
            press_key(&ui, Key::DownArrow);
        }
        take(&log);
        press_key(&ui, Key::Menu);
        assert!(ui.get_menu_open());
        find(&ui, "Edit list…", AccessibleRole::ListItem).invoke_accessible_default_action();
        assert_eq!(take(&log), ["list-edit:0"]);
        assert!(ui.get_sidebar_focused(), "the keys return to the sidebar");

        press_key(&ui, Key::Menu);
        find(&ui, "Delete list…", AccessibleRole::ListItem).invoke_accessible_default_action();
        settle();
        assert!(ui.get_confirm_open());
        let dialog = find(&ui, "Delete list “Email replies”?", AccessibleRole::Groupbox);
        assert_eq!(
            dialog.accessible_description().as_deref(),
            Some("The list and its 24 items move to the Trash. You can restore them for 30 days.")
        );
        press_key(&ui, Key::Escape);
        assert!(!ui.get_confirm_open());
        assert_eq!(take(&log), Vec::<String>::new());

        press_key(&ui, Key::Menu);
        find(&ui, "Delete list…", AccessibleRole::ListItem).invoke_accessible_default_action();
        settle();
        find(&ui, "Delete list", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["list-delete:0"]);
    }

    fn a_context_menu_is_as_wide_as_its_rows_not_the_window() {
        let (ui, _log) = start();
        for _ in 0..12 {
            if ui.get_sidebar_focused() {
                break;
            }
            press_key(&ui, Key::Backtab);
        }
        for _ in 0..4 {
            press_key(&ui, Key::DownArrow);
        }
        press_key(&ui, Key::Menu);
        let menu = find(&ui, "Actions", AccessibleRole::List);
        let size = menu.size();
        assert!(
            (180.0..=320.0).contains(&size.width),
            "the list menu is {}px wide",
            size.width
        );
        let window = ui.window().size().to_logical(ui.window().scale_factor());
        let right = menu.absolute_position().x + size.width;
        assert!(right <= window.width, "the menu ends at {right}px of {}", window.width);
    }

    fn the_toolbar_and_the_help_menu_can_show_the_panel() {
        let (ui, log) = start();
        // Without a hotkey the tooltip is plain; with one it names it.
        find(&ui, "Show Panel", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["panel"]);
        ui.set_panel_hotkey("Ctrl+Alt+Space".into());
        settle();
        find(&ui, "Show Panel (Ctrl+Alt+Space)", AccessibleRole::Button)
            .invoke_accessible_default_action();
        assert_eq!(take(&log), ["panel"]);

        find(&ui, "Help", AccessibleRole::Button).invoke_accessible_default_action();
        find(&ui, "Show Panel", AccessibleRole::ListItem).invoke_accessible_default_action();
        assert_eq!(take(&log), ["panel"]);
    }

    fn a_tag_and_the_trash_have_their_own_confirmations() {
        let (ui, log) = start();
        for _ in 0..12 {
            if ui.get_sidebar_focused() {
                break;
            }
            press_key(&ui, Key::Backtab);
        }
        press_key(&ui, Key::End);
        take(&log);
        press_key(&ui, Key::Menu);
        find(&ui, "Manage tags…", AccessibleRole::ListItem).invoke_accessible_default_action();
        assert_eq!(take(&log), ["manage-tags"]);
        press_key(&ui, Key::Menu);
        find(&ui, "Delete tag…", AccessibleRole::ListItem).invoke_accessible_default_action();
        settle();
        let _ = find(&ui, "Delete tag “sql”?", AccessibleRole::Groupbox);
        find(&ui, "Delete tag", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["tag-delete:2"]);

        // Trash: the context menu on the Trash entry empties it, after asking.
        press_key(&ui, Key::Home);
        for _ in 0..3 {
            press_key(&ui, Key::DownArrow);
        }
        press_key(&ui, Key::Menu);
        find(&ui, "Empty Trash…", AccessibleRole::ListItem).invoke_accessible_default_action();
        settle();
        let _ = find(&ui, "Empty the Trash?", AccessibleRole::Groupbox);
        assert_eq!(take(&log), Vec::<String>::new());
        find(&ui, "Empty Trash", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["empty-trash"]);
    }

    // ---- Toolbar -----------------------------------------------------------------------------------

    fn search_takes_ctrl_f_and_slash_reports_edits_and_clears() {
        let (ui, log) = start();
        assert!(!ui.get_search_focused());
        chord(ui.window(), &[Key::Control], "f");
        assert!(ui.get_search_focused());
        type_text(ui.window(), "inv");
        settle();
        assert_eq!(ui.get_query(), "inv");
        assert_eq!(take(&log), ["query:i", "query:in", "query:inv"]);
        let field = find(&ui, "Search snippets", AccessibleRole::TextInput);
        assert_eq!(field.accessible_value().as_deref(), Some("inv"));
        find(&ui, "Clear search", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(ui.get_query(), "");
        assert_eq!(take(&log), ["query:"]);

        ui.invoke_focus_items();
        settle();
        press(ui.window(), "/");
        settle();
        assert!(ui.get_search_focused(), "/ moves to the search");
        assert_eq!(ui.get_query(), "", "and is not typed into it");
    }

    fn the_help_button_opens_a_menu_of_three_pages_and_f1_asks_for_the_guide() {
        let (ui, log) = start();
        for (label, topic) in [("Troubleshoot…", 0), ("User guide", 1), ("About Snipdeck", 2)] {
            find(&ui, "Help", AccessibleRole::Button).invoke_accessible_default_action();
            settle();
            assert!(ui.get_menu_open(), "the Help menu is open");
            find(&ui, label, AccessibleRole::ListItem).invoke_accessible_default_action();
            settle();
            assert!(!ui.get_menu_open(), "choosing a page closes the menu");
            assert_eq!(take(&log), [format!("help:{topic}")]);
        }
        press_key(&ui, Key::F1);
        assert_eq!(take(&log), ["help:1"]);
    }

    fn ctrl_n_and_the_buttons_report_what_was_asked() {
        let (ui, log) = start();
        chord(ui.window(), &[Key::Control], "n");
        assert_eq!(take(&log), ["new"]);
        find(&ui, "New item", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["new"]);
        find(&ui, "Settings", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["settings"]);

        let comfortable = find(&ui, "Comfortable view", AccessibleRole::Button);
        assert_eq!(comfortable.accessible_checked(), Some(true));
        find(&ui, "Compact view", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert_eq!(ui.get_view(), MainView::Compact);
        assert_eq!(take(&log), ["view:Compact"]);
        assert_eq!(find(&ui, "Compact view", AccessibleRole::Button).accessible_checked(), Some(true));
        find(&ui, "Tile view", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert_eq!(ui.get_view(), MainView::Tile);
        assert!(ui.get_columns() >= 1);
        // Tiles move across and down.
        ui.invoke_focus_items();
        settle();
        take(&log);
        press_key(&ui, Key::RightArrow);
        assert_eq!(take(&log), ["select:1:Replace"]);
        press_key(&ui, Key::DownArrow);
        let cols = ui.get_columns();
        assert_eq!(take(&log), [format!("select:{}:Replace", (1 + cols).min(7))]);
    }

    fn the_board_button_names_the_board_and_asks_for_the_menu() {
        let (ui, _log) = start();
        let board = find(&ui, "Board: Work", AccessibleRole::Button);
        assert!(board.size().height >= MIN_HIT);
    }

    // ---- States ------------------------------------------------------------------------------------

    fn the_sidebar_collapses_to_a_rail_below_720px_and_keeps_its_names() {
        let (ui, _log) = start();
        assert!(!ui.get_rail());
        ui.window().set_size(slint::LogicalSize::new(640.0, 520.0));
        settle();
        assert!(ui.get_rail());
        for label in ["All items", "Pinned", "Email replies", "billing"] {
            let e = find(&ui, label, AccessibleRole::ListItem);
            assert!(e.size().width >= MIN_HIT && e.size().height >= MIN_HIT, "{label}");
        }
        let _ = find(&ui, "Board: Work", AccessibleRole::Button);
        ui.window().set_size(slint::LogicalSize::new(760.0, 520.0));
        settle();
        assert!(!ui.get_rail());
    }

    fn an_empty_trash_has_nothing_to_empty() {
        let (ui, _log) = start();
        fixture::set_rows(&ui, Vec::new());
        ui.set_section(MainSection::Trash);
        settle();
        assert!(missing(&ui, "Empty Trash…", AccessibleRole::Button), "nothing to empty");
    }

    fn the_trash_offers_to_empty_itself_when_it_has_items() {
        let (ui, log) = start();
        ui.set_section(MainSection::Trash);
        settle();
        find(&ui, "Empty Trash…", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert!(ui.get_confirm_open());
        find(&ui, "Empty Trash", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["empty-trash"]);
    }

    // ---- Accessibility and targets ---------------------------------------------------------------------

    fn rows_read_as_title_and_preview_and_never_leak_a_secret() {
        let (ui, _log) = start();
        let labels: Vec<String> = by_role(&ui, AccessibleRole::ListItem)
            .iter()
            .filter_map(|r| r.accessible_label().map(|l| l.to_string()))
            .collect();
        assert!(labels.iter().any(|l| l.starts_with("Thanks, received")), "{labels:?}");
        assert!(labels.iter().all(|l| !l.contains("hunter2")), "{labels:?}");
        let _ = find(&ui, "Snippets", AccessibleRole::List);
    }

    fn every_control_has_a_role_a_label_and_a_big_enough_target() {
        let (ui, _log) = start();
        ui.set_selected_count(2);
        ui.set_toast_text("Deleted 2 items".into());
        ui.set_toast_undo_label("Undo".into());
        settle();
        let mut checked = 0;
        for role in [
            AccessibleRole::Button,
            AccessibleRole::TextInput,
            AccessibleRole::ListItem,
        ] {
            for e in by_role(&ui, role) {
                let s = e.size();
                assert!(
                    e.accessible_label().is_some_and(|l| !l.is_empty()),
                    "an unlabelled {role:?}"
                );
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
        assert!(checked >= 20, "checked {checked} targets");
    }

    fn the_cjk_family_is_one_global_with_plex_as_the_default() {
        let (ui, _log) = start();
        let fonts = ui.global::<Fonts<'_>>();
        assert_eq!(fonts.get_sans(), "IBM Plex Sans");
        assert_eq!(fonts.get_mono(), "IBM Plex Mono");
        fonts.set_cjk_family("IPAGothic".into());
        assert_eq!(fonts.get_sans(), "IPAGothic");
        assert_eq!(fonts.get_mono(), "IBM Plex Mono", "code and keycaps never change");
        fonts.set_cjk_family("".into());
        assert_eq!(fonts.get_sans(), "IBM Plex Sans");
    }
}
