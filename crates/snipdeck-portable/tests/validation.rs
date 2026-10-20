//! A bad archive is refused with a specific error and changes nothing.
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
use serde_json::{Value, json};
use snipdeck_portable::{
    BoardLayout, Error, ExportOptions, ImportOptions, Warning, import_archive,
};
use snipdeck_store::{ItemRepo, Store};

fn valid() -> Vec<u8> {
    export_bytes(&sample_store(), &ExportOptions::default())
}

fn opts() -> ImportOptions<'static> {
    ImportOptions {
        layout: BoardLayout::KeepBoards,
        ..ImportOptions::default()
    }
}

/// A target library that already has data, so "changes nothing" is checkable.
fn target() -> Store {
    let mut s = sample_store();
    let tag = tag(&mut s, "pre-existing");
    let _ = tag;
    s
}

/// Imports `bytes` and demands an error and an untouched library.
fn rejected(bytes: &[u8]) -> Error {
    let mut s = target();
    let before = snapshot(&s);
    let before_counts = counts(&s);
    let err = import_archive(&mut s, Cursor::new(bytes), "x", at(1), &opts()).unwrap_err();
    assert_same(&snapshot(&s), &before);
    assert_eq!(counts(&s), before_counts);
    assert_healthy(&s);
    assert_trash_empty(&s);
    err
}

fn items_edit(edit: impl FnOnce(&mut Value)) -> Vec<u8> {
    edit_json(&valid(), "items.json", edit)
}

fn manifest_edit(edit: impl FnOnce(&mut Value)) -> Vec<u8> {
    edit_json(&valid(), "manifest.json", edit)
}

fn first_item(doc: &mut Value) -> &mut Value {
    &mut doc["boards"][0]["lists"][0]["items"][0]
}

#[test]
fn the_unmodified_archive_is_accepted() {
    let mut s = store();
    import_archive(&mut s, Cursor::new(valid()), "x", at(1), &opts()).unwrap();
}

// ---- manifest and versions ----

#[test]
fn a_newer_major_version_is_refused() {
    let err = rejected(&manifest_edit(|m| m["format_version"] = json!("2.0")));
    assert!(
        matches!(
            err,
            Error::FormatTooNew {
                supported_major: 1,
                ..
            }
        ),
        "{err}"
    );
    assert!(err.to_string().contains("update Snipdeck"));
}

#[test]
fn a_newer_major_is_refused_even_if_everything_else_changed_shape() {
    // The v2 body is unreadable to us; the version check must come first.
    let bytes = rewrite(&valid(), |map| {
        map.insert(
            "manifest.json".to_owned(),
            br#"{"format":"snipdeck-archive","format_version":"2.3","whatever":[1,2]}"#.to_vec(),
        );
        map.insert(
            "items.json".to_owned(),
            b"{\"completely\":\"different\"}".to_vec(),
        );
    });
    assert!(matches!(rejected(&bytes), Error::FormatTooNew { .. }));
}

#[test]
fn unusable_versions_and_formats_are_refused() {
    for version in ["0.9", "banana", "1", "1.x", "-1.0", ""] {
        let err = rejected(&manifest_edit(|m| m["format_version"] = json!(version)));
        assert!(
            matches!(err, Error::UnsupportedFormat(_)),
            "{version}: {err}"
        );
    }
    let err = rejected(&manifest_edit(|m| m["format"] = json!("other-app")));
    assert!(matches!(err, Error::UnsupportedFormat(_)), "{err}");
    let err = rejected(&manifest_edit(|m| {
        m.as_object_mut().unwrap().remove("format");
    }));
    assert!(matches!(err, Error::UnsupportedFormat(_)), "{err}");
}

#[test]
fn a_newer_minor_version_and_unknown_fields_are_tolerated() {
    let bytes = rewrite(&valid(), |map| {
        let mut manifest: Value = serde_json::from_slice(&map["manifest.json"]).unwrap();
        manifest["format_version"] = json!("1.9");
        manifest["future_field"] = json!({"a": [1, 2, 3]});
        manifest["counts"]["thumbnails"] = json!(4);
        map.insert(
            "manifest.json".into(),
            serde_json::to_vec(&manifest).unwrap(),
        );

        let mut doc: Value = serde_json::from_slice(&map["items.json"]).unwrap();
        doc["generator"] = json!("someone else");
        doc["boards"][0]["colour"] = json!("pink");
        doc["boards"][0]["lists"][0]["items"][0]["rating"] = json!(5);
        doc["boards"][0]["lists"][0]["items"][0]["formats"][0]["dpi"] = json!(96);
        map.insert("items.json".into(), serde_json::to_vec(&doc).unwrap());

        map.insert("thumbnails/readme.txt".into(), b"hi".to_vec());
    });
    let mut s = store();
    let report = import_archive(&mut s, Cursor::new(bytes), "x", at(1), &opts()).unwrap();
    assert_eq!(report.items_added, 6);
    assert!(report.warnings.contains(&Warning::NewerMinorVersion {
        found: "1.9".into()
    }));
    assert!(
        report
            .warnings
            .contains(&Warning::UnknownEntries { count: 1 })
    );
    assert_healthy(&s);
}

