//! Sensitive items: never plaintext in an archive, encrypted only on request, and only
//! readable with the key.
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
use snipdeck_core::{FileEntry, Format, Formats, Item, ItemKind, Placement};
use snipdeck_portable::{
    BoardLayout, Error, ExportOptions, ImportOptions, KeySource, Limits, SensitivePolicy, Warning,
    import_archive,
};
use snipdeck_store::crypto::DataKey;
use snipdeck_store::{BlobStore, ItemRepo, Store};

const SECRET: &str = "SECRET-PIN-4921";
const SECRET_FILE: &str = "FILE-SECRET-77310";

fn library() -> Store {
    let mut s = store();
    let b = board(&mut s, "Vault");
    let l = list(&mut s, b.id, "Cards");

    text_item(&mut s, l.id, "Public note", "nothing to hide");

    let mut pin = Item::new(l.id, "Bank PIN".to_owned(), ItemKind::Text, at(30));
    pin.sensitive = true;
    pin.pinned = true;
    let formats = Formats::new(vec![
        Format::Text(inline(SECRET)),
        Format::Html(inline(&format!("<b>{SECRET}</b>"))),
    ])
    .unwrap();
    s.insert_item(&pin, &formats, Placement::Last).unwrap();

    let mut doc = Item::new(l.id, "Key file".to_owned(), ItemKind::File, at(31));
    doc.sensitive = true;
    let file = FileEntry::new("id.txt", inline(SECRET_FILE)).unwrap();
    s.insert_item(
        &doc,
        &Formats::new(vec![Format::Files(vec![file])]).unwrap(),
        Placement::Last,
    )
    .unwrap();
    s
}

/// Every byte the archive contains, as entries and as the raw zip.
fn contains_anywhere(zip: &[u8], needle: &str) -> bool {
    let needle = needle.as_bytes();
    let found = |hay: &[u8]| hay.windows(needle.len()).any(|w| w == needle);
    found(zip) || entries(zip).values().any(|bytes| found(bytes))
}

fn keep() -> ImportOptions<'static> {
    ImportOptions {
        layout: BoardLayout::KeepBoards,
        ..ImportOptions::default()
    }
}

#[test]
fn the_default_is_to_leave_sensitive_items_out() {
    assert!(matches!(
        ExportOptions::default().sensitive,
        SensitivePolicy::Skip
    ));
    let s = library();
    let bytes = export_bytes(&s, &ExportOptions::default());

    for needle in [SECRET, SECRET_FILE, "Bank PIN", "Key file"] {
        assert!(!contains_anywhere(&bytes, needle), "`{needle}` leaked");
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&entries(&bytes)["manifest.json"]).unwrap();
    assert_eq!(manifest["counts"]["items"], 1);
    assert_eq!(manifest["sensitive_omitted"], 2);
    assert!(manifest.get("encryption").is_none());

    let mut fresh = store();
    let report = import_archive(&mut fresh, Cursor::new(&bytes), "x", at(1), &keep()).unwrap();
    assert_eq!(report.items_added, 1);
    assert!(
        report
            .warnings
            .contains(&Warning::SensitiveOmittedByExport { count: 2 })
    );
}

#[test]
fn encrypted_export_contains_no_plaintext_of_the_content() {
    let key = DataKey::generate().unwrap();
    let bytes = export_bytes(
        &library(),
        &ExportOptions {
            sensitive: SensitivePolicy::Encrypt(&key),
            ..ExportOptions::default()
        },
    );
    for needle in [SECRET, SECRET_FILE] {
        assert!(!contains_anywhere(&bytes, needle), "`{needle}` leaked");
    }
    // The public item is still readable plain text; titles are metadata, not content.
    assert!(contains_anywhere(&bytes, "nothing to hide"));

    let map = entries(&bytes);
    let manifest: serde_json::Value = serde_json::from_slice(&map["manifest.json"]).unwrap();
    assert_eq!(manifest["encryption"], "SNENC2");
    assert_eq!(manifest["counts"]["items"], 3);
    let items: serde_json::Value = serde_json::from_slice(&map["items.json"]).unwrap();
    let sensitive = &items["boards"][0]["lists"][0]["items"][1];
    assert_eq!(sensitive["sensitive"], true);
    assert!(sensitive.get("preview").is_none(), "no excerpt of a secret");
    assert_eq!(sensitive["formats"][0]["encrypted"], true);
    assert_eq!(
        items["boards"][0]["lists"][0]["items"][0]["formats"][0].get("encrypted"),
        None
    );
    for name in map.keys().filter(|k| k.starts_with("assets/")) {
        // Every encrypted asset is a SNENC2 envelope.
        let is_envelope = map[name].starts_with(b"SNENC2");
        let plain = [b"nothing to hide".as_slice()].contains(&map[name].as_slice());
        assert!(is_envelope || plain, "{name} is neither");
    }
}

