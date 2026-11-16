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

use i_slint_backend_testing::{
    AccessibleLiveness, AccessibleRole, ElementHandle, ElementQuery, ElementRoot, mock_elapsed_time,
};
use slint::{
    ComponentHandle, LogicalPosition, Model, ModelRc, SharedString, VecModel,
    platform::{Key, WindowEvent},
};
use snipdeck_ui::{Fonts, MainSection, MainView, MainWindow, PaletteRow, selection};

#[path = "support/mod.rs"]
mod support;
use support::{chord, compose, key, on_ui_thread, press, type_text};

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

fn press_at_centre(ui: &MainWindow, at: slint::LogicalPosition) {
    use slint::platform::{PointerEventButton, WindowEvent};
    let w = ui.window();
    w.dispatch_event(WindowEvent::PointerMoved { position: at });
    w.dispatch_event(WindowEvent::PointerPressed {
        position: at,
        button: PointerEventButton::Left,
    });
    w.dispatch_event(WindowEvent::PointerReleased {
        position: at,
        button: PointerEventButton::Left,
    });
    settle();
}

fn press_key(ui: &MainWindow, k: Key) {
    press(ui.window(), &key(k));
    settle();
}

fn shift(ui: &MainWindow, k: Key) {
    chord(ui.window(), &[Key::Shift], &key(k));
    settle();
}

fn row_titled(ui: &MainWindow, title: &str) -> ElementHandle {
    by_role(ui, AccessibleRole::ListItem)
        .into_iter()
        .find(|e| e.accessible_label().is_some_and(|l| l.starts_with(title)))
        .unwrap_or_else(|| panic!("no row titled {title:?}"))
}

fn centre_of(e: &ElementHandle) -> slint::LogicalPosition {
    let (p, s) = (e.absolute_position(), e.size());
    slint::LogicalPosition::new(p.x + s.width / 2.0, p.y + s.height / 2.0)
}

/// The button called `label` that lies inside `row`'s box (the bulk bar and the menus have buttons of their own).
fn button_in(ui: &MainWindow, row: &ElementHandle, label: &str) -> Option<ElementHandle> {
    let (rp, rs) = (row.absolute_position(), row.size());
    all(ui, label, AccessibleRole::Button)
        .into_iter()
        .find(|b| {
            let p = b.absolute_position();
            p.x >= rp.x - 0.5
                && p.x <= rp.x + rs.width
                && p.y >= rp.y - 0.5
                && p.y <= rp.y + rs.height
        })
}

