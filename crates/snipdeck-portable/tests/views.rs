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

/// A three-deep chain of tags, each used by an item on the way down.
fn nested_library() -> Store {
    let mut s = store();
    let b = board(&mut s, "Work");
    let l = list(&mut s, b.id, "Replies");
    let mut it = text_item(&mut s, l.id, "x", "x");
    let top = tag(&mut s, "billing");
    let mut mid = tag(&mut s, "invoices");
    mid.parent = Some(top.id);
    s.update_tag(&mid).unwrap();
    let mut leaf = tag(&mut s, "late");
    leaf.parent = Some(mid.id);
    s.update_tag(&leaf).unwrap();
    it.tags = [leaf.id].into_iter().collect();
    s.update_item(&it, None).unwrap();
    s
}

fn parent_names(s: &Store) -> Vec<(String, Option<String>)> {
    let tags = s.tags().unwrap();
    let mut rows: Vec<(String, Option<String>)> = tags
        .iter()
        .map(|t| {
            let parent = t
                .parent
                .and_then(|p| tags.iter().find(|o| o.id == p))
                .map(|o| o.name.to_string());
            (t.name.to_string(), parent)
        })
        .collect();
    rows.sort();
    rows
}

#[test]
fn a_tags_parent_round_trips_and_the_ancestors_of_a_used_tag_travel_with_it() {
    let s = nested_library();
    let bytes = export_bytes(&s, &ExportOptions::default());
    let fresh = read_back(&bytes);
    assert_eq!(
        parent_names(&fresh),
        vec![
            ("billing".to_owned(), None),
            ("invoices".to_owned(), Some("billing".to_owned())),
            ("late".to_owned(), Some("invoices".to_owned())),
        ]
    );
}

#[test]
fn a_parent_the_file_does_not_hold_is_refused() {
    let bytes = export_bytes(&nested_library(), &ExportOptions::default());
    let tampered = edit_json(&bytes, "items.json", |d| {
        d["tags"][0]["parent"] = serde_json::json!("nobody");
    });
    let mut fresh = store();
    let result = import_archive(
        &mut fresh,
        Cursor::new(tampered),
        "x",
        at(1),
        &keep_boards(),
    );
    assert!(result.is_err(), "an unknown parent must be refused");
}

#[test]
fn a_loop_of_parents_is_refused() {
    let bytes = export_bytes(&nested_library(), &ExportOptions::default());
    let tampered = edit_json(&bytes, "items.json", |d| {
        let ids: Vec<serde_json::Value> = d["tags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].clone())
            .collect();
        let n = ids.len();
        for i in 0..n {
            d["tags"][i]["parent"] = ids[(i + 1) % n].clone();
        }
    });
    let mut fresh = store();
    let result = import_archive(
        &mut fresh,
        Cursor::new(tampered),
        "x",
        at(1),
        &keep_boards(),
    );
    assert!(result.is_err(), "a loop must be refused");
}

#[test]
fn an_archive_without_parents_imports_flat() {
    let bytes = export_bytes(&nested_library(), &ExportOptions::default());
    let flat = edit_json(&bytes, "items.json", |d| {
        for t in d["tags"].as_array_mut().unwrap() {
            t.as_object_mut().unwrap().remove("parent");
        }
    });
    let fresh = read_back(&flat);
    let rows = parent_names(&fresh);
    // Only the tag the item uses is in the file once the ancestors' links are gone.
    assert_eq!(rows, vec![("late".to_owned(), None)]);
}
