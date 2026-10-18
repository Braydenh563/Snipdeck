//! Encryption of sensitive items at rest, through the store (review M6; `docs/crypto.md`).
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

use common::*;
use rusqlite::Connection;
use snipdeck_core::{Content, FileEntry, Format, Formats, INLINE_LIMIT, Item, ItemKind, Placement};
use snipdeck_store::crypto::{CryptoError, KdfParams, KeyRing, MemoryKeyProvider};
use snipdeck_store::{BlobStore, Error, ItemRepo, Store};

fn provider() -> MemoryKeyProvider {
    MemoryKeyProvider::new().unwrap()
}

fn ring(provider: &MemoryKeyProvider) -> KeyRing<'_> {
    KeyRing::new(provider).with_kdf_params(KdfParams::insecure_for_tests(64, 1, 1).unwrap())
}

fn on_disk() -> (tempfile::TempDir, Store, snipdeck_core::List) {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(dir.path()).unwrap();
    let b = board(&mut s, "b");
    let l = list(&mut s, b.id, "l");
    (dir, s, l)
}

fn secret_item(list: snipdeck_core::ListId, title: &str) -> Item {
    let mut it = Item::new(list, title.to_owned(), ItemKind::Text, at(0));
    it.sensitive = true;
    it
}

fn raw(dir: &tempfile::TempDir) -> Connection {
    Connection::open(dir.path().join("snipdeck.db")).unwrap()
}

fn encrypted_flag(conn: &Connection, item: &Item) -> i64 {
    conn.query_row(
        "SELECT encrypted FROM items WHERE id = ?1",
        [item.id.as_uuid().as_bytes()],
        |r| r.get(0),
    )
    .unwrap()
}

#[test]
fn a_key_ring_is_created_once_and_persisted() {
    let p = provider();
    let dir = tempfile::tempdir().unwrap();
    {
        let mut s = Store::open(dir.path()).unwrap();
        assert!(!s.key_ring_exists().unwrap());
        assert!(!s.is_unlocked());
        s.create_key_ring(&ring(&p), Some("pw")).unwrap();
        assert!(s.key_ring_exists().unwrap());
        assert!(s.is_unlocked(), "creating unlocks");
        assert!(matches!(
            s.create_key_ring(&ring(&p), None),
            Err(Error::AlreadyExists {
                entity: "key ring",
                ..
            })
        ));
    }
    let mut s = Store::open(dir.path()).unwrap();
    assert!(s.key_ring_exists().unwrap());
    assert!(!s.is_unlocked(), "a new session starts locked");
    assert!(matches!(
        s.unlock_key_ring(&ring(&p), Some("nope")),
        Err(Error::Crypto(CryptoError::WrongPassword))
    ));
    assert!(matches!(
        s.unlock_key_ring(&ring(&p), None),
        Err(Error::Crypto(CryptoError::PasswordRequired))
    ));
    s.unlock_key_ring(&ring(&p), Some("pw")).unwrap();
    assert!(s.is_unlocked() && s.data_key().is_some());
    s.lock_key_ring();
    assert!(!s.is_unlocked() && s.data_key().is_none());
}

#[test]
fn unlocking_without_a_key_ring_is_not_found() {
    let p = provider();
    let mut s = store();
    assert!(matches!(
        s.unlock_key_ring(&ring(&p), None),
        Err(Error::NotFound {
            entity: "key ring",
            ..
        })
    ));
}