#[test]
fn missing_manifest_or_items_is_refused() {
    let no_manifest = rewrite(&valid(), |m| {
        m.remove("manifest.json");
    });
    assert!(matches!(
        rejected(&no_manifest),
        Error::MissingEntry("manifest.json")
    ));
    let no_items = rewrite(&valid(), |m| {
        m.remove("items.json");
    });
    assert!(matches!(
        rejected(&no_items),
        Error::MissingEntry("items.json")
    ));
}

#[test]
fn broken_json_is_malformed() {
    for broken in [
        &b"{"[..],
        b"",
        b"null",
        b"[]",
        b"{\"boards\": 3}",
        &[0xff, 0xfe][..],
    ] {
        let bytes = rewrite(&valid(), |m| {
            m.insert("items.json".into(), broken.to_vec());
        });
        let err = rejected(&bytes);
        assert!(matches!(err, Error::Malformed { .. }), "{broken:?}: {err}");
    }
    let bytes = rewrite(&valid(), |m| {
        m.insert("manifest.json".into(), b"not json".to_vec());
    });
    assert!(matches!(rejected(&bytes), Error::Malformed { .. }));
}

#[test]
fn absurdly_deep_json_is_an_error_not_a_stack_overflow() {
    let deep = format!("{}{}", "[".repeat(200_000), "]".repeat(200_000));
    let bytes = rewrite(&valid(), |m| {
        m.insert("items.json".into(), deep.into_bytes());
    });
    assert!(matches!(rejected(&bytes), Error::Malformed { .. }));
}

// ---- counts ----

#[test]
fn counts_must_match_the_data() {
    for (field, bump) in [
        ("boards", 1),
        ("lists", 1),
        ("items", 1),
        ("tags", 1),
        ("assets", 1),
    ] {
        let bytes = manifest_edit(|m| {
            let n = m["counts"][field].as_u64().unwrap();
            m["counts"][field] = json!(n + bump);
        });
        let err = rejected(&bytes);
        assert!(
            matches!(err, Error::CountMismatch { what, .. } if what == field),
            "{field}: {err}"
        );
    }
    let bytes = manifest_edit(|m| m["counts"]["items"] = json!(0));
    assert!(matches!(rejected(&bytes), Error::CountMismatch { .. }));
}

#[test]
fn more_items_than_the_limit_is_refused_before_anything_else() {
    let limits = snipdeck_portable::Limits {
        max_items: 3,
        ..snipdeck_portable::Limits::default()
    };
    let mut s = store();
    let o = ImportOptions { limits, ..opts() };
    let err = import_archive(&mut s, Cursor::new(valid()), "x", at(1), &o).unwrap_err();
    assert!(
        matches!(
            err,
            Error::LimitExceeded {
                what: "item count",
                ..
            }
        ),
        "{err}"
    );
    assert_eq!(counts(&s), (0, 0, 0, 0));
}

// ---- ids ----

#[test]
fn malformed_ids_are_refused() {
    for bad in [
        "",
        "not-a-uuid",
        "12345",
        "{}",
        "018f3a5200007000800000000000000a",
        "{018f3a52-0000-7000-8000-00000000000a}",
        "urn:uuid:018f3a52-0000-7000-8000-00000000000a",
    ] {
        let e = rejected(&items_edit(|d| d["boards"][0]["id"] = json!(bad)));
        assert!(matches!(e, Error::Malformed { .. }), "board {bad:?}: {e}");
        let e = rejected(&items_edit(|d| {
            d["boards"][0]["lists"][0]["id"] = json!(bad);
        }));
        assert!(matches!(e, Error::Malformed { .. }), "list {bad:?}: {e}");
        let e = rejected(&items_edit(|d| first_item(d)["id"] = json!(bad)));
        assert!(matches!(e, Error::Malformed { .. }), "item {bad:?}: {e}");
        let e = rejected(&items_edit(|d| d["tags"][0]["id"] = json!(bad)));
        assert!(matches!(e, Error::Malformed { .. }), "tag {bad:?}: {e}");
    }
    let e = rejected(&items_edit(|d| d["boards"][0]["id"] = json!(42)));
    assert!(matches!(e, Error::Malformed { .. }), "{e}");
}

