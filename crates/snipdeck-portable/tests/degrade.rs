//! A backup degrades instead of stopping (DATA-4): an item whose attachment cannot be read is
//! left out, named in the report and the manifest, and everything else is written.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]

mod common;

use std::fs;
use std::path::Path;

use common::*;
use snipdeck_core::{Format, Formats, Item, ItemKind};
use snipdeck_portable::{ExportOptions, export_to_writer};
use snipdeck_store::{BlobStore, ItemRepo, Store};

fn blob_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.join("blobs")];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                found.push(p);
            }
        }
    }
    found
}

/// A library of three text items and one picture whose blob file is then deleted.
fn damaged() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(dir.path()).unwrap();
    let b = board(&mut s, "Work");
    let l = list(&mut s, b.id, "Replies");
    text_item(&mut s, l.id, "one", "aaa");
    text_item(&mut s, l.id, "two", "bbb");
    let mut pic = Item::new(l.id, "Screenshot".to_owned(), ItemKind::Image, at(12));
    let png = s.put(noise(80_000, 7)).unwrap();
    let alt = inline("alt text");
    s.insert_item(
        &pic,
        &Formats::new(vec![Format::Text(alt), Format::Png(png)]).unwrap(),
        snipdeck_core::Placement::Last,
    )
    .unwrap();
    pic.title.clear();
    text_item(&mut s, l.id, "three", "ccc");
    let files = blob_files(dir.path());
    assert_eq!(files.len(), 1, "one attachment file");
    fs::remove_file(&files[0]).unwrap();
    (dir, s)
}

#[test]
fn an_unreadable_attachment_leaves_only_its_item_out_and_names_it() {
    let (_dir, s) = damaged();
    let mut out = std::io::Cursor::new(Vec::new());
    let report = export_to_writer(&s, at(0), &ExportOptions::default(), &mut out).unwrap();
    assert_eq!(report.items, 3, "the healthy items are written");
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].title, "Screenshot");
    assert_eq!(report.skipped[0].list, "Replies");

    let parts = entries(&out.into_inner());
    let manifest: serde_json::Value = serde_json::from_slice(&parts["manifest.json"]).unwrap();
    assert_eq!(manifest["skipped_unreadable"], 1);
    assert_eq!(manifest["counts"]["items"], 3);
    // The skipped item's readable text format was not written as an orphan asset.
    assert_eq!(manifest["counts"]["assets"], 3);
    let assets: Vec<_> = parts
        .iter()
        .filter(|(k, _)| k.starts_with("assets/"))
        .collect();
    assert_eq!(assets.len(), 3);
    assert!(assets.iter().all(|(_, v)| v.as_slice() != b"alt text"));
}

#[test]
fn the_degraded_archive_imports_with_a_note() {
    let (_dir, s) = damaged();
    let mut out = std::io::Cursor::new(Vec::new());
    export_to_writer(&s, at(0), &ExportOptions::default(), &mut out).unwrap();
    let mut target = store();
    let report = snipdeck_portable::import_archive(
        &mut target,
        std::io::Cursor::new(out.into_inner()),
        "test.snipdeck",
        at(5),
        &snipdeck_portable::ImportOptions::default(),
    )
    .unwrap();
    assert_eq!(report.items_added, 3);
    assert!(
        warnings(&report).iter().any(|w| w.contains("unreadable")),
        "{:?}",
        warnings(&report)
    );
}

#[test]
fn check_reads_a_good_archive_back_and_finds_a_damaged_one() {
    use snipdeck_portable::{Depth, check_archive_path};
    let s = sample_store();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.snipdeck");
    snipdeck_portable::export_to_path(&s, at(0), &ExportOptions::default(), &path).unwrap();
    let summary = check_archive_path(&path, Depth::Everything).unwrap();
    assert_eq!((summary.boards, summary.lists), (2, 5));
    assert!(summary.items >= 6, "{summary:?}");

    // Cut off: the zip directory is gone.
    let bytes = fs::read(&path).unwrap();
    let cut = dir.path().join("cut.snipdeck");
    fs::write(&cut, &bytes[..bytes.len() / 2]).unwrap();
    assert!(check_archive_path(&cut, Depth::Counts).is_err());
    // Empty file.
    let empty = dir.path().join("empty.snipdeck");
    fs::write(&empty, b"").unwrap();
    assert!(check_archive_path(&empty, Depth::Counts).is_err());
    // Counts that disagree with the contents.
    let lying = rewrite(&bytes, |e| {
        let mut m: serde_json::Value = serde_json::from_slice(&e["manifest.json"]).unwrap();
        m["counts"]["items"] = 99.into();
        e.insert("manifest.json".to_owned(), serde_json::to_vec(&m).unwrap());
    });
    let lie = dir.path().join("lie.snipdeck");
    fs::write(&lie, lying).unwrap();
    assert!(check_archive_path(&lie, Depth::Counts).is_err());
}
