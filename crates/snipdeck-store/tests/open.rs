//! Opening, locking and reopening the data folder.
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
use snipdeck_store::{Error, Store};

#[test]
fn opening_a_new_folder_creates_the_database_and_lock() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("data").join("nested");
    let store = Store::open(&folder).unwrap();
    assert!(folder.join("snipdeck.db").is_file());
    assert!(folder.join("snipdeck.lock").is_file());
    assert_eq!(
        store.schema_version().unwrap(),
        snipdeck_store::SCHEMA_VERSION
    );
}

#[test]
fn the_database_is_in_wal_mode_with_foreign_keys_on() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.pragma_text("journal_mode").unwrap(), "wal");
    assert_eq!(store.pragma_text("foreign_keys").unwrap(), "1");
    // FULL (2): a commit that released a blob file must survive a power cut, or the surviving
    // row would point at a deleted blob (Fable v1 sweep, B1).
    assert_eq!(store.pragma_text("synchronous").unwrap(), "2");
    // WAL is persistent: a plain connection sees it too.
    let raw = Connection::open(dir.path().join("snipdeck.db")).unwrap();
    let mode: String = raw
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
}

#[test]
fn a_second_opener_gets_a_clear_error_until_the_first_closes() {
    let dir = tempfile::tempdir().unwrap();
    let first = Store::open(dir.path()).unwrap();
    let err = Store::open(dir.path()).err().unwrap();
    assert!(matches!(err, Error::Locked { .. }), "{err:?}");
    let message = err.to_string();
    assert!(
        message.contains("another Snipdeck") && message.contains("data folder"),
        "{message}"
    );
    drop(first);
    Store::open(dir.path()).unwrap();
}

#[test]
fn the_lock_does_not_stop_a_different_folder() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let _a = Store::open(a.path()).unwrap();
    let _b = Store::open(b.path()).unwrap();
}

#[test]
fn data_survives_reopening() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut s = Store::open(dir.path()).unwrap();
        board(&mut s, "Work");
    }
    let s = Store::open(dir.path()).unwrap();
    assert_eq!(board_names(&s), ["Work"]);
}

#[test]
fn in_memory_stores_are_independent() {
    let mut a = store();
    let b = store();
    board(&mut a, "only in a");
    assert_eq!(board_names(&b).len(), 0);
    assert_eq!(a.schema_version().unwrap(), snipdeck_store::SCHEMA_VERSION);
}

#[test]
fn a_database_from_a_newer_snipdeck_is_refused_and_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open(dir.path()).unwrap());
    let path = dir.path().join("snipdeck.db");
    {
        let raw = Connection::open(&path).unwrap();
        raw.pragma_update(None, "user_version", 99).unwrap();
    }
    let err = Store::open(dir.path()).err().unwrap();
    assert!(
        matches!(
            err,
            Error::SchemaTooNew {
                found: 99,
                supported: snipdeck_store::SCHEMA_VERSION
            }
        ),
        "{err:?}"
    );
    assert!(err.to_string().contains("newer Snipdeck"));

    // The refusal released the lock and changed nothing.
    let raw = Connection::open(&path).unwrap();
    let version: u32 = raw
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 99);
}

#[test]
fn a_foreign_sqlite_file_is_not_adopted() {
    let dir = tempfile::tempdir().unwrap();
    {
        let raw = Connection::open(dir.path().join("snipdeck.db")).unwrap();
        raw.execute_batch("CREATE TABLE unrelated (x);").unwrap();
    }
    let err = Store::open(dir.path()).err().unwrap();
    assert!(matches!(err, Error::ForeignDatabase { .. }), "{err:?}");
}

