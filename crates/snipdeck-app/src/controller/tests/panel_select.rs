//! The Panel's multi-selection, the bulk bar, the item menu's Move / Colour / Sensitive rows,
//! the quick-add box and its toast. Time is the fixed clock of the world.

use super::*;
use crate::{item_editor::Origin, main_window::SelectMode};

fn marked(w: &World) -> Vec<usize> {
    w.ctl
        .view()
        .rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.marked)
        .map(|(i, _)| i)
        .collect()
}

#[test]
fn ctrl_click_marks_and_a_second_ctrl_click_unmarks() {
    let mut w = world();
    w.ctl.panel_mark(0, SelectMode::Toggle);
    w.ctl.panel_mark(2, SelectMode::Toggle);
    assert_eq!(marked(&w), [0, 2]);
    assert_eq!(w.ctl.view().marked_count, 2);
    w.ctl.panel_mark(0, SelectMode::Toggle);
    assert_eq!(marked(&w), [2]);
}

#[test]
fn shift_click_marks_the_range_from_the_last_click() {
    let mut w = world();
    w.ctl.panel_mark(1, SelectMode::Toggle);
    w.ctl.panel_mark(3, SelectMode::Extend);
    assert_eq!(marked(&w), [1, 2, 3]);
}

#[test]
fn ctrl_a_marks_every_row_and_escape_clears() {
    let mut w = world();
    w.ctl.panel_mark_all();
    assert_eq!(marked(&w), [0, 1, 2, 3]);
    w.ctl.panel_clear_marks();
    assert_eq!(marked(&w), Vec::<usize>::new());
    assert_eq!(w.ctl.view().marked_count, 0);
}

#[test]
fn shift_down_and_up_extend_and_shrink_from_the_cursor() {
    let mut w = world();
    w.ctl.panel_extend_marks(1);
    w.ctl.panel_extend_marks(1);
    assert_eq!(marked(&w), [0, 1, 2]);
    assert_eq!(w.ctl.view().selected, 2);
    w.ctl.panel_extend_marks(-1);
    assert_eq!(marked(&w), [0, 1]);
}

#[test]
fn switching_list_ends_the_selection() {
    let mut w = world();
    w.ctl.panel_mark_all();
    w.ctl.select_tab(1);
    assert_eq!(w.ctl.view().marked_count, 0);
}

#[test]
fn delete_removes_the_selection_with_an_undo_toast_and_no_confirmation() {
    let mut w = world();
    w.ctl.panel_mark(0, SelectMode::Toggle);
    w.ctl.panel_mark(1, SelectMode::Toggle);
    w.ctl.delete_row(3);
    let view = w.ctl.view();
    assert_eq!(view.rows.len(), 2, "both marked rows left the list");
    assert_eq!(view.marked_count, 0);
    let toast = view.toast.expect("toast");
    assert_eq!(toast.message, ToastMessage::ItemsTrashed { count: 2 });
    assert!(toast.undo);
    w.ctl.main_undo();
    assert_eq!(w.ctl.view().rows.len(), 4, "Undo brings both back");
}

#[test]
fn the_bulk_bar_deletes_the_marked_rows() {
    let mut w = world();
    w.ctl.panel_mark_all();
    w.ctl.bulk_delete();
    assert_eq!(w.ctl.view().rows.len(), 0);
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::ItemsTrashed { count: 4 }
    );
}

#[test]
fn the_bulk_bar_moves_the_marked_rows_to_a_chosen_list() {
    let mut w = world();
    w.ctl.panel_mark(0, SelectMode::Toggle);
    w.ctl.panel_mark(1, SelectMode::Toggle);
    w.ctl.bulk_open_move();
    let menu = w.ctl.view().choose_menu.expect("lists to choose from");
    assert_eq!(menu.entries.len(), 2);
    assert_eq!(menu.entries[0].label, ChooseLabel::List("Replies".into()));
    assert!(menu.entries[0].checked, "both are in Replies");
    w.ctl.choose_item_menu(1);
    assert!(w.ctl.view().choose_menu.is_none());
    assert_eq!(w.ctl.view().rows.len(), 2);
    w.ctl.select_tab(1);
    assert_eq!(w.ctl.view().rows.len(), 3);
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::ItemsMoved {
            count: 2,
            list: "Links".into()
        }
    );
}

