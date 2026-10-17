//! Export then import: what comes back, in what order, and what the file contains.
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

use std::io::Cursor;

use common::*;
use snipdeck_portable::{
    BoardLayout, ExportOptions, ImportOptions, MergeStrategy, Warning, import_archive,
};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo};

fn keep_boards() -> ImportOptions<'static> {
    ImportOptions {
        layout: BoardLayout::KeepBoards,
        ..ImportOptions::default()
    }
}

fn import(
    store: &mut snipdeck_store::Store,
    bytes: &[u8],
    opts: &ImportOptions<'_>,
) -> snipdeck_portable::ImportReport {
    import_archive(store, Cursor::new(bytes), "backup", at(200), opts).unwrap()
}

#[test]
fn round_trip_into_a_fresh_store_is_equal() {
    let original = sample_store();
    let bytes = export_bytes(&original, &ExportOptions::default());

    let mut fresh = store();
    let report = import(&mut fresh, &bytes, &keep_boards());

    assert_same(&snapshot(&fresh), &snapshot(&original));
    assert_eq!(report.boards_added, 2);
    assert_eq!(report.lists_added, 5);
    assert_eq!(report.items_added, 6);
    assert_eq!(report.items_skipped, 0);
    assert!(!report.has_warnings(), "{:?}", report.warnings);
    assert_healthy(&fresh);
    assert_trash_empty(&fresh);
}

#[test]
fn order_is_the_users_order_not_creation_order() {
    let original = sample_store();
    let work = &original.boards().unwrap()[0];
    let replies = &original.lists(work.id).unwrap()[0];
    let titles: Vec<String> = original
        .items(replies.id)
        .unwrap()
        .into_iter()
        .map(|i| i.title)
        .collect();
    assert_eq!(
        titles,
        ["C third in creation", "B", "A"],
        "fixture: B was placed after C"
    );

    let mut fresh = store();
    import(
        &mut fresh,
        &export_bytes(&original, &ExportOptions::default()),
        &keep_boards(),
    );
    let board = &fresh.boards().unwrap()[0];
    let list = &fresh.lists(board.id).unwrap()[0];
    let imported: Vec<String> = fresh
        .items(list.id)
        .unwrap()
        .into_iter()
        .map(|i| i.title)
        .collect();
    assert_eq!(imported, titles);
}

#[test]
fn imported_records_get_new_ids_so_nothing_collides() {
    let original = sample_store();
    let bytes = export_bytes(&original, &ExportOptions::default());
    // Importing into the store it came from: every id would collide if ids were kept.
    let mut same = sample_store();
    let before = item_ids(&same);
    import(&mut same, &bytes, &keep_boards());
    let after = item_ids(&same);
    assert_eq!(after.len(), before.len() * 2);
    let unique: std::collections::HashSet<_> = after.iter().collect();
    assert_eq!(unique.len(), after.len());
    assert_healthy(&same);
}

#[test]
fn default_layout_is_one_new_board_named_after_the_file() {
    let original = sample_store();
    let bytes = export_bytes(&original, &ExportOptions::default());
    let mut fresh = store();
    let report = import_archive(
        &mut fresh,
        Cursor::new(&bytes),
        "my-snippets",
        at(200),
        &ImportOptions::default(),
    )
    .unwrap();
    assert_eq!(report.boards_added, 1);
    let boards = fresh.boards().unwrap();
    assert_eq!(boards.len(), 1);
    assert_eq!(boards[0].name.as_str(), "my-snippets");
    let lists: Vec<String> = fresh
        .lists(boards[0].id)
        .unwrap()
        .iter()
        .map(|l| l.name.to_string())
        .collect();
    // Two source boards, so the lists say where they came from.
    assert_eq!(
        lists,
        [
            "Work / Replies",
            "Work / Files",
            "Work / Empty",
            "Home / Misc",
            "Home / Nothing here"
        ]
    );
    assert_eq!(fresh.all_items().unwrap().len(), 6);
}

#[test]
fn a_single_board_archive_keeps_plain_list_names() {
    let mut one = store();
    let b = board(&mut one, "Only");
    let l = list(&mut one, b.id, "Notes");
    text_item(&mut one, l.id, "x", "y");
    let bytes = export_bytes(&one, &ExportOptions::default());
    let mut fresh = store();
    import(&mut fresh, &bytes, &ImportOptions::default());
    let board = &fresh.boards().unwrap()[0];
    assert_eq!(board.name.as_str(), "backup");
    assert_eq!(fresh.lists(board.id).unwrap()[0].name.as_str(), "Notes");
}