#[test]
fn an_empty_database_file_beside_other_library_files_is_refused_not_replaced() {
    let dir = tempfile::tempdir().unwrap();
    {
        let store = Store::open(dir.path()).unwrap();
        drop(store.close());
    }
    // A crashed sync client or a full disk leaves the file at zero bytes.
    std::fs::write(dir.path().join("snipdeck.db"), b"").unwrap();
    for stale in ["snipdeck.db-wal", "snipdeck.db-shm"] {
        let _ = std::fs::remove_file(dir.path().join(stale));
    }
    // Other library files exist: a backup folder with a file in it.
    std::fs::create_dir_all(dir.path().join("backups")).unwrap();
    std::fs::write(
        dir.path()
            .join("backups")
            .join("snipdeck-2026-01-01.snipdeck"),
        b"x",
    )
    .unwrap();
    let err = Store::open(dir.path()).err().unwrap();
    assert!(matches!(err, Error::EmptyDatabase { .. }), "{err:?}");
    assert_eq!(
        std::fs::metadata(dir.path().join("snipdeck.db"))
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn a_read_only_handle_checks_and_counts_beside_the_owner() {
    use snipdeck_core::{Item, ItemKind, Placement};
    use snipdeck_store::ItemRepo;
    let dir = tempfile::tempdir().unwrap();
    let mut owner = Store::open(dir.path()).unwrap();
    let b = board(&mut owner, "B");
    let l = list(&mut owner, b.id, "L");
    let mut it = Item::new(l.id, "pin".to_owned(), ItemKind::Text, at(0));
    it.sensitive = true;
    owner
        .insert_item(&it, &text_formats("1234"), Placement::Last)
        .unwrap();
    item(&mut owner, l.id, "plain", "x");
    // The owner still holds the folder lock; the reader takes none.
    let reader = Store::open_reader(dir.path(), None).unwrap();
    assert_eq!(reader.count_sensitive().unwrap(), 1);
    assert_eq!(owner.count_sensitive().unwrap(), 1);
    assert!(reader.key_ring_exists().is_ok());
    assert!(reader.check_integrity().is_ok());
}

#[test]
fn a_damaged_search_index_is_not_reported_healthy_by_a_read_only_handle() {
    let dir = tempfile::tempdir().unwrap();
    let mut owner = Store::open(dir.path()).unwrap();
    let b = board(&mut owner, "B");
    let l = list(&mut owner, b.id, "L");
    item(&mut owner, l.id, "alpha", "some searchable body text");
    item(&mut owner, l.id, "beta", "more searchable body text");
    {
        let raw = Connection::open(dir.path().join("snipdeck.db")).unwrap();
        raw.execute("UPDATE item_fts_data SET block = x'0000' WHERE id > 10", [])
            .unwrap();
    }
    let reader = Store::open_reader(dir.path(), None).unwrap();
    let problems = reader.check_integrity().unwrap();
    assert!(
        problems.iter().any(|p| p.contains("full-text index")),
        "{problems:?}"
    );
}

#[test]
fn a_read_only_open_creates_nothing_and_a_missing_library_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nowhere");
    let err = Store::open_read_only(&missing).expect_err("no library");
    assert!(matches!(err, Error::Invalid(_)), "{err}");
    assert!(!missing.exists(), "the folder must not be created");

    let empty = dir.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    assert!(Store::open_read_only(&empty).is_err());
    assert_eq!(
        std::fs::read_dir(&empty).unwrap().count(),
        0,
        "no database made"
    );
}

#[test]
fn an_empty_database_file_in_a_folder_with_nothing_else_opens_as_new() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("snipdeck.db"), b"").unwrap();
    Store::open(dir.path()).unwrap();
}