#[test]
fn encrypted_export_round_trips_with_the_key() {
    let key = DataKey::generate().unwrap();
    let original = library();
    let bytes = export_bytes(
        &original,
        &ExportOptions {
            sensitive: SensitivePolicy::Encrypt(&key),
            ..ExportOptions::default()
        },
    );
    let mut fresh = store();
    let opts = ImportOptions {
        key: KeySource::Given(&key),
        ..keep()
    };
    let report = import_archive(&mut fresh, Cursor::new(&bytes), "x", at(1), &opts).unwrap();
    assert_eq!(report.items_added, 3);
    assert!(!report.has_warnings(), "{:?}", report.warnings);
    assert_same(&snapshot(&fresh), &snapshot(&original));
    // The sensitive flag and its empty preview survive.
    let imported = fresh.all_items().unwrap();
    assert!(
        imported
            .iter()
            .filter(|i| i.sensitive)
            .all(|i| i.preview.is_empty())
    );
    assert_healthy(&fresh);
}

#[test]
fn without_the_key_encrypted_items_are_skipped_with_a_warning() {
    let key = DataKey::generate().unwrap();
    let bytes = export_bytes(
        &library(),
        &ExportOptions {
            sensitive: SensitivePolicy::Encrypt(&key),
            ..ExportOptions::default()
        },
    );
    let mut fresh = store();
    let report = import_archive(&mut fresh, Cursor::new(&bytes), "x", at(1), &keep()).unwrap();
    assert_eq!((report.items_added, report.items_skipped), (1, 2));
    assert!(
        report
            .warnings
            .contains(&Warning::SensitiveNeedKey { count: 2 })
    );
    assert_eq!(counts(&fresh).2, 1);
    assert_healthy(&fresh);
}

#[test]
fn another_computers_key_cannot_open_them_and_they_are_skipped() {
    let key = DataKey::generate().unwrap();
    let other = DataKey::generate().unwrap();
    let bytes = export_bytes(
        &library(),
        &ExportOptions {
            sensitive: SensitivePolicy::Encrypt(&key),
            ..ExportOptions::default()
        },
    );
    let mut fresh = store();
    let opts = ImportOptions {
        key: KeySource::Given(&other),
        ..keep()
    };
    let report = import_archive(&mut fresh, Cursor::new(&bytes), "x", at(1), &opts).unwrap();
    assert_eq!((report.items_added, report.items_skipped), (1, 2));
    assert_eq!(
        report
            .warnings
            .iter()
            .filter(|w| matches!(w, Warning::CannotDecrypt { .. }))
            .count(),
        2
    );
    assert_eq!(counts(&fresh).2, 1);
}

