//! Groups inside a list (v1.1 slice 3a): a named run of contiguous items.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]
#![allow(
    clippy::many_single_char_names,
    reason = "items A..E are named a..e in the tests"
)]

mod common;

use common::*;
use snipdeck_core::{Group, ItemId, ListId, Placement};
use snipdeck_store::{
    BoardRepo, GroupDelete, GroupEnd, GroupRepo, ItemRepo, ListRepo, Store, TrashRepo, Trashable,
};

const FIXTURE_V7: &[u8] = include_bytes!("fixtures/v7.db");

/// A list with items A..E; returns the list and the ids in order.
fn five(s: &mut Store) -> (ListId, Vec<ItemId>) {
    let b = board(s, "Work");
    let l = list(s, b.id, "Replies");
    let ids = ["A", "B", "C", "D", "E"]
        .iter()
        .map(|t| item(s, l.id, t, t).id)
        .collect();
    (l.id, ids)
}

fn runs(s: &Store, list: ListId) -> Vec<(String, Vec<ItemId>)> {
    s.groups_of(list)
        .unwrap()
        .into_iter()
        .map(|r| (r.group.name.to_string(), r.items))
        .collect()
}

#[test]
fn v8_keeps_items_and_adds_no_groups() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("snipdeck.db"), FIXTURE_V7).unwrap();
    let s = Store::open(dir.path()).unwrap();
    assert_eq!(s.schema_version().unwrap(), 9);
    assert!(dir.path().join("snipdeck.db.pre-v7.bak").exists());
    let board = s.boards().unwrap().remove(0);
    let list = s.lists(board.id).unwrap().remove(0);
    assert_eq!(titles(&s, list.id), ["Greeting", "Wifi"]);
    assert!(s.groups_of(list.id).unwrap().is_empty());
    assert_eq!(s.trashed().unwrap().len(), 1, "the trashed item survives");
    assert_healthy(&s);
}

#[test]
fn a_group_keeps_its_members_contiguous() {
    let mut s = store();
    let (l, ids) = five(&mut s);
    let [a, b, c, d, e] = ids[..] else { panic!() };
    let g = Group::new(l, name("Greetings"));
    // Members given out of order and apart: they gather at the first one's place, list order.
    s.create_group(&g, &[c, a]).unwrap();
    assert_eq!(titles(&s, l), ["A", "C", "B", "D", "E"]);
    assert_eq!(runs(&s, l), [("Greetings".to_owned(), vec![a, c])]);

    // After a member: joins. First/Last or after a non-member: leaves (or stays out).
    s.move_item(b, l, Placement::After(a)).unwrap();
    assert_eq!(runs(&s, l)[0].1, [a, b, c]);
    s.move_item(d, l, Placement::After(e)).unwrap();
    s.move_item(b, l, Placement::Last).unwrap();
    assert_eq!(titles(&s, l), ["A", "C", "E", "D", "B"]);
    assert_eq!(runs(&s, l)[0].1, [a, c]);
    s.move_item(e, l, Placement::First).unwrap();
    assert_eq!(runs(&s, l)[0].1, [a, c]);
    assert_healthy(&s);
}

#[test]
fn an_emptied_group_goes() {
    let mut s = store();
    let (l, ids) = five(&mut s);
    let g = Group::new(l, name("Pair"));
    s.create_group(&g, &ids[..2]).unwrap();
    s.move_item(ids[0], l, Placement::Last).unwrap();
    assert_eq!(runs(&s, l)[0].1, [ids[1]]);
    s.trash(Trashable::Item(ids[1]), at(5)).unwrap();
    assert!(runs(&s, l).is_empty(), "no live member, no header");
    s.restore(Trashable::Item(ids[1])).unwrap();
    assert_eq!(
        runs(&s, l).len(),
        1,
        "restored member brings the group back"
    );
}

#[test]
fn moving_to_another_list_leaves_the_group() {
    let mut s = store();
    let (l, ids) = five(&mut s);
    let bid = s.boards().unwrap()[0].id;
    let other = list(&mut s, bid, "Other");
    s.create_group(&Group::new(l, name("G")), &ids[..2])
        .unwrap();
    s.move_item(ids[0], other.id, Placement::Last).unwrap();
    assert_eq!(runs(&s, l)[0].1, [ids[1]]);
    assert!(runs(&s, other.id).is_empty());
    assert_healthy(&s);
}

