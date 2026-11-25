//! Groups nested in groups in the Main window: Ctrl+G inside a group, "Nest under…", ungroup,
//! delete and move to another list, each with its Undo.

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

/// Row shapes: `(title, is header, depth)`.
fn shape(w: &mut World) -> Vec<(String, bool, usize)> {
    w.ctl
        .main_screen()
        .rows
        .iter()
        .map(|r| {
            let depth = r.header.as_ref().map_or(r.depth, |h| h.depth);
            (r.title.clone(), r.header.is_some(), depth)
        })
        .collect()
}

fn titles(w: &mut World) -> Vec<String> {
    shape(w).into_iter().map(|r| r.0).collect()
}

fn depths(w: &mut World) -> Vec<usize> {
    shape(w).into_iter().map(|r| r.2).collect()
}

/// `[Outer] One Two [Inner] Three Greeting`... as `[Outer] [Inner] One Two` first, then the rest:
/// Outer holds One, Two, Three; Inner holds One and Two.
fn nested(w: &mut World) {
    w.ctl.main_select(0, SelectMode::Replace);
    w.ctl.main_select(1, SelectMode::Toggle);
    w.ctl.main_select(2, SelectMode::Toggle);
    w.ctl.main_group_marked(0);
    w.ctl.main_group_rename(0, "Outer");
    // Rows: Outer One Two Three Greeting; One and Two now make a group inside Outer.
    w.ctl.main_select(1, SelectMode::Replace);
    w.ctl.main_select(2, SelectMode::Toggle);
    w.ctl.main_group_marked(1);
    w.ctl.main_group_rename(1, "Inner");
}

#[test]
fn ctrl_g_on_rows_of_one_group_makes_a_group_inside_it_and_undo_dissolves_it() {
    let mut w = open_replies();
    nested(&mut w);
    assert_eq!(
        shape(&mut w),
        [
            ("Outer".to_owned(), true, 0),
            ("Inner".to_owned(), true, 1),
            ("One".to_owned(), false, 2),
            ("Two".to_owned(), false, 2),
            ("Three".to_owned(), false, 1),
            ("Greeting".to_owned(), false, 0),
        ],
        "the inner header sits inside the outer, its members one step deeper"
    );
    w.ctl.undo();
    w.ctl.undo();
    assert_eq!(
        depths(&mut w).len(),
        5,
        "Undo twice: the inner, then its rename"
    );
    w.ctl.redo();
    w.ctl.redo();
    assert_eq!(depths(&mut w), [0, 1, 2, 2, 1, 0]);
}

#[test]
fn rows_from_two_groups_make_a_group_at_the_top_level() {
    let mut w = open_replies();
    nested(&mut w);
    // Three (in Outer) and Greeting (in none) are chosen together.
    w.ctl.main_select(4, SelectMode::Replace);
    w.ctl.main_select(5, SelectMode::Toggle);
    w.ctl.main_group_marked(4);
    let rows = shape(&mut w);
    let header = rows.iter().find(|r| r.0 == "Group").expect("a new group");
    assert_eq!(header.2, 0, "a mixed selection stays at the top level");
}

#[test]
fn nest_under_lists_where_a_group_can_go_and_one_undo_step_puts_it_back() {
    let mut w = open_replies();
    w.ctl.main_select(0, SelectMode::Replace);
    w.ctl.main_group_marked(0);
    w.ctl.main_group_rename(0, "A");
    // Rows: A One Two Three Greeting; make B from Greeting.
    w.ctl.main_select(4, SelectMode::Replace);
    w.ctl.main_group_marked(4);
    w.ctl.main_group_rename(4, "B");
    assert_eq!(
        titles(&mut w),
        ["A", "One", "Two", "Three", "B", "Greeting"]
    );

    w.ctl.main_group_nest_start(4);
    let choices = w.ctl.main_screen().nest_choices;
    assert_eq!(
        choices.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
        [Some("A".to_owned())],
        "B can go under A (it is not nested yet, so no top-level entry, and not under itself)"
    );
    w.ctl.main_group_nest_chosen(0);
    assert_eq!(
        shape(&mut w),
        [
            ("A".to_owned(), true, 0),
            ("One".to_owned(), false, 1),
            ("B".to_owned(), true, 1),
            ("Greeting".to_owned(), false, 2),
            ("Two".to_owned(), false, 0),
            ("Three".to_owned(), false, 0),
        ],
        "B moved to the end of A, its member with it"
    );
    assert_eq!(
        w.ctl.main_screen().nest_choices.len(),
        0,
        "the chooser closed"
    );

    // Unnest: the chooser now leads with the top level.
    w.ctl.main_group_nest_start(2);
    let choices = w.ctl.main_screen().nest_choices;
    assert_eq!(choices[0].name, None, "the top level comes first");
    w.ctl.main_group_nest_chosen(0);
    assert_eq!(
        depths(&mut w),
        [0, 1, 0, 1, 0, 0],
        "B is back at the top level"
    );

    w.ctl.undo();
    assert_eq!(depths(&mut w), [0, 1, 1, 2, 0, 0], "Undo nests it again");
    w.ctl.undo();
    assert_eq!(
        titles(&mut w),
        ["A", "One", "Two", "Three", "B", "Greeting"],
        "and again: the rows are where they were"
    );
    assert_eq!(depths(&mut w), [0, 1, 0, 0, 0, 1]);
}