#[test]
fn an_envelope_moved_to_another_item_does_not_open() {
    let key = DataKey::generate().unwrap();
    let mut s = store();
    let b = board(&mut s, "B");
    let l = list(&mut s, b.id, "L");
    for (title, secret) in [("one", "secret-one"), ("two", "secret-two")] {
        let mut item = Item::new(l.id, title.to_owned(), ItemKind::Text, at(1));
        item.sensitive = true;
        s.insert_item(&item, &text_formats(secret), Placement::Last)
            .unwrap();
    }
    let bytes = export_bytes(
        &s,
        &ExportOptions {
            sensitive: SensitivePolicy::Encrypt(&key),
            ..ExportOptions::default()
        },
    );
    // Point item two at item one's asset (same size is not required to be valid: both are
    // 10-byte secrets, so the size matches).
    let swapped = edit_json(&bytes, "items.json", |doc| {
        let items = &mut doc["boards"][0]["lists"][0]["items"];
        let first = items[0]["formats"][0]["asset"].clone();
        items[1]["formats"][0]["asset"] = first;
    });
    // The second asset is now unreferenced and the first is used twice; counts still match.
    let mut fresh = store();
    let opts = ImportOptions {
        key: KeySource::Given(&key),
        ..keep()
    };
    let report = import_archive(&mut fresh, Cursor::new(&swapped), "x", at(1), &opts).unwrap();
    assert_eq!(
        (report.items_added, report.items_skipped),
        (1, 1),
        "{:?}",
        report.warnings
    );
    let titles: Vec<_> = fresh
        .all_items()
        .unwrap()
        .into_iter()
        .map(|i| i.title)
        .collect();
    assert_eq!(titles, ["one"]);
}

#[test]
fn an_altered_envelope_is_rejected_by_its_hash_before_decryption() {
    let key = DataKey::generate().unwrap();
    let bytes = export_bytes(
        &library(),
        &ExportOptions {
            sensitive: SensitivePolicy::Encrypt(&key),
            ..ExportOptions::default()
        },
    );
    let tampered = rewrite(&bytes, |map| {
        let name = map
            .iter()
            .find(|(_, v)| v.starts_with(b"SNENC2"))
            .map(|(k, _)| k.clone())
            .unwrap();
        let last = map.get_mut(&name).unwrap().last_mut().unwrap();
        *last ^= 1;
    });
    let mut fresh = store();
    let opts = ImportOptions {
        key: KeySource::Given(&key),
        ..keep()
    };
    let err = import_archive(&mut fresh, Cursor::new(&tampered), "x", at(1), &opts).unwrap_err();
    assert!(matches!(err, Error::AssetMismatch { .. }), "{err}");
    assert_eq!(counts(&fresh), (0, 0, 0, 0));
}

#[test]
fn encrypted_payloads_need_the_manifest_to_say_so() {
    let key = DataKey::generate().unwrap();
    let bytes = export_bytes(
        &library(),
        &ExportOptions {
            sensitive: SensitivePolicy::Encrypt(&key),
            ..ExportOptions::default()
        },
    );
    let edited = edit_json(&bytes, "manifest.json", |m| {
        m.as_object_mut().unwrap().remove("encryption");
    });
    let mut fresh = store();
    let err = import_archive(&mut fresh, Cursor::new(&edited), "x", at(1), &keep()).unwrap_err();
    assert!(matches!(err, Error::Malformed { .. }), "{err}");
}

#[test]
fn many_items_pointing_at_one_big_encrypted_asset_hit_the_read_budget() {
    let key = DataKey::generate().unwrap();
    let mut s = store();
    let b = board(&mut s, "B");
    let l = list(&mut s, b.id, "L");
    let big = "x".repeat(60_000);
    for (title, secret) in [
        ("one", big.as_str()),
        ("two", "tiny-2"),
        ("three", "tiny-3"),
    ] {
        let mut item = Item::new(l.id, title.to_owned(), ItemKind::Text, at(1));
        item.sensitive = true;
        s.insert_item(&item, &text_formats(secret), Placement::Last)
            .unwrap();
    }
    let bytes = export_bytes(
        &s,
        &ExportOptions {
            sensitive: SensitivePolicy::Encrypt(&key),
            ..ExportOptions::default()
        },
    );
    let pointed = edit_json(&bytes, "items.json", |doc| {
        let items = &mut doc["boards"][0]["lists"][0]["items"];
        let first = items[0]["formats"][0].clone();
        items[1]["formats"][0] = first.clone();
        items[2]["formats"][0] = first;
    });
    // The archive holds about 60 KB; reading the big asset three times costs about 180 KB.
    let opts = ImportOptions {
        key: KeySource::Given(&key),
        limits: Limits {
            max_total_bytes: 100_000,
            ..Limits::default()
        },
        ..keep()
    };
    let mut fresh = store();
    let err = import_archive(&mut fresh, Cursor::new(&pointed), "x", at(1), &opts).unwrap_err();
    assert!(
        matches!(
            err,
            Error::LimitExceeded {
                what: "encrypted asset bytes read",
                ..
            }
        ),
        "{err:?}"
    );
    assert_eq!(fresh.all_items().unwrap().len(), 0);
}

