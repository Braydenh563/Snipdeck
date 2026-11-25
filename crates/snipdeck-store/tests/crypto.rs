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
use snipdeck_store::{BlobStore, Error, ItemRepo, RecentRepo, Store, TrashRepo, Trashable};

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
        s.create_key_ring(&ring(&p), Some(common::pw("pw")))
            .unwrap();
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
        s.unlock_key_ring(&ring(&p), Some(common::pw("nope"))),
        Err(Error::Crypto(CryptoError::WrongPassword))
    ));
    assert!(matches!(
        s.unlock_key_ring(&ring(&p), None),
        Err(Error::Crypto(CryptoError::PasswordRequired))
    ));
    s.unlock_key_ring(&ring(&p), Some(common::pw("pw")))
        .unwrap();
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
    assert_eq!(found(&s, "4711").len(), 0, "content is not indexed");
    assert_eq!(found(&s, "bank"), [it.id], "the title is");
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
    // No key ring yet: masking only (SPEC §6).
    let (dir, mut s, l) = on_disk();
    let it = secret_item(l.id, "Wifi");
    s.insert_item(&it, &text_formats("hunter2"), Placement::Last)
        .unwrap();
    assert_eq!(s.formats(it.id).unwrap(), text_formats("hunter2"));
    assert_eq!(encrypted_flag(&raw(&dir), &it), 0);
    assert_eq!(found(&s, "hunter2").len(), 0);
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
fn plain_sensitive_items_are_reported_until_the_key_ring_encrypts_them() {
    let p = provider();
    let (_dir, mut s, l) = on_disk();
    assert!(!s.has_plain_sensitive().unwrap());
    item(&mut s, l.id, "open", "visible text");
    assert!(!s.has_plain_sensitive().unwrap(), "a plain item is not one");
    let it = secret_item(l.id, "Wifi");
    s.insert_item(&it, &text_formats("hunter2"), Placement::Last)
        .unwrap();
    // A trashed one still counts: it is restorable, and still on disk.
    s.trash(Trashable::Item(it.id), at(1)).unwrap();
    assert!(s.has_plain_sensitive().unwrap());
    s.create_key_ring(&ring(&p), None).unwrap();
    assert!(!s.has_plain_sensitive().unwrap());
}

#[test]
fn marking_an_item_sensitive_encrypts_it_and_clearing_the_flag_decrypts_it() {
    let p = provider();
    let (dir, mut s, l) = on_disk();
    s.create_key_ring(&ring(&p), None).unwrap();
    let mut it = item(&mut s, l.id, "token", "ghp_ZQ9tokenSECRET");
    assert_eq!(found(&s, "ghp_ZQ9tokenSECRET"), [it.id]);

    it.sensitive = true;
    it.preview = String::new();
    s.update_item(&it, None).unwrap();
    assert_eq!(encrypted_flag(&raw(&dir), &it), 1);
    assert_eq!(found(&s, "ghp_ZQ9tokenSECRET").len(), 0);
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
    assert_eq!(found(&s, "ghp_ZQ9tokenSECRET"), [it.id]);
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
    s.create_key_ring(&ring(&p), Some(common::pw("old")))
        .unwrap();
    let it = secret_item(l.id, "x");
    s.insert_item(&it, &text_formats("kept"), Placement::Last)
        .unwrap();
    assert!(matches!(
        s.change_master_password(
            &ring(&p),
            Some(common::pw("wrong")),
            Some(common::pw("new"))
        ),
        Err(Error::Crypto(CryptoError::WrongPassword))
    ));
    s.change_master_password(&ring(&p), Some(common::pw("old")), Some(common::pw("new")))
        .unwrap();
    s.lock_key_ring();
    assert!(matches!(
        s.unlock_key_ring(&ring(&p), Some(common::pw("old"))),
        Err(Error::Crypto(CryptoError::WrongPassword))
    ));
    s.unlock_key_ring(&ring(&p), Some(common::pw("new")))
        .unwrap();
    assert_eq!(s.formats(it.id).unwrap(), text_formats("kept"));
}