#[test]
fn a_group_cannot_go_under_itself_or_deeper_than_the_limit() {
    let mut w = open_replies();
    nested(&mut w);
    // Outer's only choices: nothing below it counts, and Inner is inside it.
    w.ctl.main_group_nest_start(0);
    assert_eq!(
        w.ctl.main_screen().nest_choices.len(),
        0,
        "Outer cannot go under Inner, its own child"
    );
    // A third level fits, a fourth is refused with a toast.
    w.ctl.main_select(2, SelectMode::Replace);
    w.ctl.main_group_marked(2);
    w.ctl.main_group_rename(2, "Third");
    assert_eq!(depths(&mut w)[2], 2, "three levels deep");
    w.ctl.main_select(3, SelectMode::Replace);
    w.ctl.main_group_marked(3);
    assert!(
        matches!(
            w.ctl.main_screen().toast.map(|t| t.message),
            Some(ToastMessage::GroupTooDeep)
        ),
        "a fourth level is refused"
    );
}

#[test]
fn ungrouping_the_outer_group_lifts_the_inner_one_and_undo_restores_both() {
    let mut w = open_replies();
    nested(&mut w);
    w.ctl.main_group_ungroup(0);
    assert_eq!(
        shape(&mut w),
        [
            ("Inner".to_owned(), true, 0),
            ("One".to_owned(), false, 1),
            ("Two".to_owned(), false, 1),
            ("Three".to_owned(), false, 0),
            ("Greeting".to_owned(), false, 0),
        ],
        "Inner moves up to the top level, Three leaves with the header"
    );
    w.ctl.undo();
    assert_eq!(
        depths(&mut w),
        [0, 1, 2, 2, 1, 0],
        "one Undo brings Outer back around Inner"
    );
    w.ctl.redo();
    assert_eq!(depths(&mut w), [0, 1, 1, 0, 0]);
}

#[test]
fn ungrouping_the_inner_group_leaves_its_items_in_the_outer_one() {
    let mut w = open_replies();
    nested(&mut w);
    w.ctl.main_group_ungroup(1);
    assert_eq!(depths(&mut w), [0, 1, 1, 1, 0]);
    w.ctl.undo();
    assert_eq!(depths(&mut w), [0, 1, 2, 2, 1, 0]);
}

#[test]
fn deleting_the_outer_group_acts_at_once_on_its_whole_branch_and_undo_restores_all() {
    let mut w = open_replies();
    nested(&mut w);
    w.ctl.main_group_delete(0);
    assert_eq!(
        titles(&mut w),
        ["Greeting"],
        "Outer, Inner and their items went"
    );
    w.ctl.undo();
    assert_eq!(
        shape(&mut w),
        [
            ("Outer".to_owned(), true, 0),
            ("Inner".to_owned(), true, 1),
            ("One".to_owned(), false, 2),
            ("Two".to_owned(), false, 2),
            ("Three".to_owned(), false, 1),
            ("Greeting".to_owned(), false, 0),
        ],
        "one Undo brings the whole branch back, nested as it was"
    );
}

#[test]
fn deleting_the_inner_group_needs_no_question() {
    let mut w = open_replies();
    nested(&mut w);
    w.ctl.main_group_delete(1);
    assert_eq!(titles(&mut w), ["Outer", "Three", "Greeting"]);
}

#[test]
fn folding_the_outer_group_hides_the_inner_one() {
    let mut w = open_replies();
    nested(&mut w);
    w.ctl.main_group_collapse(0, Some(true));
    assert_eq!(titles(&mut w), ["Outer", "Greeting"]);
    w.ctl.main_group_collapse(0, Some(false));
    assert_eq!(titles(&mut w).len(), 6);
}

#[test]
fn moving_the_outer_group_to_another_list_carries_the_inner_one_and_undo_returns_it() {
    let mut w = open_replies();
    nested(&mut w);
    let to = 1;
    let target = w.ctl.main_screen().lists[to].id;
    let from = w.ctl.main_screen().lists[0].id;
    w.ctl.main_group_move_start(0);
    w.ctl.main_move_to(to);
    assert_eq!(
        titles(&mut w),
        ["Greeting"],
        "the source list keeps only what was outside"
    );
    w.ctl.main_open(Section::List(target));
    let rows = shape(&mut w);
    let at = |name: &str| rows.iter().find(|r| r.0 == name).map(|r| (r.1, r.2));
    assert_eq!(at("Outer"), Some((true, 0)));
    assert_eq!(at("Inner"), Some((true, 1)), "nested as it was");
    assert_eq!(at("One"), Some((false, 2)));
    assert_eq!(at("Three"), Some((false, 1)));
    w.ctl.undo();
    w.ctl.main_open(Section::List(from));
    assert_eq!(
        depths(&mut w),
        [0, 1, 2, 2, 1, 0],
        "Undo puts the branch back where it was"
    );
}

#[test]
fn moving_the_inner_group_to_another_list_makes_it_a_top_level_group_there() {
    let mut w = open_replies();
    nested(&mut w);
    let to = 1;
    let target = w.ctl.main_screen().lists[to].id;
    w.ctl.main_group_move_start(1);
    w.ctl.main_move_to(to);
    w.ctl.main_open(Section::List(target));
    let rows = shape(&mut w);
    let inner = rows
        .iter()
        .find(|r| r.0 == "Inner")
        .expect("Inner came along");
    assert_eq!(inner.2, 0, "its parent stayed behind");
}