/// Marks row `i` the single selected one, as the host does after a click.
fn select_row(ui: &MainWindow, i: usize) {
    let rows = ui.get_rows();
    for n in 0..rows.row_count() {
        let mut r = rows.row_data(n).unwrap();
        r.selected = n == i;
        rows.set_row_data(n, r);
    }
    ui.set_selected_count(1);
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
    hook!(on_theme_chosen, |i| format!("theme:{i}"));
    hook!(on_new_item_requested, || "new".to_string());
    hook!(on_quick_add_requested, |t, k, s| format!(
        "quick-add:{t}:{k}:{s}"
    ));
    hook!(on_quick_add_full, |t| format!("quick-add-full:{t}"));
    hook!(on_add_from_clipboard, || "add-from-clipboard".to_string());
    hook!(on_import_requested, || "import".to_string());
    hook!(on_add_from_clipboard, || "add-from-clipboard".to_string());
    hook!(on_import_requested, || "import".to_string());
    hook!(on_select_requested, |i, m| format!("select:{i}:{m:?}"));
    hook!(on_select_all_requested, || "select-all".to_string());
    hook!(on_clear_selection_requested, || "clear".to_string());
    hook!(on_item_action, |a, i| format!("action:{a:?}:{i}"));
    hook!(on_paste_create_requested, || "paste-create".to_string());
    hook!(on_copy_selection_requested, |i| format!(
        "copy-selection:{i}"
    ));
    hook!(on_toast_edit, || "toast-edit".to_string());
    hook!(on_toast_dismissed, || "toast-dismissed".to_string());
    hook!(on_item_reorder_requested, |d| format!("reorder:{d}"));
    hook!(on_move_chosen, |i| format!("move:{i}"));
    hook!(on_paste_as_chosen, |i| format!("paste-as:{i}"));
    hook!(on_tag_toggled, |i, on| format!("tag:{i}:{on}"));
    hook!(on_reveal_requested, |i, on| format!("reveal:{i}:{on}"));
    hook!(on_settings_requested, || "settings".to_string());
    hook!(on_add_list_requested, || "add-list".to_string());
    hook!(on_close_requested, || "close".to_string());
    hook!(on_sidebar_width_changed, |w| format!("sidebar-width:{w}"));
    hook!(on_sidebar_collapsed_changed, |c| format!(
        "sidebar-collapsed:{c}"
    ));
    hook!(on_sort_chosen, |i| format!("sort:{i}"));
    hook!(on_undo_requested, || "undo".to_string());
    hook!(on_help_requested, |page| format!("help:{page:?}"));
    hook!(on_panel_requested, || "panel".to_string());
    hook!(on_list_edit_requested, |i| format!("list-edit:{i}"));
    hook!(on_list_delete_confirmed, |i| format!("list-delete:{i}"));
    hook!(on_tag_delete_confirmed, |i| format!("tag-delete:{i}"));
    hook!(on_empty_trash_confirmed, || "empty-trash".to_string());
    hook!(on_manage_tags_requested, || "manage-tags".to_string());
    hook!(on_palette_opened, || "palette-opened".to_string());
    hook!(on_palette_query_changed, |q| format!("palette-query:{q}"));
    hook!(on_command_requested, |id| format!("command:{id}"));
    hook!(on_row_dropped, |from, onto| format!(
        "row-dropped:{from}:{onto}"
    ));
    hook!(on_row_dropped_above, |from, onto| format!(
        "row-dropped-above:{from}:{onto}"
    ));
    hook!(on_row_dropped_on_list, |row, list| format!(
        "row-on-list:{row}:{list}"
    ));
    hook!(on_list_dropped, |from, onto| format!(
        "list-dropped:{from}:{onto}"
    ));
    // The host's payloads ride as user data; the tests use plain text of the same shape.
    ui.on_row_payload(|i| slint::DataTransfer::from(SharedString::from(format!("row:{i}"))));
    ui.on_list_payload(|i| slint::DataTransfer::from(SharedString::from(format!("list:{i}"))));
    let index_of = |data: &slint::DataTransfer, prefix: &str| {
        data.plain_text()
            .ok()
            .and_then(|t| t.strip_prefix(prefix).and_then(|n| n.parse::<i32>().ok()))
            .unwrap_or(-1)
    };
    ui.on_payload_row(move |d| index_of(&d, "row:"));
    ui.on_payload_list(move |d| index_of(&d, "list:"));
    ui.set_rows_reorderable(true);
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
    fn the_sidebar_toggle_lives_in_the_sidebar_header() {
        let (ui, _log) = start();
        let sidebar_w = f64::from(ui.get_sidebar_width());
        let toggle = find(&ui, "Collapse sidebar (Ctrl+B)", AccessibleRole::Button);
        let board = find(&ui, "Board: Work", AccessibleRole::Button);
        let (tp, tz) = (toggle.absolute_position(), toggle.size());
        let (bp, bz) = (board.absolute_position(), board.size());
        assert!(
            f64::from(tp.x + tz.width) <= sidebar_w,
            "inside the sidebar: right edge {} of {sidebar_w}",
            tp.x + tz.width
        );
        assert!(
            sidebar_w - f64::from(tp.x + tz.width) <= 12.0,
            "right-aligned in the sidebar header: right edge {} of {sidebar_w}",
            tp.x + tz.width
        );
        assert!(tp.x > bp.x + bz.width, "after the board switcher, on its row");
        let (tc, bc) = (tp.y + tz.height / 2.0, bp.y + bz.height / 2.0);
        assert!((tc - bc).abs() <= 1.0, "same row: {tc} vs {bc}");
        // On the rail it tops the rail, above the board button.
        ui.set_sidebar_collapsed(true);
        settle();
        let toggle = find(&ui, "Expand sidebar (Ctrl+B)", AccessibleRole::Button);
        let board = find(&ui, "Board: Work", AccessibleRole::Button);
        assert!(toggle.absolute_position().y < board.absolute_position().y, "top of the rail");
        assert!(
            f64::from(toggle.absolute_position().x + toggle.size().width) <= 48.0,
            "within the rail"
        );
    }

    fn the_tile_button_is_off_and_unchecked_while_groups_draw_the_rows() {
        let (ui, _log) = start();
        ui.set_view(snipdeck_ui::MainView::Tile);
        settle();
        let tile = || find(&ui, "Tile view", AccessibleRole::Button);
        assert_eq!(tile().accessible_enabled(), Some(true));
        assert_eq!(tile().accessible_checked(), Some(true));
        ui.set_has_groups(true);
        settle();
        assert_eq!(tile().accessible_enabled(), Some(false), "tiles cannot show groups");
        assert_eq!(tile().accessible_checked(), Some(false), "and it is not announced as checked");
        ui.set_has_groups(false);
        settle();
        assert_eq!(tile().accessible_checked(), Some(true), "the choice itself is kept");
    }

    fn a_header_made_with_a_rename_request_opens_its_editor_at_once() {
        let (ui, _log) = start();
        // Ctrl+G adds the header and asks for its rename in the same refresh: the header is born
        // with the request, so the editor must open from the start value, not from a change.
        ui.set_rename_row(0);
        ui.set_rename_seq(1);
        let mut rows = fixture::sample_rows();
        rows.truncate(3);
        rows.insert(
            0,
            snipdeck_ui::MainRow {
                title: "Group".into(),
                is_header: true,
                group_count: 2,
                ..Default::default()
            },
        );
        rows[1].grouped = true;
        rows[2].grouped = true;
        fixture::set_rows(&ui, rows);
        ui.set_has_groups(true);
        settle();
        assert!(!all(&ui, "Group name", AccessibleRole::TextInput).is_empty());
    }

    fn a_group_header_is_a_labelled_expandable_button_and_has_its_own_keys() {
        let (ui, log) = start();
        let header = |count: i32, collapsed: bool| snipdeck_ui::MainRow {
            title: "Replies".into(),
            is_header: true,
            group_count: count,
            group_collapsed: collapsed,
            ..Default::default()
        };
        let mut rows = fixture::sample_rows();
        rows.truncate(3);
        rows.insert(0, header(2, false));
        rows[1].grouped = true;
        rows[2].grouped = true;
        let model = fixture::set_rows(&ui, rows);
        ui.set_has_groups(true);
        ui.set_list_total(9);
        settle();
        // The footer counts items, not rows (a header is not an item; folded members still are).
        assert!(ElementHandle::find_by_accessible_label(&ui, "9 items").next().is_some());
        {
            let l = log.clone();
            ui.on_group_requested(move |i| l.borrow_mut().push(format!("group:{i}")));
            let l = log.clone();
            ui.on_group_toggled(move |i| l.borrow_mut().push(format!("toggle:{i}")));
            let l = log.clone();
            ui.on_group_collapsed_set(move |i, c| l.borrow_mut().push(format!("collapse:{i}:{c}")));
            let l = log.clone();
            ui.on_group_rename_requested(move |i| l.borrow_mut().push(format!("rename:{i}")));
            let l = log.clone();
            ui.on_group_delete_requested(move |i| l.borrow_mut().push(format!("delete:{i}")));
        }
        take(&log);
        let button = find(&ui, "group Replies, 2 items", AccessibleRole::Button);
        assert_eq!(button.accessible_expanded(), Some(true));
        assert_eq!(button.accessible_expandable(), Some(true));
        // Ctrl+G groups at the cursor row.
        chord(ui.window(), &[Key::Control], "g");
        settle();
        assert_eq!(take(&log).last().map(String::as_str), Some("group:0"));
        // The cursor is on the header: Left folds, Right unfolds, Enter toggles, F2 renames, Delete deletes the group.
        press_key(&ui, Key::Home);
        take(&log);
        press_key(&ui, Key::LeftArrow);
        press_key(&ui, Key::RightArrow);
        press_key(&ui, Key::Return);
        press_key(&ui, Key::F2);
        press_key(&ui, Key::Delete);
        assert_eq!(
            take(&log),
            ["collapse:0:true", "collapse:0:false", "toggle:0", "rename:0", "delete:0"]
        );
        // A click on the header toggles it.
        press_at_centre(&ui, centre_of(&button));
        assert_eq!(take(&log), ["toggle:0"]);
        // A collapsed header says so.
        let mut h = model.row_data(0).unwrap();
        h.group_collapsed = true;
        model.set_row_data(0, h);
        settle();
        assert_eq!(
            find(&ui, "group Replies, 2 items", AccessibleRole::Button).accessible_expanded(),
            Some(false)
        );
    }
    // ---- Item area: keyboard -------------------------------------------------------------------

    fn wheeling_back_toward_the_cursor_row_moves_the_list_by_the_wheel_step_only() {
        let (ui, _log) = start();
        fixture::set_rows(&ui, fixture::numbered_rows(60));
        settle();
        ui.set_cursor(3);
        settle();
        let wheel = |dy: f32| {
            ui.window().dispatch_event(WindowEvent::PointerScrolled {
                position: LogicalPosition::new(640.0, 350.0),
                delta_x: 0.0,
                delta_y: dy,
            });
            mock_elapsed_time(std::time::Duration::from_secs(1));
            settle();
            mock_elapsed_time(std::time::Duration::from_millis(100));
            settle();
        };
        wheel(-2000.0);
        let far = ui.get_scroll_y();
        assert!(far < -1000.0, "scrolled away: {far}");
        // Back toward the cursor row in 20px steps, including the one that brings its row back into the ListView.
        let mut at = far;
        for step in 0..150 {
            wheel(20.0);
            let now = ui.get_scroll_y();
            if now > -0.5 {
                break;
            }
            assert!(
                ((now - at) - 20.0).abs() < 0.5,
                "step {step}: the list moved {} for a 20px wheel step (from {at} to {now})",
                now - at
            );
            at = now;
        }
        assert!(at > -400.0, "got back to the top region: {at}");
    }

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
        chord(ui.window(), &[Key::Alt], &key(Key::Return));
        settle();
        press_key(&ui, Key::F2);
        chord(ui.window(), &[Key::Control], "e");
        chord(ui.window(), &[Key::Control], "p");
        chord(ui.window(), &[Key::Control], "d");
        press_key(&ui, Key::Delete);
        assert_eq!(
            take(&log),
            [
                "action:Use:1",
                "action:Copy:1",
                "action:UsePlain:1",
                "action:Edit:1",
                "action:Edit:1",
                "action:Pin:1",
                "action:Duplicate:1",
                "action:Delete:1"
            ]
        );
    }

    fn ctrl_d_duplicates_nothing_in_the_trash() {
        let (ui, log) = start();
        ui.set_section(MainSection::Trash);
        settle();
        take(&log);
        chord(ui.window(), &[Key::Control], "d");
        assert_eq!(take(&log), Vec::<String>::new());
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

    fn a_context_menu_in_the_shortest_window_stays_inside_it() {
        let (ui, _log) = start();
        ui.window().set_size(slint::LogicalSize::new(1040.0, 360.0));
        settle();
        press_key(&ui, Key::DownArrow);
        press_key(&ui, Key::Menu);
        assert!(ui.get_menu_open());
        let menu = find(&ui, "Item actions", AccessibleRole::List);
        let bottom = menu.absolute_position().y + menu.size().height;
        assert!(bottom <= 360.0, "the menu bottom {bottom} is inside the 360px window");
    }

    fn the_menu_key_opens_the_context_menu_and_move_to_list_opens_a_chooser() {
        let (ui, log) = start();
        press_key(&ui, Key::DownArrow);
        take(&log);
        press_key(&ui, Key::Menu);
        assert!(ui.get_menu_open());
        for label in ["Use", "Copy", "Edit", "Duplicate", "Pin", "Move to list…", "Tags…", "Move to Trash"] {
            let _ = find(&ui, label, AccessibleRole::ListItem);
        }
        find(&ui, "Duplicate", AccessibleRole::ListItem).invoke_accessible_default_action();
        settle();
        assert_eq!(take(&log), ["action:Duplicate:1"]);
        press_key(&ui, Key::Menu);
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

    fn paste_as_in_the_item_menu_opens_a_chooser_of_the_six_transforms() {
        let (ui, log) = start();
        press_key(&ui, Key::DownArrow);
        take(&log);
        press_key(&ui, Key::Menu);
        let entry = find(&ui, "Paste as…", AccessibleRole::ListItem);
        assert_eq!(entry.accessible_enabled(), Some(true));
        entry.invoke_accessible_default_action();
        settle();
        assert_eq!(take(&log), ["action:PasteAs:1"]);
        assert!(ui.get_menu_open(), "the chooser is open");
        let _ = find(&ui, "Paste as…", AccessibleRole::List);
        for label in ["Plain text", "UPPERCASE", "lowercase", "Title Case", "Trimmed", "slug-like-this"] {
            let _ = find_top(&ui, label, AccessibleRole::ListItem);
        }
        find_top(&ui, "UPPERCASE", AccessibleRole::ListItem).invoke_accessible_default_action();
        settle();
        assert_eq!(take(&log), ["paste-as:1"]);
        assert!(!ui.get_menu_open(), "choosing closes it");
    }

    fn the_paste_as_chooser_takes_arrows_enter_and_escape() {
        let (ui, log) = start();
        press_key(&ui, Key::Menu);
        find(&ui, "Paste as…", AccessibleRole::ListItem).invoke_accessible_default_action();
        settle();
        take(&log);
        // The first choice has the ring; Down twice reaches the third (lowercase).
        press_key(&ui, Key::DownArrow);
        press_key(&ui, Key::DownArrow);
        press_key(&ui, Key::Return);
        settle();
        assert_eq!(take(&log), ["paste-as:2"]);
        assert!(!ui.get_menu_open());
        assert!(ui.get_items_focused(), "focus returns to the items");

        press_key(&ui, Key::Menu);
        find(&ui, "Paste as…", AccessibleRole::ListItem).invoke_accessible_default_action();
        settle();
        take(&log);
        press_key(&ui, Key::Escape);
        settle();
        assert!(!ui.get_menu_open(), "Esc closes the second level");
        assert_eq!(take(&log), Vec::<String>::new(), "nothing is pasted");
        assert!(ui.get_items_focused());
    }

    fn paste_as_is_off_for_pictures_files_and_a_multiple_selection() {
        let (ui, _log) = start();
        for _ in 0..4 {
            press_key(&ui, Key::DownArrow);
        }
        press_key(&ui, Key::Menu);
        assert_eq!(
            find(&ui, "Paste as…", AccessibleRole::ListItem).accessible_enabled(),
            Some(false),
            "a picture has no text to transform"
        );
        press_key(&ui, Key::Escape);
        press_key(&ui, Key::UpArrow);
        press_key(&ui, Key::UpArrow);
        press_key(&ui, Key::UpArrow);
        press_key(&ui, Key::UpArrow);
        ui.set_selected_count(2);
        settle();
        press_key(&ui, Key::Menu);
        assert_eq!(
            find(&ui, "Paste as…", AccessibleRole::ListItem).accessible_enabled(),
            Some(false),
            "one item at a time"
        );
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

    fn dragging_a_row_up_to_the_top_slot_and_swapping_the_first_two_rows_works_in_the_list() {
        let (ui, log) = start();
        let _ = take(&log);
        let rows: Vec<_> = by_role(&ui, AccessibleRole::ListItem)
            .into_iter()
            .filter(|e| e.accessible_label().is_some_and(|l| !l.is_empty()))
            .collect();
        // The cards, not the sidebar's lists: the widest items.
        let widest = rows.iter().map(|e| e.size().width).fold(0.0f32, f32::max);
        let mut rows: Vec<_> = rows.into_iter().filter(|e| e.size().width > widest * 0.9).collect();
        rows.sort_by(|a, b| a.absolute_position().y.total_cmp(&b.absolute_position().y));
        let (r0, r1, r2) = (rows[0].clone(), rows[1].clone(), rows[2].clone());
        let drag = |from: slint::LogicalPosition, to: slint::LogicalPosition| {
            use slint::platform::{PointerEventButton, WindowEvent};
            let w = ui.window();
            w.dispatch_event(WindowEvent::PointerMoved { position: from });
            w.dispatch_event(WindowEvent::PointerPressed { position: from, button: PointerEventButton::Left });
            w.dispatch_event(WindowEvent::PointerMoved { position: slint::LogicalPosition::new(from.x, from.y + 12.0) });
            w.dispatch_event(WindowEvent::PointerMoved { position: to });
            w.dispatch_event(WindowEvent::PointerReleased { position: to, button: PointerEventButton::Left });
            settle();
            take(&log)
        };
        let at = |e: &ElementHandle, dy: f32| {
            slint::LogicalPosition::new(e.absolute_position().x + e.size().width / 2.0, e.absolute_position().y + dy)
        };
        // Row 2 and row 1 dropped on the top edge, upper half and middle of row 0: the first slot.
        for dy in [-2.0, 1.0, r0.size().height / 4.0, r0.size().height / 2.0] {
            assert_eq!(drag(centre_of(&r2), at(&r0, dy)), ["row-dropped:2:0"], "row 2, {dy}px");
            assert_eq!(drag(centre_of(&r1), at(&r0, dy)), ["row-dropped:1:0"], "row 1, {dy}px");
        }
        // Swapping rows 1 and 2 (indexes 0 and 1) both ways.
        assert_eq!(drag(centre_of(&r0), centre_of(&r1)), ["row-dropped:0:1"]);
        assert_eq!(drag(centre_of(&r1), centre_of(&r0)), ["row-dropped:1:0"]);
    }

    fn dragging_a_row_onto_a_row_a_list_or_a_list_onto_a_list_reports_the_drop() {
        let (ui, log) = start();
        let _ = take(&log);
        let centre_of = |e: &ElementHandle| {
            let (p, s) = (e.absolute_position(), e.size());
            slint::LogicalPosition::new(p.x + s.width / 2.0, p.y + s.height / 2.0)
        };
        let drag = |from: slint::LogicalPosition, to: slint::LogicalPosition| {
            use slint::platform::{PointerEventButton, WindowEvent};
            let w = ui.window();
            w.dispatch_event(WindowEvent::PointerMoved { position: from });
            w.dispatch_event(WindowEvent::PointerPressed { position: from, button: PointerEventButton::Left });
            w.dispatch_event(WindowEvent::PointerMoved { position: slint::LogicalPosition::new(from.x, from.y + 12.0) });
            w.dispatch_event(WindowEvent::PointerMoved { position: to });
            w.dispatch_event(WindowEvent::PointerReleased { position: to, button: PointerEventButton::Left });
            settle();
        };

        let card = |title: &str| {
            by_role(&ui, AccessibleRole::ListItem)
                .into_iter()
                .find(|e| e.accessible_label().is_some_and(|l| l.starts_with(title)))
                .unwrap_or_else(|| panic!("no card {title}"))
        };
        let row0 = centre_of(&card("Thanks, received"));
        let row2 = centre_of(&card("Retry helper"));
        drag(row0, row2);
        assert_eq!(take(&log), ["row-dropped:0:2"], "row onto row");

        let code = centre_of(&find(&ui, "Code", AccessibleRole::ListItem));
        drag(row0, code);
        assert_eq!(take(&log), ["row-on-list:0:2"], "row onto a sidebar list");

        let email = centre_of(&find(&ui, "Email replies", AccessibleRole::ListItem));
        drag(code, email);
        assert_eq!(take(&log), ["list-dropped:2:0"], "list onto list");

        // While a list is dragged over another, a 2px line marks the slot it would take: below the
        // target when it comes from above, above it when it comes from below. Nothing before or after.
        let lines = || {
            ElementQuery::from_root(&ui)
                .match_id("MainWindow::list-drop-line")
                .find_all()
        };
        let hover = |from: slint::LogicalPosition, to: slint::LogicalPosition, release: bool| {
            use slint::platform::{PointerEventButton, WindowEvent};
            let w = ui.window();
            w.dispatch_event(WindowEvent::PointerMoved { position: from });
            w.dispatch_event(WindowEvent::PointerPressed { position: from, button: PointerEventButton::Left });
            w.dispatch_event(WindowEvent::PointerMoved { position: slint::LogicalPosition::new(from.x, from.y + 12.0) });
            w.dispatch_event(WindowEvent::PointerMoved { position: to });
            settle();
            // Measured before the release: the line goes with the drop and its handle with it.
            let found: Vec<(slint::LogicalPosition, slint::LogicalSize)> = lines()
                .iter()
                .map(|l| (l.absolute_position(), l.size()))
                .collect();
            if release {
                w.dispatch_event(WindowEvent::PointerReleased { position: to, button: PointerEventButton::Left });
                settle();
            }
            found
        };
        assert!(lines().is_empty(), "no line when nothing is dragged");
        let email_item = find(&ui, "Email replies", AccessibleRole::ListItem);
        let code_item = find(&ui, "Code", AccessibleRole::ListItem);
        // Code is below Email replies: dragging it up puts the line above Email replies.
        let found = hover(centre_of(&code_item), centre_of(&email_item), true);
        assert_eq!(found.len(), 1, "one line while a list is over another");
        let (line, target) = (&found[0], &email_item);
        assert!(line.1.height <= 2.5 && line.1.height >= 1.5, "2px: {:?}", line.1);
        assert!(
            (line.0.y - target.absolute_position().y).abs() <= 1.5,
            "above the target when coming from below: {:?} {:?}",
            line.0,
            target.absolute_position()
        );
        let _ = take(&log);
        assert!(lines().is_empty(), "the line goes with the drop");
        // And from above: Email replies dragged onto Code puts it below Code.
        let found = hover(centre_of(&email_item), centre_of(&code_item), true);
        assert_eq!(found.len(), 1);
        let bottom = code_item.absolute_position().y + code_item.size().height;
        assert!(
            (found[0].0.y + found[0].1.height - bottom).abs() <= 1.5,
            "below the target when coming from above"
        );
        let _ = take(&log);

        ui.set_rows_reorderable(false);
        settle();
        drag(row0, row2);
        assert_eq!(take(&log), Vec::<String>::new(), "rows stay put in a sorted list");

        // A plain click still uses the card: the drag layer must not swallow it.
        press_at_centre(&ui, row0);
        assert!(take(&log).iter().any(|l| l.starts_with("select:")), "a click still picks the card");
    }

    fn dragging_a_tile_onto_a_tile_or_a_sidebar_list_reports_the_drop() {
        let (ui, log) = start();
        ui.set_view(MainView::Tile);
        settle();
        let _ = take(&log);
        let centre_of = |e: &ElementHandle| {
            let (p, s) = (e.absolute_position(), e.size());
            slint::LogicalPosition::new(p.x + s.width / 2.0, p.y + s.height / 2.0)
        };
        let drag = |from: slint::LogicalPosition, to: slint::LogicalPosition| {
            use slint::platform::{PointerEventButton, WindowEvent};
            let w = ui.window();
            w.dispatch_event(WindowEvent::PointerMoved { position: from });
            w.dispatch_event(WindowEvent::PointerPressed { position: from, button: PointerEventButton::Left });
            w.dispatch_event(WindowEvent::PointerMoved { position: slint::LogicalPosition::new(from.x + 12.0, from.y) });
            w.dispatch_event(WindowEvent::PointerMoved { position: to });
            w.dispatch_event(WindowEvent::PointerReleased { position: to, button: PointerEventButton::Left });
            settle();
        };
        let card = |title: &str| {
            by_role(&ui, AccessibleRole::ListItem)
                .into_iter()
                .find(|e| e.accessible_label().is_some_and(|l| l.starts_with(title)))
                .unwrap_or_else(|| panic!("no tile {title}"))
        };
        let tile0 = centre_of(&card("Thanks, received"));
        let tile2 = centre_of(&card("Retry helper"));
        drag(tile0, tile2);
        assert_eq!(take(&log), ["row-dropped:0:2"], "tile onto tile");

        let code = centre_of(&find(&ui, "Code", AccessibleRole::ListItem));
        drag(tile0, code);
        assert_eq!(take(&log), ["row-on-list:0:2"], "tile onto a sidebar list");

        ui.set_rows_reorderable(false);
        settle();
        drag(tile0, tile2);
        assert_eq!(take(&log), Vec::<String>::new(), "tiles stay put in a sorted list");
    }

    fn dragging_a_row_at_the_edge_of_a_long_list_scrolls_it_and_stops_when_the_pointer_leaves() {
        use slint::platform::{PointerEventButton, WindowEvent};
        let (ui, log) = start();
        fixture::set_rows(&ui, fixture::numbered_rows(60));
        settle();
        let area_top = by_role(&ui, AccessibleRole::ListItem)
            .into_iter()
            .filter(|e| e.size().width > 300.0 && e.absolute_position().x > 200.0)
            .map(|e| e.absolute_position().y)
            .fold(f32::MAX, f32::min)
            - 4.0;
        let tick = |ms: u64| {
            mock_elapsed_time(std::time::Duration::from_millis(ms));
            settle();
        };
        let wheel = |dy: f32| {
            ui.window().dispatch_event(WindowEvent::PointerScrolled {
                position: LogicalPosition::new(640.0, 350.0),
                delta_x: 0.0,
                delta_y: dy,
            });
            tick(1000);
            tick(100);
        };
        wheel(-2000.0);
        let far = ui.get_scroll_y();
        assert!(far < -1000.0, "scrolled away: {far}");
        let rows: Vec<_> = {
            let mut v: Vec<_> = by_role(&ui, AccessibleRole::ListItem)
                .into_iter()
                .filter(|e| e.size().width > 300.0 && e.absolute_position().x > 200.0)
                .collect();
            v.sort_by(|a, b| a.absolute_position().y.total_cmp(&b.absolute_position().y));
            v
        };
        let mid = &rows[rows.len() / 2];
        let x = mid.absolute_position().x + mid.size().width / 2.0;
        let start_at = LogicalPosition::new(x, mid.absolute_position().y + mid.size().height / 2.0);
        let w = ui.window();
        let _ = take(&log);
        w.dispatch_event(WindowEvent::PointerMoved { position: start_at });
        w.dispatch_event(WindowEvent::PointerPressed { position: start_at, button: PointerEventButton::Left });
        w.dispatch_event(WindowEvent::PointerMoved { position: LogicalPosition::new(x + 12.0, start_at.y) });
        // Out of the band: nothing scrolls.
        let inside = LogicalPosition::new(x, start_at.y);
        w.dispatch_event(WindowEvent::PointerMoved { position: inside });
        let before = ui.get_scroll_y();
        tick(500);
        assert!((ui.get_scroll_y() - before).abs() < 0.5, "no scroll in the middle");
        // Held 4px under the top edge of the item area: the list scrolls toward its top, one step a tick.
        let at_top = LogicalPosition::new(x, area_top + 4.0);
        w.dispatch_event(WindowEvent::PointerMoved { position: at_top });
        for _ in 0..10 {
            tick(16);
        }
        let after = ui.get_scroll_y();
        assert!(after > before + 20.0, "held at the top edge: scrolled {before} -> {after}");
        // Leaving the band stops it.
        w.dispatch_event(WindowEvent::PointerMoved { position: inside });
        tick(16);
        let stopped = ui.get_scroll_y();
        for _ in 0..30 {
            tick(16);
        }
        assert!((ui.get_scroll_y() - stopped).abs() < 0.5, "stopped after leaving the band");
        // Back in the band, long enough to reach the top, never past it.
        w.dispatch_event(WindowEvent::PointerMoved { position: at_top });
        for _ in 0..400 {
            tick(16);
        }
        assert!(ui.get_scroll_y().abs() < 0.5, "reached the top: {}", ui.get_scroll_y());
        // The drop on the first row lands at index 0 (the band is off at the top, so the row takes it).
        let first_row = LogicalPosition::new(x, area_top + 12.0);
        w.dispatch_event(WindowEvent::PointerMoved { position: first_row });
        w.dispatch_event(WindowEvent::PointerReleased { position: first_row, button: PointerEventButton::Left });
        settle();
        let lines: Vec<_> = take(&log).into_iter().filter(|l| l.starts_with("row-dropped")).collect();
        assert_eq!(lines.len(), 1, "one drop: {lines:?}");
        assert!(lines[0].ends_with(":0"), "dropped on the first row: {lines:?}");
    }

    fn dragging_a_row_held_at_the_bottom_edge_scrolls_the_list_to_its_end_and_no_further() {
        use slint::platform::{PointerEventButton, WindowEvent};
        let (ui, log) = start();
        fixture::set_rows(&ui, fixture::numbered_rows(60));
        settle();
        let cards = || {
            let mut v: Vec<_> = by_role(&ui, AccessibleRole::ListItem)
                .into_iter()
                .filter(|e| e.size().width > 300.0 && e.absolute_position().x > 200.0)
                .collect();
            v.sort_by(|a, b| a.absolute_position().y.total_cmp(&b.absolute_position().y));
            v
        };
        let area_top = cards().first().expect("rows").absolute_position().y - 4.0;
        let area_bottom = area_top + ui.get_list_height();
        let first = cards().remove(2);
        let x = first.absolute_position().x + first.size().width / 2.0;
        let from = LogicalPosition::new(x, first.absolute_position().y + first.size().height / 2.0);
        let w = ui.window();
        let _ = take(&log);
        w.dispatch_event(WindowEvent::PointerMoved { position: from });
        w.dispatch_event(WindowEvent::PointerPressed { position: from, button: PointerEventButton::Left });
        w.dispatch_event(WindowEvent::PointerMoved { position: LogicalPosition::new(x + 12.0, from.y) });
        let at_bottom = LogicalPosition::new(x, area_bottom - 4.0);
        w.dispatch_event(WindowEvent::PointerMoved { position: at_bottom });
        let mut last = ui.get_scroll_y();
        for _ in 0..600 {
            mock_elapsed_time(std::time::Duration::from_millis(16));
            settle();
            let now = ui.get_scroll_y();
            assert!(now <= last + 0.5, "only ever toward the end: {last} -> {now}");
            last = now;
        }
        let end = ui.get_scroll_y();
        let bound = ui.get_list_height() - ui.get_list_content_height();
        assert!(
            (end - bound).abs() < 0.5,
            "the viewport ends exactly at the content bound: {end} vs {bound}"
        );
        for _ in 0..30 {
            mock_elapsed_time(std::time::Duration::from_millis(16));
            settle();
        }
        assert!((ui.get_scroll_y() - bound).abs() < 0.5, "stays at the end: {}", ui.get_scroll_y());
        // Released inside the band: the row under the pointer takes the drop.
        w.dispatch_event(WindowEvent::PointerReleased { position: at_bottom, button: PointerEventButton::Left });
        settle();
        let lines: Vec<_> = take(&log).into_iter().filter(|l| l.starts_with("row-dropped")).collect();
        assert_eq!(lines.len(), 1, "a drop in the bottom band lands: {lines:?}");
        assert!(
            lines[0].ends_with(":59") || lines[0].ends_with(":58"),
            "on one of the last rows: {lines:?}"
        );
    }

    fn dragging_a_tile_held_at_the_top_edge_of_the_grid_scrolls_it_up_and_a_drop_lands_on_the_first_tile() {
        use slint::platform::{PointerEventButton, WindowEvent};
        let (ui, log) = start();
        ui.set_view(MainView::Tile);
        fixture::set_rows(&ui, fixture::numbered_rows(60));
        settle();
        let tiles = || {
            let mut v: Vec<_> = by_role(&ui, AccessibleRole::ListItem)
                .into_iter()
                .filter(|e| e.size().width > 100.0 && e.absolute_position().x > 200.0)
                .collect();
            v.sort_by(|a, b| a.absolute_position().y.total_cmp(&b.absolute_position().y));
            v
        };
        let area_top = tiles().first().expect("tiles").absolute_position().y - 4.0;
        let wheel = |dy: f32| {
            ui.window().dispatch_event(WindowEvent::PointerScrolled {
                position: LogicalPosition::new(640.0, 350.0),
                delta_x: 0.0,
                delta_y: dy,
            });
            mock_elapsed_time(std::time::Duration::from_secs(1));
            settle();
            mock_elapsed_time(std::time::Duration::from_millis(100));
            settle();
        };
        wheel(-1500.0);
        assert!(ui.get_grid_scroll_y() < -800.0, "scrolled away: {}", ui.get_grid_scroll_y());
        let t = tiles();
        let src = &t[t.len() / 2];
        let from = LogicalPosition::new(
            src.absolute_position().x + src.size().width / 2.0,
            src.absolute_position().y + src.size().height / 2.0,
        );
        let w = ui.window();
        let _ = take(&log);
        w.dispatch_event(WindowEvent::PointerMoved { position: from });
        w.dispatch_event(WindowEvent::PointerPressed { position: from, button: PointerEventButton::Left });
        w.dispatch_event(WindowEvent::PointerMoved { position: LogicalPosition::new(from.x + 12.0, from.y) });
        let at_top = LogicalPosition::new(from.x, area_top + 4.0);
        w.dispatch_event(WindowEvent::PointerMoved { position: at_top });
        for _ in 0..600 {
            mock_elapsed_time(std::time::Duration::from_millis(16));
            settle();
        }
        assert!(ui.get_grid_scroll_y().abs() < 0.5, "reached the top: {}", ui.get_grid_scroll_y());
        let first = LogicalPosition::new(from.x, area_top + 60.0);
        w.dispatch_event(WindowEvent::PointerMoved { position: first });
        w.dispatch_event(WindowEvent::PointerReleased { position: first, button: PointerEventButton::Left });
        settle();
        let lines: Vec<_> = take(&log).into_iter().filter(|l| l.starts_with("row-dropped")).collect();
        assert_eq!(lines.len(), 1, "one drop: {lines:?}");
        assert!(lines[0].starts_with("row-dropped:") && lines[0].split(':').nth(2).is_some_and(|n| n.parse::<usize>().unwrap() < 6), "dropped on a tile of the first row: {lines:?}");
    }

    fn an_item_dropped_on_the_top_half_of_a_group_header_lands_above_it_and_on_the_bottom_half_joins() {
        use slint::platform::{PointerEventButton, WindowEvent};
        let (ui, log) = start();
        let mut rows = fixture::sample_rows();
        rows.truncate(4);
        rows.insert(
            0,
            snipdeck_ui::MainRow {
                title: "Replies".into(),
                is_header: true,
                group_count: 2,
                ..Default::default()
            },
        );
        rows[1].grouped = true;
        rows[2].grouped = true;
        fixture::set_rows(&ui, rows);
        ui.set_has_groups(true);
        settle();
        let header = find(&ui, "group Replies, 2 items", AccessibleRole::Button);
        let (p, size) = (header.absolute_position(), header.size());
        let at = |f: f32| LogicalPosition::new(p.x + size.width / 2.0, p.y + size.height * f);
        let mut cards: Vec<_> = by_role(&ui, AccessibleRole::ListItem)
            .into_iter()
            .filter(|e| e.size().width > 300.0 && e.absolute_position().x > 200.0)
            .collect();
        cards.sort_by(|a, b| a.absolute_position().y.total_cmp(&b.absolute_position().y));
        let item = cards.last().expect("an item row");
        let from = LogicalPosition::new(
            item.absolute_position().x + item.size().width / 2.0,
            item.absolute_position().y + item.size().height / 2.0,
        );
        let w = ui.window();
        let drop_at = |to: LogicalPosition| {
            let _ = take(&log);
            w.dispatch_event(WindowEvent::PointerMoved { position: from });
            w.dispatch_event(WindowEvent::PointerPressed { position: from, button: PointerEventButton::Left });
            w.dispatch_event(WindowEvent::PointerMoved { position: LogicalPosition::new(from.x + 12.0, from.y) });
            w.dispatch_event(WindowEvent::PointerMoved { position: to });
            w.dispatch_event(WindowEvent::PointerReleased { position: to, button: PointerEventButton::Left });
            settle();
            take(&log).into_iter().filter(|l| l.starts_with("row-dropped")).collect::<Vec<_>>()
        };
        assert_eq!(drop_at(at(0.2)), ["row-dropped-above:4:0"], "top half: above the group");
        assert_eq!(drop_at(at(0.8)), ["row-dropped:4:0"], "bottom half: joins the group");
    }

    fn a_tile_dropped_in_the_gaps_of_the_grid_lands_on_the_neighbouring_tile() {
        let (ui, log) = start();
        ui.set_view(MainView::Tile);
        settle();
        let _ = take(&log);
        let drag = |from: slint::LogicalPosition, to: slint::LogicalPosition| {
            use slint::platform::{PointerEventButton, WindowEvent};
            let w = ui.window();
            w.dispatch_event(WindowEvent::PointerMoved { position: from });
            w.dispatch_event(WindowEvent::PointerPressed { position: from, button: PointerEventButton::Left });
            w.dispatch_event(WindowEvent::PointerMoved { position: slint::LogicalPosition::new(from.x + 12.0, from.y) });
            w.dispatch_event(WindowEvent::PointerMoved { position: to });
            w.dispatch_event(WindowEvent::PointerReleased { position: to, button: PointerEventButton::Left });
            settle();
            take(&log)
        };
        let first = by_role(&ui, AccessibleRole::ListItem)
            .into_iter()
            .find(|e| e.accessible_label().is_some_and(|l| l.starts_with("Thanks, received")))
            .expect("the first tile");
        let size = first.size();
        let mut tiles: Vec<_> = by_role(&ui, AccessibleRole::ListItem)
            .into_iter()
            .filter(|e| (e.size().width - size.width).abs() < 1.0 && (e.size().height - size.height).abs() < 1.0)
            .collect();
        tiles.sort_by(|a, b| {
            let (pa, pb) = (a.absolute_position(), b.absolute_position());
            pa.y.total_cmp(&pb.y).then(pa.x.total_cmp(&pb.x))
        });
        assert!(tiles.len() >= 3, "the fixture shows tiles");
        let (p0, p1) = (tiles[0].absolute_position(), tiles[1].absolute_position());
        let at = |x: f32, y: f32| slint::LogicalPosition::new(x, y);
        let from = |n: usize| {
            let (p, s) = (tiles[n].absolute_position(), tiles[n].size());
            at(p.x + s.width / 2.0, p.y + s.height / 2.0)
        };
        // Above the first tile (the top padding of the grid): the first slot.
        let lines = drag(from(2), at(p0.x + size.width / 2.0, p0.y - 2.0));
        assert_eq!(lines, ["row-dropped:2:0"], "the gap above the first tile");
        // Between tile 0 and tile 1 (the column gap): it belongs to a neighbour, so a drop lands there.
        let gap_x = p0.x + size.width + (p1.x - p0.x - size.width) / 2.0;
        let lines = drag(from(2), at(gap_x, p0.y + size.height / 2.0));
        assert!(
            lines == ["row-dropped:2:0"] || lines == ["row-dropped:2:1"],
            "the column gap between two tiles takes a drop: {lines:?}"
        );
    }

    fn alt_up_and_alt_down_ask_to_move_the_cursor_row_in_the_list_and_in_the_tile_grid() {
        let (ui, log) = start();
        for view in [MainView::Comfortable, MainView::Compact, MainView::Tile] {
            ui.set_view(view);
            settle();
            take(&log);
            chord(ui.window(), &[Key::Alt], &key(Key::DownArrow));
            chord(ui.window(), &[Key::Alt], &key(Key::UpArrow));
            assert_eq!(take(&log), ["reorder:1", "reorder:-1"], "{view:?}");
            press_key(&ui, Key::DownArrow);
            assert!(take(&log).iter().all(|l| !l.starts_with("reorder")), "{view:?}: a plain arrow only moves");
        }
    }

    fn the_item_menu_offers_move_up_and_down_and_disables_them_at_the_ends() {
        let (ui, log) = start();
        let rows = i32::try_from(ui.get_rows().row_count()).expect("rows");
        press_key(&ui, Key::Menu);
        assert_eq!(find(&ui, "Move up", AccessibleRole::ListItem).accessible_enabled(), Some(false), "first row");
        assert_eq!(find(&ui, "Move down", AccessibleRole::ListItem).accessible_enabled(), Some(true));
        take(&log);
        find(&ui, "Move down", AccessibleRole::ListItem).invoke_accessible_default_action();
        settle();
        assert_eq!(take(&log), ["reorder:1"]);
        assert!(!ui.get_menu_open());

        press_key(&ui, Key::End);
        assert_eq!(ui.get_cursor(), rows - 1);
        press_key(&ui, Key::Menu);
        assert_eq!(find(&ui, "Move down", AccessibleRole::ListItem).accessible_enabled(), Some(false), "last row");
        assert_eq!(find(&ui, "Move up", AccessibleRole::ListItem).accessible_enabled(), Some(true));
        take(&log);
        find(&ui, "Move up", AccessibleRole::ListItem).invoke_accessible_default_action();
        settle();
        assert_eq!(take(&log), ["reorder:-1"]);

        ui.set_rows_reorderable(false);
        settle();
        press_key(&ui, Key::Menu);
        assert_eq!(find(&ui, "Move up", AccessibleRole::ListItem).accessible_enabled(), Some(false), "sorted list");
        assert_eq!(find(&ui, "Move down", AccessibleRole::ListItem).accessible_enabled(), Some(false));
    }

    fn a_tile_click_and_right_click_still_work_under_the_drag_layer() {
        use slint::platform::{PointerEventButton, WindowEvent};
        let (ui, log) = start();
        ui.set_view(MainView::Tile);
        settle();
        let tile = by_role(&ui, AccessibleRole::ListItem)
            .into_iter()
            .find(|e| e.accessible_label().is_some_and(|l| l.starts_with("Retry helper")))
            .map(|e| {
                let (p, s) = (e.absolute_position(), e.size());
                slint::LogicalPosition::new(p.x + s.width / 2.0, p.y + s.height / 2.0)
            })
            .expect("a tile");
        take(&log);
        press_at_centre(&ui, tile);
        assert!(take(&log).iter().any(|l| l.starts_with("select:")), "a click picks the tile");

        let w = ui.window();
        w.dispatch_event(WindowEvent::PointerMoved { position: tile });
        w.dispatch_event(WindowEvent::PointerPressed { position: tile, button: PointerEventButton::Right });
        w.dispatch_event(WindowEvent::PointerReleased { position: tile, button: PointerEventButton::Right });
        settle();
        assert!(ui.get_menu_open(), "a right-click opens the item menu");
        let _ = find(&ui, "Move down", AccessibleRole::ListItem);
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

    // ---- Quiet rows: Use, Copy, Edit, More ---------------------------------------------------------

    fn click_selects_not_uses() {
        let (ui, log) = start();
        take(&log);
        let row = row_titled(&ui, "Retry helper");
        press_at_centre(&ui, centre_of(&row));
        let seen = take(&log);
        assert!(seen.iter().any(|l| l.starts_with("select:2")), "a click selects: {seen:?}");
        assert!(!seen.iter().any(|l| l.starts_with("action:")), "and runs nothing: {seen:?}");
    }

    fn use_action_uses_once() {
        let (ui, log) = start();
        select_row(&ui, 1);
        take(&log);
        let row = row_titled(&ui, "Invoice reminder");
        let use_it = button_in(&ui, &row, "Use (Enter)").expect("a visible Use action on the selected row");
        use_it.invoke_accessible_default_action();
        settle();
        assert_eq!(take(&log), ["action:Use:1"], "Use runs once and does not select");
        let more = button_in(&ui, &row, "More");
        let edit = button_in(&ui, &row, "Edit");
        assert!(more.is_some() && edit.is_some(), "Edit and More are there too");
        edit.unwrap().invoke_accessible_default_action();
        assert_eq!(take(&log), ["action:Edit:1"]);
        more.unwrap().invoke_accessible_default_action();
        settle();
        assert!(ui.get_menu_open(), "More opens the item menu");
    }

    fn copy_hidden_when_use_copies() {
        let (ui, _log) = start();
        select_row(&ui, 1);
        let row = row_titled(&ui, "Invoice reminder");
        assert!(button_in(&ui, &row, "Copy").is_some(), "with a paste target Copy is its own action");
        ui.set_use_label("Copy".into());
        ui.set_use_copies(true);
        settle();
        let row = row_titled(&ui, "Invoice reminder");
        assert!(button_in(&ui, &row, "Copy").is_none(), "Use already copies: no second Copy");
        assert!(button_in(&ui, &row, "Copy (Enter)").is_some(), "the Use action reads Copy (Enter)");
    }

    fn trash_use_action_restores() {
        let (ui, log) = start();
        ui.set_section(MainSection::Trash);
        select_row(&ui, 1);
        take(&log);
        let row = row_titled(&ui, "Invoice reminder");
        assert!(button_in(&ui, &row, "Use (Enter)").is_none());
        assert!(button_in(&ui, &row, "Edit").is_none(), "a trashed item is not edited");
        button_in(&ui, &row, "Restore").expect("Restore").invoke_accessible_default_action();
        assert_eq!(take(&log), ["action:Restore:1"]);
    }

    fn where_line_in_mixed_views_only() {
        let (ui, _log) = start();
        let plain = row_titled(&ui, "Invoice reminder").size().height;
        assert!((plain - 56.0).abs() < 0.5, "Comfortable row: {plain}");
        fixture::set_rows(&ui, fixture::search_rows());
        ui.set_query("re".into());
        settle();
        let mixed = row_titled(&ui, "Thanks, received").size().height;
        assert!((mixed - 68.0).abs() < 0.5, "a mixed view adds the 12px where line: {mixed}");
        ui.set_view(MainView::Compact);
        settle();
        let compact_mixed = row_titled(&ui, "Thanks, received").size().height;
        assert!((compact_mixed - 52.0).abs() < 0.5, "Compact mixed: {compact_mixed}");
        fixture::set_rows(&ui, fixture::sample_rows());
        ui.set_query("".into());
        settle();
        let compact = row_titled(&ui, "Invoice reminder").size().height;
        assert!((compact - 36.0).abs() < 0.5, "Compact: {compact}");
    }

    fn actions_on_preview_line_only() {
        let (ui, _log) = start();
        select_row(&ui, 1);
        let row = row_titled(&ui, "Invoice reminder");
        let (top, bottom) = (row.absolute_position().y, row.absolute_position().y + row.size().height);
        for name in ["Use (Enter)", "Copy", "Edit", "More"] {
            let b = button_in(&ui, &row, name).unwrap_or_else(|| panic!("{name}"));
            // The title line is the first 24px (4 + 20); the 32px action boxes belong to the preview band.
            assert!(b.absolute_position().y >= top + 24.0 - 0.5, "{name} is below the title line");
            assert!(b.absolute_position().y + b.size().height <= bottom + 0.5, "{name} is inside the row");
            assert!(b.size().width >= 32.0 - 0.5 && b.size().height >= 32.0 - 0.5, "{name} is a 32px hit box");
        }
    }

    fn actions_hide_when_several_rows_are_selected() {
        let (ui, _log) = start();
        select_row(&ui, 1);
        let rows = ui.get_rows();
        let mut other = rows.row_data(2).unwrap();
        other.selected = true;
        rows.set_row_data(2, other);
        ui.set_selected_count(2);
        settle();
        let row = row_titled(&ui, "Invoice reminder");
        assert!(button_in(&ui, &row, "Edit").is_none(), "an action on one row would act on all of them");
    }

    fn search_status_counts() {
        let (ui, _log) = start();
        let status = |ui: &MainWindow| -> Vec<String> {
            by_role(ui, AccessibleRole::Text)
                .into_iter()
                .filter_map(|e| e.accessible_label().map(|l| l.to_string()))
                .filter(|l| l.contains("match") || l.ends_with(" items") || l.ends_with(" item"))
                .collect()
        };
        fixture::set_rows(&ui, fixture::search_rows());
        ui.set_query("re".into());
        settle();
        assert!(status(&ui).contains(&"4 matches in Work".to_string()), "{:?}", status(&ui));
        fixture::set_rows(&ui, fixture::search_rows().into_iter().take(1).collect());
        settle();
        assert!(status(&ui).contains(&"1 match in Work".to_string()), "{:?}", status(&ui));
        fixture::set_rows(&ui, Vec::new());
        settle();
        assert!(status(&ui).contains(&"No matches in Work".to_string()), "{:?}", status(&ui));
        ui.set_query("".into());
        fixture::set_rows(&ui, fixture::sample_rows());
        settle();
        assert!(status(&ui).contains(&"8 items".to_string()), "{:?}", status(&ui));
    }

    // ---- Colour labels ---------------------------------------------------------------------------

    fn tile_height_fits_content() {
        let (ui, _log) = start();
        ui.set_view(MainView::Tile);
        settle();
        // The sidebar's entries are list items too (28 tall); the tiles are the rest.
        let tiles: Vec<_> = by_role(&ui, AccessibleRole::ListItem)
            .into_iter()
            .filter(|t| t.size().height > 40.0)
            .collect();
        assert!(tiles.len() >= 6, "the fixture fills several tiles: {}", tiles.len());
        let heights: Vec<f32> = tiles.iter().map(|t| t.size().height).collect();
        assert!(heights.iter().all(|h| *h >= 96.0 - 0.5), "min 96: {heights:?}");
        assert!(
            heights.iter().any(|h| (h - 96.0).abs() < 0.5),
            "a short tile is 96 tall, not the tallest possible tile: {heights:?}"
        );
        // Tiles of one grid row share its height (an even row).
        for a in &tiles {
            for b in &tiles {
                if (a.absolute_position().y - b.absolute_position().y).abs() < 0.5 {
                    assert!((a.size().height - b.size().height).abs() < 0.5, "an even grid row");
                }
            }
        }
        // Rows are 6 apart and nothing else (no empty half-card) lies between them.
        let mut ys: Vec<(f32, f32)> = tiles.iter().map(|t| (t.absolute_position().y, t.size().height)).collect();
        ys.sort_by(|a, b| a.0.total_cmp(&b.0));
        ys.dedup_by(|a, b| (a.0 - b.0).abs() < 0.5);
        for w in ys.windows(2) {
            let gap = w[1].0 - (w[0].0 + w[0].1);
            assert!((gap - 6.0).abs() < 0.5, "rows are 6 apart: {gap}");
        }
    }

    fn tile_glyph_inline_with_title() {
        let (ui, _log) = start();
        ui.set_view(MainView::Tile);
        settle();
        let tile = row_titled(&ui, "Invoice reminder");
        let heading_text = ElementQuery::from_root(&ui)
            .match_id("ItemCard::title-text")
            .find_all()
            .into_iter()
            .find(|t| {
                let (p, o, s) = (t.absolute_position(), tile.absolute_position(), tile.size());
                p.x >= o.x && p.x <= o.x + s.width && p.y >= o.y && p.y <= o.y + s.height
            })
            .expect("the tile's title");
        assert!(
            (heading_text.absolute_position().x - tile.absolute_position().x - 40.0).abs() < 0.5,
            "the 24px glyph tile sits left of the title: the title starts at x 40"
        );
        assert!(
            heading_text.absolute_position().y - tile.absolute_position().y < 16.0,
            "the title is on the glyph's line, not under it"
        );
    }

    fn empty_all_offers_clipboard_new_import() {
        let (ui, log) = start();
        fixture::set_rows(&ui, Vec::new());
        ui.set_section(MainSection::All);
        settle();
        find(&ui, "No snippets yet", AccessibleRole::Text);
        find(&ui, "Save what you paste often.", AccessibleRole::Text);
        take(&log);
        find(&ui, "Save from clipboard", AccessibleRole::Button).invoke_accessible_default_action();
        find(&ui, "New item", AccessibleRole::Button).invoke_accessible_default_action();
        find(&ui, "Import…", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert_eq!(take(&log), ["add-from-clipboard", "new", "import"]);
        // A list that is empty keeps its own words and one New item button.
        ui.set_section(MainSection::List);
        settle();
        assert!(missing(&ui, "Import…", AccessibleRole::Button), "only the whole deck offers an import");
        find(&ui, "This list is empty", AccessibleRole::Text);
    }

    fn palette_no_match_empty_state() {
        let (ui, _log) = start();
        chord(ui.window(), &[Key::Control], "k");
        palette_rows(&ui, &SAMPLE);
        type_text(ui.window(), "zzq");
        palette_rows(&ui, &[]);
        // The shared empty state: the query is named, the footer is the one live region, and no stray list row is left.
        find(&ui, "No commands match \u{201c}zzq\u{201d}", AccessibleRole::Text);
        assert_eq!(live_status(&ui), "0 found");
        assert!(by_role(&ui, AccessibleRole::ListItem).iter().all(|e| e.accessible_label().is_none_or(|l| !l.contains("New item") || l.contains("zzq"))));
        assert!(missing(&ui, "No matching command", AccessibleRole::Text));
    }

    fn toolbar_commands_and_view_are_labelled() {
        let (ui, _log) = start();
        let texts = |ui: &MainWindow| -> Vec<String> {
            ElementQuery::from_root(ui)
                .match_id("ToolbarButton::label-text")
                .find_all()
                .into_iter()
                .filter_map(|e| e.accessible_label().map(|l| l.to_string()))
                .collect()
        };
        assert_eq!(texts(&ui), ["Commands", "View"], "ghost buttons with a label at 808 px");
        // Commands shows its key: two caps, Ctrl and K.
        let commands = find(&ui, "Commands (Ctrl+K)", AccessibleRole::Button);
        assert!(commands.size().height >= 32.0 - 0.5 && commands.size().height <= 32.0 + 0.5, "h32: {}", commands.size().height);
        assert!(commands.size().width > 100.0, "label and keycaps: {}", commands.size().width);
        let view = find(&ui, "View", AccessibleRole::Button);
        assert!(view.size().width > 32.0, "label and caret: {}", view.size().width);
    }

    fn the_toolbar_sheds_keys_then_labels_as_the_item_area_narrows() {
        let (ui, log) = start();
        // 232 sidebar + the item area; every step is one pixel either side of a threshold.
        let at = |area: f32| {
            ui.window().set_size(slint::LogicalSize::new(232.0 + area, 640.0));
            settle();
        };
        let label_count = || ElementQuery::from_root(&ui).match_id("ToolbarButton::label-text").find_all().len();
        let width = |name: &str| find(&ui, name, AccessibleRole::Button).size().width;
        at(736.0);
        assert_eq!(label_count(), 2, "both labels at 736");
        let with_keys = width("Commands (Ctrl+K)");
        at(735.0);
        assert!(width("Commands (Ctrl+K)") < with_keys - 40.0, "the Ctrl K chips go below 736");
        assert_eq!(label_count(), 2, "the labels stay");
        at(658.0);
        assert_eq!(label_count(), 2, "both labels at 658");
        at(657.0);
        assert_eq!(label_count(), 1, "the Commands label goes below 658");
        assert!((width("Commands (Ctrl+K)") - 32.0).abs() < 0.5, "Commands is a 32 px icon button");
        at(608.0);
        assert_eq!(label_count(), 1, "the View label stays at 608");
        at(607.0);
        assert_eq!(label_count(), 0, "the View label goes below 608");
        for name in ["Commands (Ctrl+K)", "View"] {
            let b = find(&ui, name, AccessibleRole::Button);
            assert!((b.size().width - 32.0).abs() < 0.5 && (b.size().height - 32.0).abs() < 0.5, "{name} is a 32 px icon button: {:?}", b.size());
        }
        take(&log);
        find(&ui, "Commands (Ctrl+K)", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert!(ui.get_palette_open(), "still opens the palette");
        press_key(&ui, Key::Escape);
        settle();
        at(736.0);
        assert_eq!(label_count(), 2, "the labels return at 736");
    }

    fn narrow_toolbar_view_menu_holds_the_tile_view() {
        let (ui, log) = start();
        ui.window().set_size(slint::LogicalSize::new(232.0 + 573.0, 640.0));
        settle();
        take(&log);
        find(&ui, "View", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        find(&ui, "Tile view", AccessibleRole::ListItem).invoke_accessible_default_action();
        settle();
        assert_eq!(take(&log), ["view:Tile"], "the View menu stands in for the dropped toggles");
    }

    fn a_colour_label_is_named_in_the_cards_description_and_never_colour_alone() {
        let (ui, _log) = start();
        let mut plain = fixture::row(snipdeck_ui::ItemKind::Text, "Plain", "");
        plain.label = 0;
        // A label index without its name draws and says nothing.
        let mut labelled = fixture::row(snipdeck_ui::ItemKind::Text, "Tagged", "");
        labelled.label = 1;
        labelled.label_name = "Teal".into();
        let mut mixed = fixture::row(snipdeck_ui::ItemKind::Text, "In a list", "");
        mixed.label = 7;
        mixed.label_name = "Slate".into();
        mixed.list_name = "Work".into();
        ui.set_rows(slint::ModelRc::new(VecModel::from(vec![plain, labelled, mixed])));
        settle();
        let description = |title: &str| find(&ui, title, AccessibleRole::ListItem).accessible_description().map(|d| d.to_string());
        assert_eq!(description("Plain").as_deref(), Some(""));
        assert_eq!(description("Tagged").as_deref(), Some("label: Teal"));
        assert_eq!(description("In a list").as_deref(), Some("Work, label: Slate"));
    }

    fn the_label_filter_row_names_each_colour_and_a_chosen_chip_clears_on_a_second_choice() {
        use snipdeck_ui::LabelChipEntry;
        let (ui, _log) = start();
        let asked: Rc<RefCell<Vec<i32>>> = Rc::default();
        {
            let asked = asked.clone();
            ui.on_label_filter_requested(move |i| asked.borrow_mut().push(i));
        }
        assert!(missing(&ui, "Teal", AccessibleRole::Button), "no labels, no row");
        ui.set_label_chips(slint::ModelRc::new(VecModel::from(vec![
            LabelChipEntry { label: 1, name: "Teal".into(), count: 2 },
            LabelChipEntry { label: 3, name: "Amber".into(), count: 1 },
        ])));
        settle();
        for name in ["Teal", "Amber"] {
            let chip = find(&ui, name, AccessibleRole::Button);
            let s = chip.size();
            assert!(s.height >= MIN_HIT && s.width >= MIN_HIT, "{name}");
            assert_eq!(chip.accessible_checked(), Some(false));
        }
        assert_eq!(find(&ui, "Teal", AccessibleRole::Button).accessible_value().as_deref(), Some("2"));
        assert!(missing(&ui, "Clear filter", AccessibleRole::Button), "nothing to clear yet");

        find(&ui, "Teal", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(*asked.borrow(), [1]);
        // The host applies it.
        ui.set_label_filter(1);
        settle();
        assert_eq!(find(&ui, "Teal", AccessibleRole::Button).accessible_checked(), Some(true));
        assert_eq!(find(&ui, "Amber", AccessibleRole::Button).accessible_checked(), Some(false));
        find(&ui, "Teal", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(*asked.borrow(), [1, -1], "choosing the active chip again clears");
        find(&ui, "Clear filter", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(*asked.borrow(), [1, -1, -1]);
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
        for label in ["Move to list…", "Tag", "Pin", "Move to Trash", "Clear selection"] {
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
        assert!(missing(&ui, "Move to list…", AccessibleRole::Button));
        let _ = find(&ui, "Restore", AccessibleRole::Button);
        find(&ui, "Delete for good…", AccessibleRole::Button).invoke_accessible_default_action();
        assert!(ui.get_confirm_open());
    }

    fn the_trash_context_menu_says_delete_for_good_like_the_button_and_the_dialog() {
        let (ui, _log) = start();
        ui.set_section(MainSection::Trash);
        settle();
        press_key(&ui, Key::Menu);
        assert!(ui.get_menu_open());
        let _ = find(&ui, "Delete for good…", AccessibleRole::ListItem);
        assert!(missing(&ui, "Delete forever…", AccessibleRole::ListItem));
    }

    fn the_status_line_is_a_polite_live_region() {
        use i_slint_backend_testing::AccessibleLiveness;
        let (ui, _log) = start();
        ui.set_selected_count(2);
        settle();
        let status = find(&ui, "2 selected", AccessibleRole::Text);
        assert_eq!(status.accessible_live_region(), Some(AccessibleLiveness::Polite));
    }

    // ---- Sidebar -----------------------------------------------------------------------------------

    fn the_sidebar_is_one_tab_stop_with_arrows_home_end_and_enter() {
        let (ui, log) = start();
        // Back through the toolbar's controls (one stop each) to the sidebar.
        for _ in 0..16 {
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
        for _ in 0..16 {
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

        // UX-20: a list goes to the Trash, so Delete acts at once and asks nothing.
        press_key(&ui, Key::Menu);
        find(&ui, "Delete list", AccessibleRole::ListItem).invoke_accessible_default_action();
        settle();
        assert!(!ui.get_confirm_open(), "no confirmation dialog");
        assert_eq!(take(&log), ["list-delete:0"]);
    }

    fn a_context_menu_is_as_wide_as_its_rows_not_the_window() {
        let (ui, _log) = start();
        for _ in 0..16 {
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

        // While the Panel is showing, the button offers to hide it.
        ui.set_panel_visible(true);
        settle();
        find(&ui, "Hide Panel (Ctrl+Alt+Space)", AccessibleRole::Button)
            .invoke_accessible_default_action();
        assert_eq!(take(&log), ["panel"]);
        ui.set_panel_visible(false);
        settle();

        find(&ui, "Help", AccessibleRole::Button).invoke_accessible_default_action();
        find(&ui, "Show Panel", AccessibleRole::ListItem).invoke_accessible_default_action();
        assert_eq!(take(&log), ["panel"]);
    }

    fn a_tag_and_the_trash_have_their_own_confirmations() {
        let (ui, log) = start();
        for _ in 0..16 {
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

    fn the_search_clear_button_is_centred_smaller_than_the_box_and_inset_like_the_magnifier() {
        let (ui, _log) = start();
        chord(ui.window(), &[Key::Control], "f");
        type_text(ui.window(), "inv");
        settle();
        let field = find(&ui, "Search snippets", AccessibleRole::TextInput);
        let clear = find(&ui, "Clear search", AccessibleRole::Button);
        let (fp, fz) = (field.absolute_position(), field.size());
        let (cp, cz) = (clear.absolute_position(), clear.size());
        let (fc, cc) = (fp.y + fz.height / 2.0, cp.y + cz.height / 2.0);
        assert!((fc - cc).abs() <= 1.0, "the clear button is centred in the box: {cc} vs {fc}");
        assert!(cz.height <= fz.height - 4.0, "clear of the border: {} in {}", cz.height, fz.height);
        let inset = (fp.x + fz.width) - (cp.x + cz.width);
        // The glyph is centred in its button, so the button's inset plus its padding mirrors the
        // magnifier's 8px inset on the left.
        let glyph_inset = inset + (cz.width - 16.0 * 0.75) / 2.0;
        assert!((glyph_inset - 10.0).abs() <= 2.0, "right inset mirrors the magnifier: {glyph_inset}");
    }

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

    fn the_help_button_opens_a_menu_of_four_pages_and_f1_asks_for_the_guide() {
        let (ui, log) = start();
        for (label, topic) in [
            ("Troubleshoot…", "Troubleshoot"),
            ("User guide", "Guide"),
            ("About Snipdeck", "About"),
            ("Keyboard shortcuts", "Shortcuts"),
        ] {
            find(&ui, "Help", AccessibleRole::Button).invoke_accessible_default_action();
            settle();
            assert!(ui.get_menu_open(), "the Help menu is open");
            find(&ui, label, AccessibleRole::ListItem).invoke_accessible_default_action();
            settle();
            assert!(!ui.get_menu_open(), "choosing a page closes the menu");
            assert_eq!(take(&log), [format!("help:{topic}")]);
        }
        press_key(&ui, Key::F1);
        assert_eq!(take(&log), ["help:Guide"]);
    }

    fn the_view_button_opens_a_theme_menu_whose_rows_report_their_index_and_esc_closes_it() {
        let (ui, log) = start();
        for (label, index) in [("Theme: System", 0), ("Theme: Light", 1), ("Theme: Dark", 2)] {
            find(&ui, "View", AccessibleRole::Button).invoke_accessible_default_action();
            settle();
            assert!(ui.get_menu_open(), "the View menu is open");
            find(&ui, label, AccessibleRole::ListItem).invoke_accessible_default_action();
            settle();
            assert!(!ui.get_menu_open(), "choosing a theme closes the menu");
            assert_eq!(take(&log), [format!("theme:{index}")]);
        }
        find(&ui, "View", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        press_key(&ui, Key::Escape);
        assert!(!ui.get_menu_open(), "Esc closes the menu");
        assert!(take(&log).is_empty(), "and chooses nothing");
        // The row of the theme in force has the keyboard, so Enter keeps it.
        ui.set_theme_index(2);
        find(&ui, "View", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        press_key(&ui, Key::Return);
        assert_eq!(take(&log), ["theme:2"]);
    }

    fn the_view_menu_has_the_two_row_densities_like_the_toolbar_buttons() {
        let (ui, log) = start();
        assert_eq!(ui.get_view(), MainView::Comfortable);
        for (label, view, logged) in [
            ("Main list density: Compact", MainView::Compact, "view:Compact"),
            ("Main list density: Comfortable", MainView::Comfortable, "view:Comfortable"),
        ] {
            find(&ui, "View", AccessibleRole::Button).invoke_accessible_default_action();
            settle();
            find(&ui, label, AccessibleRole::ListItem).invoke_accessible_default_action();
            settle();
            assert!(!ui.get_menu_open(), "choosing a density closes the menu");
            assert_eq!(ui.get_view(), view);
            assert_eq!(take(&log), [logged]);
        }
    }

    fn ctrl_n_in_an_open_list_focuses_the_quick_add_box_and_saves_with_enter() {
        let (ui, log) = start();
        assert!(missing(&ui, "New item in All items", AccessibleRole::Button), "no box outside a list");
        ui.set_section(MainSection::List);
        ui.set_section_index(0);
        settle();
        find(&ui, "New item in Email replies", AccessibleRole::Button);
        take(&log);
        chord(ui.window(), &[Key::Control], "n");
        settle();
        assert!(take(&log).is_empty(), "the editor does not open");
        type_text(ui.window(), "a/b");
        take(&log);
        press(ui.window(), &key(Key::Return));
        assert_eq!(take(&log), ["quick-add:a/b:0:false"], "typing a / does not jump to the search");
        settle();
        find(&ui, "New item in Email replies", AccessibleRole::Button);
        assert!(ui.get_items_focused(), "creating folds the box and the focus goes back to the items");
        chord(ui.window(), &[Key::Control], "n");
        settle();
        find(&ui, "More options: open the full editor", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["quick-add-full:"]);
        press(ui.window(), &key(Key::Escape));
        settle();
        assert!(!missing(&ui, "New item in Email replies", AccessibleRole::Button), "Esc folds the box back");
        assert!(ui.get_items_focused(), "and the focus goes back to the items");
        ui.set_section(MainSection::Pinned);
        settle();
        chord(ui.window(), &[Key::Control], "n");
        assert_eq!(take(&log), ["new"], "a system view opens the editor as before");
    }

    fn an_open_quick_add_box_that_goes_away_stops_owning_the_typing_keys() {
        let (ui, log) = start();
        ui.set_section(MainSection::List);
        ui.set_section_index(0);
        settle();
        chord(ui.window(), &[Key::Control], "n");
        settle();
        chord(ui.window(), &[Key::Control], "z");
        assert!(!take(&log).contains(&"undo".to_string()), "the box keeps its own typing keys");
        ui.set_section(MainSection::Pinned);
        settle();
        take(&log);
        chord(ui.window(), &[Key::Control], "z");
        assert_eq!(take(&log), ["undo"], "Ctrl+Z undoes again once the box is gone");
        // Coming back to the list shows the folded row, not a stale open box.
        ui.set_section(MainSection::List);
        settle();
        find(&ui, "New item in Email replies", AccessibleRole::Button);
    }

    fn an_empty_list_shows_no_collapsed_row_and_ctrl_n_opens_the_box() {
        let (ui, log) = start();
        fixture::set_rows(&ui, Vec::new());
        ui.set_section(MainSection::List);
        ui.set_section_index(0);
        settle();
        assert!(missing(&ui, "New item in Email replies", AccessibleRole::Button), "no third New item");
        take(&log);
        chord(ui.window(), &[Key::Control], "n");
        settle();
        assert!(take(&log).is_empty(), "the editor does not open");
        type_text(ui.window(), "hi");
        press(ui.window(), &key(Key::Return));
        assert_eq!(take(&log), ["quick-add:hi:0:false"], "the box is open and focused");
        press(ui.window(), &key(Key::Escape));
        settle();
        assert!(missing(&ui, "Create item", AccessibleRole::Button), "Esc closes it again");
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

    fn ctrl_shift_n_adds_a_list_ctrl_w_closes_and_ctrl_comma_opens_settings() {
        let (ui, log) = start();
        chord(ui.window(), &[Key::Control, Key::Shift], "N");
        assert_eq!(take(&log), ["add-list"], "not a new item");
        chord(ui.window(), &[Key::Control], "w");
        assert_eq!(take(&log), ["close"]);
        chord(ui.window(), &[Key::Control], ",");
        assert_eq!(take(&log), ["settings"]);
    }

    fn ctrl_z_undoes_unless_the_search_field_is_being_typed_in() {
        let (ui, log) = start();
        ui.invoke_focus_items();
        settle();
        take(&log);
        chord(ui.window(), &[Key::Control], "z");
        assert_eq!(take(&log), ["undo"]);
        chord(ui.window(), &[Key::Control], "f");
        take(&log);
        chord(ui.window(), &[Key::Control], "z");
        assert_eq!(take(&log), Vec::<String>::new(), "the field undoes its own typing");
    }

    fn the_board_button_names_the_board_and_asks_for_the_menu() {
        let (ui, _log) = start();
        let board = find(&ui, "Board: Work", AccessibleRole::Button);
        assert!(board.size().height >= MIN_HIT);
    }

    // ---- Sidebar width, collapse and the list sort ---------------------------------------------------

    fn ctrl_b_toggles_sidebar() {
        let (ui, log) = start();
        assert!(!ui.get_rail());
        chord(ui.window(), &[Key::Control], "b");
        settle();
        assert!(ui.get_rail(), "folded to the rail");
        assert_eq!(take(&log), ["sidebar-collapsed:true"]);
        assert!(missing(&ui, "Sidebar width", AccessibleRole::Slider), "no splitter on the rail");
        chord(ui.window(), &[Key::Control], "b");
        settle();
        assert!(!ui.get_rail());
        assert_eq!(take(&log), ["sidebar-collapsed:false"]);
        // The toolbar button does the same and says the chord.
        find(&ui, "Collapse sidebar (Ctrl+B)", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert!(ui.get_rail());
        let _ = find(&ui, "Expand sidebar (Ctrl+B)", AccessibleRole::Button);
        take(&log);
        // Below 720 px the rail is automatic and Ctrl+B has nothing to fold.
        ui.set_sidebar_collapsed(false);
        ui.window().set_size(slint::LogicalSize::new(640.0, 520.0));
        settle();
        chord(ui.window(), &[Key::Control], "b");
        assert_eq!(take(&log), Vec::<String>::new());
    }

    fn splitter_keys_step_8px() {
        let (ui, log) = start();
        let splitter = find(&ui, "Sidebar width", AccessibleRole::Slider);
        assert_eq!(splitter.accessible_value().as_deref(), Some("232"));
        assert!((splitter.size().width - 8.0).abs() < 0.5, "8 px hit area: {:?}", splitter.size());
        press_at_centre(&ui, centre_of(&splitter));
        take(&log);
        press_key(&ui, Key::RightArrow);
        assert_eq!(take(&log), ["sidebar-width:240"]);
        assert_eq!(ui.get_sidebar_width(), 240);
        let moved = find(&ui, "Sidebar width", AccessibleRole::Slider);
        assert!((moved.absolute_position().x - (240.0 - 4.0)).abs() < 0.5, "{:?}", moved.absolute_position());
        press_key(&ui, Key::LeftArrow);
        press_key(&ui, Key::LeftArrow);
        assert_eq!(take(&log), ["sidebar-width:232", "sidebar-width:224"]);
        for _ in 0..10 {
            press_key(&ui, Key::LeftArrow);
        }
        assert_eq!(ui.get_sidebar_width(), 200, "stops at 200");
        for _ in 0..20 {
            press_key(&ui, Key::RightArrow);
        }
        assert_eq!(ui.get_sidebar_width(), 320, "stops at 320");
    }

    fn ctrl_b_with_the_splitter_focused_moves_focus_to_the_list() {
        let (ui, log) = start();
        let splitter = find(&ui, "Sidebar width", AccessibleRole::Slider);
        // The strip straddles the sidebar's edge: 4 px each side of x = 232.
        let p = splitter.absolute_position();
        assert!((p.x - 228.0).abs() < 0.5 && (splitter.size().width - 8.0).abs() < 0.5, "{p:?}");
        press_at_centre(&ui, centre_of(&splitter));
        assert!(!ui.get_items_focused());
        take(&log);
        chord(ui.window(), &[Key::Control], "b");
        settle();
        assert_eq!(take(&log), ["sidebar-collapsed:true"]);
        assert!(ui.get_items_focused(), "the keys go to the list, not nowhere");
    }

    fn the_sort_menu_names_the_order_and_reports_the_choice() {
        let (ui, log) = start();
        assert!(missing(&ui, "Sort by: Manual", AccessibleRole::Button), "only inside a list");
        ui.set_section(MainSection::List);
        ui.set_section_index(0);
        settle();
        let button = find(&ui, "Sort by: Manual", AccessibleRole::Button);
        assert!(button.size().height >= MIN_HIT);
        ui.set_list_sort(1);
        settle();
        let _ = find(&ui, "Sort by: Most used", AccessibleRole::Button);
        ui.set_list_sort(0);
        settle();
        take(&log);
        find(&ui, "Sort by: Manual", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        let _ = find(&ui, "Sort by", AccessibleRole::List);
        press_key(&ui, Key::DownArrow);
        press_key(&ui, Key::Return);
        assert_eq!(take(&log), ["sort:1"]);
    }

    fn the_sidebar_rail_has_a_settings_button_when_collapsed() {
        let (ui, _log) = start();
        ui.set_sidebar_collapsed(true);
        settle();
        assert!(ui.get_rail());
        let _ = find(&ui, "Settings", AccessibleRole::Button);
    }

    fn a_toolbar_tooltip_goes_when_the_palette_opens_and_esc_closes_one_first() {
        let (ui, log) = start();
        let tips = |ui: &MainWindow| ElementQuery::from_root(ui).match_inherits("Tooltip").find_all().len();
        let rest_on = |ui: &MainWindow, label: &str| {
            let button = find(ui, label, AccessibleRole::Button);
            ui.window().dispatch_event(WindowEvent::PointerMoved { position: centre_of(&button) });
            mock_elapsed_time(std::time::Duration::from_millis(600));
            settle();
        };
        rest_on(&ui, "Compact view");
        assert_eq!(tips(&ui), 1, "the view button's tip is up after a rest");
        chord(ui.window(), &[Key::Control], "k");
        settle();
        assert!(ui.get_palette_open());
        assert_eq!(tips(&ui), 0, "no tip above the palette's scrim");
        press_key(&ui, Key::Escape);
        settle();
        assert!(!ui.get_palette_open());
        mock_elapsed_time(std::time::Duration::from_millis(600));
        settle();
        assert_eq!(tips(&ui), 0, "it does not come back when the palette closes");

        rest_on(&ui, "Comfortable view");
        assert_eq!(tips(&ui), 1);
        take(&log);
        press_key(&ui, Key::Escape);
        settle();
        assert_eq!(tips(&ui), 0, "Esc closes the tip");
        assert_eq!(take(&log), Vec::<String>::new(), "and does nothing else on that press");
        mock_elapsed_time(std::time::Duration::from_millis(600));
        settle();
        assert_eq!(tips(&ui), 0, "it stays closed while the pointer rests there");
        // With no tip up, Esc does its usual work again.
        ui.set_selected_count(2);
        settle();
        press_key(&ui, Key::Escape);
        assert_eq!(take(&log), ["clear"], "Esc with no tip clears the selection");
    }

    fn escape_during_a_search_composition_goes_to_the_composition_not_the_tooltip() {
        let (ui, log) = start();
        let tips = |ui: &MainWindow| ElementQuery::from_root(ui).match_inherits("Tooltip").find_all().len();
        find(&ui, "Search snippets", AccessibleRole::TextInput).mock_single_click(slint::platform::PointerEventButton::Left);
        settle();
        let button = find(&ui, "Compact view", AccessibleRole::Button);
        ui.window().dispatch_event(WindowEvent::PointerMoved { position: centre_of(&button) });
        mock_elapsed_time(std::time::Duration::from_millis(600));
        settle();
        assert_eq!(tips(&ui), 1, "the view button's tip is up");
        compose(ui.window(), "nihon");
        settle();
        take(&log);
        press_key(&ui, Key::Escape);
        assert_eq!(tips(&ui), 1, "Esc belongs to the input method while composing: the tip stays");
        assert_eq!(take(&log), Vec::<String>::new());
    }

    fn a_card_tip_is_not_closed_by_the_card_the_pointer_left() {
        assert_eq!(copy_to_copy(1, 2), 1, "moving up: the new card's Copy keeps its tip");
        assert_eq!(copy_to_copy(2, 1), 1, "moving down: the new card's Copy keeps its tip");
    }

    fn a_tooltip_goes_with_its_control_when_the_window_widens_and_removes_it() {
        let (ui, _log) = start();
        let tips = |ui: &MainWindow| ElementQuery::from_root(ui).match_inherits("Tooltip").find_all().len();
        ui.window().set_size(slint::LogicalSize::new(640.0, 520.0));
        settle();
        let board = find(&ui, "Board: Work", AccessibleRole::Button);
        ui.window().dispatch_event(WindowEvent::PointerMoved { position: centre_of(&board) });
        mock_elapsed_time(std::time::Duration::from_millis(600));
        settle();
        assert_eq!(tips(&ui), 1, "the narrow board button's tip is up");
        // Wider: the icon button gives way to the full switcher while the pointer rests where it was.
        ui.window().set_size(slint::LogicalSize::new(1040.0, 640.0));
        settle();
        // Time passes as on a clock, so the layer's 100ms check ticks each step.
        for _ in 0..4 {
            mock_elapsed_time(std::time::Duration::from_millis(100));
            settle();
        }
        assert_eq!(tips(&ui), 0, "the tip does not outlive its button");
    }

    fn rail_tooltips_stay_inside_the_window() {
        let (ui, _log) = start();
        ui.set_sidebar_collapsed(true);
        settle();
        assert!(ui.get_rail());
        let win = ui.window().size().to_logical(ui.window().scale_factor());
        let mut seen = 0;
        for label in ["Settings", "Board: Work", "Expand sidebar (Ctrl+B)"] {
            let Some(button) = all(&ui, label, AccessibleRole::Button).into_iter().next() else {
                continue;
            };
            ui.window().dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: centre_of(&button),
            });
            i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(900));
            settle();
            let tips = ElementQuery::from_root(&ui).match_inherits("Tooltip").find_all();
            let Some(tip) = tips.last() else { continue };
            seen += 1;
            let (p, z) = (tip.absolute_position(), tip.size());
            assert!(p.x >= 8.0, "{label}: tooltip starts at x {}", p.x);
            assert!(p.x + z.width <= win.width - 8.0, "{label}: tooltip ends at {}", p.x + z.width);
            ui.window().dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(win.width - 2.0, win.height - 2.0),
            });
            settle();
        }
        assert!(seen >= 2, "tooltips measured: {seen}");
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

    fn ctrl_v_and_ctrl_c_on_the_item_list_ask_for_a_paste_and_a_copy_of_the_cursor_row() {
        let (ui, log) = start();
        ui.invoke_focus_items();
        settle();
        take(&log);
        chord(ui.window(), &[Key::Control], "v");
        assert_eq!(take(&log), ["paste-create"]);
        chord(ui.window(), &[Key::Control], "c");
        assert_eq!(take(&log), ["copy-selection:0"]);
    }

    fn the_toast_has_a_close_button() {
        let (ui, log) = start();
        ui.set_toast_text("Copied".into());
        settle();
        take(&log);
        find(&ui, "Dismiss", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["toast-dismissed"]);
    }

    fn the_toast_edit_button_shows_only_with_its_label() {
        let (ui, log) = start();
        ui.set_toast_text("Added to Replies".into());
        ui.set_toast_undo_label("Undo".into());
        settle();
        // The item rows have Edit buttons of their own: count them.
        let rows_only = all(&ui, "Edit", AccessibleRole::Button).len();
        ui.set_toast_edit_label("Edit".into());
        settle();
        let with_toast = all(&ui, "Edit", AccessibleRole::Button);
        assert_eq!(with_toast.len(), rows_only + 1, "the toast gained an Edit button");
        take(&log);
        for button in &with_toast {
            // Only the toast's button reports `toast-edit`.
            button.invoke_accessible_default_action();
        }
        assert!(take(&log).contains(&"toast-edit".to_string()));
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

// ---- Command palette and shortcut hints ------------------------------------------------------------

/// What the palette's polite live region says now.
fn live_status(ui: &MainWindow) -> String {
    let live: Vec<ElementHandle> = by_role(ui, AccessibleRole::Text)
        .into_iter()
        .filter(|e| e.accessible_live_region() == Some(AccessibleLiveness::Polite))
        // Not the status line along the window's bottom edge, which is a live region of its own.
        .filter(|e| {
            e.absolute_position().y + e.size().height
                < f32::from(u16::try_from(ui.window().size().height).unwrap_or(u16::MAX))
                    / ui.window().scale_factor()
                    - 1.0
        })
        .collect();
    assert_eq!(live.len(), 1, "one polite live region in the palette");
    live[0].accessible_label().expect("a label").to_string()
}

/// Sets the rows the host would send: `(id, label, keys)`.
fn palette_rows(ui: &MainWindow, rows: &[(i32, &str, &[&str])]) {
    let rows: Vec<PaletteRow> = rows
        .iter()
        .map(|(id, label, keys)| PaletteRow {
            id: *id,
            label: (*label).into(),
            keys: ModelRc::new(VecModel::from(
                keys.iter()
                    .map(|k| (*k).into())
                    .collect::<Vec<SharedString>>(),
            )),
            spoken: keys.join(" ").into(),
            ..Default::default()
        })
        .collect();
    ui.set_palette_rows(ModelRc::new(VecModel::from(rows)));
    settle();
}

const SAMPLE: [(i32, &str, &[&str]); 3] = [
    (0, "New item", &["Ctrl", "N"]),
    (2, "Search", &["Ctrl", "F"]),
    (9, "Keyboard shortcuts", &["Ctrl", "/"]),
];

ui_tests! {
    fn ctrl_k_ctrl_shift_p_and_the_toolbar_button_open_the_palette_and_ask_for_rows() {
        let (ui, log) = start();
        assert!(!ui.get_palette_open());
        chord(ui.window(), &[Key::Control], "k");
        settle();
        assert!(ui.get_palette_open());
        assert_eq!(take(&log), ["palette-opened"]);
        press_key(&ui, Key::Escape);
        assert!(!ui.get_palette_open());

        chord(ui.window(), &[Key::Control, Key::Shift], "P");
        settle();
        assert!(ui.get_palette_open(), "Ctrl+Shift+P opens it too");
        press_key(&ui, Key::Escape);
        take(&log);

        find(&ui, "Commands (Ctrl+K)", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert!(ui.get_palette_open(), "the toolbar button opens it");
        assert_eq!(take(&log), ["palette-opened"]);
    }

    fn the_palette_lists_commands_as_list_items_with_their_shortcuts_spoken() {
        let (ui, _log) = start();
        chord(ui.window(), &[Key::Control], "k");
        palette_rows(&ui, &SAMPLE);
        let list = by_role(&ui, AccessibleRole::List);
        assert!(!list.is_empty());
        let item = find(&ui, "New item", AccessibleRole::ListItem);
        assert_eq!(item.accessible_description().as_deref(), Some("Ctrl N"));
        assert_eq!(item.accessible_item_index(), Some(0));
        assert_eq!(item.accessible_item_count(), Some(3));
        assert_eq!(item.accessible_item_selected(), Some(true), "the first row is highlighted");
        assert_eq!(find(&ui, "Search", AccessibleRole::ListItem).accessible_item_selected(), Some(false));
        assert_eq!(live_status(&ui), "New item, 1 of 3", "the live footer names the highlighted row and its place");
    }

    fn typing_reports_the_query_and_the_hosts_rows_replace_the_list() {
        let (ui, log) = start();
        chord(ui.window(), &[Key::Control], "k");
        palette_rows(&ui, &SAMPLE);
        take(&log);
        type_text(ui.window(), "se");
        settle();
        assert_eq!(take(&log), ["palette-query:s", "palette-query:se"]);
        palette_rows(&ui, &[SAMPLE[1]]);
        assert!(missing(&ui, "New item", AccessibleRole::ListItem));
        find(&ui, "Search", AccessibleRole::ListItem);
        assert_eq!(live_status(&ui), "Search, 1 of 1");
        palette_rows(&ui, &[]);
        find(&ui, "No commands match \u{201c}se\u{201d}", AccessibleRole::Text);
        assert_eq!(live_status(&ui), "0 found");
    }

    fn a_long_palette_list_keeps_its_rows_clear_of_the_scrollbar_strip() {
        let (ui, _log) = start();
        chord(ui.window(), &[Key::Control], "k");
        let labels: Vec<String> = (0..24).map(|i| format!("Command number {i}")).collect();
        let many: Vec<(i32, &str, &[&str])> = labels
            .iter()
            .enumerate()
            .map(|(i, l)| (i32::try_from(i).unwrap(), l.as_str(), &["Ctrl", "K"][..]))
            .collect();
        let right = |e: &ElementHandle| e.absolute_position().x + e.size().width;
        let field = find(&ui, "Commands", AccessibleRole::TextInput);
        let strip = 24.0; // Space.hit-min

        // Few rows fit, no scrollbar: the rows use the full width the search field has.
        palette_rows(&ui, &SAMPLE);
        let row = find(&ui, "New item", AccessibleRole::ListItem);
        assert!(
            right(&row) >= right(&field) - 0.5,
            "no strip is reserved while nothing scrolls: row {} field {}",
            right(&row),
            right(&field)
        );

        // More rows than the list shows: the scrollbar owns the strip at the right edge, on the first row
        // and on the last (the highlight wraps to it with Up).
        palette_rows(&ui, &many);
        let first = find(&ui, "Command number 0", AccessibleRole::ListItem);
        assert!(
            right(&first) <= right(&field) - strip + 0.5,
            "the first row ends before the scrollbar strip: row {} field {}",
            right(&first),
            right(&field)
        );
        assert!(
            (first.absolute_position().x - field.absolute_position().x).abs() <= 0.5,
            "and it starts at the field's left edge, not centred in what is left"
        );
        press_key(&ui, Key::UpArrow);
        settle();
        let last = find(&ui, "Command number 23", AccessibleRole::ListItem);
        assert_eq!(last.accessible_item_selected(), Some(true), "Up wrapped to the last row");
        assert!(right(&last) <= right(&field) - strip + 0.5, "the last row too");
    }

    fn arrows_move_the_highlight_wrapping_and_enter_runs_the_command_and_gives_focus_back() {
        let (ui, log) = start();
        chord(ui.window(), &[Key::Control], "k");
        palette_rows(&ui, &SAMPLE);
        take(&log);
        press_key(&ui, Key::DownArrow);
        assert_eq!(find(&ui, "Search", AccessibleRole::ListItem).accessible_item_selected(), Some(true));
        assert_eq!(live_status(&ui), "Search, 2 of 3", "the live region follows the highlight");
        press_key(&ui, Key::UpArrow);
        press_key(&ui, Key::UpArrow);
        assert_eq!(
            find(&ui, "Keyboard shortcuts", AccessibleRole::ListItem).accessible_item_selected(),
            Some(true),
            "Up from the first row wraps to the last"
        );
        press_key(&ui, Key::Return);
        assert_eq!(take(&log), ["command:9"], "the id of the highlighted row, not its position");
        assert!(!ui.get_palette_open());
        assert!(ui.get_items_focused(), "the keyboard is back on the items");
    }

    fn clicking_a_row_runs_it_and_escape_closes_without_running() {
        let (ui, log) = start();
        chord(ui.window(), &[Key::Control], "k");
        palette_rows(&ui, &SAMPLE);
        take(&log);
        press_key(&ui, Key::Escape);
        assert!(!ui.get_palette_open());
        assert!(ui.get_items_focused(), "Escape returns the focus");
        assert_eq!(take(&log), Vec::<String>::new());

        chord(ui.window(), &[Key::Control], "k");
        palette_rows(&ui, &SAMPLE);
        take(&log);
        find(&ui, "Search", AccessibleRole::ListItem).invoke_accessible_default_action();
        settle();
        assert_eq!(take(&log), ["command:2"]);
        assert!(!ui.get_palette_open());
    }

    fn the_palette_swallows_the_windows_other_shortcuts_while_it_is_open() {
        let (ui, log) = start();
        chord(ui.window(), &[Key::Control], "k");
        palette_rows(&ui, &SAMPLE);
        take(&log);
        chord(ui.window(), &[Key::Control], "n");
        settle();
        assert!(!take(&log).contains(&"new".to_string()), "Ctrl+N does not add an item behind the palette");
    }

    fn ctrl_comma_and_ctrl_slash_open_settings_and_the_shortcuts_sheet() {
        let (ui, log) = start();
        chord(ui.window(), &[Key::Control], ",");
        assert_eq!(take(&log), ["settings"]);
        chord(ui.window(), &[Key::Control], "/");
        assert_eq!(take(&log), ["help:Shortcuts"]);
        assert!(!ui.get_search_focused(), "Ctrl+/ is not the bare / that opens the search");
    }

    fn the_help_menu_lists_the_shortcuts_with_their_keys() {
        let (ui, log) = start();
        find(&ui, "Help", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        let row = find(&ui, "Keyboard shortcuts", AccessibleRole::ListItem);
        assert_eq!(row.accessible_description().as_deref(), Some("Ctrl+/"));
        row.invoke_accessible_default_action();
        settle();
        assert_eq!(take(&log), ["help:Shortcuts"]);
    }

    fn an_empty_deck_names_the_key_beside_new_item() {
        let (ui, _log) = start();
        fixture::set_rows(&ui, Vec::new());
        settle();
        find(&ui, "Control N", AccessibleRole::Text);
        assert!(
            missing(&ui, "Press Ctrl+N to add a snippet, or Ctrl+K to see every command.", AccessibleRole::Text),
            "no text line under the buttons"
        );
    }

    fn the_recent_list_is_one_tab_stop_with_a_roving_active_row() {
        let (ui, log) = start();
        let hook = |name: &'static str| {
            let log = log.clone();
            move |i: i32| log.borrow_mut().push(format!("{name}:{i}"))
        };
        ui.on_recent_copied(hook("copy"));
        ui.on_recent_deleted(hook("delete"));
        {
            let log = log.clone();
            ui.on_recent_saved(move |r, t| log.borrow_mut().push(format!("save:{r}:{t}")));
        }
        let entry = |id: i32, text: &str| snipdeck_ui::RecentRow { id, preview: text.into(), app: "".into(), age_minutes: 1, kind: snipdeck_ui::ItemKind::Text, image: slint::Image::default() };
        ui.set_recent_on(true);
        ui.set_recent_rows(ModelRc::new(VecModel::from(vec![entry(10, "one"), entry(20, "two"), entry(30, "three")])));
        ui.set_section(MainSection::Recent);
        ui.invoke_focus_items();
        settle();
        take(&log);
        press_key(&ui, Key::Return);
        press_key(&ui, Key::DownArrow);
        press_key(&ui, Key::DownArrow);
        press_key(&ui, Key::DownArrow);
        press_key(&ui, Key::Return);
        press_key(&ui, Key::Home);
        press_key(&ui, Key::End);
        chord(ui.window(), &[Key::Control], &key(Key::Delete));
        press_key(&ui, Key::Delete);
        press_key(&ui, Key::Home);
        press_key(&ui, Key::RightArrow);
        assert_eq!(take(&log), ["copy:10", "copy:30", "delete:30"]);
        let chooser = ElementHandle::find_by_accessible_label(&ui, "Save to…").count();
        assert!(chooser > 0, "Right arrow opens the Save to… chooser");
    }

    fn clearing_the_recent_history_asks_first_and_the_locked_list_offers_the_unlock() {
        let (ui, log) = start();
        {
            let cleared = log.clone();
            ui.on_recent_cleared(move || cleared.borrow_mut().push("cleared".to_string()));
            let unlock = log.clone();
            ui.on_recent_unlock_requested(move || unlock.borrow_mut().push("unlock".to_string()));
        }
        let entry = |id: i32| snipdeck_ui::RecentRow { id, preview: "one".into(), app: "".into(), age_minutes: 1, kind: snipdeck_ui::ItemKind::Text, image: slint::Image::default() };
        ui.set_recent_on(true);
        ui.set_recent_rows(ModelRc::new(VecModel::from(vec![entry(1), entry(2)])));
        ui.set_section(MainSection::Recent);
        settle();
        take(&log);
        find(&ui, "Clear recent history", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert!(ui.get_confirm_open());
        assert_eq!(take(&log), Vec::<String>::new(), "one click clears nothing");
        find(&ui, "Clear history", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["cleared"]);

        ui.set_recent_rows(ModelRc::new(VecModel::from(Vec::<snipdeck_ui::RecentRow>::new())));
        ui.set_recent_locked(true);
        fixture::set_rows(&ui, Vec::new());
        settle();
        assert!(ElementHandle::find_by_accessible_label(&ui, "Recent copies are locked").next().is_some());
        find(&ui, "Unlock", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["unlock"]);
    }
}

// ---- Palette scrolling -------------------------------------------------------------------------------

/// Forty commands: more than the palette shows, so its list scrolls.
fn many_palette_rows(ui: &MainWindow) {
    let labels: Vec<String> = (0..40).map(|i| format!("Command {i}")).collect();
    let rows: Vec<(i32, &str, &[&str])> = labels
        .iter()
        .enumerate()
        .map(|(i, l)| (i32::try_from(i).unwrap_or(0), l.as_str(), &[][..]))
        .collect();
    palette_rows(ui, &rows);
}

/// How far the palette list is scrolled, in px: read from the first row still in view (a row
/// scrolled out of the viewport is not found), given where row 0 sat unscrolled and the row height.
fn scrolled(ui: &MainWindow, top0: f32, row_h: f32) -> f32 {
    (0..40u8)
        .find_map(|i| {
            all(ui, &format!("Command {i}"), AccessibleRole::ListItem)
                .first()
                .map(|e| top0 + row_h * f32::from(i) - e.absolute_position().y)
        })
        .expect("a row in view")
}

/// Card `selected` shows its actions without hover; the pointer rests on card `hovered`'s Copy, then moves
/// straight onto the selected card's Copy, so one move leaves one Copy and enters the other.
fn copy_to_copy(selected: usize, hovered: usize) -> usize {
    let (ui, _log) = start();
    let tips = |ui: &MainWindow| {
        ElementQuery::from_root(ui)
            .match_inherits("Tooltip")
            .find_all()
            .len()
    };
    select_row(&ui, selected);
    settle();
    let titles = ["Thanks, received", "Invoice reminder", "Retry helper"];
    let row = row_titled(&ui, titles[hovered]);
    ui.window().dispatch_event(WindowEvent::PointerMoved {
        position: centre_of(&row),
    });
    settle();
    let from = button_in(&ui, &row, "Copy").expect("the hovered card's Copy");
    ui.window().dispatch_event(WindowEvent::PointerMoved {
        position: centre_of(&from),
    });
    mock_elapsed_time(std::time::Duration::from_millis(600));
    settle();
    assert_eq!(tips(&ui), 1, "Copy names itself");
    let to = button_in(&ui, &row_titled(&ui, titles[selected]), "Copy")
        .expect("the selected card's Copy");
    ui.window().dispatch_event(WindowEvent::PointerMoved {
        position: centre_of(&to),
    });
    mock_elapsed_time(std::time::Duration::from_millis(100));
    settle();
    tips(&ui)
}

/// The palette open on forty commands at Windows text size `scale` (rows grow with it).
fn palette_at(scale: f32) -> MainWindow {
    let (ui, _log) = start();
    ui.global::<snipdeck_ui::SystemLook<'_>>()
        .set_text_scale(scale);
    settle();
    chord(ui.window(), &[Key::Control], "k");
    settle();
    many_palette_rows(&ui);
    ui
}

/// A wheel turn (one notch is 60px on the winit backend; a touchpad sends many small ones) over `at`,
/// then the pointer moves a hair, as a hand on a touchpad does, so the row under it is hovered.
fn wheel(ui: &MainWindow, at: LogicalPosition, delta_y: f32) {
    ui.window().dispatch_event(WindowEvent::PointerScrolled {
        position: at,
        delta_x: 0.0,
        delta_y,
    });
    mock_elapsed_time(std::time::Duration::from_millis(400));
    settle();
    ui.window().dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(at.x, at.y + 0.5),
    });
    mock_elapsed_time(std::time::Duration::from_millis(50));
    settle();
}

ui_tests! {
    fn the_palette_list_scrolls_with_the_wheel_and_stays_where_it_was_scrolled() {
        for scale in [1.0, 1.5] {
        let ui = palette_at(scale);
        let first = find(&ui, "Command 0", AccessibleRole::ListItem);
        let top0 = first.absolute_position().y;
        let row_h = first.size().height;
        let over = LogicalPosition::new(first.absolute_position().x + 40.0, top0 + row_h * 2.5);
        ui.window().dispatch_event(WindowEvent::PointerMoved { position: over });
        settle();

        // Far enough that the row under the pointer is past the first screenful: the old list re-pinned the
        // highlight (that row) to its bottom edge, so every notch from there on jumped back.
        for notch in 1..=8u16 {
            wheel(&ui, over, -60.0);
            let now = scrolled(&ui, top0, row_h);
            assert!(
                (now - 60.0 * f32::from(notch)).abs() < 1.0,
                "after {notch} notches the list is {} px down, got {now}",
                60 * notch
            );
        }
        wheel(&ui, over, 60.0);
        assert!(
            (scrolled(&ui, top0, row_h) - 420.0).abs() < 1.0,
            "a notch back up returns 60px"
        );
        }
    }

    fn many_small_touchpad_deltas_scroll_the_palette_as_far_as_their_sum() {
        for scale in [1.0, 1.5] {
        let ui = palette_at(scale);
        let first = find(&ui, "Command 0", AccessibleRole::ListItem);
        let top0 = first.absolute_position().y;
        let row_h = first.size().height;
        let over = LogicalPosition::new(first.absolute_position().x + 40.0, top0 + row_h * 2.5);
        ui.window().dispatch_event(WindowEvent::PointerMoved { position: over });
        settle();
        // A precision touchpad: WM_MOUSEWHEEL with deltas of a few units of 120, so a few px each.
        for _ in 0..120 {
            wheel(&ui, over, -4.0);
        }
        let moved = scrolled(&ui, top0, row_h);
        assert!((moved - 480.0).abs() < 1.0, "120 deltas of 4px scroll 480px, got {moved}");
        }
    }

    fn the_keyboard_still_brings_the_highlighted_row_into_view_after_a_wheel_scroll() {
        for scale in [1.0, 1.5] {
        let ui = palette_at(scale);
        let first = find(&ui, "Command 0", AccessibleRole::ListItem);
        let top0 = first.absolute_position().y;
        let row_h = first.size().height;
        let over = LogicalPosition::new(first.absolute_position().x + 40.0, top0 + row_h * 2.5);
        ui.window().dispatch_event(WindowEvent::PointerMoved { position: over });
        settle();
        wheel(&ui, over, -600.0);
        // Move the pointer off the list so it does not hover a row, then walk to the last command.
        ui.window().dispatch_event(WindowEvent::PointerExited);
        // The list wraps, so walk down until the footer says the last command is highlighted.
        let at_last = |ui: &MainWindow| {
            ElementHandle::find_by_accessible_label(ui, "Command 39, 40 of 40").next().is_some()
        };
        for _ in 0..40 {
            if at_last(&ui) {
                break;
            }
            press_key(&ui, Key::DownArrow);
            settle();
        }
        assert!(at_last(&ui), "the last command is highlighted");
        // The last row is in view and the list is at its end (a further turn down moves nothing), so the row
        // sits at the bottom edge, whatever the viewport's height at this text size.
        let _ = find(&ui, "Command 39", AccessibleRole::ListItem);
        let at_end = scrolled(&ui, top0, row_h);
        assert!(at_end > row_h, "at {scale}x the list scrolled to its end, got {at_end}");
        wheel(&ui, over, -600.0);
        assert!(
            (scrolled(&ui, top0, row_h) - at_end).abs() < 1.0,
            "at {scale}x the last row was already at the bottom: {at_end}"
        );
        ui.window().dispatch_event(WindowEvent::PointerExited);
        // The wheel's hover moved the highlight; walk up from there to the first command.
        let at_first = |ui: &MainWindow| {
            ElementHandle::find_by_accessible_label(ui, "Command 0, 1 of 40").next().is_some()
        };
        for _ in 0..40 {
            if at_first(&ui) {
                break;
            }
            press_key(&ui, Key::UpArrow);
        }
        assert!(at_first(&ui), "the first command is highlighted");
        assert!(
            scrolled(&ui, top0, row_h).abs() < 1.0,
            "walking back up brings the first row back to the top"
        );
        }
    }
}