#[test]
fn board_name_option_wins() {
    let bytes = export_bytes(&sample_store(), &ExportOptions::default());
    let mut fresh = store();
    let opts = ImportOptions {
        board_name: Some("  From Alice\n".to_owned()),
        ..ImportOptions::default()
    };
    import(&mut fresh, &bytes, &opts);
    assert_eq!(fresh.boards().unwrap()[0].name.as_str(), "From Alice");
}

#[test]
fn skip_duplicates_skips_by_content_not_by_title() {
    let original = sample_store();
    let bytes = export_bytes(&original, &ExportOptions::default());

    let mut target = sample_store();
    let opts = ImportOptions {
        merge: MergeStrategy::SkipDuplicates,
        ..keep_boards()
    };
    let report = import(&mut target, &bytes, &opts);
    assert_eq!((report.items_added, report.items_skipped), (0, 6));
    assert_eq!(
        report.boards_added, 0,
        "an import with nothing new creates no board"
    );
    assert_eq!(counts(&target), counts(&original));

    // One item with the same title but new content is imported; one with new title and old
    // content is skipped.
    let mut changed = sample_store();
    let work = changed.boards().unwrap().remove(0);
    let replies = changed.lists(work.id).unwrap().remove(0);
    let wifi_board = changed.boards().unwrap().remove(1);
    let misc = changed.lists(wifi_board.id).unwrap().remove(0);
    text_item(
        &mut changed,
        replies.id,
        "C third in creation",
        "different body",
    );
    text_item(&mut changed, misc.id, "renamed wifi", "hunter-wifi-pass");
    let bytes = export_bytes(&changed, &ExportOptions::default());
    let report = import(&mut target, &bytes, &opts);
    assert_eq!((report.items_added, report.items_skipped), (1, 7));
    assert_eq!(report.boards_added, 1);
    assert_eq!(counts(&target).2, 7);
}

#[test]
fn skip_duplicates_also_drops_repeats_inside_the_archive() {
    let mut s = store();
    let b = board(&mut s, "B");
    let l = list(&mut s, b.id, "L");
    text_item(&mut s, l.id, "one", "same");
    text_item(&mut s, l.id, "two", "same");
    text_item(&mut s, l.id, "three", "other");
    let bytes = export_bytes(&s, &ExportOptions::default());
    let mut fresh = store();
    let opts = ImportOptions {
        merge: MergeStrategy::SkipDuplicates,
        ..ImportOptions::default()
    };
    let report = import(&mut fresh, &bytes, &opts);
    assert_eq!((report.items_added, report.items_skipped), (2, 1));
}

#[test]
fn free_tier_limit_is_a_warning_only() {
    let bytes = export_bytes(&sample_store(), &ExportOptions::default());
    let mut target = store();
    for name in ["a", "b", "c"] {
        board(&mut target, name);
    }
    let opts = ImportOptions {
        free_board_limit: Some(3),
        ..keep_boards()
    };
    let report = import(&mut target, &bytes, &opts);
    assert_eq!(report.boards_added, 2, "nothing is blocked");
    assert_eq!(target.boards().unwrap().len(), 5);
    assert!(
        report
            .warnings
            .contains(&Warning::BoardLimit { limit: 3, total: 5 })
    );

    // No limit given: no warning.
    let mut other = store();
    let report = import(&mut other, &bytes, &keep_boards());
    assert!(!report.has_warnings());
}

#[test]
fn the_archive_is_a_zip_with_the_documented_entries() {
    let bytes = export_bytes(&sample_store(), &ExportOptions::default());
    let map = entries(&bytes);
    assert!(map.contains_key("manifest.json"));
    assert!(map.contains_key("items.json"));
    let assets: Vec<&String> = map.keys().filter(|k| k.starts_with("assets/")).collect();
    assert_ne!(assets.len(), 0);
    for (name, content) in &map {
        if let Some(hash) = name.strip_prefix("assets/") {
            assert_eq!(hash.len(), 64);
            assert_eq!(
                blake3::hash(content).to_hex().as_str(),
                hash,
                "asset name is its hash"
            );
        }
    }
    let manifest: serde_json::Value = serde_json::from_slice(&map["manifest.json"]).unwrap();
    assert_eq!(manifest["format"], "snipdeck-archive");
    assert_eq!(manifest["format_version"], "1.0");
    assert_eq!(manifest["counts"]["boards"], 2);
    assert_eq!(manifest["counts"]["lists"], 5);
    assert_eq!(manifest["counts"]["items"], 6);
    assert_eq!(
        manifest["counts"]["tags"], 2,
        "the unused tag is not exported"
    );
    assert_eq!(manifest["counts"]["assets"], assets.len());
    assert!(
        manifest["created_at"]
            .as_str()
            .unwrap()
            .starts_with("2027-")
    );
    assert!(manifest.get("encryption").is_none());
}