#[test]
fn move_run_moves_a_whole_group() {
    let mut s = store();
    let (l, ids) = five(&mut s);
    let [a, b, c, d, e] = ids[..] else { panic!() };
    s.create_group(&Group::new(l, name("BC")), &[b, c]).unwrap();
    s.move_run(s.groups_of(l).unwrap()[0].group.id, Placement::Last)
        .unwrap();
    assert_eq!(titles(&s, l), ["A", "D", "E", "B", "C"]);
    s.move_run(s.groups_of(l).unwrap()[0].group.id, Placement::First)
        .unwrap();
    assert_eq!(titles(&s, l), ["B", "C", "A", "D", "E"]);
    s.move_run(s.groups_of(l).unwrap()[0].group.id, Placement::After(d))
        .unwrap();
    assert_eq!(titles(&s, l), ["A", "D", "B", "C", "E"]);

    // After a member of another group lands after that group's end, never inside it.
    s.create_group(&Group::new(l, name("AD")), &[a, d]).unwrap();
    let bc = s
        .groups_of(l)
        .unwrap()
        .into_iter()
        .find(|r| r.items[0] == b)
        .unwrap();
    s.move_run(bc.group.id, Placement::After(a)).unwrap();
    assert_eq!(titles(&s, l), ["A", "D", "B", "C", "E"]);
    assert_eq!(runs(&s, l).len(), 2);
    let _ = e;
    assert_healthy(&s);
}

#[test]
fn deleting_a_group_keeps_or_trashes_its_items() {
    let mut s = store();
    let (l, ids) = five(&mut s);
    let g = Group::new(l, name("G"));
    s.create_group(&g, &ids[1..3]).unwrap();
    s.delete_group(g.id, GroupDelete::Keep).unwrap();
    assert!(runs(&s, l).is_empty());
    assert_eq!(titles(&s, l), ["A", "B", "C", "D", "E"]);

    let g2 = Group::new(l, name("G2"));
    s.create_group(&g2, &ids[1..3]).unwrap();
    s.delete_group(g2.id, GroupDelete::TrashItems(at(9)))
        .unwrap();
    assert_eq!(titles(&s, l), ["A", "D", "E"]);
    assert_eq!(s.trashed().unwrap().len(), 2);
    assert!(s.groups_of(l).unwrap().is_empty());
}

#[test]
fn renaming_collapsing_and_joining() {
    let mut s = store();
    let (l, ids) = five(&mut s);
    let mut g = Group::new(l, name("Old"));
    s.create_group(&g, &ids[..2]).unwrap();
    g.name = name("New");
    g.collapsed = true;
    s.update_group(&g).unwrap();
    s.join_group(ids[4], g.id, GroupEnd::Last).unwrap();
    let r = s.groups_of(l).unwrap();
    assert_eq!(
        (r[0].group.name.as_str(), r[0].group.collapsed),
        ("New", true)
    );
    assert_eq!(r[0].items, [ids[0], ids[1], ids[4]]);
    assert_eq!(titles(&s, l), ["A", "B", "E", "C", "D"]);
    // A group needs a member and members of its own list.
    assert!(s.create_group(&Group::new(l, name("X")), &[]).is_err());
    let bid = s.boards().unwrap()[0].id;
    let other = list(&mut s, bid, "Other");
    let stray = item(&mut s, other.id, "Z", "Z").id;
    assert!(s.create_group(&Group::new(l, name("X")), &[stray]).is_err());
}

#[test]
fn a_batch_can_group_and_roll_back() {
    let mut s = store();
    let (l, ids) = five(&mut s);
    let done: Result<(), snipdeck_store::Error> = s.transaction(|b| {
        b.create_group(&Group::new(l, name("G")), &ids[..2])?;
        Err(snipdeck_store::Error::Corrupt("stop".to_owned()))
    });
    assert!(done.is_err());
    assert!(s.groups_of(l).unwrap().is_empty());
}

#[test]
fn moving_from_another_list_after_a_member_joins_its_group() {
    let mut s = store();
    let (l, ids) = five(&mut s);
    let bid = s.boards().unwrap()[0].id;
    let other = list(&mut s, bid, "Other");
    let x = item(&mut s, other.id, "X", "X").id;
    s.create_group(&Group::new(l, name("G")), &ids[..3])
        .unwrap();
    s.move_item(x, l, Placement::After(ids[0])).unwrap();
    assert_eq!(runs(&s, l)[0].1, [ids[0], x, ids[1], ids[2]]);
    // A move out of a group into another list's plain spot leaves no group behind.
    s.move_item(x, other.id, Placement::Last).unwrap();
    assert_eq!(runs(&s, l)[0].1, [ids[0], ids[1], ids[2]]);
    assert!(runs(&s, other.id).is_empty());
}

