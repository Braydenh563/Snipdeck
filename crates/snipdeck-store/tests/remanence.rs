//! Review H1: once an existing item is made sensitive and encrypted, its old plaintext must not
//! survive in `snipdeck.db`, `-wal`, `-shm` or the blob folder, after a clean close or a crash.
//! What SQLite does and does not guarantee is in `docs/crypto.md`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]

mod common;

use common::*;
use rusqlite::Connection;
use snipdeck_core::{Format, Formats, INLINE_LIMIT, Item, ItemKind, Placement};
use snipdeck_store::crypto::{KdfParams, KeyRing, MemoryKeyProvider};
use snipdeck_store::{BlobStore, ItemRepo, Store, TrashRepo, Trashable};

const SECRET: &str = "my pin is ZQ9SecretPlainTextZQ9 ok";
const BLOB_SECRET: &[u8] = b"ZQ9BlobSecretPlainText";

/// Case-insensitive, so the lower-cased FTS token counts too.
fn holds_secret(bytes: &[u8]) -> bool {
    let lower: Vec<u8> = bytes.iter().map(u8::to_ascii_lowercase).collect();
    [&b"zq9secretplaintextzq9"[..], b"zq9blobsecretplaintext"]
        .iter()
        .any(|needle| lower.windows(needle.len()).any(|w| w == *needle))
}

fn files_holding_the_secret(dir: &std::path::Path) -> Vec<String> {
    content_files(dir)
        .into_iter()
        .filter(|path| holds_secret(&std::fs::read(path).unwrap()))
        .map(|path| path.display().to_string())
        .collect()
}

/// Opens a store with filler items, one plain item holding the secret (text inline and a large
/// image blob), returning it ready to be made sensitive.
fn library(dir: &std::path::Path, filler: usize) -> (Store, Item, Formats) {
    let mut s = Store::open(dir).unwrap();
    let b = board(&mut s, "b");
    let l = list(&mut s, b.id, "l");
    for n in 0..filler {
        item(
            &mut s,
            l.id,
            &format!("filler {n}"),
            &format!("filler body {n}"),
        );
    }
    let mut big = BLOB_SECRET.to_vec();
    big.resize(INLINE_LIMIT + 100, b'.');
    let png = s.put(big).unwrap();
    let formats = Formats::new(vec![Format::Text(text(SECRET)), Format::Png(png)]).unwrap();
    let mut it = Item::new(l.id, "pin".to_owned(), ItemKind::Text, at(0));
    it.preview = Item::preview_of(SECRET, false);
    s.insert_item(&it, &formats, Placement::Last).unwrap();
    for n in 0..filler {
        item(
            &mut s,
            l.id,
            &format!("more {n}"),
            &format!("more body {n}"),
        );
    }
    assert!(
        !files_holding_the_secret(dir).is_empty(),
        "the probe must see the plaintext before"
    );
    (s, it, formats)
}

fn make_sensitive(s: &mut Store, provider: &MemoryKeyProvider, it: &mut Item) {
    let ring =
        KeyRing::new(provider).with_kdf_params(KdfParams::insecure_for_tests(64, 1, 1).unwrap());
    s.create_key_ring(&ring, None).unwrap();
    it.sensitive = true;
    it.preview = String::new();
    s.update_item(it, None).unwrap();
}

#[test]
fn no_plaintext_remains_after_a_clean_close() {
    for filler in [0, 50, 500] {
        let dir = tempfile::tempdir().unwrap();
        let provider = MemoryKeyProvider::new().unwrap();
        let (mut s, mut it, _) = library(dir.path(), filler);
        make_sensitive(&mut s, &provider, &mut it);
        drop(s);
        assert_eq!(
            files_holding_the_secret(dir.path()),
            Vec::<String>::new(),
            "{filler} filler items"
        );
    }
}

#[test]
fn no_plaintext_remains_after_a_crash() {
    for filler in [0, 50, 500] {
        let dir = tempfile::tempdir().unwrap();
        let provider = MemoryKeyProvider::new().unwrap();
        let (mut s, mut it, _) = library(dir.path(), filler);
        make_sensitive(&mut s, &provider, &mut it);
        // The process dies: no close, no final checkpoint.
        std::mem::forget(s);
        assert_eq!(
            files_holding_the_secret(dir.path()),
            Vec::<String>::new(),
            "{filler} filler items"
        );
    }
}

#[test]
fn no_plaintext_remains_after_a_purge() {
    let dir = tempfile::tempdir().unwrap();
    let (mut s, it, _) = library(dir.path(), 20);
    s.trash(Trashable::Item(it.id), at(1)).unwrap();
    s.purge(at(2)).unwrap();
    std::mem::forget(s);
    assert_eq!(files_holding_the_secret(dir.path()), Vec::<String>::new());
}

#[test]
fn the_secret_is_still_readable_through_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let provider = MemoryKeyProvider::new().unwrap();
    let (mut s, mut it, formats) = library(dir.path(), 5);
    make_sensitive(&mut s, &provider, &mut it);
    let back = s.formats(it.id).unwrap();
    let read = |f: &Formats, i: usize| match f.iter().nth(i).unwrap() {
        Format::Text(c) | Format::Png(c) => s.read(c).unwrap().into_owned(),
        _ => panic!(),
    };
    assert_eq!(read(&back, 0), read(&formats, 0));
    assert_eq!(read(&back, 1)[..BLOB_SECRET.len()], *BLOB_SECRET);
}

#[test]
fn the_integrity_check_reports_when_secure_delete_is_off() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open(dir.path()).unwrap());
    let raw = Connection::open(dir.path().join("snipdeck.db")).unwrap();
    raw.execute(
        "INSERT INTO item_fts (item_fts, rank) VALUES ('secure-delete', 0)",
        [],
    )
    .unwrap();
    drop(raw);
    let s = Store::open(dir.path()).unwrap();
    assert_eq!(s.pragma_text("secure_delete").unwrap(), "1");
    let problems = s.check_integrity().unwrap().join("\n");
    assert!(
        problems.contains("full-text index does not use secure-delete"),
        "{problems}"
    );
}
