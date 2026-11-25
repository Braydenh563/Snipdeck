//! Groups inside groups: a nested group's run is a contiguous part of its parent's.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]
#![allow(
    clippy::many_single_char_names,
    reason = "items A..H are named a..h in the tests"
)]

mod common;

use common::*;
use snipdeck_core::{Group, GroupId, ItemId, ListId, Placement};
use snipdeck_store::{Error, GroupDelete, GroupEnd, GroupRepo, Store, TrashRepo, Trashable};

/// A list with items A..H; returns the list and the ids in order.
fn eight(s: &mut Store) -> (ListId, Vec<ItemId>) {
    let b = board(s, "Work");
    let l = list(s, b.id, "Replies");
    let ids = ["A", "B", "C", "D", "E", "F", "G", "H"]
        .iter()
        .map(|t| item(s, l.id, t, t).id)
        .collect();
    (l.id, ids)
}

fn child_of(list: ListId, name_: &str, parent: &Group) -> Group {
    Group {
        parent: Some(parent.id),
        ..Group::new(list, name(name_))
    }
}

/// Outer = A B C D, Inner (in Outer) = B C.
fn nested(s: &mut Store) -> (ListId, Vec<ItemId>, Group, Group) {
    let (l, ids) = eight(s);
    let outer = Group::new(l, name("Outer"));
    s.create_group(&outer, &ids[..4]).unwrap();
    let inner = child_of(l, "Inner", &outer);
    s.create_group(&inner, &ids[1..3]).unwrap();
    (l, ids, outer, inner)
}

/// (name, depth, own members, whole run) of every listed group.
fn shape(s: &Store, l: ListId) -> Vec<(String, usize, Vec<ItemId>, Vec<ItemId>)> {
    s.groups_of(l)
        .unwrap()
        .into_iter()
        .map(|r| (r.group.name.to_string(), r.depth, r.items, r.run))
        .collect()
}

fn order(s: &Store, l: ListId) -> Vec<String> {
    titles(s, l)
}

#[test]
fn a_group_made_inside_another_is_a_part_of_its_run() {
    let mut s = store();
    let (l, ids, outer, inner) = nested(&mut s);
    let got = shape(&s, l);
    assert_eq!(got.len(), 2);
    assert_eq!(
        got[0],
        (
            "Outer".to_owned(),
            0,
            vec![ids[0], ids[3]],
            ids[..4].to_vec()
        )
    );
    assert_eq!(
        got[1],
        (
            "Inner".to_owned(),
            1,
            vec![ids[1], ids[2]],
            vec![ids[1], ids[2]]
        )
    );
    let runs = s.groups_of(l).unwrap();
    assert_eq!(runs[1].group.parent, Some(outer.id));
    assert_eq!(runs[0].group.parent, None);
    assert_eq!(inner.parent, Some(outer.id));
    assert_healthy(&s);
}

#[test]
fn a_nested_group_takes_only_the_members_of_its_parent() {
    let mut s = store();
    let (l, ids, outer, _) = nested(&mut s);
    // E is in no group, B is in Inner: neither is a direct member of Outer.
    let stray = child_of(l, "Stray", &outer);
    assert!(matches!(
        s.create_group(&stray, &[ids[0], ids[4]]),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        s.create_group(&stray, &[ids[0], ids[1]]),
        Err(Error::Invalid(_))
    ));
    assert_eq!(s.groups_of(l).unwrap().len(), 2, "nothing was made");
}

#[test]
fn groups_nest_three_levels_deep_and_no_deeper() {
    let mut s = store();
    let (l, ids, _, inner) = nested(&mut s);
    let third = child_of(l, "Third", &inner);
    s.create_group(&third, &ids[1..2]).unwrap();
    assert_eq!(shape(&s, l)[2].1, 2);
    let fourth = child_of(l, "Fourth", &third);
    assert!(matches!(
        s.create_group(&fourth, &ids[1..2]),
        Err(Error::GroupParent)
    ));
    let ghost = Group {
        parent: Some(GroupId::generate()),
        ..Group::new(l, name("Ghost"))
    };
    assert!(matches!(
        s.create_group(&ghost, &ids[5..6]),
        Err(Error::GroupParent)
    ));
}