#[test]
fn a_damaged_database_is_set_aside_and_a_fresh_library_opens() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("snipdeck.db"), vec![0x42u8; 4096]).unwrap();
    std::fs::create_dir_all(dir.path().join("blobs")).unwrap();
    std::fs::write(dir.path().join("blobs").join("keepme"), b"x").unwrap();
    assert!(Store::open(dir.path()).is_err());

    let (store, kept) =
        Store::open_or_set_aside(dir.path(), "snipdeck.damaged-20261116-1000").unwrap();
    assert_eq!(kept.as_deref(), Some("snipdeck.damaged-20261116-1000"));
    assert_eq!(
        store.schema_version().unwrap(),
        snipdeck_store::SCHEMA_VERSION
    );
    let aside = dir.path().join("snipdeck.damaged-20261116-1000");
    assert_eq!(
        std::fs::read(aside.join("snipdeck.db")).unwrap(),
        vec![0x42u8; 4096]
    );
    assert!(aside.join("blobs").join("keepme").is_file());
    assert!(!dir.path().join("blobs").join("keepme").exists());
    assert!(!dir.path().join("snipdeck.rebuild").exists());
}

#[test]
fn an_empty_database_beside_backups_is_set_aside_too() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("snipdeck.db"), b"").unwrap();
    std::fs::create_dir_all(dir.path().join("backups")).unwrap();
    std::fs::write(dir.path().join("backups").join("a.snipdeck"), b"x").unwrap();
    let (_store, kept) = Store::open_or_set_aside(dir.path(), "snipdeck.damaged-1").unwrap();
    assert!(kept.is_some());
    // The backups stay where Rebuild looks for them.
    assert!(dir.path().join("backups").join("a.snipdeck").is_file());
}

#[test]
fn a_healthy_library_opens_with_nothing_set_aside() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open(dir.path()).unwrap());
    let (_store, kept) = Store::open_or_set_aside(dir.path(), "snipdeck.damaged-2").unwrap();
    assert!(kept.is_none());
    assert!(!dir.path().join("snipdeck.damaged-2").exists());
}

#[test]
fn a_locked_folder_is_never_set_aside() {
    let dir = tempfile::tempdir().unwrap();
    let _first = Store::open(dir.path()).unwrap();
    let err = Store::open_or_set_aside(dir.path(), "snipdeck.damaged-3")
        .err()
        .unwrap();
    assert!(matches!(err, Error::Locked { .. }), "{err:?}");
    assert!(!dir.path().join("snipdeck.damaged-3").exists());
}

#[test]
fn a_newer_database_is_never_set_aside() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open(dir.path()).unwrap());
    let raw = Connection::open(dir.path().join("snipdeck.db")).unwrap();
    raw.pragma_update(None, "user_version", 999).unwrap();
    drop(raw);
    let err = Store::open_or_set_aside(dir.path(), "snipdeck.damaged-4")
        .err()
        .unwrap();
    assert!(matches!(err, Error::SchemaTooNew { .. }), "{err:?}");
    assert!(dir.path().join("snipdeck.db").is_file());
    assert!(!dir.path().join("snipdeck.damaged-4").exists());
}

#[test]
fn a_busy_database_is_not_reported_as_a_damaged_search_index() {
    let dir = tempfile::tempdir().unwrap();
    let mut owner = Store::open(dir.path()).unwrap();
    let b = board(&mut owner, "B");
    let l = list(&mut owner, b.id, "L");
    item(&mut owner, l.id, "alpha", "some searchable body text");
    let reader = Store::open_reader(dir.path(), None).unwrap();
    // Another writer holds the write lock, so the check's writable connection times out.
    let raw = Connection::open(dir.path().join("snipdeck.db")).unwrap();
    raw.execute_batch("BEGIN IMMEDIATE").unwrap();
    let problems = reader.check_integrity().unwrap();
    raw.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        problems.iter().filter(|p| p.contains("damaged")).count(),
        0,
        "{problems:?}"
    );
}

#[test]
fn a_read_only_store_checks_a_library_beside_a_running_app() {
    let dir = tempfile::tempdir().unwrap();
    let _running = Store::open(dir.path()).unwrap();
    let ro = Store::open_read_only(dir.path()).expect("no folder lock is needed");
    assert!(ro.check_integrity().unwrap().is_empty());
}
