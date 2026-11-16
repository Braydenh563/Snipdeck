//! Restoring a Snipdeck backup (DATA-1, DATA-2): items are matched by where they are and what
//! they are, never by content alone, and each goes back into its own board and list.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]

mod common;

use std::io::Cursor;

use common::*;
use snipdeck_core::Trigger;
use snipdeck_portable::{
    BoardLayout, ExportOptions, ImportOptions, MergeStrategy, export_to_writer, import_archive,
};
use snipdeck_store::{
    BoardRepo, ItemRepo, ListRepo, Store, TagRepo, TrashRepo, Trashable, TriggerRepo,
};

fn backup_of(store: &Store) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    export_to_writer(store, at(0), &ExportOptions::default(), &mut out).unwrap();
    out.into_inner()
}

fn restore(store: &mut Store, bytes: &[u8]) -> snipdeck_portable::ImportReport {
    let options = ImportOptions {
        layout: BoardLayout::KeepBoards,
        merge: MergeStrategy::Restore,
        ..ImportOptions::default()
    };
    import_archive(
        store,
        Cursor::new(bytes.to_vec()),
        "backup",
        at(50),
        &options,
    )
    .unwrap()
}

fn board_names(s: &Store) -> Vec<String> {
    s.boards()
        .unwrap()
        .iter()
        .map(|b| b.name.to_string())
        .collect()
}

fn live_items(s: &Store) -> usize {
    s.boards()
        .unwrap()
        .iter()
        .flat_map(|b| s.lists(b.id).unwrap())
        .map(|l| s.items(l.id).unwrap().len())
        .sum()
}

#[test]
fn two_items_with_the_same_text_both_come_back_with_their_triggers() {
    let mut s = store();
    let b = board(&mut s, "Work");
    let a = list(&mut s, b.id, "A");
    let c = list(&mut s, b.id, "C");
    let first = text_item(&mut s, a.id, "Signature", "Best regards");
    let second = text_item(&mut s, c.id, "Sign-off", "Best regards");
    s.set_trigger(first.id, Some(&Trigger::new(";br").unwrap()))
        .unwrap();
    s.set_trigger(second.id, Some(&Trigger::new(";ty").unwrap()))
        .unwrap();
    let bytes = backup_of(&s);

    let mut fresh = store();
    let report = restore(&mut fresh, &bytes);
    assert_eq!(report.items_added, 2);
    assert_eq!(report.duplicates_skipped, 0);
    assert_eq!(live_items(&fresh), 2);
    assert_eq!(fresh.triggers().unwrap().len(), 2);
}

#[test]
fn a_missing_item_goes_back_into_its_own_list_and_no_board_is_added() {
    let mut s = sample_store();
    let bytes = backup_of(&s);
    let boards_before = board_names(&s);
    let lists_before: usize = s
        .boards()
        .unwrap()
        .iter()
        .map(|b| s.lists(b.id).unwrap().len())
        .sum();
    let wifi = first_by_title(&s, "Wifi");
    s.trash(Trashable::Item(wifi), at(100)).unwrap();
    let items_before = live_items(&s);

    let report = restore(&mut s, &bytes);
    assert_eq!(report.items_added, 1, "{report:?}");
    assert_eq!((report.boards_added, report.lists_added), (0, 0));
    assert_eq!(board_names(&s), boards_before, "no second Home board");
    let lists_after: usize = s
        .boards()
        .unwrap()
        .iter()
        .map(|b| s.lists(b.id).unwrap().len())
        .sum();
    assert_eq!(lists_after, lists_before);
    assert_eq!(live_items(&s), items_before + 1);
    let home = s
        .boards()
        .unwrap()
        .into_iter()
        .find(|b| b.name.as_str() == "Home")
        .unwrap();
    let misc = s
        .lists(home.id)
        .unwrap()
        .into_iter()
        .find(|l| l.name.as_str() == "Misc")
        .unwrap();
    assert_eq!(s.items(misc.id).unwrap().len(), 1, "back in Home / Misc");
}

#[test]
fn restoring_into_an_unchanged_library_adds_nothing() {
    let mut s = sample_store();
    let bytes = backup_of(&s);
    let report = restore(&mut s, &bytes);
    assert_eq!(
        (
            report.items_added,
            report.boards_added,
            report.lists_added,
            report.tags_added
        ),
        (0, 0, 0, 0)
    );
}

#[test]
fn identical_items_in_one_list_are_matched_one_for_one() {
    let mut s = store();
    let b = board(&mut s, "Work");
    let l = list(&mut s, b.id, "A");
    text_item(&mut s, l.id, "Same", "same text");
    let second = text_item(&mut s, l.id, "Same", "same text");
    let bytes = backup_of(&s);
    s.trash(Trashable::Item(second.id), at(100)).unwrap();
    let report = restore(&mut s, &bytes);
    assert_eq!(report.items_added, 1);
    assert_eq!(live_items(&s), 2);
}

#[test]
fn a_foreign_import_still_skips_content_duplicates() {
    let mut s = store();
    let b = board(&mut s, "Work");
    let l = list(&mut s, b.id, "A");
    text_item(&mut s, l.id, "One", "same text");
    let mut other = store();
    let ob = board(&mut other, "X");
    let ol = list(&mut other, ob.id, "Y");
    text_item(&mut other, ol.id, "Different title", "same text");
    let bytes = backup_of(&other);
    let options = ImportOptions {
        merge: MergeStrategy::SkipDuplicates,
        ..ImportOptions::default()
    };
    let report = import_archive(&mut s, Cursor::new(bytes), "x", at(60), &options).unwrap();
    assert_eq!((report.items_added, report.duplicates_skipped), (0, 1));
}

fn first_by_title(s: &Store, title: &str) -> snipdeck_core::ItemId {
    s.boards()
        .unwrap()
        .iter()
        .flat_map(|b| s.lists(b.id).unwrap())
        .flat_map(|l| s.items(l.id).unwrap())
        .find(|i| i.title == title)
        .unwrap()
        .id
}

/// DATA-3: a backup holds the whole library but the Trash: unused tags and empty boards and
/// lists too.
#[test]
fn a_backup_keeps_unused_tags_and_empty_boards_and_lists() {
    let mut s = store();
    let b = board(&mut s, "Work");
    let _empty_list = list(&mut s, b.id, "Nothing yet");
    let _empty_board = board(&mut s, "Archive");
    tag(&mut s, "unused");
    let options = snipdeck_portable::ExportOptions {
        every_tag: true,
        ..ExportOptions::default()
    };
    let mut out = Cursor::new(Vec::new());
    export_to_writer(&s, at(0), &options, &mut out).unwrap();

    let mut fresh = store();
    let report = restore(&mut fresh, &out.into_inner());
    assert_eq!(board_names(&fresh), ["Work", "Archive"]);
    assert_eq!(fresh.tags().unwrap().len(), 1);
    assert_eq!(report.tags_added, 1);
    let lists: usize = fresh
        .boards()
        .unwrap()
        .iter()
        .map(|b| fresh.lists(b.id).unwrap().len())
        .sum();
    assert_eq!(lists, 1);
}

#[test]
fn an_ordinary_export_still_leaves_unused_tags_out() {
    let mut s = store();
    board(&mut s, "Work");
    tag(&mut s, "unused");
    let bytes = backup_of(&s);
    let mut fresh = store();
    restore(&mut fresh, &bytes);
    assert!(fresh.tags().unwrap().is_empty());
}
