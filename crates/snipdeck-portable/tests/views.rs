//! What the library views added to `.snipdeck` archives (format 1.3): a tag's colour, a board's
//! accent and icon, and the longer list sort words. Older archives, without them, still import.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]

mod common;

use std::io::Cursor;

use common::*;
use snipdeck_core::{ItemLabel, ListAccent, ListIcon, Sort, SortKey};
use snipdeck_portable::{BoardLayout, ExportOptions, ImportOptions, import_archive};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, Store, TagRepo};

fn keep_boards() -> ImportOptions<'static> {
    ImportOptions {
        layout: BoardLayout::KeepBoards,
        ..ImportOptions::default()
    }
}

fn read_back(bytes: &[u8]) -> Store {
    let mut fresh = store();
    import_archive(
        &mut fresh,
        Cursor::new(bytes),
        "backup",
        at(200),
        &keep_boards(),
    )
    .unwrap();
    fresh
}

#[test]
fn tag_colours_board_looks_and_every_sort_round_trip() {
    let mut s = store();
    let mut work = board(&mut s, "Work");
    work.accent = Some(ListAccent::Violet);
    work.icon = Some(ListIcon::Star);
    s.update_board(&work).unwrap();
    let mut billing = tag(&mut s, "billing");
    billing.colour = Some(ItemLabel::Orange);
    s.update_tag(&billing).unwrap();
    let plain = tag(&mut s, "plain");
    let mut sorts = Vec::new();
    for (i, key) in SortKey::ALL.iter().enumerate() {
        let mut l = list(&mut s, work.id, &format!("List {i}"));
        l.sort = Sort::new(*key, i % 2 == 1);
        s.update_list(&l).unwrap();
        sorts.push(l.sort);
        let mut it = text_item(&mut s, l.id, "x", "x");
        it.tags = [billing.id, plain.id].into_iter().collect();
        s.update_item(&it, None).unwrap();
    }

    let fresh = read_back(&export_bytes(&s, &ExportOptions::default()));

    let b = &fresh.boards().unwrap()[0];
    assert_eq!(
        (b.accent, b.icon),
        (Some(ListAccent::Violet), Some(ListIcon::Star))
    );
    let tags = fresh.tags().unwrap();
    let colour = |n: &str| tags.iter().find(|t| t.name.as_str() == n).unwrap().colour;
    assert_eq!(colour("billing"), Some(ItemLabel::Orange));
    assert_eq!(colour("plain"), None);
    let got: Vec<Sort> = fresh.lists(b.id).unwrap().iter().map(|l| l.sort).collect();
    assert_eq!(got, sorts);
}

#[test]
fn an_older_archive_without_the_new_fields_imports_with_none_of_them() {
    let mut s = store();
    let b = board(&mut s, "Work");
    list(&mut s, b.id, "Replies");
    let bytes = export_bytes(&s, &ExportOptions::default());
    // The writer leaves the new fields out when they are empty, as an older writer would.
    let text = String::from_utf8_lossy(&bytes);
    for word in ["\"colour\"", "\"accent\":\"", "\"icon\":\""] {
        assert!(!text.contains(word), "{word} absent by default");
    }
    let fresh = read_back(&bytes);
    let board = &fresh.boards().unwrap()[0];
    assert_eq!((board.accent, board.icon), (None, None));
    assert_eq!(fresh.lists(board.id).unwrap()[0].sort, Sort::MANUAL);
}

#[test]
fn an_unknown_tag_colour_or_sort_word_is_refused_as_invalid() {
    let mut s = store();
    let b = board(&mut s, "Work");
    let l = list(&mut s, b.id, "Replies");
    let mut it = text_item(&mut s, l.id, "x", "x");
    let t = tag(&mut s, "billing");
    it.tags = [t.id].into_iter().collect();
    s.update_item(&it, None).unwrap();
    let bytes = export_bytes(&s, &ExportOptions::default());
    let bad = |edit: fn(&mut serde_json::Value)| {
        let tampered = rewrite(&bytes, |map| {
            let mut doc: serde_json::Value = serde_json::from_slice(&map["items.json"]).unwrap();
            edit(&mut doc);
            map.insert("items.json".into(), serde_json::to_vec(&doc).unwrap());
        });
        let mut fresh = store();
        import_archive(
            &mut fresh,
            Cursor::new(tampered),
            "x",
            at(1),
            &keep_boards(),
        )
        .is_err()
    };
    assert!(bad(|d| d["tags"][0]["colour"] = serde_json::json!("pink")));
    assert!(bad(
        |d| d["boards"][0]["lists"][0]["sort"] = serde_json::json!("random")
    ));
    assert!(bad(|d| d["boards"][0]["icon"] = serde_json::json!("nope")));
    assert!(bad(
        |d| d["boards"][0]["lists"][0]["sort"] = serde_json::json!("manual-reversed")
    ));
    assert!(!bad(|d| d["tags"][0]["colour"] = serde_json::json!("teal")));
}
