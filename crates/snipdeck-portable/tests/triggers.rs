//! Triggers in `.snipdeck` archives (v1.1 slice 5a): they round-trip, and archives without
//! them still import.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]

mod common;

use std::io::Cursor;

use common::*;
use serde_json::json;
use snipdeck_core::Trigger;
use snipdeck_portable::{
    BoardLayout, ExportOptions, ImportOptions, KeySource, SensitivePolicy, Warning, import_archive,
};
use snipdeck_store::crypto::DataKey;
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, Store, TriggerRepo};

fn keep_boards() -> ImportOptions<'static> {
    ImportOptions {
        layout: BoardLayout::KeepBoards,
        ..ImportOptions::default()
    }
}

fn trig(text: &str) -> Trigger {
    Trigger::new(text).unwrap()
}

/// Work / Replies: A (;a), B (none), C (;c).
fn with_triggers() -> Store {
    let mut s = store();
    let b = board(&mut s, "Work");
    let l = list(&mut s, b.id, "Replies");
    let ids: Vec<_> = ["A", "B", "C"]
        .iter()
        .map(|t| text_item(&mut s, l.id, t, t).id)
        .collect();
    s.set_trigger(ids[0], Some(&trig(";a"))).unwrap();
    s.set_trigger(ids[2], Some(&trig(";c"))).unwrap();
    s
}

/// (title, trigger) of the first list.
fn pairs(s: &Store) -> Vec<(String, Option<String>)> {
    let b = s.boards().unwrap()[0].id;
    let l = s.lists(b).unwrap()[0].id;
    s.items(l)
        .unwrap()
        .into_iter()
        .map(|i| {
            (
                i.title.clone(),
                s.trigger_of(i.id).unwrap().map(|t| t.to_string()),
            )
        })
        .collect()
}

fn expected() -> Vec<(String, Option<String>)> {
    vec![
        ("A".to_owned(), Some(";a".to_owned())),
        ("B".to_owned(), None),
        ("C".to_owned(), Some(";c".to_owned())),
    ]
}

#[test]
fn triggers_round_trip_through_export() {
    let original = with_triggers();
    assert_eq!(pairs(&original), expected());
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
    assert_eq!(pairs(&fresh), expected());
    assert_healthy(&fresh);
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
    assert_eq!(pairs(&third), expected());
}

#[test]
fn an_older_archive_imports_without_triggers() {
    let bytes = export_bytes(&with_triggers(), &ExportOptions::default());
    let old = edit_json(&bytes, "items.json", |doc| {
        for item in doc["boards"][0]["lists"][0]["items"]
            .as_array_mut()
            .unwrap()
        {
            item.as_object_mut().unwrap().remove("trigger");
        }
    });
    let old = edit_json(&old, "manifest.json", |m| {
        m["format_version"] = json!("1.1");
    });
    let mut fresh = store();
    let report =
        import_archive(&mut fresh, Cursor::new(&old), "b", at(200), &keep_boards()).unwrap();
    assert!(!report.has_warnings(), "{:?}", report.warnings);
    assert!(fresh.triggers().unwrap().is_empty());
    assert_eq!(counts(&fresh).2, 3);
}

#[test]
fn a_trigger_already_in_the_library_stays_with_its_owner() {
    let bytes = export_bytes(&with_triggers(), &ExportOptions::default());
    // Import into the library the archive came from: the codes are taken.
    let mut s = with_triggers();
    let report = import_archive(&mut s, Cursor::new(&bytes), "b", at(200), &keep_boards()).unwrap();
    let notes = report
        .warnings
        .iter()
        .filter(|w| matches!(w, Warning::Note(n) if n.contains("already used")))
        .count();
    assert_eq!(notes, 2, "{:?}", report.warnings);
    assert_eq!(s.triggers().unwrap().len(), 2, "the owners kept them");
    assert_eq!(counts(&s).2, 6);
    assert_healthy(&s);
}

#[test]
fn a_bad_trigger_is_refused_and_a_sensitive_one_is_ignored() {
    let bytes = export_bytes(&with_triggers(), &ExportOptions::default());
    let bad = edit_json(&bytes, "items.json", |doc| {
        doc["boards"][0]["lists"][0]["items"][0]["trigger"] = json!("two words");
    });
    let mut s = store();
    assert!(import_archive(&mut s, Cursor::new(&bad), "b", at(1), &keep_boards()).is_err());
    assert_eq!(counts(&s).2, 0, "nothing was written");

    // A sensitive item that arrives with a trigger (hand-edited archive) is imported, with a
    // key, but without the trigger.
    let key = DataKey::generate().unwrap();
    let mut src = with_triggers();
    let b = src.boards().unwrap()[0].id;
    let l = src.lists(b).unwrap()[0].id;
    let mut secret = src.items(l).unwrap()[1].clone(); // B
    secret.sensitive = true;
    secret.preview.clear();
    src.update_item(&secret, None).unwrap();
    let bytes = export_bytes(
        &src,
        &ExportOptions {
            sensitive: SensitivePolicy::Encrypt(&key),
            ..ExportOptions::default()
        },
    );
    let edited = edit_json(&bytes, "items.json", |doc| {
        let items = doc["boards"][0]["lists"][0]["items"]
            .as_array_mut()
            .unwrap();
        let hidden = items
            .iter_mut()
            .find(|i| i["sensitive"] == json!(true))
            .unwrap();
        hidden["trigger"] = json!(";s");
    });
    let mut s = store();
    let opts = ImportOptions {
        layout: BoardLayout::KeepBoards,
        key: KeySource::Given(&key),
        ..ImportOptions::default()
    };
    import_archive(&mut s, Cursor::new(&edited), "b", at(1), &opts).unwrap();
    let b = s.boards().unwrap()[0].id;
    let l = s.lists(b).unwrap()[0].id;
    let items = s.items(l).unwrap();
    let hidden = items
        .iter()
        .find(|i| i.sensitive)
        .expect("the sensitive item came in");
    assert_eq!(s.trigger_of(hidden.id).unwrap(), None);
    assert!(
        s.triggers()
            .unwrap()
            .iter()
            .all(|(t, _)| t.as_str() != ";s")
    );
}