#[test]
fn a_group_nests_under_another_and_its_run_goes_to_the_end_of_the_parents() {
    let mut s = store();
    let (l, ids, outer, _) = nested(&mut s);
    // Solo = F, then nest it in Outer: it lands after D, as Outer's last part.
    let solo = Group::new(l, name("Solo"));
    s.create_group(&solo, &ids[5..6]).unwrap();
    s.reparent_group(solo.id, Some(outer.id), Placement::After(ids[3]))
        .unwrap();
    assert_eq!(order(&s, l), ["A", "B", "C", "D", "F", "E", "G", "H"]);
    let got = shape(&s, l);
    assert_eq!(got[0].3, [ids[0], ids[1], ids[2], ids[3], ids[5]]);
    assert_eq!(got[2].0, "Solo");
    assert_eq!(got[2].1, 1);
    assert_healthy(&s);
}

#[test]
fn nesting_refuses_itself_a_group_inside_it_and_too_deep() {
    let mut s = store();
    let (l, ids, outer, inner) = nested(&mut s);
    let at = Placement::After(ids[3]);
    assert!(matches!(
        s.reparent_group(outer.id, Some(outer.id), at),
        Err(Error::GroupParent)
    ));
    assert!(matches!(
        s.reparent_group(outer.id, Some(inner.id), at),
        Err(Error::GroupParent)
    ));
    // Outer holds Inner: under a group that is one deep it would be three, which fits; a
    // third level under Inner then has no room for Outer's two.
    let other = Group::new(l, name("Other"));
    s.create_group(&other, &ids[5..7]).unwrap();
    let under = child_of(l, "Under", &other);
    s.create_group(&under, &ids[5..6]).unwrap();
    assert!(matches!(
        s.reparent_group(outer.id, Some(under.id), Placement::After(ids[5])),
        Err(Error::GroupParent)
    ));
    assert_eq!(order(&s, l), ["A", "B", "C", "D", "E", "F", "G", "H"]);
}

#[test]
fn unnesting_puts_the_group_after_its_old_parents_run_at_the_top_level() {
    let mut s = store();
    let (l, ids, outer, inner) = nested(&mut s);
    // Inner out of Outer: after D.
    s.reparent_group(inner.id, None, Placement::After(ids[3]))
        .unwrap();
    assert_eq!(order(&s, l), ["A", "D", "B", "C", "E", "F", "G", "H"]);
    let got = shape(&s, l);
    assert_eq!(got[0].0, "Outer");
    assert_eq!(got[0].3, [ids[0], ids[3]]);
    assert_eq!(
        got[1],
        (
            "Inner".to_owned(),
            0,
            vec![ids[1], ids[2]],
            vec![ids[1], ids[2]]
        )
    );
    assert_eq!(s.groups_of(l).unwrap()[1].group.parent, None);
    // Undo: back under Outer, in its old place after A.
    s.reparent_group(inner.id, Some(outer.id), Placement::After(ids[0]))
        .unwrap();
    assert_eq!(order(&s, l), ["A", "B", "C", "D", "E", "F", "G", "H"]);
    assert_eq!(shape(&s, l)[1].1, 1);
    assert_healthy(&s);
}

#[test]
fn moving_a_parent_takes_its_nested_groups_along_and_never_lands_inside_another_run() {
    let mut s = store();
    let (l, ids, outer, _) = nested(&mut s);
    // After G: A B C D go below G.
    s.move_run(outer.id, Placement::After(ids[6])).unwrap();
    assert_eq!(order(&s, l), ["E", "F", "G", "A", "B", "C", "D", "H"]);
    // After C (inside Outer's run) means after Outer's last member.
    let solo = Group::new(l, name("Solo"));
    s.create_group(&solo, &ids[7..8]).unwrap();
    s.move_run(solo.id, Placement::After(ids[2])).unwrap();
    assert_eq!(order(&s, l), ["E", "F", "G", "A", "B", "C", "D", "H"]);
    assert_healthy(&s);
}