#[test]
fn the_slow_half_of_a_master_password_step_can_run_on_another_thread() {
    let p = std::sync::Arc::new(provider());
    let mut s = store();
    let b = board(&mut s, "b");
    let l = list(&mut s, b.id, "l");
    let it = secret_item(l.id, "x");

    // Set: the key and record are made off the store's thread, then installed.
    let (key, record) = {
        let p = p.clone();
        std::thread::spawn(move || ring(&p).create(Some(common::pw("one"))).unwrap())
            .join()
            .unwrap()
    };
    s.install_key_ring(key, &record).unwrap();
    assert!(s.is_unlocked());
    s.insert_item(&it, &text_formats("kept"), Placement::Last)
        .unwrap();
    assert!(matches!(
        s.install_key_ring(provider_key(&p), &record),
        Err(Error::AlreadyExists { .. })
    ));

    // Change: the record is rebuilt off-thread from a copy, then written.
    let old = s.key_record().unwrap().unwrap();
    let rotation = {
        let p = p.clone();
        std::thread::spawn(move || {
            ring(&p)
                .rotate(&old, Some(common::pw("one")), Some(common::pw("two")))
                .unwrap()
        })
        .join()
        .unwrap()
    };
    s.rotate_key_ring(rotation).unwrap();
    assert!(s.is_unlocked());
    s.lock_key_ring();

    // Unlock: the key is derived off-thread, then handed over.
    let stored = s.key_record().unwrap().unwrap();
    let key = {
        let p = p.clone();
        std::thread::spawn(move || ring(&p).unlock(&stored, Some(common::pw("two"))).unwrap())
            .join()
            .unwrap()
    };
    s.open_with_key(key).unwrap();
    assert_eq!(s.formats(it.id).unwrap(), text_formats("kept"));
}

fn provider_key(p: &MemoryKeyProvider) -> snipdeck_store::crypto::DataKey {
    ring(p).create(None).unwrap().0
}

