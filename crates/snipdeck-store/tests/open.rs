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
    assert_eq!(store.schema_version().unwrap(), 6);
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
    assert_eq!(a.schema_version().unwrap(), 6);
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
                supported: 6
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
