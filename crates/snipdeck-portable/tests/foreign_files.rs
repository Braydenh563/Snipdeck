//! DATA-9 and DATA-10: what an archive claims about use, time and triggers is not trusted.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]

mod common;

use std::io::Cursor;

use common::*;
use snipdeck_portable::{
    BoardLayout, ExportOptions, ImportOptions, MergeStrategy, Triggers, import_archive,
};
use snipdeck_store::TriggerRepo;

fn archive(trigger: &str) -> Vec<u8> {
    let mut s = store();
    let b = board(&mut s, "B");
    let l = list(&mut s, b.id, "L");
    text_item(&mut s, l.id, "One", "alpha");
    let bytes = export_bytes(&s, &ExportOptions::default());
    edit_json(&bytes, "items.json", |doc| {
        for board in doc["boards"].as_array_mut().unwrap() {
            for list in board["lists"].as_array_mut().unwrap() {
                for item in list["items"].as_array_mut().unwrap() {
                    item["trigger"] = serde_json::json!(trigger);
                    item["use_count"] = serde_json::json!(4_000_000);
                    item["last_used_at"] = serde_json::json!("2200-01-01T00:00:00Z");
                    item["created_at"] = serde_json::json!("2200-01-01T00:00:00Z");
                    item["updated_at"] = serde_json::json!("2200-01-01T00:00:00Z");
                }
            }
        }
    })
}

fn import(bytes: &[u8], options: &ImportOptions<'_>) -> (snipdeck_store::Store, snipdeck_portable::ImportReport) {
    let mut s = store();
    let report = import_archive(&mut s, Cursor::new(bytes.to_vec()), "f", at(10), options).unwrap();
    (s, report)
}

#[test]
fn use_counts_and_future_times_in_a_foreign_file_are_reset() {
    let (s, _) = import(&archive(";one"), &ImportOptions::default());
    let item = first_item(&s);
    assert_eq!(item.use_count, 0);
    assert_eq!(item.last_used_at, None);
    assert!(item.created_at <= at(10), "created {:?}", item.created_at);
    assert!(item.updated_at <= at(10));
}

#[test]
fn a_restore_keeps_use_counts_but_not_the_future() {
    let options = ImportOptions {
        layout: BoardLayout::KeepBoards,
        merge: MergeStrategy::Restore,
        ..ImportOptions::default()
    };
    let (s, _) = import(&archive(";one"), &options);
    let item = first_item(&s);
    assert_eq!(item.use_count, 4_000_000);
    assert!(item.last_used_at.is_none_or(|t| t <= at(10)));
    assert!(item.created_at <= at(10));
}

#[test]
fn a_one_character_trigger_is_dropped_with_a_note() {
    let (s, report) = import(&archive("a"), &ImportOptions::default());
    assert!(s.triggers().unwrap().is_empty());
    assert!(warnings(&report).iter().any(|w| w.contains("too short")), "{:?}", warnings(&report));
}

#[test]
fn triggers_can_be_left_out_and_the_preview_lists_them() {
    let bytes = archive(";one");
    let preview = ImportOptions {
        dry_run: true,
        ..ImportOptions::default()
    };
    let (_, report) = import(&bytes, &preview);
    assert_eq!(report.triggers, vec![";one".to_owned()]);

    let leave = ImportOptions {
        triggers: Triggers::Leave,
        ..ImportOptions::default()
    };
    let (s, _) = import(&bytes, &leave);
    assert!(s.triggers().unwrap().is_empty());
    assert_eq!(first_item(&s).title.as_str(), "One");
}