#[test]
fn an_envelope_moved_to_another_format_of_the_same_item_does_not_open() {
    let key = DataKey::generate().unwrap();
    let bytes = export_bytes(
        &library(),
        &ExportOptions {
            sensitive: SensitivePolicy::Encrypt(&key),
            ..ExportOptions::default()
        },
    );
    // "Bank PIN" has a text and an html format; give each the other's payload.
    let swapped = edit_json(&bytes, "items.json", |doc| {
        let formats = &mut doc["boards"][0]["lists"][0]["items"][1]["formats"];
        let (a, b) = (formats[0].clone(), formats[1].clone());
        for field in ["asset", "size"] {
            formats[0][field] = b[field].clone();
            formats[1][field] = a[field].clone();
        }
    });
    let mut fresh = store();
    let opts = ImportOptions {
        key: KeySource::Given(&key),
        ..keep()
    };
    let report = import_archive(&mut fresh, Cursor::new(&swapped), "x", at(1), &opts).unwrap();
    let titles: Vec<_> = fresh
        .all_items()
        .unwrap()
        .into_iter()
        .map(|i| i.title)
        .collect();
    assert_eq!(titles, ["Public note", "Key file"], "{:?}", report.warnings);
    assert!(report.warnings.contains(&Warning::CannotDecrypt {
        title: "Bank PIN".to_owned()
    }));
}

// ---- with the store's own key ring (encryption at rest) ----

use snipdeck_store::crypto::{KdfParams, KeyRing, MemoryKeyProvider};

fn encrypted_vault() -> (Store, MemoryKeyProvider) {
    let provider = MemoryKeyProvider::new().unwrap();
    let mut s = store();
    s.create_key_ring(
        &KeyRing::new(&provider).with_kdf_params(KdfParams::insecure_for_tests(64, 1, 1).unwrap()),
        None,
    )
    .unwrap();
    let b = board(&mut s, "Vault");
    let l = list(&mut s, b.id, "L");
    text_item(&mut s, l.id, "public", "public text");
    let mut secret = Item::new(l.id, "Card".to_owned(), ItemKind::Text, at(1));
    secret.sensitive = true;
    s.insert_item(&secret, &text_formats("AT-REST-SECRET-9"), Placement::Last)
        .unwrap();
    (s, provider)
}

#[test]
fn a_store_with_encryption_at_rest_exports_and_restores_through_its_own_key() {
    let (mut s, _provider) = encrypted_vault();
    assert!(s.is_unlocked());
    let bytes = export_bytes(
        &s,
        &ExportOptions {
            sensitive: SensitivePolicy::Encrypt(s.data_key().unwrap()),
            ..ExportOptions::default()
        },
    );
    assert!(!contains_anywhere(&bytes, "AT-REST-SECRET-9"));

    // Restore into the same unlocked store: the key is borrowed from the store itself, and the
    // store encrypts the restored copy at rest again.
    let opts = ImportOptions {
        key: KeySource::Library,
        ..keep()
    };
    let report = import_archive(&mut s, Cursor::new(&bytes), "x", at(1), &opts).unwrap();
    assert_eq!(report.items_added, 2);
    assert!(!report.has_warnings(), "{:?}", report.warnings);
    let secrets: Vec<_> = s
        .all_items()
        .unwrap()
        .into_iter()
        .filter(|i| i.sensitive)
        .collect();
    assert_eq!(secrets.len(), 2);
    for item in &secrets {
        let formats = s.formats(item.id).unwrap();
        let Format::Text(c) = formats.primary() else {
            panic!()
        };
        assert_eq!(s.read(c).unwrap().as_ref(), b"AT-REST-SECRET-9");
    }
    assert_healthy(&s);
}

