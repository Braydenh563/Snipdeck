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
use snipdeck_core::{Group, ListAccent};
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

/// The colours of the first list's groups, top to bottom.
fn colours(s: &Store) -> Vec<Option<ListAccent>> {
    let board = &s.boards().unwrap()[0];
    let list = &s.lists(board.id).unwrap()[0];
    s.groups_of(list.id)
        .unwrap()
        .into_iter()
        .map(|r| r.group.colour)
        .collect()
}

#[test]
fn a_groups_colour_round_trips_and_an_unknown_key_is_refused() {
    let mut s = grouped();
    let b = s.boards().unwrap()[0].id;
    let l = s.lists(b).unwrap()[0].id;
    let mut pair = s.groups_of(l).unwrap().remove(0).group;
    pair.colour = Some(ListAccent::Rose);
    s.update_group(&pair).unwrap();
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
    assert_eq!(colours(&fresh), [Some(ListAccent::Rose), None]);

    // An archive without the key (1.2 and older) imports with no colour.
    let plain = export_bytes(&grouped(), &ExportOptions::default());
    let items = edit_json(&plain, "items.json", |doc| {
        let first = &doc["boards"][0]["lists"][0]["groups"][0];
        assert_eq!(first.get("colour"), None, "no colour is not written");
    });
    let mut other = store();
    import_archive(
        &mut other,
        Cursor::new(&items),
        "b",
        at(200),
        &keep_boards(),
    )
    .unwrap();
    assert_eq!(colours(&other), [None, None]);

    let bad = edit_json(&bytes, "items.json", |doc| {
        doc["boards"][0]["lists"][0]["groups"][0]["colour"] = json!("pink");
    });
    let mut s2 = store();
    assert!(import_archive(&mut s2, Cursor::new(&bad), "b", at(1), &keep_boards()).is_err());
    assert_eq!(counts(&s2).2, 0, "nothing was written");
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
    assert_eq!(
        shape(&fresh),
        [] as [(
            std::string::String,
            bool,
            std::vec::Vec<std::string::String>
        ); 0]
    );
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

/// Work / Replies: A B C D E, "Outer" (A B C D) with "Inner" (B C) in it, "Solo" (E).
fn nested_groups() -> Store {
    let mut s = store();
    let b = board(&mut s, "Work");
    let l = list(&mut s, b.id, "Replies");
    let ids: Vec<_> = ["A", "B", "C", "D", "E"]
        .iter()
        .map(|t| text_item(&mut s, l.id, t, t).id)
        .collect();
    let outer = Group::new(l.id, name("Outer"));
    s.create_group(&outer, &ids[..4]).unwrap();
    let inner = Group {
        parent: Some(outer.id),
        ..Group::new(l.id, name("Inner"))
    };
    s.create_group(&inner, &ids[1..3]).unwrap();
    s.create_group(&Group::new(l.id, name("Solo")), &ids[4..])
        .unwrap();
    s
}

/// (name, depth, whole run as titles) of the first list's groups.
fn nesting(s: &Store) -> Vec<(String, usize, Vec<String>)> {
    let board = &s.boards().unwrap()[0];
    let list = &s.lists(board.id).unwrap()[0];
    let items = s.items(list.id).unwrap();
    s.groups_of(list.id)
        .unwrap()
        .into_iter()
        .map(|run| {
            let titles = run
                .run
                .iter()
                .map(|id| items.iter().find(|i| i.id == *id).unwrap().title.clone())
                .collect();
            (run.group.name.to_string(), run.depth, titles)
        })
        .collect()
}

fn nesting_expected() -> Vec<(String, usize, Vec<String>)> {
    let t = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
    vec![
        ("Outer".to_owned(), 0, t(&["A", "B", "C", "D"])),
        ("Inner".to_owned(), 1, t(&["B", "C"])),
        ("Solo".to_owned(), 0, t(&["E"])),
    ]
}

#[test]
fn nested_groups_round_trip_with_their_parent() {
    let original = nested_groups();
    assert_eq!(nesting(&original), nesting_expected());
    let bytes = export_bytes(&original, &ExportOptions::default());
    let doc = edit_json(&bytes, "items.json", |doc| {
        let groups = &doc["boards"][0]["lists"][0]["groups"];
        assert_eq!(
            groups[0].get("parent"),
            None,
            "a top-level group writes none"
        );
        assert_eq!(groups[1]["parent"], groups[0]["id"]);
    });
    let mut fresh = store();
    let report =
        import_archive(&mut fresh, Cursor::new(&doc), "b", at(200), &keep_boards()).unwrap();
    assert!(!report.has_warnings(), "{:?}", report.warnings);
    assert_eq!(nesting(&fresh), nesting_expected());
    assert_healthy(&fresh);
}

#[test]
fn a_parent_with_no_member_of_its_own_and_one_whose_children_were_left_out_both_travel() {
    let mut s = nested_groups();
    let b = s.boards().unwrap()[0].id;
    let l = s.lists(b).unwrap()[0].id;
    let items = s.items(l).unwrap();
    // A and D leave Outer: only Inner's B and C remain in it.
    s.set_item_group(items[0].id, None).unwrap();
    s.set_item_group(items[3].id, None).unwrap();
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
    let got = nesting(&fresh);
    assert_eq!(
        got[0],
        ("Outer".to_owned(), 0, vec!["B".to_owned(), "C".to_owned()])
    );
    assert_eq!(got[1].1, 1);
    // A sensitive B and C are left out: both groups have nothing to write.
    for i in [1, 2] {
        let mut secret = items[i].clone();
        secret.sensitive = true;
        secret.preview.clear();
        s.update_item(&secret, None).unwrap();
    }
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
    assert_eq!(nesting(&fresh).len(), 1, "only Solo is left");
}

#[test]
fn an_archive_without_parents_imports_flat_and_a_bad_parent_is_refused() {
    let bytes = export_bytes(&nested_groups(), &ExportOptions::default());
    let flat = edit_json(&bytes, "items.json", |doc| {
        for g in doc["boards"][0]["lists"][0]["groups"]
            .as_array_mut()
            .unwrap()
        {
            g.as_object_mut().unwrap().remove("parent");
        }
    });
    let mut s = store();
    import_archive(&mut s, Cursor::new(&flat), "b", at(1), &keep_boards()).unwrap();
    let got = nesting(&s);
    assert_eq!(got.len(), 3);
    assert_eq!(
        got.iter().map(|g| g.1).sum::<usize>(),
        0,
        "every group is top level"
    );
    assert_healthy(&s);

    for bad in [
        // a parent the list does not hold
        json!("0198f3b0-0000-7000-8000-000000000001"),
        // the group in itself
        json!("self"),
        // a group in the group listed after it
        json!("later"),
    ] {
        let doc = edit_json(&bytes, "items.json", |doc| {
            let groups = doc["boards"][0]["lists"][0]["groups"]
                .as_array_mut()
                .unwrap();
            let value = match bad.as_str().unwrap() {
                "self" => groups[0]["id"].clone(),
                "later" => groups[1]["id"].clone(),
                other => json!(other),
            };
            groups[0]["parent"] = value;
        });
        let mut s = store();
        assert!(import_archive(&mut s, Cursor::new(&doc), "b", at(1), &keep_boards()).is_err());
        assert_eq!(counts(&s).2, 0, "nothing was written");
    }
}

#[test]
fn groups_nested_deeper_than_three_levels_are_refused() {
    let bytes = export_bytes(&nested_groups(), &ExportOptions::default());
    let doc = edit_json(&bytes, "items.json", |doc| {
        let groups = doc["boards"][0]["lists"][0]["groups"]
            .as_array_mut()
            .unwrap();
        let (outer, inner) = (groups[0]["id"].clone(), groups[1]["id"].clone());
        let mut third = groups[1].clone();
        third["id"] = json!("0198f3b0-0000-7000-8000-0000000000a3");
        third["parent"] = inner;
        let mut fourth = third.clone();
        fourth["id"] = json!("0198f3b0-0000-7000-8000-0000000000a4");
        fourth["parent"] = third["id"].clone();
        let _ = outer;
        groups.push(third);
        groups.push(fourth);
    });
    let mut s = store();
    assert!(import_archive(&mut s, Cursor::new(&doc), "b", at(1), &keep_boards()).is_err());
}