#[test]
fn the_bulk_bar_colours_the_marked_rows_and_no_label_clears_it() {
    let mut w = world();
    w.ctl.panel_mark(0, SelectMode::Toggle);
    w.ctl.panel_mark(2, SelectMode::Toggle);
    w.ctl.bulk_open_colour();
    let menu = w.ctl.view().choose_menu.expect("colours");
    assert_eq!(menu.entries.len(), 9, "no label and eight colours");
    // Entry 6 is Red (no label, then blue teal green amber orange red).
    w.ctl.choose_item_menu(6);
    let rows = w.ctl.view().rows;
    assert_eq!(rows[0].label, Some(snipdeck_core::ItemLabel::Red));
    assert_eq!(rows[1].label, None);
    assert_eq!(rows[2].label, Some(snipdeck_core::ItemLabel::Red));
    assert_eq!(
        marked(&w),
        [0, 2],
        "the selection stays for the next action"
    );
    w.ctl.bulk_open_colour();
    w.ctl.choose_item_menu(0);
    assert!(w.ctl.view().rows.iter().all(|r| r.label.is_none()));
}

#[test]
fn copy_all_joins_the_marked_texts_in_row_order() {
    let mut w = world();
    w.ctl.panel_mark(2, SelectMode::Toggle);
    w.ctl.panel_mark(0, SelectMode::Toggle);
    w.ctl.services.clipboard.written.clear();
    w.ctl.bulk_copy();
    let written = &w.ctl.services.clipboard.written;
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].data(), [Data::Text("first\nthird".into())]);
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::Copied
    );
}

#[test]
fn the_item_menu_on_a_marked_row_acts_on_every_marked_row() {
    let mut w = world();
    w.ctl.panel_mark(0, SelectMode::Toggle);
    w.ctl.panel_mark(1, SelectMode::Toggle);
    w.ctl.open_item_menu(1, false);
    let entries = w.ctl.view().item_menu.expect("menu");
    let at = entries
        .iter()
        .position(|e| *e == ItemMenuEntry::Trash)
        .expect("a Delete row");
    w.ctl.choose_item_menu(at);
    assert_eq!(w.ctl.view().rows.len(), 2);
}

#[test]
fn the_item_menu_offers_move_to_colour_and_sensitive() {
    let mut w = world();
    w.ctl.open_item_menu(0, false);
    let entries = w.ctl.view().item_menu.expect("menu");
    assert!(entries.contains(&ItemMenuEntry::MoveTo));
    assert!(entries.contains(&ItemMenuEntry::Colour));
    assert!(entries.contains(&ItemMenuEntry::Sensitive {
        on: false,
        enabled: true
    }));
    let at = entries
        .iter()
        .position(|e| *e == ItemMenuEntry::MoveTo)
        .expect("row");
    w.ctl.choose_item_menu(at);
    assert!(
        w.ctl.view().item_menu.is_none(),
        "the second level took its place"
    );
    assert!(w.ctl.view().choose_menu.is_some());
    w.ctl.close_item_menu();
    assert!(w.ctl.view().choose_menu.is_none());
}

#[test]
fn show_numbers_is_a_menu_row_that_flips_the_setting() {
    let mut w = world();
    assert!(w.ctl.view().show_numbers);
    let at = APP_MENU
        .iter()
        .position(|e| *e == AppMenuEntry::ShowNumbers)
        .expect("row");
    w.ctl.open_app_menu();
    w.ctl.choose_app_menu(at);
    assert!(!w.ctl.view().show_numbers);
    assert!(!w.ctl.settings().show_numbers);
    w.ctl.open_app_menu();
    w.ctl.choose_app_menu(at);
    assert!(w.ctl.view().show_numbers);
}

// ---- Quick-add ---------------------------------------------------------------------------------

#[test]
fn quick_add_saves_text_at_the_end_of_the_shown_list_and_selects_it() {
    let mut w = world();
    assert!(
        w.ctl
            .quick_add("remember the milk\nand eggs", None, false, Origin::Panel)
    );
    let view = w.ctl.view();
    assert_eq!(view.rows.len(), 5);
    let row = &view.rows[4];
    assert_eq!(
        row.title, "remember the milk",
        "the title is the first line"
    );
    assert_eq!(row.kind, ItemKind::Text);
    assert_eq!(view.selected, 4);
    let toast = view.toast.expect("toast");
    assert_eq!(
        (toast.kind, toast.message),
        (
            ToastKind::Success,
            ToastMessage::Saved {
                list: "Replies".into()
            }
        )
    );
    assert!(toast.undo && toast.edit, "Undo and Edit");
}