#[test]
fn a_locked_store_can_still_export_without_its_sensitive_items() {
    let (mut s, _provider) = encrypted_vault();
    s.lock_key_ring();
    // Skipping never reads a sensitive item, so a locked library backs up its public items.
    let bytes = export_bytes(&s, &ExportOptions::default());
    assert!(!contains_anywhere(&bytes, "AT-REST-SECRET-9"));
    let mut fresh = store();
    let report = import_archive(&mut fresh, Cursor::new(&bytes), "x", at(1), &keep()).unwrap();
    assert_eq!(report.items_added, 1);

    // Without the key there is nothing to encrypt with.
    let opts = ImportOptions {
        key: KeySource::Library,
        ..keep()
    };
    let mut other = store();
    let report = import_archive(&mut other, Cursor::new(&bytes), "x", at(1), &opts).unwrap();
    assert_eq!(report.items_added, 1);
}

#[test]
fn a_locked_store_cannot_be_restored_into_with_a_sensitive_archive() {
    let (mut s, _provider) = encrypted_vault();
    let key = DataKey::generate().unwrap();
    // An archive from elsewhere with one sensitive item, encrypted under `key`.
    let mut src = store();
    let b = board(&mut src, "B");
    let l = list(&mut src, b.id, "L");
    let mut secret = Item::new(l.id, "S".to_owned(), ItemKind::Text, at(1));
    secret.sensitive = true;
    src.insert_item(&secret, &text_formats("x"), Placement::Last)
        .unwrap();
    let bytes = export_bytes(
        &src,
        &ExportOptions {
            sensitive: SensitivePolicy::Encrypt(&key),
            ..ExportOptions::default()
        },
    );
    s.lock_key_ring();
    let before = counts(&s);
    let opts = ImportOptions {
        key: KeySource::Given(&key),
        ..keep()
    };
    let err = import_archive(&mut s, Cursor::new(&bytes), "x", at(1), &opts).unwrap_err();
    assert!(
        matches!(err, Error::Store(snipdeck_store::Error::EncryptionLocked)),
        "{err}"
    );
    assert_eq!(counts(&s), before, "the failed import was rolled back");
    assert_healthy(&s);
}

// ---- security review 2, finding 1: imported sensitive payloads never reach the blob folder ----

/// A disk library with encryption at rest, plus an archive holding one sensitive item whose
/// image and file are too big to be inline. Returns the library's folder, the archive, the key
/// it is encrypted under, and the plaintext payloads.
fn big_sensitive_archive() -> (Vec<u8>, DataKey, Vec<Vec<u8>>) {
    let key = DataKey::generate().unwrap();
    let mut src = store();
    let b = board(&mut src, "B");
    let l = list(&mut src, b.id, "L");
    let png = noise(80_000, 11);
    let file = noise(70_000, 13);
    let mut secret = Item::new(l.id, "Scan".to_owned(), ItemKind::Image, at(1));
    secret.sensitive = true;
    let entry = FileEntry::new("scan.bin", src.put(file.clone()).unwrap()).unwrap();
    let formats = Formats::new(vec![
        Format::Png(src.put(png.clone()).unwrap()),
        Format::Files(vec![entry]),
    ])
    .unwrap();
    src.insert_item(&secret, &formats, Placement::Last).unwrap();
    let bytes = export_bytes(
        &src,
        &ExportOptions {
            sensitive: SensitivePolicy::Encrypt(&key),
            ..ExportOptions::default()
        },
    );
    (bytes, key, vec![png, file])
}

/// Makes the blob folder refuse a plaintext payload: a directory sits where its file would be
/// written, so any attempt to store the plaintext there fails the import.
fn block_plaintext_files(folder: &std::path::Path, payloads: &[Vec<u8>]) {
    for payload in payloads {
        let hex = blake3::hash(payload).to_hex().to_string();
        let shard = folder.join("blobs").join(&hex[..2]);
        std::fs::create_dir_all(shard.join(&hex[2..])).unwrap();
    }
}

