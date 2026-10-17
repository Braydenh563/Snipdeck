//! Blob garbage collection and the integrity check, through the public API.
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

use std::time::Duration;

use common::*;
use rusqlite::Connection;
use snipdeck_core::{Format, Formats, INLINE_LIMIT, Item, ItemKind, Placement};
use snipdeck_store::{BlobStore, ItemRepo, Store};

fn blob_paths(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    for shard in std::fs::read_dir(dir.join("blobs"))
        .into_iter()
        .flatten()
        .flatten()
    {
        found.extend(
            std::fs::read_dir(shard.path())
                .unwrap()
                .flatten()
                .map(|e| e.path()),
        );
    }
    found
}

fn age(path: &std::path::Path, by: Duration) {
    let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(std::time::SystemTime::now() - by)
        .unwrap();
}

#[test]
fn gc_removes_old_orphans_and_keeps_referenced_and_recent_blobs() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(dir.path()).unwrap();
    let b = board(&mut s, "b");
    let l = list(&mut s, b.id, "l");

    let used = s.put(vec![1; INLINE_LIMIT + 1]).unwrap();
    let it = Item::new(l.id, "uses".to_owned(), ItemKind::Image, at(0));
    s.insert_item(
        &it,
        &Formats::new(vec![Format::Png(used.clone())]).unwrap(),
        Placement::Last,
    )
    .unwrap();
    s.put(vec![2; INLINE_LIMIT + 1]).unwrap(); // orphan, old
    s.put(vec![3; INLINE_LIMIT + 1]).unwrap(); // orphan, brand new: maybe about to be attached
    let paths = blob_paths(dir.path());
    assert_eq!(paths.len(), 3);
    for path in &paths {
        let bytes = std::fs::read(path).unwrap();
        if bytes[0] == 1 || bytes[0] == 2 {
            age(path, Duration::from_hours(1));
        }
    }
    let stray = paths[0].parent().unwrap().join(".tmp-crashed");
    std::fs::write(&stray, b"half a blob").unwrap();
    age(&stray, Duration::from_hours(1));

    let report = s.gc_blobs(Duration::from_mins(10)).unwrap();
    assert_eq!(
        (report.blobs, report.bytes, report.leftovers),
        (1, INLINE_LIMIT as u64 + 1, 1)
    );
    let left: Vec<u8> = blob_paths(dir.path())
        .iter()
        .map(|p| std::fs::read(p).unwrap()[0])
        .collect();
    assert_eq!(left.len(), 2);
    assert!(left.contains(&1) && left.contains(&3));
    assert!(!stray.exists());
    assert!(s.read(&used).is_ok());
    assert_healthy(&s);

    // With no grace period the recent orphan goes too.
    assert_eq!(s.gc_blobs(Duration::ZERO).unwrap().blobs, 1);
}

#[test]
fn putting_bytes_that_are_already_stored_refreshes_their_age() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(dir.path()).unwrap();
    s.put(vec![7; INLINE_LIMIT + 1]).unwrap();
    for path in blob_paths(dir.path()) {
        age(&path, Duration::from_hours(1));
    }
    // The caller is about to attach it to an item: a sweep in between must not take it.
    s.put(vec![7; INLINE_LIMIT + 1]).unwrap();
    assert_eq!(s.gc_blobs(Duration::from_mins(10)).unwrap().blobs, 0);
}

#[test]
fn in_memory_gc_works_too() {
    let mut s = store();
    s.put(vec![1; INLINE_LIMIT + 1]).unwrap();
    assert_eq!(s.gc_blobs(Duration::ZERO).unwrap().blobs, 1);
}

#[test]
fn the_integrity_check_reports_what_the_schema_cannot_prevent() {
    let dir = tempfile::tempdir().unwrap();
    let (blob, item_id);
    {
        let mut s = Store::open(dir.path()).unwrap();
        let b = board(&mut s, "b");
        let l = list(&mut s, b.id, "l");
        let content = s.put(vec![4; INLINE_LIMIT + 1]).unwrap();
        let it = Item::new(l.id, "x".to_owned(), ItemKind::Image, at(0));
        s.insert_item(
            &it,
            &Formats::new(vec![Format::Png(content.clone())]).unwrap(),
            Placement::Last,
        )
        .unwrap();
        item_id = it.id;
        blob = content;
        assert_healthy(&s);
    }
    // Damage the store behind its back.
    let raw = Connection::open(dir.path().join("snipdeck.db")).unwrap();
    raw.pragma_update(None, "foreign_keys", true).unwrap();
    raw.execute("DELETE FROM item_formats", []).unwrap(); // an item without content
    raw.execute("UPDATE items SET title = 'edited behind the index'", [])
        .unwrap();
    drop(raw);
    let s = Store::open(dir.path()).unwrap();
    let problems = s.check_integrity().unwrap().join("\n");
    assert!(problems.contains("items without any format"), "{problems}");
    assert!(problems.contains("titles out of date"), "{problems}");

    // And a blob file that went missing.
    let mut s = s;
    let it = s.item(item_id).unwrap();
    let new = Formats::new(vec![Format::Png(blob.clone())]).unwrap();
    s.update_item(&it, Some(&new)).unwrap();
    for path in blob_paths(dir.path()) {
        std::fs::remove_file(path).unwrap();
    }
    let problems = s.check_integrity().unwrap().join("\n");
    assert!(problems.contains("missing or the wrong size"), "{problems}");
}

/// Review L9: one stored name that is not a valid `Name` neither fails the whole listing nor
/// passes the integrity check unnoticed.
#[test]
fn one_bad_row_does_not_fail_the_whole_listing() {
    use snipdeck_store::{BoardRepo, ListRepo, TagRepo};
    let dir = tempfile::tempdir().unwrap();
    let b;
    {
        let mut s = Store::open(dir.path()).unwrap();
        b = board(&mut s, "good");
        list(&mut s, b.id, "fine");
    }
    let raw = Connection::open(dir.path().join("snipdeck.db")).unwrap();
    raw.execute(
        "INSERT INTO boards (id, name, rank, created_at, updated_at)
         VALUES (randomblob(16), 'a' || char(10) || 'b', 999999999, 0, 0)",
        [],
    )
    .unwrap();
    raw.execute(
        "INSERT INTO lists (id, board_id, name, list_view, list_sort, rank, created_at, updated_at)
         SELECT randomblob(16), id, '   ', 'compact', 'manual', 999999999, 0, 0 FROM boards
         WHERE name = 'good'",
        [],
    )
    .unwrap();
    raw.execute(
        "INSERT INTO tags (id, name, name_key) VALUES (randomblob(16), ?1, 'long')",
        [&"x".repeat(300)],
    )
    .unwrap();
    drop(raw);

    let s = Store::open(dir.path()).unwrap();
    let boards = s.boards().unwrap();
    assert_eq!(boards.len(), 2, "the bad board is still listed");
    assert_eq!(boards[0].name.as_str(), "good");
    assert_eq!(s.lists(b.id).unwrap().len(), 2);
    assert_eq!(s.tags().unwrap().len(), 1);
    let problems = s.check_integrity().unwrap().join("\n");
    assert!(
        problems.contains("1 boards with an invalid name"),
        "{problems}"
    );
    assert!(
        problems.contains("1 lists with an invalid name"),
        "{problems}"
    );
    assert!(
        problems.contains("1 tags with an invalid name"),
        "{problems}"
    );
}
