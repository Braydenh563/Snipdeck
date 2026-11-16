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
    assert_eq!(store.schema_version().unwrap(), 9);
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
    assert_eq!(a.schema_version().unwrap(), 9);
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
                supported: 9
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
    assert_eq!(store.schema_version().unwrap(), 9);
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
