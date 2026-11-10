//! Groups in `.snipdeck` archives (v1.1 slice 3a): they round-trip, and archives without them
//! still import.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]

mod common;

use std::io::Cursor;

use common::*;
use serde_json::json;
use snipdeck_core::Group;
use snipdeck_portable::{BoardLayout, ExportOptions, ImportOptions, import_archive};
use snipdeck_store::{BoardRepo, GroupRepo, ItemRepo, ListRepo, Store, TrashRepo, Trashable};

fn keep_boards() -> ImportOptions<'static> {
    ImportOptions {
        layout: BoardLayout::KeepBoards,
        ..ImportOptions::default()
    }
}

/// Work / Replies: A B C D E, "Pair" (B, C, collapsed), "Solo" (E).
fn grouped() -> Store {
    let mut s = store();
    let b = board(&mut s, "Work");
    let l = list(&mut s, b.id, "Replies");
    let ids: Vec<_> = ["A", "B", "C", "D", "E"]
        .iter()
        .map(|t| text_item(&mut s, l.id, t, t).id)
        .collect();
    let mut pair = Group::new(l.id, name("Pair"));
    pair.collapsed = true;
    s.create_group(&pair, &ids[1..3]).unwrap();
    s.create_group(&Group::new(l.id, name("Solo")), &ids[4..])
        .unwrap();
    s
}

/// (group name, collapsed, member titles) of the first list, top to bottom.
fn shape(s: &Store) -> Vec<(String, bool, Vec<String>)> {
    let board = &s.boards().unwrap()[0];
    let list = &s.lists(board.id).unwrap()[0];
    let items = s.items(list.id).unwrap();
    s.groups_of(list.id)
        .unwrap()
        .into_iter()
        .map(|run| {
            let titles = run
                .items
                .iter()
                .map(|id| items.iter().find(|i| i.id == *id).unwrap().title.clone())
                .collect();
            (run.group.name.to_string(), run.group.collapsed, titles)
        })
        .collect()
}

fn expected() -> Vec<(String, bool, Vec<String>)> {
    vec![
        (
            "Pair".to_owned(),
            true,
            vec!["B".to_owned(), "C".to_owned()],
        ),
        ("Solo".to_owned(), false, vec!["E".to_owned()]),
    ]
}

#[test]
fn groups_round_trip_through_export() {
    let original = grouped();
    assert_eq!(shape(&original), expected());
    let bytes = export_bytes(&original, &ExportOptions::default());
    let mut fresh = store();
    let report = import_archive(
        &mut fresh,
        Cursor::new(&bytes),
        "b",
        at(200),
        &keep_boards(),
    )
    .unwrap();
    assert!(!report.has_warnings(), "{:?}", report.warnings);
    assert_eq!(shape(&fresh), expected());
    assert_healthy(&fresh);

    // A second generation is byte-stable in meaning too.
    let again = export_bytes(&fresh, &ExportOptions::default());
    let mut third = store();
    import_archive(
        &mut third,
        Cursor::new(&again),
        "b",
        at(300),
        &keep_boards(),
    )
    .unwrap();
    assert_eq!(shape(&third), expected());
}

#[test]
fn a_group_whose_members_were_all_left_out_is_not_written() {
    let mut s = grouped();
    let b = s.boards().unwrap()[0].id;
    let l = s.lists(b).unwrap()[0].id;
    let all = s.items(l).unwrap();
    let mut secret = all[4].clone(); // E, the only member of Solo
    secret.sensitive = true;
    secret.preview.clear();
    s.update_item(&secret, None).unwrap();
    let bytes = export_bytes(&s, &ExportOptions::default());
    let mut fresh = store();
    import_archive(
        &mut fresh,
        Cursor::new(&bytes),
        "b",
        at(200),
        &keep_boards(),
    )
    .unwrap();
    let got = shape(&fresh);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].0, "Pair");
}