#[test]
fn quick_add_makes_a_lone_address_a_link_and_a_placeholder_a_template() {
    let mut w = world();
    w.ctl
        .quick_add("  https://example.org/a  ", None, false, Origin::Panel);
    w.ctl
        .quick_add("Dear {field:Name}", None, false, Origin::Panel);
    w.ctl
        .quick_add("see https://example.org", None, false, Origin::Panel);
    let kinds: Vec<ItemKind> = w.ctl.view().rows[4..].iter().map(|r| r.kind).collect();
    assert_eq!(kinds, [ItemKind::Link, ItemKind::Template, ItemKind::Text]);
}

#[test]
fn quick_add_takes_a_kind_the_user_chose() {
    let mut w = world();
    w.ctl.quick_add(
        "https://example.org",
        Some(ItemKind::Text),
        false,
        Origin::Panel,
    );
    assert_eq!(w.ctl.view().rows[4].kind, ItemKind::Text);
}

#[test]
fn quick_add_with_blank_text_does_nothing() {
    let mut w = world();
    assert!(!w.ctl.quick_add("  \n\t ", None, false, Origin::Panel));
    assert_eq!(w.ctl.view().rows.len(), 4);
    assert!(w.ctl.view().toast.is_none());
}

#[test]
fn quick_add_goes_to_the_list_on_the_tab() {
    let mut w = world();
    w.ctl.select_tab(1);
    w.ctl
        .quick_add("https://example.net", None, false, Origin::Panel);
    assert_eq!(w.ctl.view().rows.len(), 2);
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::Saved {
            list: "Links".into()
        }
    );
}

#[test]
fn the_saved_toast_undo_moves_the_new_item_to_the_trash() {
    let mut w = world();
    w.ctl.quick_add("oops", None, false, Origin::Panel);
    assert_eq!(w.ctl.view().rows.len(), 5);
    w.ctl.toast_action();
    assert_eq!(w.ctl.view().rows.len(), 4);
}

#[test]
fn the_saved_toast_edit_opens_the_new_item_in_the_editor() {
    let mut w = world();
    w.ctl.quick_add("polish me", None, false, Origin::Panel);
    let id = w.ctl.view().rows[4].id;
    assert_eq!(w.ctl.toast_edit(), [Effect::EditItem(id)]);
    assert!(w.ctl.view().toast.is_none(), "the toast is gone");
    assert!(w.ctl.toast_edit().is_empty(), "once");
}

#[test]
fn ctrl_shift_n_opens_the_full_editor_prefilled_with_what_was_typed() {
    let mut w = world();
    w.ctl
        .open_new_item_with(Origin::Panel, "https://example.org\n");
    let view = w.ctl.item_editor_view().expect("editor");
    assert!(view.is_new);
    assert_eq!(view.fields.content, "https://example.org");
    assert_eq!(view.fields.kind, ItemKind::Link);
}

#[test]
fn more_options_is_remembered_in_the_settings() {
    let mut w = world();
    assert!(!w.ctl.settings().editor_more_open);
    w.ctl.set_editor_more_open(true);
    assert!(w.ctl.settings().editor_more_open);
    w.ctl.open_new_item(Origin::Panel);
    assert!(w.ctl.item_editor_view().expect("editor").more_open);
}

#[test]
fn reset_size_and_position_clears_the_stored_geometry_of_this_monitor_only() {
    use crate::frame::{Monitor, Rect};
    let mut w = world();
    let first = Monitor {
        id: Monitor::id_of("DISPLAY1", 1920, 1080),
        work_area: Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        },
        scale: 1.0,
        taskbar: None,
    };
    let second = Monitor {
        id: Monitor::id_of("DISPLAY2", 1280, 1024),
        work_area: Rect {
            x: 1920,
            y: 0,
            w: 1280,
            h: 984,
        },
        scale: 1.0,
        taskbar: None,
    };
    w.ctl.frame_changed(
        &second,
        Rect {
            x: 2000,
            y: 30,
            w: 450,
            h: 700,
        },
    );
    w.ctl.frame_changed(
        &first,
        Rect {
            x: 10,
            y: 20,
            w: 300,
            h: 500,
        },
    );
    let index = crate::controller::APP_MENU
        .iter()
        .position(|e| *e == crate::controller::AppMenuEntry::ResetGeometry)
        .expect("the kebab has the row");
    w.ctl.open_app_menu();
    let effects = w.ctl.choose_app_menu(index);
    assert!(matches!(effects.as_slice(), [Effect::PlaceWindow(_)]));
    let ids: Vec<_> = w
        .ctl
        .settings()
        .panel_geometry
        .iter()
        .map(|g| g.monitor_id.clone())
        .collect();
    assert_eq!(
        ids,
        std::slice::from_ref(&second.id),
        "only the current monitor's entry is gone"
    );
    assert!(w.ctl.view().toast.is_none(), "no Undo toast");
}