#[test]
fn joining_at_the_start_keeps_a_firsts_membership_on_undo() {
    let mut s = store();
    let (l, ids) = five(&mut s);
    let g = Group::new(l, name("G"));
    s.create_group(&g, &ids[1..4]).unwrap(); // B C D
    // Move the first member out (as a drag would), then undo it: back at the start.
    s.move_item(ids[1], l, Placement::Last).unwrap();
    assert_eq!(runs(&s, l)[0].1, [ids[2], ids[3]]);
    s.join_group(ids[1], g.id, GroupEnd::First).unwrap();
    assert_eq!(runs(&s, l)[0].1, [ids[1], ids[2], ids[3]]);
    assert_eq!(titles(&s, l), ["A", "B", "C", "D", "E"]);
    // With nothing before the run it goes to the very top.
    s.move_item(ids[0], l, Placement::Last).unwrap();
    s.join_group(ids[4], g.id, GroupEnd::First).unwrap();
    assert_eq!(titles(&s, l), ["E", "B", "C", "D", "A"]);
    assert_healthy(&s);
}

#[test]
fn restoring_after_trash_items_brings_the_header_back() {
    let mut s = store();
    let (l, ids) = five(&mut s);
    let g = Group::new(l, name("G"));
    s.create_group(&g, &ids[1..3]).unwrap();
    s.delete_group(g.id, GroupDelete::TrashItems(at(9)))
        .unwrap();
    assert!(runs(&s, l).is_empty());
    s.restore(Trashable::Item(ids[1])).unwrap();
    s.restore(Trashable::Item(ids[2])).unwrap();
    assert_eq!(runs(&s, l), [("G".to_owned(), vec![ids[1], ids[2]])]);
}

fn group_rows(dir: &std::path::Path) -> i64 {
    rusqlite::Connection::open(dir.join("snipdeck.db"))
        .unwrap()
        .query_row("SELECT count(*) FROM item_groups", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn purging_the_last_members_removes_the_group() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut s = Store::open(dir.path()).unwrap();
        let (l, ids) = five(&mut s);
        s.create_group(&Group::new(l, name("G")), &ids[1..3])
            .unwrap();
        s.trash(Trashable::Item(ids[1]), at(1)).unwrap();
        s.trash(Trashable::Item(ids[2]), at(2)).unwrap();
        s.purge(at(10)).unwrap();
    }
    assert_eq!(group_rows(dir.path()), 0);
    {
        let mut s = Store::open(dir.path()).unwrap();
        let (l, ids) = five(&mut s);
        s.create_group(&Group::new(l, name("G")), &ids[..1])
            .unwrap();
        s.trash(Trashable::Item(ids[0]), at(1)).unwrap();
        s.purge_entry(Trashable::Item(ids[0])).unwrap();
    }
    assert_eq!(group_rows(dir.path()), 0);
}

/// A restored item whose rank lies inside another group's run (not reachable through the
/// store's own calls, but a damaged or hand-edited file could) never splits that run.
#[test]
fn a_restored_item_inside_a_run_never_splits_it() {
    let dir = tempfile::tempdir().unwrap();
    let (l, ids) = {
        let mut s = Store::open(dir.path()).unwrap();
        let (l, ids) = five(&mut s);
        s.create_group(&Group::new(l, name("G")), &ids[..2])
            .unwrap(); // A B
        s.trash(Trashable::Item(ids[3]), at(1)).unwrap(); // D, ungrouped
        (l, ids)
    };
    {
        // Put D's rank between A and B.
        let raw = rusqlite::Connection::open(dir.path().join("snipdeck.db")).unwrap();
        raw.execute(
            "UPDATE items SET rank = (SELECT (a.rank + b.rank) / 2 FROM items a, items b
                                       WHERE a.id = ?1 AND b.id = ?2) WHERE id = ?3",
            rusqlite::params![
                ids[0].as_uuid().as_bytes(),
                ids[1].as_uuid().as_bytes(),
                ids[3].as_uuid().as_bytes()
            ],
        )
        .unwrap();
    }
    let mut s = Store::open(dir.path()).unwrap();
    s.restore(Trashable::Item(ids[3])).unwrap();
    let r = runs(&s, l);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].1, [ids[0], ids[3], ids[1]], "joined rather than split");
}
