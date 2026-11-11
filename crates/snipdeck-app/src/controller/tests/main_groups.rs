//! Groups in the Main window's lists: the same controller paths as the Panel, driven by the Main window's rows.

use super::*;
use crate::main_window::{Section, SelectMode};

/// Opens the Replies list (One, Two, Three, Greeting) in the Main window.
fn open_replies() -> World {
    let mut w = world();
    let board = w.ctl.store().boards().expect("boards")[0].id;
    let replies = w.ctl.store().lists(board).expect("lists")[0].id;
    w.ctl.main_open(Section::List(replies));
    w
}

/// Row shapes: `(title, is header, grouped)`.
fn shape(w: &mut World) -> Vec<(String, bool, bool)> {
    w.ctl
        .main_screen()
        .rows
        .iter()
        .map(|r| (r.title.clone(), r.header.is_some(), r.grouped))
        .collect()
}

fn group_first_two(w: &mut World) {
    w.ctl.main_select(0, SelectMode::Replace);
    w.ctl.main_select(1, SelectMode::Toggle);
    w.ctl.main_group_marked(0);
}

/// `[Group] One Two [Second] Three Greeting`.
fn two_groups(w: &mut World) {
    group_first_two(w);
    w.ctl.main_select(3, SelectMode::Replace);
    w.ctl.main_select(4, SelectMode::Toggle);
    w.ctl.main_group_marked(3);
    w.ctl.main_group_rename(3, "Second");
}

#[test]
fn ctrl_g_groups_the_selected_rows_as_one_undo_step_and_starts_the_rename() {
    let mut w = open_replies();
    group_first_two(&mut w);
    let screen = w.ctl.main_screen();
    assert_eq!(screen.rows.len(), 5, "a header joins the four items");
    assert_eq!(screen.rows[0].header.as_ref().map(|h| h.count), Some(2));
    assert!(screen.rows[1].grouped && screen.rows[2].grouped && !screen.rows[3].grouped);
    assert_eq!(screen.rename_row, Some(0), "the new header starts renaming");
    assert_eq!(screen.selected_count, 0, "the marks are spent");
    w.ctl.undo();
    assert_eq!(w.ctl.main_screen().rows.len(), 4, "one Undo ungroups");
    w.ctl.redo();
    assert!(w.ctl.main_screen().rows[0].header.is_some());
}

#[test]
fn ctrl_g_on_the_cursor_row_alone_groups_that_item() {
    let mut w = open_replies();
    w.ctl.main_group_marked(2);
    let rows = shape(&mut w);
    assert_eq!(rows.len(), 5);
    assert!(rows[2].1 && rows[3].2, "Three sits under its own header");
}

#[test]
fn a_list_that_sorts_itself_offers_manual_order_instead_of_grouping() {
    let mut w = open_replies();
    let board = w.ctl.store().boards().expect("boards")[0].id;
    let id = w.ctl.store().lists(board).expect("lists")[0].id;
    w.ctl.set_list_sort(id, snipdeck_core::ListSort::MostUsed);
    w.ctl.main_select(0, SelectMode::Replace);
    w.ctl.main_group_marked(0);
    assert_eq!(shape(&mut w).iter().filter(|r| r.1).count(), 0);
    assert!(
        matches!(
            w.ctl.main_screen().toast.map(|t| t.message),
            Some(ToastMessage::SwitchToManual { .. })
        ),
        "the toast offers manual order"
    );
}

#[test]
fn a_header_collapses_and_expands_and_the_cursor_stays_on_it() {
    let mut w = open_replies();
    group_first_two(&mut w);
    w.ctl.main_group_collapse(0, Some(true));
    assert_eq!(shape(&mut w).len(), 3, "header + Three + Greeting");
    assert_eq!(w.ctl.main_screen().cursor, 0);
    assert!(
        w.ctl.main_screen().rows[0]
            .header
            .as_ref()
            .is_some_and(|h| h.collapsed)
    );
    w.ctl.main_group_collapse(0, None);
    assert_eq!(shape(&mut w).len(), 5, "None toggles");
    w.ctl.main_group_collapse(0, Some(false));
    assert_eq!(shape(&mut w).len(), 5, "already open: nothing changes");
}

