//! `ItemRepo::change_counter`: unchanged by reads, grows with every kind of write.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]

mod common;

use common::*;
use snipdeck_core::{Placement, Tag};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, TagRepo, TrashRepo, Trashable};

#[test]
fn reads_leave_the_counter_alone() {
    let mut s = store();
    let b = board(&mut s, "B");
    let l = list(&mut s, b.id, "L");
    item(&mut s, l.id, "t", "body");
    let before = s.change_counter();
    s.boards().unwrap();
    s.lists(b.id).unwrap();
    s.items(l.id).unwrap();
    s.all_items().unwrap();
    s.tags().unwrap();
    assert_eq!(s.change_counter(), before);
}

#[test]
fn every_kind_of_write_moves_the_counter() {
    let mut s = store();
    let mut last = s.change_counter();
    let mut grew = |s: &mut snipdeck_store::Store, what: &str| {
        let now = s.change_counter();
        assert!(now > last, "{what} did not move the counter");
        last = now;
    };

    let b = board(&mut s, "B");
    grew(&mut s, "insert board");
    let l = list(&mut s, b.id, "L");
    grew(&mut s, "insert list");
    let mut it = item(&mut s, l.id, "t", "body");
    grew(&mut s, "insert item");

    it.title = "renamed".to_owned();
    s.update_item(&it, None).unwrap();
    grew(&mut s, "edit item");

    let tag = Tag::new(name("x"));
    s.insert_tag(&tag).unwrap();
    grew(&mut s, "insert tag");

    s.trash(Trashable::Item(it.id), at(1)).unwrap();
    grew(&mut s, "trash item");
}

#[test]
fn a_transaction_moves_the_counter_once_it_wrote() {
    let mut s = store();
    let b = board(&mut s, "B");
    let before = s.change_counter();
    s.transaction::<_, snipdeck_store::Error>(|tx| {
        let l = snipdeck_core::List::new(b.id, name("L"), at(0));
        tx.insert_list(&l, Placement::Last)?;
        Ok(())
    })
    .unwrap();
    assert!(s.change_counter() > before);
}