fn open_vault(folder: &std::path::Path) -> Store {
    let provider = MemoryKeyProvider::new().unwrap();
    let mut s = Store::open(folder).unwrap();
    s.create_key_ring(
        &KeyRing::new(&provider).with_kdf_params(KdfParams::insecure_for_tests(64, 1, 1).unwrap()),
        None,
    )
    .unwrap();
    s
}

#[test]
fn importing_a_big_sensitive_item_never_writes_its_plaintext_to_the_blob_folder() {
    let (bytes, key, payloads) = big_sensitive_archive();
    let dir = tempfile::tempdir().unwrap();
    let mut s = open_vault(dir.path());
    block_plaintext_files(dir.path(), &payloads);
    let opts = ImportOptions {
        key: KeySource::Given(&key),
        ..keep()
    };
    let report = import_archive(&mut s, Cursor::new(&bytes), "x", at(1), &opts).unwrap();
    assert_eq!(report.items_added, 1);
    assert!(!report.has_warnings(), "{:?}", report.warnings);
    assert_healthy(&s);
}

#[test]
fn a_refused_import_of_a_big_sensitive_item_leaves_no_plaintext_file_behind() {
    let (bytes, key, payloads) = big_sensitive_archive();
    let dir = tempfile::tempdir().unwrap();
    let mut s = open_vault(dir.path());
    s.lock_key_ring();
    block_plaintext_files(dir.path(), &payloads);
    let opts = ImportOptions {
        key: KeySource::Given(&key),
        ..keep()
    };
    let err = import_archive(&mut s, Cursor::new(&bytes), "x", at(1), &opts).unwrap_err();
    assert!(
        matches!(err, Error::Store(snipdeck_store::Error::EncryptionLocked)),
        "{err}"
    );
    assert_healthy(&s);
}

// ---- security review 2, finding 4: a locked library can still restore and skip duplicates ----

#[test]
fn a_locked_library_with_encrypted_items_can_still_skip_duplicates() {
    let (mut s, _provider) = encrypted_vault();
    // A backup of the public items, taken while the library is unlocked or locked.
    s.lock_key_ring();
    let bytes = export_bytes(&s, &ExportOptions::default());
    let opts = ImportOptions {
        merge: snipdeck_portable::MergeStrategy::SkipDuplicates,
        ..keep()
    };
    let before = counts(&s);
    let report = import_archive(&mut s, Cursor::new(&bytes), "x", at(1), &opts).unwrap();
    assert_eq!(report.items_added, 0, "{report:?}");
    assert_eq!(report.items_skipped, 1, "the public item is already there");
    assert_eq!(counts(&s), before);

    // Something new still arrives, next to the encrypted item it cannot read.
    let mut other = store();
    let b = board(&mut other, "Vault");
    let l = list(&mut other, b.id, "L");
    text_item(&mut other, l.id, "new", "brand new text");
    let bytes = export_bytes(&other, &ExportOptions::default());
    let report = import_archive(&mut s, Cursor::new(&bytes), "x", at(1), &opts).unwrap();
    assert_eq!(report.items_added, 1, "{report:?}");
    assert_healthy(&s);
}

#[test]
fn an_encrypted_payload_on_an_item_marked_not_sensitive_is_refused() {
    let key = DataKey::generate().unwrap();
    let bytes = export_bytes(
        &library(),
        &ExportOptions {
            sensitive: SensitivePolicy::Encrypt(&key),
            ..ExportOptions::default()
        },
    );
    let crafted = edit_json(&bytes, "items.json", |doc| {
        doc["boards"][0]["lists"][0]["items"][1]["sensitive"] = false.into();
    });
    for dry_run in [false, true] {
        let mut fresh = store();
        let before = snapshot(&fresh);
        let opts = ImportOptions {
            key: KeySource::Given(&key),
            dry_run,
            ..keep()
        };
        let err = import_archive(&mut fresh, Cursor::new(&crafted), "x", at(1), &opts).unwrap_err();
        assert!(
            matches!(err, Error::Malformed { .. }),
            "dry_run {dry_run}: {err:?}"
        );
        assert!(err.to_string().contains("Bank PIN"), "{err}");
        assert_same(&snapshot(&fresh), &before);
    }
}
