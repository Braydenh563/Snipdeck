//! Boards, lists and items keep the order the user gave them (Placement), across moves,
//! reloads and rebalancing.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]
#![allow(
    unknown_lints,
    clippy::assert_is_empty,
    reason = "`assert!(x.is_empty())` reads fine; the lint is newer than our pinned toolchain"
)]

mod common;

use common::*;
use snipdeck_core::{BoardId, ItemId, ListId, Placement};
use snipdeck_store::{BoardRepo, Error, ItemRepo, ListRepo, Store};

#[test]
fn boards_come_back_in_insertion_order_for_last() {
    let mut s = store();
    for label in ["a", "b", "c"] {
        board(&mut s, label);
    }
    assert_eq!(board_names(&s), ["a", "b", "c"]);
}

#[test]
fn first_goes_before_every_sibling() {
    let mut s = store();
    board(&mut s, "b");
    let a = snipdeck_core::Board::new(name("a"), at(0));
    s.insert_board(&a, Placement::First).unwrap();
    assert_eq!(board_names(&s), ["a", "b"]);
}

#[test]
fn after_puts_a_board_directly_behind_its_sibling() {
    let mut s = store();
    let a = board(&mut s, "a");
    board(&mut s, "c");
    let b = snipdeck_core::Board::new(name("b"), at(0));
    s.insert_board(&b, Placement::After(a.id)).unwrap();
    assert_eq!(board_names(&s), ["a", "b", "c"]);
}

#[test]
fn after_the_last_sibling_appends() {
    let mut s = store();
    board(&mut s, "a");
    let b = board(&mut s, "b");
    let c = snipdeck_core::Board::new(name("c"), at(0));
    s.insert_board(&c, Placement::After(b.id)).unwrap();
    assert_eq!(board_names(&s), ["a", "b", "c"]);
}

#[test]
fn move_board_reorders() {
    let mut s = store();
    let a = board(&mut s, "a");
    let b = board(&mut s, "b");
    let c = board(&mut s, "c");
    s.move_board(c.id, Placement::First).unwrap();
    assert_eq!(board_names(&s), ["c", "a", "b"]);
    s.move_board(c.id, Placement::After(a.id)).unwrap();
    assert_eq!(board_names(&s), ["a", "c", "b"]);
    s.move_board(a.id, Placement::Last).unwrap();
    assert_eq!(board_names(&s), ["c", "b", "a"]);
    s.move_board(b.id, Placement::After(b.id)).unwrap(); // after itself: stays put
    assert_eq!(board_names(&s), ["c", "b", "a"]);
    assert_healthy(&s);
}

#[test]
fn unknown_ids_are_not_found() {
    let mut s = store();
    let a = board(&mut s, "a");
    let ghost = BoardId::generate();
    assert!(matches!(
        s.move_board(ghost, Placement::Last),
        Err(Error::NotFound {
            entity: "board",
            ..
        })
    ));
    assert!(matches!(
        s.move_board(a.id, Placement::After(ghost)),
        Err(Error::NotFound {
            entity: "board",
            ..
        })
    ));
    let b = snipdeck_core::Board::new(name("b"), at(0));
    assert!(matches!(
        s.insert_board(&b, Placement::After(ghost)),
        Err(Error::NotFound { .. })
    ));
    // A failed insert leaves nothing behind.
    assert_eq!(board_names(&s), ["a"]);
}

#[test]
fn inserting_the_same_id_twice_fails() {
    let mut s = store();
    let a = board(&mut s, "a");
    assert!(matches!(
        s.insert_board(&a, Placement::Last),
        Err(Error::AlreadyExists {
            entity: "board",
            ..
        })
    ));
}

#[test]
fn lists_are_ordered_per_board() {
    let mut s = store();
    let b1 = board(&mut s, "b1");
    let b2 = board(&mut s, "b2");
    list(&mut s, b1.id, "x");
    list(&mut s, b2.id, "y");
    let z = list(&mut s, b1.id, "z");
    assert_eq!(list_names(&s, b1.id), ["x", "z"]);
    assert_eq!(list_names(&s, b2.id), ["y"]);
    s.move_list(z.id, b1.id, Placement::First).unwrap();
    assert_eq!(list_names(&s, b1.id), ["z", "x"]);
}