#[test]
fn renaming_a_group_is_undoable_and_an_empty_name_does_nothing() {
    let mut w = open_replies();
    group_first_two(&mut w);
    w.ctl.main_group_rename(0, "Work");
    assert_eq!(w.ctl.main_screen().rows[0].title, "Work");
    w.ctl.main_group_rename(0, "  ");
    assert_eq!(w.ctl.main_screen().rows[0].title, "Work");
    w.ctl.undo();
    assert_eq!(w.ctl.main_screen().rows[0].title, "Group");
}

#[test]
fn f2_on_a_header_asks_the_window_to_rename_it_in_place() {
    let mut w = open_replies();
    group_first_two(&mut w);
    w.ctl.group_rename_ended();
    let before = w.ctl.main_screen().rename_seq;
    w.ctl.main_group_rename_start(0);
    let screen = w.ctl.main_screen();
    assert_eq!(screen.rename_row, Some(0));
    assert_eq!(screen.rename_seq, before + 1);
    w.ctl.group_rename_ended();
    assert_eq!(w.ctl.main_screen().rename_row, None);
    w.ctl.main_group_rename_start(1);
    assert_eq!(
        w.ctl.main_screen().rename_row,
        None,
        "an item row has no header to rename"
    );
}

#[test]
fn ungroup_keeps_the_items_and_undo_brings_the_header_back() {
    let mut w = open_replies();
    group_first_two(&mut w);
    w.ctl.main_group_ungroup(0);
    assert_eq!(shape(&mut w).len(), 4);
    w.ctl.undo();
    assert!(shape(&mut w)[0].1);
}

#[test]
fn delete_group_trashes_the_header_and_its_items_in_one_undo() {
    let mut w = open_replies();
    group_first_two(&mut w);
    w.ctl.main_group_delete(0);
    assert_eq!(shape(&mut w).len(), 2);
    assert!(w.ctl.main_screen().toast.expect("toast").undo);
    w.ctl.undo();
    assert_eq!(shape(&mut w).len(), 5, "items and header are back");
}

#[test]
fn a_header_is_never_selected_and_select_all_leaves_it_out() {
    let mut w = open_replies();
    group_first_two(&mut w);
    w.ctl.main_select(0, SelectMode::Replace);
    assert_eq!(w.ctl.main_screen().selected_count, 0);
    w.ctl.main_select_all();
    assert_eq!(w.ctl.main_screen().selected_count, 4);
}

#[test]
fn dragging_a_group_down_onto_the_next_group_moves_it_below() {
    let mut w = open_replies();
    two_groups(&mut w);
    let before = shape(&mut w);
    w.ctl.main_drop_row(0, 3);
    assert_eq!(shape(&mut w), [&before[3..], &before[..3]].concat());
    w.ctl.undo();
    assert_eq!(shape(&mut w), before, "one Undo puts it back");
}

#[test]
fn dragging_an_item_onto_a_header_joins_that_group_at_its_top() {
    let mut w = open_replies();
    group_first_two(&mut w);
    w.ctl.main_drop_row(4, 0);
    let rows = shape(&mut w);
    assert_eq!(rows[0].0, "Group");
    assert_eq!(rows[1].0, "Greeting", "it joined at the top");
    assert!(rows[1].2);
    w.ctl.undo();
    assert_eq!(shape(&mut w).len(), 5);
    assert_eq!(shape(&mut w)[1].0, "One");
}

#[test]
fn alt_arrows_on_a_header_move_the_whole_group_one_step() {
    let mut w = open_replies();
    two_groups(&mut w);
    w.ctl.main_select(3, SelectMode::Replace);
    w.ctl.main_reorder_items(-1);
    let rows = shape(&mut w);
    assert_eq!(
        rows[0].0, "Second",
        "the second group moved above the first"
    );
    assert_eq!(rows[1].0, "Three");
    assert_eq!(
        w.ctl.main_screen().cursor,
        0,
        "the cursor follows the header"
    );
    w.ctl.main_reorder_items(-1);
    assert_eq!(
        shape(&mut w)[0].0,
        "Second",
        "at the top there is nowhere to go"
    );
    w.ctl.main_reorder_items(1);
    assert_eq!(shape(&mut w)[0].0, "Group");
}