#[test]
fn a_nested_group_moves_inside_its_parents_run_only() {
    let mut s = store();
    let (l, ids, _, inner) = nested(&mut s);
    // Below D: after the run's last member within Outer.
    s.move_run(inner.id, Placement::After(ids[3])).unwrap();
    assert_eq!(order(&s, l), ["A", "D", "B", "C", "E", "F", "G", "H"]);
    // Way past Outer: stays at its end. The very top: stays at its start.
    s.move_run(inner.id, Placement::After(ids[7])).unwrap();
    assert_eq!(order(&s, l), ["A", "D", "B", "C", "E", "F", "G", "H"]);
    s.move_run(inner.id, Placement::First).unwrap();
    assert_eq!(order(&s, l), ["B", "C", "A", "D", "E", "F", "G", "H"]);
    assert_eq!(shape(&s, l)[0].3, [ids[1], ids[2], ids[0], ids[3]]);
    assert_healthy(&s);
}

#[test]
fn ungrouping_a_parent_lifts_its_members_and_nested_groups_to_its_parent() {
    let mut s = store();
    let (l, ids, outer, inner) = nested(&mut s);
    let third = child_of(l, "Third", &inner);
    s.create_group(&third, &ids[1..2]).unwrap();
    // Inner goes: B C lift to Outer, Third to Outer.
    s.delete_group(inner.id, GroupDelete::Keep).unwrap();
    let got = shape(&s, l);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].2, [ids[0], ids[2], ids[3]]);
    assert_eq!(got[1].0, "Third");
    assert_eq!(got[1].1, 1);
    assert_eq!(s.groups_of(l).unwrap()[1].group.parent, Some(outer.id));
    // Outer goes: everything is top level, Third lifted to the top.
    s.delete_group(outer.id, GroupDelete::Keep).unwrap();
    let got = shape(&s, l);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].1, 0);
    assert_eq!(order(&s, l), ["A", "B", "C", "D", "E", "F", "G", "H"]);
    assert_healthy(&s);
}

#[test]
fn deleting_a_parent_with_its_items_trashes_the_whole_run_and_restoring_brings_it_back() {
    let mut s = store();
    let (l, ids, outer, _) = nested(&mut s);
    s.delete_group(outer.id, GroupDelete::TrashItems(at(9)))
        .unwrap();
    assert_eq!(order(&s, l), ["E", "F", "G", "H"]);
    assert_eq!(s.groups_of(l).unwrap().len(), 0);
    for id in &ids[..4] {
        s.restore(Trashable::Item(*id)).unwrap();
    }
    let got = shape(&s, l);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].3.len(), 4);
    assert_eq!(got[1].1, 1);
    assert_healthy(&s);
}

#[test]
fn a_parent_with_no_member_of_its_own_stays_while_a_nested_group_has_items() {
    let mut s = store();
    let (l, ids, _, _) = nested(&mut s);
    // A and D leave Outer.
    s.set_item_group(ids[0], None).unwrap();
    s.set_item_group(ids[3], None).unwrap();
    assert_eq!(shape(&s, l)[0].2.len(), 0);
    assert_eq!(shape(&s, l)[0].3, [ids[1], ids[2]]);
    // The nested group's items go to the trash: both groups are unlisted.
    s.trash(Trashable::Item(ids[1]), at(1)).unwrap();
    s.trash(Trashable::Item(ids[2]), at(1)).unwrap();
    assert_eq!(s.groups_of(l).unwrap().len(), 0);
    // Gone for good: both rows go (the nested one first, then the parent it left empty).
    s.purge_entry(Trashable::Item(ids[1])).unwrap();
    s.purge_entry(Trashable::Item(ids[2])).unwrap();
    assert_healthy(&s);
    s.restore_group(&Group::new(l, name("Again")), &[ids[0]], &[])
        .unwrap();
    assert_eq!(s.groups_of(l).unwrap().len(), 1);
}