#[test]
fn duplicate_ids_are_refused() {
    let e = rejected(&items_edit(|d| {
        let id = d["boards"][0]["id"].clone();
        d["boards"][1]["id"] = id;
    }));
    assert!(matches!(e, Error::DuplicateId { kind: "board", .. }), "{e}");
    let e = rejected(&items_edit(|d| {
        let id = d["boards"][0]["lists"][0]["id"].clone();
        d["boards"][0]["lists"][1]["id"] = id;
    }));
    assert!(matches!(e, Error::DuplicateId { kind: "list", .. }), "{e}");
    let e = rejected(&items_edit(|d| {
        let id = first_item(d)["id"].clone();
        d["boards"][0]["lists"][0]["items"][1]["id"] = id;
    }));
    assert!(matches!(e, Error::DuplicateId { kind: "item", .. }), "{e}");
    let e = rejected(&items_edit(|d| {
        let id = d["tags"][0]["id"].clone();
        d["tags"][1]["id"] = id;
    }));
    assert!(matches!(e, Error::DuplicateId { kind: "tag", .. }), "{e}");
    // Ids are compared as UUIDs, not as text: upper case is the same id.
    let e = rejected(&items_edit(|d| {
        let id = d["boards"][0]["id"].as_str().unwrap().to_uppercase();
        d["boards"][1]["id"] = json!(id);
    }));
    assert!(matches!(e, Error::DuplicateId { kind: "board", .. }), "{e}");
}

#[test]
fn a_reference_to_an_unknown_tag_is_refused() {
    let e = rejected(&items_edit(|d| {
        first_item(d)["tags"] = json!(["018f3a52-0000-7000-8000-000000000000"]);
    }));
    assert!(matches!(e, Error::Malformed { .. }), "{e}");
}

// ---- values ----

#[test]
fn bad_timestamps_are_refused() {
    for bad in [
        "yesterday",
        "",
        "2027-13-45T00:00:00Z",
        "9999-01-01T00:00:00Z",
        "1000-01-01T00:00:00Z",
    ] {
        let e = rejected(&items_edit(|d| first_item(d)["created_at"] = json!(bad)));
        assert!(matches!(e, Error::Malformed { .. }), "{bad:?}: {e}");
    }
    let e = rejected(&items_edit(|d| {
        first_item(d)["last_used_at"] = json!("soon");
    }));
    assert!(matches!(e, Error::Malformed { .. }), "{e}");
    let e = rejected(&items_edit(|d| d["boards"][0]["created_at"] = json!(5)));
    assert!(matches!(e, Error::Malformed { .. }), "{e}");
}

#[test]
fn unknown_keys_are_refused() {
    let e = rejected(&items_edit(|d| first_item(d)["kind"] = json!("hologram")));
    assert!(matches!(e, Error::Malformed { .. }), "{e}");
    let e = rejected(&items_edit(|d| {
        d["boards"][0]["lists"][0]["accent"] = json!("plaid");
    }));
    assert!(matches!(e, Error::Malformed { .. }), "{e}");
    let e = rejected(&items_edit(|d| {
        d["boards"][0]["lists"][0]["icon"] = json!("sparkle");
    }));
    assert!(matches!(e, Error::Malformed { .. }), "{e}");
    let e = rejected(&items_edit(|d| first_item(d)["label"] = json!("plaid")));
    assert!(matches!(e, Error::Malformed { .. }), "{e}");
    let e = rejected(&items_edit(|d| {
        d["boards"][0]["lists"][0]["view"] = json!("huge");
    }));
    assert!(matches!(e, Error::Malformed { .. }), "{e}");
    let e = rejected(&items_edit(|d| {
        d["boards"][0]["lists"][0]["sort"] = json!("alphabetical");
    }));
    assert!(matches!(e, Error::Malformed { .. }), "{e}");
    let e = rejected(&items_edit(|d| {
        first_item(d)["formats"][0]["kind"] = json!("video/mp4");
    }));
    assert!(matches!(e, Error::Malformed { .. }), "{e}");
}