#[test]
fn identical_payloads_share_one_asset() {
    let mut s = store();
    let b = board(&mut s, "B");
    let l = list(&mut s, b.id, "L");
    for i in 0..5 {
        text_item(&mut s, l.id, &format!("t{i}"), "same text");
    }
    let map = entries(&export_bytes(&s, &ExportOptions::default()));
    assert_eq!(map.keys().filter(|k| k.starts_with("assets/")).count(), 1);
}

#[test]
fn an_empty_library_round_trips() {
    let bytes = export_bytes(&store(), &ExportOptions::default());
    let mut fresh = store();
    let report = import(&mut fresh, &bytes, &ImportOptions::default());
    assert_eq!(
        report.boards_added, 1,
        "the default layout still creates its board"
    );
    assert_eq!(counts(&fresh), (1, 0, 0, 0));
    let mut kept = store();
    import(&mut kept, &bytes, &keep_boards());
    assert_eq!(counts(&kept), (0, 0, 0, 0));
}

#[test]
fn trashed_records_are_not_exported() {
    use snipdeck_core::Timestamp;
    use snipdeck_store::{TrashRepo, Trashable};
    let mut s = sample_store();
    let work = s.boards().unwrap().remove(0);
    let replies = s.lists(work.id).unwrap().remove(0);
    let gone = s.items(replies.id).unwrap().remove(0);
    s.trash(
        Trashable::Item(gone.id),
        Timestamp::from_second(1_800_000_050).unwrap(),
    )
    .unwrap();
    let mut fresh = store();
    import(
        &mut fresh,
        &export_bytes(&s, &ExportOptions::default()),
        &keep_boards(),
    );
    assert_eq!(counts(&fresh).2, 5);
    let titles: Vec<String> = snapshot(&fresh).boards[0].lists[0]
        .items
        .iter()
        .map(|i| i.title.clone())
        .collect();
    assert_eq!(titles, ["B", "A"]);
}

#[test]
fn export_to_path_is_atomic_and_replaces_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.snipdeck");
    std::fs::write(&path, b"old contents").unwrap();
    let s = sample_store();
    snipdeck_portable::export_to_path(&s, at(5), &ExportOptions::default(), &path).unwrap();
    let mut fresh = store();
    snipdeck_portable::import_archive_path(&mut fresh, &path, at(6), &keep_boards()).unwrap();
    assert_same(&snapshot(&fresh), &snapshot(&s));
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(leftovers, [std::ffi::OsString::from("out.snipdeck")]);
    // The default board name comes from the file name.
    let mut named = store();
    snipdeck_portable::import_archive_path(&mut named, &path, at(6), &ImportOptions::default())
        .ok();
    assert!(
        named
            .boards()
            .unwrap()
            .iter()
            .any(|b| b.name.as_str() == "out")
    );
}

/// More distinct assets than a classic zip directory can count (65 535), which makes the writer
/// use zip64 records. Slow in a debug build, so run it on demand:
/// `cargo test -p snipdeck-portable -- --ignored` (about two minutes).
#[test]
#[ignore = "slow: writes 70 000 items"]
fn a_library_with_more_than_65535_assets_round_trips_through_zip64() {
    let mut s = store();
    let b = board(&mut s, "Big");
    let l = list(&mut s, b.id, "Big list");
    for i in 0..70_000 {
        text_item(
            &mut s,
            l.id,
            &format!("item {i}"),
            &format!("body number {i}"),
        );
    }
    let bytes = export_bytes(&s, &ExportOptions::default());
    let mut fresh = store();
    let report = import(&mut fresh, &bytes, &keep_boards());
    assert_eq!(report.items_added, 70_000);
    assert_eq!(counts(&fresh), (1, 1, 70_000, 0));
    assert_healthy(&fresh);
}