/// Gives the first `n` rows of the open list the Red label.
fn label_first_rows_red(w: &mut World, n: usize) {
    let ids: Vec<_> = w
        .ctl
        .main_screen()
        .rows
        .iter()
        .take(n)
        .filter_map(|r| match r.key {
            crate::main_window::Key::Item(id) => Some(id),
            _ => None,
        })
        .collect();
    w.ctl.label_items(&ids, Some(snipdeck_core::ItemLabel::Red));
}

#[test]
fn ctrl_g_under_a_colour_filter_groups_nothing_because_no_header_could_show() {
    let mut w = open_replies();
    label_first_rows_red(&mut w, 2);
    w.ctl
        .main_set_label_filter(Some(snipdeck_core::ItemLabel::Red));
    assert_eq!(shape(&mut w).len(), 2, "the filter shows two rows");
    w.ctl.main_select(0, SelectMode::Replace);
    w.ctl.main_select(1, SelectMode::Toggle);
    w.ctl.main_group_marked(0);
    w.ctl.main_set_label_filter(None);
    assert_eq!(
        shape(&mut w).iter().filter(|r| r.1).count(),
        0,
        "no group was made behind the filter"
    );
    assert!(
        w.ctl.main_screen().rename_row.is_none(),
        "and no rename is waiting for a header that is not there"
    );
}

#[test]
fn a_refused_ctrl_g_under_a_colour_filter_leaves_the_existing_group_and_its_undo_alone() {
    let mut w = open_replies();
    group_first_two(&mut w);
    w.ctl.group_rename_ended();
    label_first_rows_red(&mut w, 1);
    w.ctl
        .main_set_label_filter(Some(snipdeck_core::ItemLabel::Red));
    w.ctl.main_select(0, SelectMode::Replace);
    w.ctl.main_group_marked(0);
    w.ctl.main_set_label_filter(None);
    let rows = shape(&mut w);
    assert_eq!(rows.iter().filter(|r| r.1).count(), 1, "{rows:?}");
    assert_eq!(rows.iter().filter(|r| r.2).count(), 2, "{rows:?}");
    // The one Undo is still the first grouping: it ungroups, and nothing else was recorded on top of it.
    w.ctl.undo();
    assert_eq!(shape(&mut w).iter().filter(|r| r.1).count(), 0);
}

/// Whether the screen shows the "not saved" toast.
fn toast_failed(w: &mut World) -> bool {
    matches!(
        w.ctl.main_screen().toast.map(|t| t.message),
        Some(ToastMessage::NotSaved)
    )
}

fn titles(w: &mut World) -> Vec<String> {
    shape(w).into_iter().map(|r| r.0).collect()
}

#[test]
fn alt_down_on_the_only_member_of_a_group_leaves_it_without_a_failed_toast() {
    let mut w = open_replies();
    w.ctl.main_group_marked(2); // [One Two [Group] Three Greeting]
    w.ctl.main_select(3, SelectMode::Replace);
    w.ctl.main_reorder_items(1);
    assert!(!toast_failed(&mut w), "no failed toast");
    assert_eq!(titles(&mut w), ["One", "Two", "Greeting", "Three"]);
    assert_eq!(w.ctl.main_screen().cursor, 3, "the cursor follows the item");
    w.ctl.undo();
    assert_eq!(
        titles(&mut w),
        ["One", "Two", "Group", "Three", "Greeting"],
        "one Undo puts the group back"
    );
}

#[test]
fn alt_up_on_the_first_member_leaves_the_group_and_records_exactly_one_step() {
    let mut w = open_replies();
    group_first_two(&mut w); // [Group] One Two Three Greeting
    w.ctl.main_select(1, SelectMode::Replace);
    w.ctl.main_reorder_items(-1);
    assert_eq!(titles(&mut w), ["One", "Group", "Two", "Three", "Greeting"]);
    w.ctl.undo();
    assert_eq!(
        titles(&mut w),
        ["Group", "One", "Two", "Three", "Greeting"],
        "one Undo re-joins"
    );
    w.ctl.undo();
    assert_eq!(
        titles(&mut w),
        ["One", "Two", "Three", "Greeting"],
        "the next Undo is the grouping, not an empty step"
    );
}