#[test]
fn bad_names_are_refused() {
    for bad in ["", "   ", "line\nbreak", &"x".repeat(101)] {
        let e = rejected(&items_edit(|d| d["boards"][0]["name"] = json!(bad)));
        assert!(matches!(e, Error::Malformed { .. }), "board {bad:?}: {e}");
        let e = rejected(&items_edit(|d| {
            d["boards"][0]["lists"][0]["name"] = json!(bad);
        }));
        assert!(matches!(e, Error::Malformed { .. }), "list {bad:?}: {e}");
        let e = rejected(&items_edit(|d| d["tags"][0]["name"] = json!(bad)));
        assert!(matches!(e, Error::Malformed { .. }), "tag {bad:?}: {e}");
    }
}

#[test]
fn wrong_types_and_out_of_range_numbers_are_refused() {
    for value in [
        json!(-1),
        json!(4_294_967_296_u64),
        json!("5"),
        json!(1.5),
        json!(null),
    ] {
        let e = rejected(&items_edit(|d| first_item(d)["use_count"] = value.clone()));
        assert!(matches!(e, Error::Malformed { .. }), "{value}: {e}");
    }
    let e = rejected(&items_edit(|d| first_item(d)["pinned"] = json!("yes")));
    assert!(matches!(e, Error::Malformed { .. }), "{e}");
}

// ---- formats and assets ----

#[test]
fn item_format_structure_is_checked() {
    let e = rejected(&items_edit(|d| first_item(d)["formats"] = json!([])));
    assert!(matches!(e, Error::Malformed { .. }), "no formats: {e}");

    let e = rejected(&items_edit(|d| {
        let f = first_item(d)["formats"][0].clone();
        first_item(d)["formats"] = json!([f.clone(), f]);
    }));
    assert!(
        matches!(e, Error::Malformed { .. }),
        "duplicate format: {e}"
    );

    let e = rejected(&items_edit(|d| {
        first_item(d)["formats"][0]
            .as_object_mut()
            .unwrap()
            .remove("asset");
    }));
    assert!(matches!(e, Error::Malformed { .. }), "no asset: {e}");

    let e = rejected(&items_edit(|d| {
        first_item(d)["formats"][0]
            .as_object_mut()
            .unwrap()
            .remove("size");
    }));
    assert!(matches!(e, Error::Malformed { .. }), "no size: {e}");
}

fn files_item(doc: &mut Value) -> &mut Value {
    // The sample's "Two files" item is first in the Files list.
    &mut doc["boards"][0]["lists"][1]["items"][0]
}

#[test]
fn files_formats_are_checked() {
    let e = rejected(&items_edit(|d| {
        files_item(d)["formats"][0]["files"] = json!([]);
    }));
    assert!(matches!(e, Error::Malformed { .. }), "empty files: {e}");
    for bad in [
        "../evil",
        "a/b",
        "a\\b",
        "",
        "CON:",
        "trailing.",
        "ctl\u{7}",
    ] {
        let e = rejected(&items_edit(|d| {
            files_item(d)["formats"][0]["files"][0]["name"] = json!(bad);
        }));
        assert!(matches!(e, Error::Core(_)), "{bad:?}: {e}");
    }
}

#[test]
fn file_names_that_collide_on_windows_are_refused_before_anything_is_written() {
    let e = rejected(&items_edit(|d| {
        let files = &mut files_item(d)["formats"][0]["files"];
        let second = files[1].clone();
        files[0]["name"] = json!("Report.TXT");
        files[1] = second;
        files[1]["name"] = json!("report.txt");
    }));
    assert!(matches!(e, Error::Core(_)), "{e}");
    let e = rejected(&items_edit(|d| {
        files_item(d)["formats"][0]["files"][0]["name"] = json!("nul.txt");
    }));
    assert!(matches!(e, Error::Core(_)), "{e}");
}

#[test]
fn asset_references_are_checked() {
    for bad in [
        "short",
        &"A".repeat(64),
        &"g".repeat(64),
        &format!("../{}", "a".repeat(61)),
    ] {
        let e = rejected(&items_edit(|d| {
            first_item(d)["formats"][0]["asset"] = json!(bad);
        }));
        assert!(matches!(e, Error::Malformed { .. }), "{bad:?}: {e}");
    }
    // A well-formed hash that is not in the archive.
    let e = rejected(&items_edit(|d| {
        first_item(d)["formats"][0]["asset"] = json!("0".repeat(64));
    }));
    assert!(matches!(e, Error::AssetMismatch { .. }), "{e}");
    // The declared size must equal the entry's size.
    let e = rejected(&items_edit(|d| {
        let n = first_item(d)["formats"][0]["size"].as_u64().unwrap();
        first_item(d)["formats"][0]["size"] = json!(n + 1);
    }));
    assert!(matches!(e, Error::AssetMismatch { .. }), "{e}");
}

