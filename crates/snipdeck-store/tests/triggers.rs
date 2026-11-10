//! Typed triggers (v1.1 slice 5a): one code per item, none shared, freed by the trash.
#![allow(
    clippy::many_single_char_names,
    reason = "items a and c, lists l and l3"
)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]

mod common;

use common::*;
use snipdeck_core::{ItemId, Matcher, Trigger};
use snipdeck_store::{
    BoardRepo, Error, GroupDelete, GroupRepo, ItemRepo, ListRepo, Store, TrashRepo, Trashable,
    TriggerRepo,
};

const FIXTURE_V8: &[u8] = include_bytes!("fixtures/v8.db");

fn trig(text: &str) -> Trigger {
    Trigger::new(text).unwrap()
}

fn two(s: &mut Store) -> (snipdeck_core::ListId, ItemId, ItemId) {
    let b = board(s, "Work");
    let l = list(s, b.id, "Replies");
    let a = item(s, l.id, "A", "aa").id;
    let c = item(s, l.id, "C", "cc").id;
    (l.id, a, c)
}

#[test]
fn v9_keeps_items_and_adds_no_triggers() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("snipdeck.db"), FIXTURE_V8).unwrap();
    let mut s = Store::open(dir.path()).unwrap();
    assert_eq!(s.schema_version().unwrap(), 9);
    assert!(dir.path().join("snipdeck.db.pre-v8.bak").exists());
    let board = s.boards().unwrap().remove(0);
    let list = s.lists(board.id).unwrap().remove(0);
    assert_eq!(titles(&s, list.id), ["Greeting", "Wifi"]);
    assert_eq!(
        s.triggers().unwrap(),
        [] as [(snipdeck_core::Trigger, snipdeck_core::ItemId); 0]
    );
    assert_eq!(s.trashed().unwrap().len(), 1, "the trashed item survives");
    // The new table works on the migrated file.
    let greeting = s.items(list.id).unwrap().remove(0).id;
    s.set_trigger(greeting, Some(&trig(";hi"))).unwrap();
    assert_eq!(s.trigger_of(greeting).unwrap(), Some(trig(";hi")));
    assert_healthy(&s);
}

#[test]
fn a_trigger_can_be_set_replaced_and_removed() {
    let mut s = store();
    let (_, a, _) = two(&mut s);
    assert_eq!(s.trigger_of(a).unwrap(), None);
    s.set_trigger(a, Some(&trig(";sig"))).unwrap();
    s.set_trigger(a, Some(&trig(";sig"))).unwrap();
    assert_eq!(s.trigger_holder(&trig(";sig")).unwrap(), Some(a));
    s.set_trigger(a, Some(&trig(";addr"))).unwrap();
    assert_eq!(s.triggers().unwrap(), [(trig(";addr"), a)], "one per item");
    s.set_trigger(a, None).unwrap();
    assert_eq!(
        s.triggers().unwrap(),
        [] as [(snipdeck_core::Trigger, snipdeck_core::ItemId); 0]
    );
}

#[test]
fn a_duplicate_trigger_is_refused() {
    let mut s = store();
    let (_, a, c) = two(&mut s);
    s.set_trigger(a, Some(&trig(";sig"))).unwrap();
    s.set_trigger(c, Some(&trig(";addr"))).unwrap();
    let err = s.set_trigger(c, Some(&trig(";sig"))).unwrap_err();
    assert!(matches!(err, Error::TriggerTaken(t) if t == ";sig"));
    assert_eq!(
        s.trigger_of(c).unwrap(),
        Some(trig(";addr")),
        "the refused change keeps the old code"
    );
    // Case counts.
    s.set_trigger(c, Some(&trig(";SIG"))).unwrap();
    assert_eq!(s.triggers().unwrap().len(), 2);
}