#[test]
fn trashed_items_are_not_exported_with_their_group() {
    let mut s = grouped();
    let b = s.boards().unwrap()[0].id;
    let l = s.lists(b).unwrap()[0].id;
    let c = s.items(l).unwrap()[2].id;
    s.trash(Trashable::Item(c), at(5)).unwrap();
    let bytes = export_bytes(&s, &ExportOptions::default());
    let mut fresh = store();
    import_archive(
        &mut fresh,
        Cursor::new(&bytes),
        "b",
        at(200),
        &keep_boards(),
    )
    .unwrap();
    assert_eq!(shape(&fresh)[0].2, ["B"]);
}

#[test]
fn an_older_archive_imports_without_groups() {
    let bytes = export_bytes(&grouped(), &ExportOptions::default());
    let old = edit_json(&bytes, "items.json", |doc| {
        for board in doc["boards"].as_array_mut().unwrap() {
            for list in board["lists"].as_array_mut().unwrap() {
                list.as_object_mut().unwrap().remove("groups");
                for item in list["items"].as_array_mut().unwrap() {
                    item.as_object_mut().unwrap().remove("group");
                }
            }
        }
    });
    let old = edit_json(&old, "manifest.json", |m| {
        m["format_version"] = json!("1.1");
    });
    let mut fresh = store();
    let report =
        import_archive(&mut fresh, Cursor::new(&old), "b", at(200), &keep_boards()).unwrap();
    assert!(!report.has_warnings(), "{:?}", report.warnings);
    assert!(shape(&fresh).is_empty());
    assert_eq!(counts(&fresh).2, 5);
}

#[test]
fn a_dangling_or_duplicate_group_reference_is_refused() {
    let bytes = export_bytes(&grouped(), &ExportOptions::default());
    let dangling = edit_json(&bytes, "items.json", |doc| {
        doc["boards"][0]["lists"][0]["items"][0]["group"] = json!("not-a-listed-group");
    });
    let mut s = store();
    assert!(import_archive(&mut s, Cursor::new(&dangling), "b", at(1), &keep_boards()).is_err());
    let dup = edit_json(&bytes, "items.json", |doc| {
        let g = doc["boards"][0]["lists"][0]["groups"][0].clone();
        doc["boards"][0]["lists"][0]["groups"]
            .as_array_mut()
            .unwrap()
            .push(g);
    });
    assert!(import_archive(&mut s, Cursor::new(&dup), "b", at(1), &keep_boards()).is_err());
    assert_eq!(counts(&s).2, 0, "nothing was written");
}

/// Members that are not next to each other in an archive are gathered on import (normalised,
/// not refused): the group keeps all its members and the list order otherwise holds.
#[test]
fn non_contiguous_members_are_gathered_on_import() {
    let bytes = export_bytes(&grouped(), &ExportOptions::default());
    let edited = edit_json(&bytes, "items.json", |doc| {
        let list = &mut doc["boards"][0]["lists"][0];
        let pair = list["groups"][0]["id"].clone();
        let solo = list["groups"][1]["id"].clone();
        list["items"][4]["group"] = pair; // B C [D] E
        list["groups"].as_array_mut().unwrap().remove(1);
        let _ = solo;
    });
    let mut fresh = store();
    import_archive(
        &mut fresh,
        Cursor::new(&edited),
        "b",
        at(200),
        &keep_boards(),
    )
    .unwrap();
    let got = shape(&fresh);
    assert_eq!(
        got,
        [(
            "Pair".to_owned(),
            true,
            vec!["B".into(), "C".into(), "E".into()]
        )]
    );
    let b = fresh.boards().unwrap()[0].id;
    let l = fresh.lists(b).unwrap()[0].id;
    let order: Vec<String> = fresh
        .items(l)
        .unwrap()
        .into_iter()
        .map(|i| i.title)
        .collect();
    assert_eq!(order, ["A", "B", "C", "E", "D"]);
    assert_healthy(&fresh);
}