/// Replaces the bytes of the asset with the given name, keeping its entry name: the hash no
/// longer matches.
fn with_asset_bytes(zip: &[u8], name: &str, bytes: Vec<u8>) -> Vec<u8> {
    rewrite(zip, |map| {
        map.insert(name.to_owned(), bytes);
    })
}

#[test]
fn an_asset_that_does_not_match_its_hash_is_refused() {
    let zip = valid();
    for (name, bytes) in entries(&zip)
        .into_iter()
        .filter(|(k, _)| k.starts_with("assets/"))
    {
        let mut altered = bytes.clone();
        altered[0] ^= 0xff;
        let e = rejected(&with_asset_bytes(&zip, &name, altered));
        let mut ok = matches!(e, Error::AssetMismatch { .. });
        // Flipping a text byte can also make UTF-8 invalid; that is an AssetMismatch too.
        ok |= matches!(&e, Error::Malformed { .. });
        assert!(ok, "{name}: {e}");
    }
}

#[test]
fn text_must_be_utf8_and_within_the_text_limit() {
    // Find the "ccc" text asset (3 bytes): small and used once.
    let zip = valid();
    let ccc = blake3::hash(b"ccc").to_hex().to_string();
    let name = format!("assets/{ccc}");
    assert!(entries(&zip).contains_key(&name));

    // Rebuild with a different payload of the same hash-relation by editing both sides.
    let edit = |payload: Vec<u8>| {
        let new_hash = blake3::hash(&payload).to_hex().to_string();
        let size = payload.len();
        rewrite(&zip, |map| {
            map.remove(&name);
            map.insert(format!("assets/{new_hash}"), payload);
            let mut doc: Value = serde_json::from_slice(&map["items.json"]).unwrap();
            let f = &mut doc["boards"][0]["lists"][0]["items"][0]["formats"][0];
            assert_eq!(f["asset"], json!(ccc));
            f["asset"] = json!(new_hash);
            f["size"] = json!(size);
            map.insert("items.json".into(), serde_json::to_vec(&doc).unwrap());
        })
    };

    let e = rejected(&edit(vec![0xff, 0xfe, 0xfd]));
    assert!(
        matches!(e, Error::AssetMismatch { .. }),
        "invalid UTF-8: {e}"
    );

    let letters =
        |n: usize| -> Vec<u8> { noise(n, 3).into_iter().map(|b| b'a' + b % 26).collect() };
    let e = rejected(&edit(letters(5 * 1024 * 1024 + 1)));
    assert!(
        matches!(
            e,
            Error::LimitExceeded {
                what: "text size",
                ..
            }
        ),
        "{e}"
    );

    // Exactly at the limit is fine.
    let mut s = store();
    import_archive(
        &mut s,
        Cursor::new(edit(vec![b'a'; 5 * 1024 * 1024])),
        "x",
        at(1),
        &opts(),
    )
    .unwrap();
    assert_healthy(&s);
}

#[test]
fn unreferenced_assets_are_a_warning() {
    let extra = b"nobody uses me".to_vec();
    let hash = blake3::hash(&extra).to_hex().to_string();
    let bytes = rewrite(&valid(), |map| {
        map.insert(format!("assets/{hash}"), extra);
    });
    let mut doc_fix = edit_json(&bytes, "manifest.json", |m| {
        let n = m["counts"]["assets"].as_u64().unwrap();
        m["counts"]["assets"] = json!(n + 1);
    });
    let mut s = store();
    let report = import_archive(
        &mut s,
        Cursor::new(std::mem::take(&mut doc_fix)),
        "x",
        at(1),
        &opts(),
    )
    .unwrap();
    assert!(
        report
            .warnings
            .contains(&Warning::UnreferencedAssets { count: 1 })
    );
}

#[test]
fn an_oversized_title_and_source_app_are_cut_with_a_warning() {
    let bytes = items_edit(|doc| {
        let item = &mut doc["boards"][0]["lists"][0]["items"][0];
        item["title"] = json!("é".repeat(5000));
        item["source_app"] = json!("a".repeat(5000));
    });
    let mut s = target();
    let report = import_archive(&mut s, Cursor::new(&bytes), "x", at(1), &opts()).unwrap();
    assert!(
        warnings(&report).iter().any(|w| w.contains("was cut")),
        "{:?}",
        report.warnings
    );
    let long: Vec<_> = s
        .all_items()
        .unwrap()
        .into_iter()
        .filter(|i| i.title.starts_with('é'))
        .collect();
    assert_eq!(long.len(), 1);
    assert_eq!(long[0].title.chars().count(), 1000);
    assert_eq!(long[0].source_app.as_deref().map(str::len), Some(256));
}
