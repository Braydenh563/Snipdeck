//! The Recent inbox: dedupe, order, retention boundaries, clear, and its separation from
//! the library. Every timestamp is fixed.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]

mod common;

use std::time::Duration;

use common::*;
use snipdeck_store::{ItemRepo, RecentRepo};

const WEEK: Duration = Duration::from_hours(24 * 7);

fn texts(s: &snipdeck_store::Store) -> Vec<String> {
    s.recent().unwrap().into_iter().map(|e| e.text).collect()
}

#[test]
fn entries_come_back_newest_first_with_their_source() {
    let mut s = store();
    s.add_recent("a", Some("notepad.exe"), at(1)).unwrap();
    s.add_recent("b", None, at(2)).unwrap();
    let all = s.recent().unwrap();
    assert_eq!(texts(&s), ["b", "a"]);
    assert_eq!(all[1].source_app.as_deref(), Some("notepad.exe"));
    assert_eq!(all[0].source_app, None);
    assert_eq!(all[0].copied_at, at(2));
}

#[test]
fn recopying_bumps_the_entry_to_the_top_without_a_duplicate() {
    let mut s = store();
    s.add_recent("a", None, at(1)).unwrap();
    s.add_recent("b", None, at(2)).unwrap();
    s.add_recent("a", Some("code.exe"), at(3)).unwrap();
    assert_eq!(texts(&s), ["a", "b"]);
    let top = &s.recent().unwrap()[0];
    assert_eq!(top.copied_at, at(3));
    assert_eq!(top.source_app.as_deref(), Some("code.exe"));
}

#[test]
fn a_bump_at_the_same_instant_still_lands_on_top() {
    let mut s = store();
    s.add_recent("a", None, at(1)).unwrap();
    s.add_recent("b", None, at(1)).unwrap();
    s.add_recent("a", None, at(1)).unwrap();
    assert_eq!(texts(&s), ["a", "b"]);
}

#[test]
fn the_entry_cap_keeps_exactly_the_newest_n() {
    let mut s = store();
    for i in 0..201 {
        s.add_recent(&format!("t{i}"), None, at(i)).unwrap();
    }
    // 201 entries, cap 200: the oldest goes.
    assert_eq!(s.prune_recent(at(300), 200, WEEK).unwrap(), 1);
    let all = texts(&s);
    assert_eq!(all.len(), 200);
    assert_eq!(all[0], "t200");
    assert_eq!(all[199], "t1");
    // Exactly at the cap: nothing more goes.
    assert_eq!(s.prune_recent(at(300), 200, WEEK).unwrap(), 0);
}

#[test]
fn the_age_limit_keeps_an_entry_exactly_seven_days_old() {
    let mut s = store();
    let now = at(10 * 86_400);
    s.add_recent("exactly", None, at(3 * 86_400)).unwrap();
    s.add_recent("just older", None, at(3 * 86_400 - 1))
        .unwrap();
    s.add_recent("fresh", None, at(9 * 86_400)).unwrap();
    assert_eq!(s.prune_recent(now, 200, WEEK).unwrap(), 1);
    assert_eq!(texts(&s), ["fresh", "exactly"]);
}

#[test]
fn delete_and_clear_remove_entries() {
    let mut s = store();
    let a = s.add_recent("a", None, at(1)).unwrap();
    s.add_recent("b", None, at(2)).unwrap();
    s.add_recent("c", None, at(3)).unwrap();
    s.delete_recent(a).unwrap();
    s.delete_recent(a).unwrap(); // already gone: fine
    assert_eq!(texts(&s), ["c", "b"]);
    assert_eq!(s.clear_recent().unwrap(), 2);
    assert_eq!(s.recent().unwrap().len(), 0);
    assert_eq!(s.clear_recent().unwrap(), 0);
}

#[test]
fn entries_are_not_library_items_and_are_not_searchable() {
    let mut s = store();
    s.add_recent("needle haystack", None, at(1)).unwrap();
    assert_eq!(s.all_items().unwrap().len(), 0);
}

#[test]
fn reading_leaves_the_change_counter_alone_and_writing_moves_it() {
    let mut s = store();
    s.add_recent("a", None, at(1)).unwrap();
    let before = s.change_counter();
    s.recent().unwrap();
    assert_eq!(s.change_counter(), before);
    s.add_recent("b", None, at(2)).unwrap();
    assert!(s.change_counter() > before);
}

#[test]
fn entries_survive_reopening() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut s = snipdeck_store::Store::open(dir.path()).unwrap();
        s.add_recent("kept", Some("x.exe"), at(5)).unwrap();
    }
    let s = snipdeck_store::Store::open(dir.path()).unwrap();
    assert_eq!(texts(&s), ["kept"]);
}
