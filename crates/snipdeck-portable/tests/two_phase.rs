//! The two halves of an import: decode (no library) and preview or apply (the library).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]

mod common;

use std::io::Cursor;

use common::*;
use snipdeck_portable::{
    Decoded, ExportOptions, ImportOptions, ImportReport, KeySource, Known, MergeStrategy,
    decode_archive, decode_beeftext, export_to_writer, import_beeftext,
};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, Store, TagRepo, TriggerRepo};

fn skip() -> ImportOptions<'static> {
    ImportOptions {
        merge: MergeStrategy::SkipDuplicates,
        key: KeySource::None,
        ..ImportOptions::default()
    }
}

fn snapshot(s: &Store) -> (usize, usize, usize, usize) {
    let boards = s.boards().unwrap();
    let lists: usize = boards.iter().map(|b| s.lists(b.id).unwrap().len()).sum();
    (
        boards.len(),
        lists,
        s.all_items().unwrap().len(),
        s.tags().unwrap().len(),
    )
}

fn without_ids(mut r: ImportReport) -> ImportReport {
    r.boards.clear();
    r
}

fn source_archive() -> Vec<u8> {
    let mut src = store();
    let b = board(&mut src, "Src");
    let l = list(&mut src, b.id, "L");
    let tg = tag(&mut src, "work");
    let mut item =
        snipdeck_core::Item::new(l.id, "one".to_owned(), snipdeck_core::ItemKind::Text, at(3));
    item.tags = [tg.id].into_iter().collect();
    src.insert_item(
        &item,
        &text_formats("hello"),
        snipdeck_core::Placement::Last,
    )
    .unwrap();
    text_item(&mut src, l.id, "two", "world");
    let mut out = Vec::new();
    export_to_writer(
        &src,
        at(5),
        &ExportOptions::default(),
        Cursor::new(&mut out),
    )
    .unwrap();
    out
}

#[test]
fn a_decoded_import_can_move_to_a_worker() {
    fn send<T: Send>() {}
    send::<Decoded<'static>>();
}

#[test]
fn a_preview_reports_exactly_what_the_apply_then_adds() {
    let json = r#"{"version":8,"groups":[{"uuid":"g","name":"G"}],"combos":[
      {"uuid":"1","name":"A","keyword":";hi","snippet":"alpha","group":"g"},
      {"uuid":"2","name":"B","keyword":";hix","snippet":"beta","group":"g"},
      {"uuid":"3","name":"C","keyword":";ok","snippet":"gamma","group":"g"},
      {"uuid":"4","name":"D","keyword":";ok","snippet":"delta","group":"g"}]}"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.json");
    std::fs::write(&path, json).unwrap();
    let mut s = store();
    let b = board(&mut s, "Mine");
    let l = list(&mut s, b.id, "Mine");
    let held = text_item(&mut s, l.id, "held", "alpha");
    let code = snipdeck_core::Trigger::new(";hi").unwrap();
    s.set_trigger(held.id, Some(&code)).unwrap();

    let decoded = decode_beeftext(&path, at(9), &skip()).unwrap();
    let before = snapshot(&s);
    let previewed = decoded.preview(&s, None).unwrap();
    assert_eq!(snapshot(&s), before, "a preview writes nothing");
    let applied = decoded.apply(&mut s, None).unwrap();
    assert_eq!(previewed, without_ids(applied));
    assert!(
        previewed.has_warnings(),
        "the taken and overlapping codes are told"
    );
    assert_eq!(previewed.duplicates_skipped, 1, "`alpha` is already held");
}

#[test]
fn a_preview_of_an_archive_counts_new_tags_and_matches_the_apply() {
    let bytes = source_archive();
    let mut target = store();
    tag(&mut target, "Work");
    let decoded = decode_archive(Cursor::new(bytes), "x", at(7), &skip()).unwrap();
    let previewed = decoded.preview(&target, None).unwrap();
    assert_eq!(
        previewed.tags_added, 0,
        "the library's tag is reused by name"
    );
    let applied = decoded.apply(&mut target, None).unwrap();
    assert_eq!(previewed, without_ids(applied));
}

#[test]
fn an_item_added_after_the_snapshot_is_still_skipped_when_known_is_read_fresh() {
    let bytes = source_archive();
    let mut target = store();
    let decoded = decode_archive(Cursor::new(bytes), "x", at(7), &skip()).unwrap();
    let stale = Known::of(&target).unwrap();
    // A duplicate appears after the snapshot was read.
    let b = board(&mut target, "Mine");
    let l = list(&mut target, b.id, "Mine");
    text_item(&mut target, l.id, "typed", "hello");

    let report = decoded.preview(&target, None).unwrap();
    assert_eq!(
        report.duplicates_skipped, 1,
        "read fresh, it sees the new item"
    );
    let with_stale = decoded.preview(&target, Some(&stale)).unwrap();
    assert_eq!(
        with_stale.duplicates_skipped, 0,
        "a stale set is the caller's to refresh"
    );
    let applied = decoded.apply(&mut target, None).unwrap();
    assert_eq!(applied.duplicates_skipped, 1);
    assert_eq!(report, without_ids(applied));
}

#[test]
fn a_corrupt_source_fails_to_decode_and_the_library_is_never_involved() {
    let mut bytes = source_archive();
    let cut = bytes.len() / 2;
    bytes.truncate(cut);
    assert!(decode_archive(Cursor::new(bytes), "x", at(7), &skip()).is_err());
    assert!(decode_archive(Cursor::new(b"not a zip".to_vec()), "x", at(7), &skip()).is_err());

    // The one-call form leaves the library as it was too.
    let mut s = store();
    let before = snapshot(&s);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.json");
    std::fs::write(&path, b"{ not json").unwrap();
    assert!(import_beeftext(&mut s, &path, at(9), &skip()).is_err());
    assert_eq!(snapshot(&s), before);
}