#[test]
fn sensitive_items_are_stored_encrypted_and_read_back_in_plain_text() {
    let p = provider();
    let (dir, mut s, l) = on_disk();
    s.create_key_ring(&ring(&p), None).unwrap();

    let big = vec![b'Q'; INLINE_LIMIT + 10];
    let large = s.put_sensitive(big.clone()).unwrap();
    let formats = Formats::new(vec![
        Format::Text(text("pin 4711 ZQ9inlineSECRET")),
        Format::Png(large.clone()),
        Format::Files(vec![
            FileEntry::new("a.txt", text("file-a ZQ9fileSECRET")).unwrap(),
            FileEntry::new("b.txt", text("file-b")).unwrap(),
        ]),
    ])
    .unwrap();
    let it = secret_item(l.id, "Bank");
    s.insert_item(&it, &formats, Placement::Last).unwrap();

    let back = s.formats(it.id).unwrap();
    let plain: Vec<Vec<u8>> = back
        .iter()
        .flat_map(|f| match f {
            Format::Files(files) => files.iter().map(|f| f.content().clone()).collect(),
            Format::Text(c) | Format::Html(c) | Format::Rtf(c) | Format::Png(c) => {
                vec![c.clone()]
            }
        })
        .map(|c| s.read(&c).unwrap().into_owned())
        .collect();
    assert_eq!(plain[0], b"pin 4711 ZQ9inlineSECRET");
    assert_eq!(plain[1], big);
    assert_eq!(plain[2], b"file-a ZQ9fileSECRET");
    let names: Vec<String> = back
        .iter()
        .filter_map(|f| match f {
            Format::Files(files) => Some(files.iter().map(|f| f.name().to_owned())),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(names, ["a.txt", "b.txt"]);

    // On disk: flagged, every inline payload is an envelope, no plaintext anywhere.
    let conn = raw(&dir);
    assert_eq!(encrypted_flag(&conn, &it), 1);
    let inline: Vec<Vec<u8>> = conn
        .prepare(
            "SELECT inline_bytes FROM item_formats WHERE inline_bytes IS NOT NULL
             UNION ALL SELECT inline_bytes FROM item_files WHERE inline_bytes IS NOT NULL",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(inline.len(), 3);
    assert!(inline.iter().all(|b| b.starts_with(b"SNENC2")));
    drop(conn);
    assert_no_plaintext(
        dir.path(),
        &[b"ZQ9inlineSECRET", b"ZQ9fileSECRET", &big[..64]],
    );
    assert!(
        s.search("4711", 5).unwrap().is_empty(),
        "content is not indexed"
    );
    assert_eq!(s.search("bank", 5).unwrap(), [it.id], "the title is");
    assert_healthy(&s);
}

/// Every content file under `dir` (database, WAL, blobs) is free of each needle.
fn assert_no_plaintext(dir: &std::path::Path, needles: &[&[u8]]) {
    for path in content_files(dir) {
        let bytes = std::fs::read(&path).unwrap();
        for needle in needles {
            assert!(
                !bytes.windows(needle.len()).any(|w| w == *needle),
                "{} holds {:?}",
                path.display(),
                String::from_utf8_lossy(&needle[..needle.len().min(20)])
            );
        }
    }
}

#[test]
fn a_put_blob_used_by_a_sensitive_item_is_replaced_by_its_envelope() {
    let p = provider();
    let (dir, mut s, l) = on_disk();
    s.create_key_ring(&ring(&p), None).unwrap();
    let mut secret = b"ZQ9blobSECRET".to_vec();
    secret.resize(INLINE_LIMIT + 1, b'z');
    let content = s.put(secret.clone()).unwrap();
    let it = secret_item(l.id, "scan");
    s.insert_item(
        &it,
        &Formats::new(vec![Format::Png(content)]).unwrap(),
        Placement::Last,
    )
    .unwrap();
    assert_no_plaintext(dir.path(), &[b"ZQ9blobSECRET"]);
    let Format::Png(back) = s.formats(it.id).unwrap().iter().next().unwrap().clone() else {
        panic!()
    };
    assert_eq!(s.read(&back).unwrap().as_ref(), secret.as_slice());
}

#[test]
fn a_locked_store_refuses_to_read_or_write_encrypted_content() {
    let p = provider();
    let mut s = store();
    let b = board(&mut s, "b");
    let l = list(&mut s, b.id, "l");
    s.create_key_ring(&ring(&p), None).unwrap();
    let it = secret_item(l.id, "Wifi");
    s.insert_item(&it, &text_formats("hunter2"), Placement::Last)
        .unwrap();
    s.lock_key_ring();

    assert!(matches!(s.formats(it.id), Err(Error::EncryptionLocked)));
    assert_eq!(
        s.item(it.id).unwrap().title,
        "Wifi",
        "metadata stays readable"
    );
    // Renaming needs no key.
    let mut renamed = it.clone();
    renamed.title = "Home wifi".to_owned();
    s.update_item(&renamed, None).unwrap();
    // New content, a new sensitive item, or clearing the flag all need it.
    assert!(matches!(
        s.update_item(&renamed, Some(&text_formats("new"))),
        Err(Error::EncryptionLocked)
    ));
    assert!(matches!(
        s.insert_item(&secret_item(l.id, "x"), &text_formats("y"), Placement::Last),
        Err(Error::EncryptionLocked)
    ));
    let mut plain = renamed.clone();
    plain.sensitive = false;
    assert!(matches!(
        s.update_item(&plain, None),
        Err(Error::EncryptionLocked)
    ));
    // Nothing changed.
    s.unlock_key_ring(&ring(&p), None).unwrap();
    assert!(s.item(it.id).unwrap().sensitive);
    assert_eq!(s.formats(it.id).unwrap(), text_formats("hunter2"));
}

#[test]
fn without_a_key_ring_sensitive_items_are_kept_in_plain_text() {
    // Free: masking only (SPEC §6).
    let (dir, mut s, l) = on_disk();
    let it = secret_item(l.id, "Wifi");
    s.insert_item(&it, &text_formats("hunter2"), Placement::Last)
        .unwrap();
    assert_eq!(s.formats(it.id).unwrap(), text_formats("hunter2"));
    assert_eq!(encrypted_flag(&raw(&dir), &it), 0);
    assert!(s.search("hunter2", 5).unwrap().is_empty());
}

#[test]
fn creating_the_key_ring_encrypts_existing_sensitive_items() {
    let p = provider();
    let (dir, mut s, l) = on_disk();
    let it = secret_item(l.id, "Wifi");
    s.insert_item(&it, &text_formats("ZQ9earlySECRET"), Placement::Last)
        .unwrap();
    let open = item(&mut s, l.id, "open", "visible text");
    s.create_key_ring(&ring(&p), None).unwrap();
    let conn = raw(&dir);
    assert_eq!(encrypted_flag(&conn, &it), 1);
    assert_eq!(encrypted_flag(&conn, &open), 0);
    drop(conn);
    assert_eq!(s.formats(it.id).unwrap(), text_formats("ZQ9earlySECRET"));
    assert_eq!(s.formats(open.id).unwrap(), text_formats("visible text"));
    drop(s);
    assert_no_plaintext(dir.path(), &[b"ZQ9earlySECRET", b"zq9earlysecret"]);
}

#[test]
fn marking_an_item_sensitive_encrypts_it_and_clearing_the_flag_decrypts_it() {
    let p = provider();
    let (dir, mut s, l) = on_disk();
    s.create_key_ring(&ring(&p), None).unwrap();
    let mut it = item(&mut s, l.id, "token", "ghp_ZQ9tokenSECRET");
    assert_eq!(s.search("ghp_ZQ9tokenSECRET", 5).unwrap(), [it.id]);

    it.sensitive = true;
    it.preview = String::new();
    s.update_item(&it, None).unwrap();
    assert_eq!(encrypted_flag(&raw(&dir), &it), 1);
    assert!(s.search("ghp_ZQ9tokenSECRET", 5).unwrap().is_empty());
    assert_eq!(
        s.formats(it.id).unwrap(),
        text_formats("ghp_ZQ9tokenSECRET")
    );

    it.sensitive = false;
    s.update_item(&it, None).unwrap();
    assert_eq!(encrypted_flag(&raw(&dir), &it), 0);
    assert_eq!(
        s.formats(it.id).unwrap(),
        text_formats("ghp_ZQ9tokenSECRET")
    );
    assert_eq!(s.search("ghp_ZQ9tokenSECRET", 5).unwrap(), [it.id]);
    assert_healthy(&s);
}

#[test]
fn large_decrypted_content_can_be_saved_back_unchanged() {
    let p = provider();
    let (dir, mut s, l) = on_disk();
    s.create_key_ring(&ring(&p), None).unwrap();
    let big = vec![b'w'; INLINE_LIMIT * 2];
    let content = s.put_sensitive(big.clone()).unwrap();
    let mut it = secret_item(l.id, "big");
    s.insert_item(
        &it,
        &Formats::new(vec![Format::Text(content)]).unwrap(),
        Placement::Last,
    )
    .unwrap();
    // The editor loads the content and saves it again with a new title.
    let loaded = s.formats(it.id).unwrap();
    it.title = "bigger".to_owned();
    s.update_item(&it, Some(&loaded)).unwrap();
    let Format::Text(c) = s.formats(it.id).unwrap().iter().next().unwrap().clone() else {
        panic!()
    };
    assert_eq!(s.read(&c).unwrap().as_ref(), big.as_slice());
    // Making it plain again writes the plaintext blob to disk, as for any item.
    it.sensitive = false;
    s.update_item(&it, None).unwrap();
    s.lock_key_ring();
    let Format::Text(c) = s.formats(it.id).unwrap().iter().next().unwrap().clone() else {
        panic!()
    };
    assert_eq!(s.read(&c).unwrap().as_ref(), big.as_slice());
    assert!(dir.path().join("blobs").is_dir());
    assert_healthy(&s);
}

#[test]
fn changing_the_master_password_keeps_items_readable() {
    let p = provider();
    let mut s = store();
    let b = board(&mut s, "b");
    let l = list(&mut s, b.id, "l");
    s.create_key_ring(&ring(&p), Some("old")).unwrap();
    let it = secret_item(l.id, "x");
    s.insert_item(&it, &text_formats("kept"), Placement::Last)
        .unwrap();
    assert!(matches!(
        s.change_master_password(&ring(&p), Some("wrong"), Some("new")),
        Err(Error::Crypto(CryptoError::WrongPassword))
    ));
    s.change_master_password(&ring(&p), Some("old"), Some("new"))
        .unwrap();
    s.lock_key_ring();
    assert!(matches!(
        s.unlock_key_ring(&ring(&p), Some("old")),
        Err(Error::Crypto(CryptoError::WrongPassword))
    ));
    s.unlock_key_ring(&ring(&p), Some("new")).unwrap();
    assert_eq!(s.formats(it.id).unwrap(), text_formats("kept"));
}

#[test]
fn an_envelope_copied_to_another_item_does_not_decrypt() {
    let p = provider();
    let (dir, mut s, l) = on_disk();
    s.create_key_ring(&ring(&p), None).unwrap();
    let alpha = secret_item(l.id, "a");
    let beta = secret_item(l.id, "b");
    s.insert_item(&alpha, &text_formats("alpha"), Placement::Last)
        .unwrap();
    s.insert_item(&beta, &text_formats("beta"), Placement::Last)
        .unwrap();
    raw(&dir)
        .execute(
            "UPDATE item_formats SET inline_bytes =
               (SELECT inline_bytes FROM item_formats WHERE item_id = ?1) WHERE item_id = ?2",
            [alpha.id.as_uuid().as_bytes(), beta.id.as_uuid().as_bytes()],
        )
        .unwrap();
    assert!(matches!(
        s.formats(beta.id),
        Err(Error::Crypto(CryptoError::AuthenticationFailed))
    ));
}

#[test]
fn plain_text_that_looks_like_an_envelope_stays_plain_text() {
    let p = provider();
    let mut s = store();
    let b = board(&mut s, "b");
    let l = list(&mut s, b.id, "l");
    s.create_key_ring(&ring(&p), None).unwrap();
    let lookalike = format!("SNENC2{}", "x".repeat(60));
    let it = item(&mut s, l.id, "copied", &lookalike);
    assert_eq!(s.formats(it.id).unwrap(), text_formats(&lookalike));
    assert_healthy(&s);
}

#[test]
fn the_integrity_check_reports_plain_payloads_of_encrypted_items() {
    let p = provider();
    let (dir, mut s, l) = on_disk();
    s.create_key_ring(&ring(&p), None).unwrap();
    let it = secret_item(l.id, "x");
    s.insert_item(&it, &text_formats("secret"), Placement::Last)
        .unwrap();
    assert_healthy(&s);
    raw(&dir)
        .execute("UPDATE item_formats SET inline_bytes = x'6869'", [])
        .unwrap();
    let problems = s.check_integrity().unwrap().join("\n");
    assert!(
        problems.contains("encrypted items with a payload that is not an envelope"),
        "{problems}"
    );
}

#[test]
fn sensitive_content_staged_in_memory_never_touches_the_disk() {
    let (dir, mut s, _l) = on_disk();
    let mut secret = b"ZQ9stagedSECRET".to_vec();
    secret.resize(INLINE_LIMIT + 1, b's');
    let staged = s.put_sensitive(secret.clone()).unwrap();
    assert!(matches!(staged, Content::Blob { .. }));
    assert_eq!(s.read(&staged).unwrap().as_ref(), secret.as_slice());
    assert_no_plaintext(dir.path(), &[b"ZQ9stagedSECRET"]);
}

fn staged(s: &mut Store, tag: u8) -> (Vec<u8>, Content) {
    let secret = vec![tag; INLINE_LIMIT + 10];
    let content = s.put_sensitive(secret.clone()).unwrap();
    assert!(matches!(content, Content::Blob { .. }));
    assert_eq!(s.read(&content).unwrap().as_ref(), secret.as_slice());
    (secret, content)
}

/// PR 4 review 3: a staged payload is plaintext in RAM only until the item that uses it is
/// written; it must not stay in memory (and grow with every import) until the next lock.
#[test]
fn staged_plaintext_is_dropped_once_the_item_is_written() {
    let p = provider();
    let (_dir, mut s, l) = on_disk();
    s.create_key_ring(&ring(&p), None).unwrap();
    let (secret, content) = staged(&mut s, 7);
    let it = secret_item(l.id, "kept");
    s.insert_item(
        &it,
        &Formats::new(vec![Format::Text(content.clone())]).unwrap(),
        Placement::Last,
    )
    .unwrap();
    assert!(
        matches!(s.read(&content), Err(Error::NotFound { .. })),
        "the staged copy is gone"
    );
    // The item itself still reads back (decrypted from the envelope).
    let back = s.formats(it.id).unwrap();
    let Format::Text(c) = back.primary() else {
        panic!("text")
    };
    assert_eq!(s.read(c).unwrap().as_ref(), secret.as_slice());
}

#[test]
fn staged_plaintext_is_dropped_when_the_write_is_rolled_back() {
    let p = provider();
    let (_dir, mut s, l) = on_disk();
    s.create_key_ring(&ring(&p), None).unwrap();
    let (_, content) = staged(&mut s, 8);
    let it = secret_item(l.id, "never");
    let failed = s.transaction(|b| {
        b.insert_item(
            &it,
            &Formats::new(vec![Format::Text(content.clone())]).unwrap(),
            Placement::Last,
        )?;
        Err::<(), Error>(Error::Corrupt("abandon".to_owned()))
    });
    assert!(failed.is_err());
    assert!(
        matches!(s.read(&content), Err(Error::NotFound { .. })),
        "a rolled-back write leaves no plaintext behind"
    );

    // Staged inside the transaction and never used: also dropped on rollback.
    let mut inside = None;
    let failed = s.transaction(|b| {
        inside = Some(b.put_sensitive(vec![9; INLINE_LIMIT + 10])?);
        Err::<(), Error>(Error::Corrupt("abandon".to_owned()))
    });
    assert!(failed.is_err());
    assert!(matches!(
        s.read(&inside.unwrap()),
        Err(Error::NotFound { .. })
    ));
}

/// Two items of one import may carry the same payload: the first write must not take it from
/// the second.
#[test]
fn staged_plaintext_shared_by_two_items_in_one_transaction_survives_until_the_end() {
    let keys = provider();
    let (_dir, mut store, lst) = on_disk();
    store.create_key_ring(&ring(&keys), None).unwrap();
    let (secret, content) = staged(&mut store, 5);
    let (first, second) = (secret_item(lst.id, "a"), secret_item(lst.id, "b"));
    store
        .transaction(|tx| {
            for it in [&first, &second] {
                tx.insert_item(
                    it,
                    &Formats::new(vec![Format::Text(content.clone())]).unwrap(),
                    Placement::Last,
                )?;
            }
            Ok::<_, Error>(())
        })
        .unwrap();
    assert!(matches!(store.read(&content), Err(Error::NotFound { .. })));
    for it in [&first, &second] {
        let back = store.formats(it.id).unwrap();
        let Format::Text(c) = back.primary() else {
            panic!("text")
        };
        assert_eq!(store.read(c).unwrap().as_ref(), secret.as_slice());
    }
}