#[test]
fn restoring_a_member_inside_a_nested_run_joins_the_innermost_group() {
    let dir = tempfile::tempdir().unwrap();
    let (l, ids, inner) = {
        let mut s = Store::open(dir.path()).unwrap();
        let (l, ids, _, inner) = nested(&mut s);
        // D is Outer's own; trash it and give it a rank between B and C.
        s.trash(Trashable::Item(ids[3]), at(1)).unwrap();
        (l, ids, inner)
    };
    {
        let raw = rusqlite::Connection::open(dir.path().join("snipdeck.db")).unwrap();
        raw.execute(
            "UPDATE items SET rank = (SELECT (a.rank + b.rank) / 2 FROM items a, items b
                                       WHERE a.id = ?1 AND b.id = ?2) WHERE id = ?3",
            rusqlite::params![
                ids[1].as_uuid().as_bytes(),
                ids[2].as_uuid().as_bytes(),
                ids[3].as_uuid().as_bytes()
            ],
        )
        .unwrap();
    }
    let mut s = Store::open(dir.path()).unwrap();
    s.restore(Trashable::Item(ids[3])).unwrap();
    let got = shape(&s, l);
    assert_eq!(got[1].0, "Inner");
    assert_eq!(got[1].2, [ids[1], ids[3], ids[2]], "D joined Inner");
    assert_eq!(got[0].2, [ids[0]]);
    let _ = inner;
    assert_healthy(&s);
}

#[test]
fn an_item_joins_a_parent_group_above_or_below_its_whole_run() {
    let mut s = store();
    let (l, ids, outer, _) = nested(&mut s);
    s.join_group(ids[5], outer.id, GroupEnd::Last).unwrap();
    assert_eq!(order(&s, l), ["A", "B", "C", "D", "F", "E", "G", "H"]);
    s.join_group(ids[6], outer.id, GroupEnd::First).unwrap();
    assert_eq!(order(&s, l), ["G", "A", "B", "C", "D", "F", "E", "H"]);
    let got = shape(&s, l);
    assert_eq!(got[0].3.len(), 6);
    assert_eq!(got[0].2.len(), 4, "A, D, F and G are Outer's own");
    assert_healthy(&s);
}

#[test]
fn restore_group_remakes_a_parent_over_its_nested_group_without_moving_rows() {
    let mut s = store();
    let (l, ids, outer, inner) = nested(&mut s);
    let before = order(&s, l);
    s.delete_group(outer.id, GroupDelete::Keep).unwrap();
    assert_eq!(shape(&s, l).len(), 1);
    s.restore_group(&outer, &[ids[0], ids[3]], &[inner.id])
        .unwrap();
    assert_eq!(order(&s, l), before);
    let got = shape(&s, l);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].2, [ids[0], ids[3]]);
    assert_eq!(got[1].1, 1);
    // A parent with nothing of its own, only the group in it.
    s.delete_group(outer.id, GroupDelete::Keep).unwrap();
    s.set_item_group(ids[0], None).unwrap();
    s.set_item_group(ids[3], None).unwrap();
    let bare = Group::new(l, name("Bare"));
    s.restore_group(&bare, &[], &[inner.id]).unwrap();
    assert_eq!(shape(&s, l)[0].0, "Bare");
    assert_healthy(&s);
}

#[test]
fn groups_with_the_wrong_parent_stored_are_read_as_top_level() {
    let dir = tempfile::tempdir().unwrap();
    let (l, ids) = {
        let mut s = Store::open(dir.path()).unwrap();
        let (l, ids, _, inner) = nested(&mut s);
        let raw = rusqlite::Connection::open(dir.path().join("snipdeck.db")).unwrap();
        raw.execute(
            "UPDATE item_groups SET parent = ?1 WHERE id = ?2",
            rusqlite::params![[7u8; 16], inner.id.as_uuid().as_bytes()],
        )
        .unwrap();
        (l, ids)
    };
    let s = Store::open(dir.path()).unwrap();
    let got = shape(&s, l);
    assert_eq!(got.len(), 2);
    assert_eq!(got[1].1, 0);
    let _ = ids;
}