#[test]
fn undoing_a_trash_keeps_the_trigger() {
    let mut s = store();
    let (l, a, c) = two(&mut s);
    s.set_trigger(a, Some(&trig(";a"))).unwrap();
    s.set_trigger(c, Some(&trig(";c"))).unwrap();
    // An item: out of the live set while trashed, back on restore.
    s.trash(Trashable::Item(a), at(10)).unwrap();
    assert_eq!(s.triggers().unwrap(), [(trig(";c"), c)]);
    assert_eq!(s.trigger_holder(&trig(";a")).unwrap(), None);
    s.restore(Trashable::Item(a)).unwrap();
    assert_eq!(s.trigger_of(a).unwrap(), Some(trig(";a")));
    // A list, and the board above it.
    s.trash(Trashable::List(l), at(11)).unwrap();
    assert_eq!(
        s.triggers().unwrap(),
        [] as [(snipdeck_core::Trigger, snipdeck_core::ItemId); 0]
    );
    s.restore(Trashable::List(l)).unwrap();
    assert_eq!(s.triggers().unwrap().len(), 2);
    let board = s.boards().unwrap().last().unwrap().id;
    s.trash(Trashable::Board(board), at(12)).unwrap();
    assert_eq!(
        s.triggers().unwrap(),
        [] as [(snipdeck_core::Trigger, snipdeck_core::ItemId); 0]
    );
    s.restore(Trashable::Board(board)).unwrap();
    assert_eq!(s.triggers().unwrap().len(), 2);
    // A group trashed with its items.
    let g = snipdeck_core::Group::new(l, name("G"));
    s.create_group(&g, &[a]).unwrap();
    s.delete_group(g.id, GroupDelete::TrashItems(at(13)))
        .unwrap();
    assert_eq!(s.triggers().unwrap(), [(trig(";c"), c)]);
    s.restore(Trashable::Item(a)).unwrap();
    assert_eq!(s.trigger_of(a).unwrap(), Some(trig(";a")));
    assert_healthy(&s);
}

#[test]
fn a_code_taken_while_the_item_was_in_the_trash_is_not_given_back() {
    let mut s = store();
    let (_, a, c) = two(&mut s);
    s.set_trigger(a, Some(&trig(";sig"))).unwrap();
    s.trash(Trashable::Item(a), at(10)).unwrap();
    s.set_trigger(c, Some(&trig(";sig"))).unwrap();
    s.restore(Trashable::Item(a)).unwrap();
    assert_eq!(s.trigger_of(a).unwrap(), None, "dropped, not stolen back");
    assert_eq!(s.trigger_holder(&trig(";sig")).unwrap(), Some(c));
    assert_healthy(&s);
}

#[test]
fn a_prefix_of_another_code_is_reported() {
    let mut s = store();
    let (_, a, c) = two(&mut s);
    assert_eq!(
        s.set_trigger(a, Some(&trig(";addr2"))).unwrap(),
        [] as [snipdeck_core::Trigger; 0]
    );
    assert_eq!(
        s.set_trigger(c, Some(&trig(";addr"))).unwrap(),
        [trig(";addr2")],
        "the new code is a prefix of an old one"
    );
    assert_eq!(
        s.set_trigger(a, Some(&trig(";addr22"))).unwrap(),
        [trig(";addr")],
        "an old code is a prefix of the new one"
    );
    assert_eq!(
        s.set_trigger(a, Some(&trig(";zzz"))).unwrap(),
        [] as [snipdeck_core::Trigger; 0]
    );
}

#[test]
fn purging_removes_the_row_and_a_sensitive_item_has_none() {
    let mut s = store();
    let (_, a, c) = two(&mut s);
    s.set_trigger(a, Some(&trig(";a"))).unwrap();
    s.trash(Trashable::Item(a), at(10)).unwrap();
    s.purge(at(1000)).unwrap();
    assert_eq!(
        s.triggers().unwrap(),
        [] as [(snipdeck_core::Trigger, snipdeck_core::ItemId); 0]
    );
    assert_healthy(&s);

    // Making an item sensitive drops its code; a sensitive item cannot be given one.
    s.set_trigger(c, Some(&trig(";c"))).unwrap();
    let mut it = s.item(c).unwrap();
    it.sensitive = true;
    it.preview = String::new();
    s.update_item(&it, None).unwrap();
    assert_eq!(
        s.triggers().unwrap(),
        [] as [(snipdeck_core::Trigger, snipdeck_core::ItemId); 0]
    );
    let err = s.set_trigger(c, Some(&trig(";c"))).unwrap_err();
    assert!(matches!(err, Error::SensitiveTrigger), "{err:?}");
}

#[test]
fn stored_triggers_feed_the_matcher() {
    let mut s = store();
    let (_, a, c) = two(&mut s);
    s.set_trigger(a, Some(&trig(";sig"))).unwrap();
    s.set_trigger(c, Some(&trig(";addr"))).unwrap();
    let mut m = Matcher::new(s.triggers().unwrap());
    let hit = ";addr".chars().find_map(|ch| m.feed(ch)).unwrap();
    assert_eq!((hit.item, hit.chars), (c, 5));
}