#[test]
fn an_item_steps_past_a_folded_group_instead_of_joining_it() {
    let mut w = open_replies();
    group_first_two(&mut w);
    w.ctl.main_group_collapse(0, Some(true)); // [Group*] Three Greeting
    w.ctl.main_select(1, SelectMode::Replace);
    w.ctl.main_reorder_items(-1);
    assert_eq!(titles(&mut w), ["Three", "Group", "Greeting"]);
    let count = |w: &mut World| {
        w.ctl
            .main_screen()
            .rows
            .iter()
            .find_map(|r| r.header.as_ref().map(|h| h.count))
    };
    assert_eq!(count(&mut w), Some(2), "Three did not join");
    w.ctl.main_select(0, SelectMode::Replace);
    w.ctl.main_reorder_items(1);
    assert_eq!(titles(&mut w), ["Group", "Three", "Greeting"]);
    assert_eq!(count(&mut w), Some(2), "nor on the way back");
    w.ctl.undo();
    w.ctl.undo();
    assert_eq!(titles(&mut w), ["Group", "Three", "Greeting"]);
}

#[test]
fn the_palette_groups_folds_renames_moves_ungroups_and_deletes_like_the_keys() {
    use crate::commands::{Command, CursorGroup};
    let mut w = open_replies();
    // Group the cursor row (Two) from the palette: the cursor item alone, as Ctrl+G does.
    w.ctl.run_command(Command::GroupItems, 1);
    assert!(
        shape(&mut w).iter().any(|r| r.1),
        "a header appeared: {:?}",
        shape(&mut w)
    );
    w.ctl.group_rename_ended();
    // Put the cursor on the header (row 1: One, [Group], Two, ...).
    let head = shape(&mut w).iter().position(|r| r.1).expect("header");
    w.ctl.main_select(head, SelectMode::Replace);
    let screen = w.ctl.main_screen();
    assert_eq!(
        screen.cursor_group,
        Some(CursorGroup {
            up: true,
            down: true
        }),
        "a place above and below"
    );
    // Fold and unfold.
    let rows = screen.rows.len();
    w.ctl.run_command(Command::FoldGroup, head);
    assert_eq!(w.ctl.main_screen().rows.len(), rows - 1, "Two is hidden");
    w.ctl.run_command(Command::FoldGroup, head);
    assert_eq!(w.ctl.main_screen().rows.len(), rows, "and shown again");
    // Rename starts the inline edit on that header.
    w.ctl.run_command(Command::RenameGroup, head);
    assert_eq!(w.ctl.main_screen().rename_row, Some(head));
    w.ctl.group_rename_ended();
    // Move up: the header passes the item above it; the cursor stays on it.
    w.ctl.run_command(Command::MoveGroupUp, head);
    let screen = w.ctl.main_screen();
    assert_eq!(screen.cursor, head - 1);
    assert!(screen.rows[head - 1].header.is_some());
    assert_eq!(
        screen.cursor_group,
        Some(CursorGroup {
            up: false,
            down: true
        }),
        "now first"
    );
    w.ctl.run_command(Command::MoveGroupDown, head - 1);
    assert!(w.ctl.main_screen().rows[head].header.is_some());
    // Ungroup keeps the items; the header goes.
    w.ctl.run_command(Command::UngroupGroup, head);
    assert!(shape(&mut w).iter().all(|r| !r.1), "no header left");
    assert_eq!(w.ctl.main_screen().rows.len(), 4);
    // Delete group trashes the header's items (Undo is the usual toast).
    w.ctl.main_select(1, SelectMode::Replace);
    w.ctl.run_command(Command::GroupItems, 1);
    w.ctl.group_rename_ended();
    let head = shape(&mut w).iter().position(|r| r.1).expect("header");
    w.ctl.run_command(Command::DeleteGroup, head);
    assert_eq!(
        w.ctl.main_screen().rows.len(),
        3,
        "its item went to the Trash"
    );
}

#[test]
fn an_item_row_has_no_cursor_group() {
    let mut w = open_replies();
    group_first_two(&mut w);
    w.ctl.group_rename_ended();
    w.ctl.main_select(1, SelectMode::Replace);
    assert_eq!(w.ctl.main_screen().cursor_group, None);
}