#[test]
fn a_list_moves_to_another_board_at_the_chosen_position() {
    let mut s = store();
    let b1 = board(&mut s, "b1");
    let b2 = board(&mut s, "b2");
    let x = list(&mut s, b1.id, "x");
    let p = list(&mut s, b2.id, "p");
    list(&mut s, b2.id, "q");
    s.move_list(x.id, b2.id, Placement::After(p.id)).unwrap();
    assert_eq!(list_names(&s, b2.id), ["p", "x", "q"]);
    assert!(list_names(&s, b1.id).is_empty());
    assert_eq!(s.list(x.id).unwrap().board_id, b2.id);
    assert_healthy(&s);
}

#[test]
fn a_sibling_in_another_container_is_not_a_valid_anchor() {
    let mut s = store();
    let b1 = board(&mut s, "b1");
    let b2 = board(&mut s, "b2");
    let x = list(&mut s, b1.id, "x");
    let other = list(&mut s, b2.id, "other");
    assert!(matches!(
        s.move_list(x.id, b1.id, Placement::After(other.id)),
        Err(Error::NotFound { entity: "list", .. })
    ));
    let bad = snipdeck_core::List::new(b1.id, name("n"), at(0));
    assert!(matches!(
        s.insert_list(&bad, Placement::After(other.id)),
        Err(Error::NotFound { entity: "list", .. })
    ));
}

#[test]
fn items_move_within_and_between_lists() {
    let mut s = store();
    let b = board(&mut s, "b");
    let l1 = list(&mut s, b.id, "l1");
    let l2 = list(&mut s, b.id, "l2");
    let one = item(&mut s, l1.id, "one", "1");
    let two = item(&mut s, l1.id, "two", "2");
    let three = item(&mut s, l1.id, "three", "3");
    let four = item(&mut s, l2.id, "four", "4");

    s.move_item(three.id, l1.id, Placement::First).unwrap();
    assert_eq!(titles(&s, l1.id), ["three", "one", "two"]);
    s.move_item(one.id, l2.id, Placement::After(four.id))
        .unwrap();
    assert_eq!(titles(&s, l1.id), ["three", "two"]);
    assert_eq!(titles(&s, l2.id), ["four", "one"]);
    s.move_item(two.id, l2.id, Placement::First).unwrap();
    assert_eq!(titles(&s, l2.id), ["two", "four", "one"]);
    assert_eq!(s.item(two.id).unwrap().list_id, l2.id);
    assert_healthy(&s);
}

#[test]
fn moving_into_a_missing_list_or_after_a_missing_item_fails() {
    let mut s = store();
    let b = board(&mut s, "b");
    let l = list(&mut s, b.id, "l");
    let i = item(&mut s, l.id, "i", "x");
    assert!(matches!(
        s.move_item(i.id, ListId::generate(), Placement::Last),
        Err(Error::NotFound { entity: "list", .. })
    ));
    assert!(matches!(
        s.move_item(i.id, l.id, Placement::After(ItemId::generate())),
        Err(Error::NotFound { entity: "item", .. })
    ));
    assert_eq!(titles(&s, l.id), ["i"]);
}

#[test]
fn update_with_a_new_parent_moves_to_the_end_of_it() {
    let mut s = store();
    let b = board(&mut s, "b");
    let l1 = list(&mut s, b.id, "l1");
    let l2 = list(&mut s, b.id, "l2");
    item(&mut s, l2.id, "existing", "x");
    let mut moving = item(&mut s, l1.id, "moving", "y");
    moving.list_id = l2.id;
    s.update_item(&moving, None).unwrap();
    assert_eq!(titles(&s, l2.id), ["existing", "moving"]);
    assert!(titles(&s, l1.id).is_empty());
}