#[test]
fn rotating_a_key_needs_a_key_ring() {
    let p = provider();
    let mut s = store();
    let (_, record) = ring(&p).create(None).unwrap();
    let rotation = ring(&p).rotate(&record, None, None).unwrap();
    assert!(matches!(
        s.rotate_key_ring(rotation),
        Err(Error::NotFound { .. })
    ));
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

// ---- Master password lifecycle (SPEC 5.7, V1-PLAN C3) -----------------------------------------

#[test]
fn the_store_says_whether_a_master_password_is_set() {
    let p = provider();
    let mut s = store();
    assert!(
        !s.has_master_password(&ring(&p)).unwrap(),
        "no key ring: no password"
    );
    s.create_key_ring(&ring(&p), None).unwrap();
    assert!(!s.has_master_password(&ring(&p)).unwrap());
    s.change_master_password(&ring(&p), None, Some(common::pw("pw")))
        .unwrap();
    assert!(s.has_master_password(&ring(&p)).unwrap());
    s.change_master_password(&ring(&p), Some(common::pw("pw")), None)
        .unwrap();
    assert!(!s.has_master_password(&ring(&p)).unwrap());
}

#[test]
fn a_password_added_later_locks_a_restart_and_removing_it_opens_without_one() {
    let p = provider();
    let dir = tempfile::tempdir().unwrap();
    let item;
    {
        let mut s = Store::open(dir.path()).unwrap();
        let b = board(&mut s, "b");
        let l = list(&mut s, b.id, "l");
        s.create_key_ring(&ring(&p), None).unwrap();
        item = secret_item(l.id, "x");
        s.insert_item(&item, &text_formats("kept"), Placement::Last)
            .unwrap();
        s.change_master_password(&ring(&p), None, Some(common::pw("pw")))
            .unwrap();
        assert!(
            s.is_unlocked(),
            "setting a password does not lock the session"
        );
    }
    let mut s = Store::open(dir.path()).unwrap();
    assert!(s.has_master_password(&ring(&p)).unwrap());
    assert!(matches!(
        s.unlock_key_ring(&ring(&p), None),
        Err(Error::Crypto(CryptoError::PasswordRequired))
    ));
    assert!(!s.is_unlocked());
    assert!(matches!(s.formats(item.id), Err(Error::EncryptionLocked)));
    s.unlock_key_ring(&ring(&p), Some(common::pw("pw")))
        .unwrap();
    assert_eq!(s.formats(item.id).unwrap(), text_formats("kept"));
    s.change_master_password(&ring(&p), Some(common::pw("pw")), None)
        .unwrap();
    s.lock_key_ring();
    s.unlock_key_ring(&ring(&p), None).unwrap();
    assert_eq!(s.formats(item.id).unwrap(), text_formats("kept"));
}

#[test]
fn a_wrong_current_password_changes_nothing_and_the_old_one_still_works() {
    let p = provider();
    let mut s = store();
    s.create_key_ring(&ring(&p), Some(common::pw("old")))
        .unwrap();
    assert!(matches!(
        s.change_master_password(&ring(&p), Some(common::pw("wrong")), None),
        Err(Error::Crypto(CryptoError::WrongPassword))
    ));
    assert!(s.has_master_password(&ring(&p)).unwrap());
    s.lock_key_ring();
    s.unlock_key_ring(&ring(&p), Some(common::pw("old")))
        .unwrap();
}

#[test]
fn a_rotated_record_opens_with_the_new_password_while_the_old_record_still_opens_with_the_old() {
    // The record handed back is good before it is stored, so a wrong write cannot lock the
    // user out.
    let p = provider();
    let ring = ring(&p);
    let (_, old) = ring.create(Some(common::pw("one"))).unwrap();
    let new = ring
        .rotate(&old, Some(common::pw("one")), Some(common::pw("two")))
        .unwrap()
        .record;
    assert!(
        ring.unlock(&new, Some(common::pw("two"))).is_ok(),
        "the new record opens"
    );
    assert!(
        ring.unlock(&new, Some(common::pw("one"))).is_err(),
        "and only with the new password"
    );
    assert!(
        ring.unlock(&old, Some(common::pw("one"))).is_ok(),
        "the old record is untouched"
    );
}

#[test]
fn a_failed_key_setup_leaves_no_envelope_files_behind() {
    let p = provider();
    let (dir, mut s, l) = on_disk();
    let big = s.put(vec![b'Q'; INLINE_LIMIT + 10]).unwrap();
    let it = secret_item(l.id, "Bank");
    s.insert_item(
        &it,
        &Formats::new(vec![Format::Png(big)]).unwrap(),
        Placement::Last,
    )
    .unwrap();
    let before = content_files(dir.path()).len();
    // The envelope is written first; the row update then fails, so the commit never happens.
    raw(&dir)
        .execute_batch(
            "CREATE TRIGGER boom BEFORE UPDATE OF encrypted ON items \
             BEGIN SELECT RAISE(ABORT, 'boom'); END;",
        )
        .unwrap();
    assert!(s.create_key_ring(&ring(&p), None).is_err());
    assert_eq!(content_files(dir.path()).len(), before);
}

#[test]
fn a_rotation_from_another_key_is_refused_and_the_old_record_stays() {
    let p = provider();
    let mut s = store();
    let b = board(&mut s, "b");
    let list = list(&mut s, b.id, "l");
    s.create_key_ring(&ring(&p), Some(common::pw("pw")))
        .unwrap();
    let it = secret_item(list.id, "bank");
    s.insert_item(&it, &text_formats("kept"), Placement::Last)
        .unwrap();
    let before = s.key_record().unwrap().unwrap();

    // A record (and key) made for some other library.
    let (_, other_record) = ring(&p).create(Some(common::pw("pw"))).unwrap();
    for locked in [false, true] {
        if locked {
            s.lock_key_ring();
        }
        let wrong = ring(&p)
            .rotate(
                &other_record,
                Some(common::pw("pw")),
                Some(common::pw("pw")),
            )
            .unwrap();
        assert!(matches!(s.rotate_key_ring(wrong), Err(Error::Invalid(_))));
        assert_eq!(
            s.key_record().unwrap().unwrap().as_bytes(),
            before.as_bytes()
        );
    }
}

/// PRIVACY-1/2: no older record survives a change, so neither the old OS-only record nor an old
/// password opens the library afterwards.
#[test]
fn no_earlier_key_record_is_kept_after_a_change() {
    let p = provider();
    let (dir, mut s, l) = on_disk();
    s.create_key_ring(&ring(&p), None).unwrap();
    let it = secret_item(l.id, "bank");
    s.insert_item(&it, &text_formats("kept"), Placement::Last)
        .unwrap();
    for (from, to) in [
        (None, Some("one")),
        (Some("one"), Some("two")),
        (Some("two"), None),
    ] {
        s.change_master_password(&ring(&p), from.map(common::pw), to.map(common::pw))
            .unwrap();
        let previous: Option<Vec<u8>> = raw(&dir)
            .query_row("SELECT previous FROM key_ring WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(previous, None, "after {from:?} -> {to:?}");
    }
    s.lock_key_ring();
    s.unlock_key_ring(&ring(&p), None).unwrap();
    assert_eq!(s.formats(it.id).unwrap(), text_formats("kept"));
}

/// PRIVACY-3: an earlier copy of the database (with its record) opens nothing written after a
/// password change, and the live library no longer opens with the old record's key.
#[test]
fn a_change_re_seals_everything_under_a_new_key() {
    let p = provider();
    let (dir, mut s, l) = on_disk();
    let big = s.put(vec![b'Q'; INLINE_LIMIT + 10]).unwrap();
    s.create_key_ring(&ring(&p), None).unwrap();
    let it = secret_item(l.id, "bank");
    let formats = Formats::new(vec![Format::Text(text("kept")), Format::Png(big)]).unwrap();
    s.insert_item(&it, &formats, Placement::Last).unwrap();
    s.add_recent(
        snipdeck_store::RecentContent::Text {
            text: "recent secret",
            html: None,
            rtf: None,
        },
        Some("app.exe"),
        at(5),
    )
    .unwrap();
    let old_record = s.key_record().unwrap().unwrap();
    let old_key = ring(&p).unlock(&old_record, None).unwrap();
    let in_blobs = |dir: &std::path::Path| {
        content_files(dir)
            .into_iter()
            .filter(|f| f.components().any(|c| c.as_os_str() == "blobs"))
            .collect::<Vec<_>>()
    };
    let blobs_before = in_blobs(dir.path());
    assert!(!blobs_before.is_empty());

    s.change_master_password(&ring(&p), None, Some(common::pw("pw")))
        .unwrap();

    // The old record still opens the old key, but that key opens nothing now stored.
    let (stored, kind): (Vec<u8>, String) = raw(&dir)
        .query_row(
            "SELECT inline_bytes, kind FROM item_formats WHERE kind = 'text'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(kind, "text");
    assert!(
        snipdeck_store::crypto::decrypt_payload(
            &old_key,
            &it.id,
            snipdeck_store::crypto::Slot::Format(snipdeck_core::FormatKind::Text),
            &stored
        )
        .is_err()
    );
    let after = in_blobs(dir.path());
    assert!(
        blobs_before.iter().all(|b| !after.contains(b)),
        "old envelope files are gone"
    );
    assert_eq!(s.formats(it.id).unwrap(), formats);
    assert_eq!(s.recent().unwrap().len(), 1);
    s.lock_key_ring();
    drop(s);
    let mut s = Store::open(dir.path()).unwrap();
    assert!(matches!(
        s.unlock_key_ring(&ring(&p), Some(common::pw("old"))),
        Err(Error::Crypto(CryptoError::WrongPassword))
    ));
    s.unlock_key_ring(&ring(&p), Some(common::pw("pw")))
        .unwrap();
    assert_eq!(s.formats(it.id).unwrap(), formats);
    assert_eq!(s.recent().unwrap()[0].text, "recent secret");
}

/// A library made before this change may still hold a previous record: opening it clears it,
/// and it still opens with the right secret and not the wrong one.
#[test]
fn an_older_library_loses_its_previous_record_and_still_opens() {
    let p = provider();
    let dir = tempfile::tempdir().unwrap();
    let item;
    {
        let mut s = Store::open(dir.path()).unwrap();
        let b = board(&mut s, "b");
        let l = list(&mut s, b.id, "l");
        s.create_key_ring(&ring(&p), Some(common::pw("pw")))
            .unwrap();
        item = secret_item(l.id, "bank");
        s.insert_item(&item, &text_formats("kept"), Placement::Last)
            .unwrap();
    }
    {
        let conn = raw(&dir);
        // Put the file back to what version 9 had: the columns and indexes of versions 12 to 16
        // go, so that opening runs the whole of version 10 onward again.
        conn.execute_batch(
            "UPDATE key_ring SET previous = record;
             DROP INDEX item_groups_by_parent;
             ALTER TABLE item_groups DROP COLUMN parent;
             DROP INDEX tags_by_parent;
             ALTER TABLE tags DROP COLUMN parent;
             ALTER TABLE item_groups DROP COLUMN colour;
             ALTER TABLE tags DROP COLUMN colour;
             ALTER TABLE boards DROP COLUMN accent;
             ALTER TABLE boards DROP COLUMN icon;
             ALTER TABLE triggers DROP COLUMN fires;
             ALTER TABLE triggers DROP COLUMN case_mode;
             PRAGMA user_version = 9;",
        )
        .unwrap();
    }
    let mut s = Store::open(dir.path()).unwrap();
    let previous: Option<Vec<u8>> = raw(&dir)
        .query_row("SELECT previous FROM key_ring WHERE id = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(previous, None);
    assert!(matches!(
        s.unlock_key_ring(&ring(&p), Some(common::pw("nope"))),
        Err(Error::Crypto(CryptoError::WrongPassword))
    ));
    s.unlock_key_ring(&ring(&p), Some(common::pw("pw")))
        .unwrap();
    assert_eq!(s.formats(item.id).unwrap(), text_formats("kept"));
}

#[test]
fn plain_formats_returns_every_unencrypted_live_item_in_one_pass() {
    let prov = provider();
    let (_dir, mut s, lst) = on_disk();
    s.create_key_ring(&ring(&prov), None).unwrap();

    let big = s.put(vec![b'Q'; INLINE_LIMIT + 10]).unwrap();
    let rich = Formats::new(vec![
        Format::Text(text("plain")),
        Format::Png(big),
        Format::Files(vec![
            FileEntry::new("a.txt", text("file-a")).unwrap(),
            FileEntry::new("b.txt", text("file-b")).unwrap(),
        ]),
    ])
    .unwrap();
    let first = Item::new(lst.id, "rich".to_owned(), ItemKind::Text, at(0));
    s.insert_item(&first, &rich, Placement::Last).unwrap();
    let second = Item::new(lst.id, "simple".to_owned(), ItemKind::Text, at(1));
    s.insert_item(&second, &text_formats("two"), Placement::Last)
        .unwrap();
    let secret = secret_item(lst.id, "secret");
    s.insert_item(&secret, &text_formats("hidden"), Placement::Last)
        .unwrap();
    let gone = Item::new(lst.id, "trashed".to_owned(), ItemKind::Text, at(2));
    s.insert_item(&gone, &text_formats("gone"), Placement::Last)
        .unwrap();
    s.trash(Trashable::Item(gone.id), at(3)).unwrap();

    let mut got = s.plain_formats().unwrap();
    got.sort_by_key(|(id, _)| *id == second.id);
    assert_eq!(got.len(), 2, "no encrypted and no trashed item");
    assert_eq!(
        got[0],
        (first.id, rich),
        "same as formats(), files and blob included"
    );
    assert_eq!(got[1], (second.id, text_formats("two")));
    assert_eq!(s.formats(first.id).unwrap(), got[0].1);
}

#[test]
fn rich_item_ids_finds_the_html_and_rtf_items_without_reading_content() {
    let prov = provider();
    let (_dir, mut s, lst) = on_disk();
    s.create_key_ring(&ring(&prov), None).unwrap();
    let plain = Item::new(lst.id, "plain".to_owned(), ItemKind::Text, at(0));
    s.insert_item(&plain, &text_formats("p"), Placement::Last)
        .unwrap();
    let rich = Item::new(lst.id, "rich".to_owned(), ItemKind::Text, at(1));
    let both = Formats::new(vec![
        Format::Text(text("t")),
        Format::Html(text("<b>t</b>")),
        Format::Rtf(text("{\\rtf1 t}")),
    ])
    .unwrap();
    s.insert_item(&rich, &both, Placement::Last).unwrap();
    let secret = secret_item(lst.id, "secret");
    let secret_rich = Formats::new(vec![
        Format::Text(text("s")),
        Format::Html(text("<i>s</i>")),
    ])
    .unwrap();
    s.insert_item(&secret, &secret_rich, Placement::Last)
        .unwrap();
    let gone = Item::new(lst.id, "gone".to_owned(), ItemKind::Text, at(2));
    s.insert_item(&gone, &both, Placement::Last).unwrap();
    s.trash(Trashable::Item(gone.id), at(3)).unwrap();

    assert_eq!(
        s.rich_item_ids().unwrap(),
        vec![rich.id],
        "once each, live, unencrypted only"
    );
}