#[test]
fn order_survives_closing_and_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let (l, expected);
    {
        let mut s = Store::open(dir.path()).unwrap();
        let b = board(&mut s, "b");
        l = list(&mut s, b.id, "l").id;
        let first = item(&mut s, l, "first", "1");
        item(&mut s, l, "second", "2");
        item_at(&mut s, l, "squeezed", "3", Placement::After(first.id));
        item_at(&mut s, l, "front", "4", Placement::First);
        expected = titles(&s, l);
        assert_eq!(expected, ["front", "first", "squeezed", "second"]);
    }
    let s = Store::open(dir.path()).unwrap();
    assert_eq!(titles(&s, l), expected);
    assert_healthy(&s);
}

#[test]
fn thousands_of_inserts_at_the_same_position_keep_the_order() {
    let mut s = store();
    let b = board(&mut s, "b");
    let l = list(&mut s, b.id, "l");
    let head = item(&mut s, l.id, "head", "h");
    item(&mut s, l.id, "tail", "t");
    // Every insert lands directly after `head`, so each one halves the same gap: the store has
    // to renumber over and over without ever losing the order.
    let n = 3000;
    for i in 0..n {
        item_at(
            &mut s,
            l.id,
            &format!("n{i}"),
            "x",
            Placement::After(head.id),
        );
    }
    let titles = titles(&s, l.id);
    assert_eq!(titles.len(), n + 2);
    assert_eq!(titles.first().map(String::as_str), Some("head"));
    assert_eq!(titles.last().map(String::as_str), Some("tail"));
    let middle: Vec<usize> = titles[1..=n]
        .iter()
        .map(|t| t[1..].parse().unwrap())
        .collect();
    let expected: Vec<usize> = (0..n).rev().collect();
    assert_eq!(
        middle, expected,
        "each new item sits right behind head, ahead of earlier ones"
    );
    assert_healthy(&s);
}

#[test]
fn thousands_of_inserts_at_the_front_and_back_keep_the_order() {
    let mut s = store();
    let b = board(&mut s, "b");
    let l = list(&mut s, b.id, "l");
    for i in 0..500 {
        item_at(&mut s, l.id, &format!("f{i}"), "x", Placement::First);
        item_at(&mut s, l.id, &format!("l{i}"), "x", Placement::Last);
    }
    let titles = titles(&s, l.id);
    assert_eq!(titles.len(), 1000);
    assert_eq!(titles[0], "f499");
    assert_eq!(titles[499], "f0");
    assert_eq!(titles[500], "l0");
    assert_eq!(titles[999], "l499");
}

#[test]
fn moving_an_item_repeatedly_into_the_same_gap_keeps_the_order() {
    let mut s = store();
    let b = board(&mut s, "b");
    let l = list(&mut s, b.id, "l");
    let head = item(&mut s, l.id, "head", "h");
    item(&mut s, l.id, "tail", "t");
    let movers: Vec<_> = (0..60)
        .map(|i| item(&mut s, l.id, &format!("m{i}"), "x"))
        .collect();
    for m in &movers {
        s.move_item(m.id, l.id, Placement::After(head.id)).unwrap();
    }
    let titles = titles(&s, l.id);
    assert_eq!(&titles[..2], ["head", "m59"]);
    assert_eq!(titles.last().map(String::as_str), Some("tail"));
    assert_healthy(&s);
}

/// Review L10: the lists of a board that is missing or trashed are `NotFound`, not empty.
#[test]
fn listing_lists_of_a_missing_or_trashed_board_is_not_found() {
    use snipdeck_store::{TrashRepo, Trashable};
    let mut s = store();
    let b = board(&mut s, "b");
    assert!(s.lists(b.id).unwrap().is_empty(), "a live empty board");
    assert!(matches!(
        s.lists(BoardId::generate()),
        Err(Error::NotFound {
            entity: "board",
            ..
        })
    ));
    s.trash(Trashable::Board(b.id), at(1)).unwrap();
    assert!(matches!(
        s.lists(b.id),
        Err(Error::NotFound {
            entity: "board",
            ..
        })
    ));
}
