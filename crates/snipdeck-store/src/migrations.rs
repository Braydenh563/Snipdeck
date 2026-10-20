//! Schema migrations: an ordered, embedded, forward-only list, applied on open.
//!
//! The applied version lives in SQLite's `user_version` header field rather than in a table of
//! our own: it is written in the same transaction as the migration (so a migration and its
//! version bump are atomic), costs no table, and every SQLite tool can read it. The file is
//! recognised as ours by `application_id`.
//!
//! See `docs/store.md` for the policy and for how to add a migration.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension};

use crate::{Error, Result, schema};

/// Marks a SQLite file as Snipdeck's.
pub(crate) const APPLICATION_ID: i32 = 0x534E_444B;

/// One forward step of the schema.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Migration {
    /// The version the database is at after this step; versions are 1, 2, 3, 4, 5, 6 without gaps.
    pub version: u32,
    /// Short description, for error messages.
    pub name: &'static str,
    /// SQL run inside a transaction. Do not put `BEGIN`/`COMMIT` or `PRAGMA user_version` in it.
    pub sql: &'static str,
    /// Data changes SQL cannot express, run after `sql` in the same transaction.
    pub post: Option<fn(&Connection) -> Result<()>>,
}

/// Every migration, oldest first. Append only: a released entry is never edited or removed.
pub(crate) const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial schema",
        sql: schema::V1,
        post: None,
    },
    Migration {
        version: 2,
        name: "encryption at rest, secure delete, folded tag keys",
        sql: schema::V2,
        post: Some(crate::tags::rekey),
    },
    Migration {
        version: 3,
        name: "recent inbox",
        sql: crate::recent::V3,
        post: None,
    },
    Migration {
        version: 4,
        name: "item colour labels and list icons",
        sql: schema::V4,
        post: None,
    },
    Migration {
        version: 5,
        name: "recent inbox encrypted at rest",
        sql: crate::recent::V5,
        post: None,
    },
    Migration {
        version: 6,
        name: "previous key record kept on replacement",
        sql: schema::V6,
        post: None,
    },
];

/// The newest schema version this build writes.
pub(crate) fn latest(migrations: &[Migration]) -> u32 {
    migrations.last().map_or(0, |m| m.version)
}

/// What [`migrate`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Outcome {
    /// The version the database was at when opened (0 for a new file).
    pub from: u32,
    /// The version it is at now.
    pub to: u32,
}

/// Brings `conn` up to the newest version in `migrations`.
///
/// `path` names the file for error messages. When `backup` is true and the database already
/// holds data (version 1 or later) a copy is written next to it as `<file>.pre-v<N>.bak`
/// before the first step, using `VACUUM INTO`, so a bad migration never costs the user their
/// library.
pub(crate) fn migrate(
    conn: &mut Connection,
    migrations: &[Migration],
    path: &Path,
    backup: bool,
) -> Result<Outcome> {
    let current: u32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let latest = latest(migrations);

    if current == 0 {
        let tables: i64 = conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0))?;
        if tables > 0 {
            return Err(foreign(path));
        }
    } else {
        let app: i32 = conn.pragma_query_value(None, "application_id", |r| r.get(0))?;
        if app != APPLICATION_ID {
            return Err(foreign(path));
        }
    }
    if current > latest {
        return Err(Error::SchemaTooNew {
            found: current,
            supported: latest,
        });
    }
    if backup && current > 0 {
        scrub_older_backups(path, current);
    }
    if current == latest {
        return Ok(Outcome {
            from: current,
            to: current,
        });
    }

    if backup && current > 0 {
        write_backup(conn, path, current)?;
    }

    // Rebuilding a table (the only way SQLite can change some constraints) means dropping it
    // while other tables point at it, which `foreign_keys = ON` forbids; the pragma cannot be
    // changed inside a transaction. So: off, migrate, check every reference, on again.
    conn.pragma_update(None, "foreign_keys", false)?;
    let result = run_steps(conn, migrations, current);
    conn.pragma_update(None, "foreign_keys", true)?;
    result?;
    Ok(Outcome {
        from: current,
        to: latest,
    })
}

fn foreign(path: &Path) -> Error {
    Error::ForeignDatabase {
        path: path.to_owned(),
    }
}

fn run_steps(conn: &mut Connection, migrations: &[Migration], current: u32) -> Result<()> {
    for step in migrations.iter().filter(|m| m.version > current) {
        // The source error is kept (review N7), its text repeated in `reason`.
        let fail = |source: Option<rusqlite::Error>, reason: String| Error::Migration {
            version: step.version,
            name: step.name,
            reason,
            source,
        };
        let sqlite = |e: rusqlite::Error| {
            let reason = e.to_string();
            fail(Some(e), reason)
        };
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sqlite)?;
        tx.execute_batch(step.sql).map_err(sqlite)?;
        if let Some(post) = step.post {
            post(&tx).map_err(|e| match e {
                Error::Sqlite(e) => sqlite(e),
                other => fail(None, other.to_string()),
            })?;
        }
        let dangling: Option<String> = tx
            .query_row(
                "SELECT \"table\" FROM pragma_foreign_key_check LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(sqlite)?;
        if let Some(table) = dangling {
            return Err(fail(
                None,
                format!("rows in `{table}` refer to rows that do not exist"),
            ));
        }
        // `PRAGMA` cannot bind a parameter; `version` is a number from our own list.
        tx.execute_batch(&format!("PRAGMA user_version = {};", step.version))
            .map_err(sqlite)?;
        tx.commit().map_err(sqlite)?;
    }
    Ok(())
}

/// Copies the database to `<file>.pre-v<from>.bak`, replacing an older copy. The new copy is
/// written beside it and renamed over the old one, so a failed copy leaves the old one.
fn write_backup(conn: &Connection, path: &Path, from: u32) -> Result<()> {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".pre-v{from}.bak"));
    let target = std::path::PathBuf::from(name);
    let mut temp_name = target.as_os_str().to_owned();
    temp_name.push(".tmp");
    let temp = std::path::PathBuf::from(temp_name);
    // `VACUUM INTO` needs a file that does not exist; this one is ours, from an earlier crash.
    match std::fs::remove_file(&temp) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let copied = conn
        .execute("VACUUM INTO ?1", [&temp.to_string_lossy().into_owned()])
        .map_err(Error::from);
    if let Err(e) = copied {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    if from < 5 {
        scrub_or_delete(&temp);
    }
    if !temp.exists() {
        // Deleted because it could not be scrubbed: the old copy is just as plain, so it goes too.
        return match std::fs::remove_file(&target) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        };
    }
    if let Err(e) = std::fs::rename(&temp, &target) {
        let _ = std::fs::remove_file(&temp);
        return Err(e.into());
    }
    Ok(())
}

/// The first schema version whose Recent text is sealed; earlier copies hold it plain.
const SEALED_RECENT_SINCE: u32 = 5;

/// Scrubs the Recent inbox of the rollback copies earlier upgrades left (`pre-v1` to `pre-v4`):
/// those hold plain text too. Runs at every open that has a backup name to look for, and does
/// nothing for a copy that is not there or has no entries left.
fn scrub_older_backups(path: &Path, current: u32) {
    for from in 1..SEALED_RECENT_SINCE.min(current + 1) {
        let mut name = path.as_os_str().to_owned();
        name.push(format!(".pre-v{from}.bak"));
        let copy = std::path::PathBuf::from(name);
        if copy.is_file() {
            scrub_or_delete(&copy);
        }
    }
}

/// Scrubs the Recent inbox of a backup; if that fails (a file held by antivirus, a damaged copy)
/// the backup is deleted instead, because a plain copy is worse than none and a failed scrub
/// must not stop the library opening.
fn scrub_or_delete(backup: &Path) {
    if scrub_recent(&backup.to_string_lossy()).is_err() {
        let _ = std::fs::remove_file(backup);
    }
}

/// Empties the Recent inbox in a backup: its text was plain before schema version 5, and a
/// rollback copy is not a reason to keep it. Nothing to do for a table that is not there (older
/// schemas have none) or is empty; a file that cannot be opened or rewritten is an error.
fn scrub_recent(backup: &str) -> Result<()> {
    let copy = Connection::open(backup)?;
    // No -wal or -journal file: the old pages must not pass through a file that is only deleted.
    copy.pragma_update(None, "journal_mode", "MEMORY")?;
    copy.pragma_update(None, "secure_delete", "ON")?;
    let has_table: bool = copy.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'recent_entries')",
        [],
        |r| r.get(0),
    )?;
    if has_table {
        let entries: i64 =
            copy.query_row("SELECT count(*) FROM recent_entries", [], |r| r.get(0))?;
        if entries > 0 {
            copy.execute_batch("DELETE FROM recent_entries; VACUUM;")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    const NEXT_ADD_COLUMN: Migration = Migration {
        version: 7,
        name: "add boards.colour",
        sql: "ALTER TABLE boards ADD COLUMN colour TEXT;",
        post: None,
    };

    const NEXT_BROKEN: Migration = Migration {
        version: 7,
        name: "broken",
        sql: "ALTER TABLE boards ADD COLUMN colour TEXT; THIS IS NOT SQL;",
        post: None,
    };

    fn with_next(step: Migration) -> Vec<Migration> {
        let mut all = MIGRATIONS.to_vec();
        all.push(step);
        all
    }

    fn version(conn: &Connection) -> u32 {
        conn.pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap()
    }

    fn fresh() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    fn insert_board(conn: &Connection) {
        conn.execute(
            "INSERT INTO boards (id, name, rank, created_at, updated_at) VALUES (?1, 'Work', 1, 0, 0)",
            params![vec![1u8; 16]],
        )
        .unwrap();
    }

    #[test]
    fn versions_are_contiguous_from_one() {
        for (i, m) in MIGRATIONS.iter().enumerate() {
            assert_eq!(m.version as usize, i + 1, "{}", m.name);
        }
        assert_eq!(latest(MIGRATIONS), u32::try_from(MIGRATIONS.len()).unwrap());
    }

    #[test]
    fn a_new_database_is_created_at_the_latest_version() {
        let mut conn = fresh();
        let outcome = migrate(&mut conn, MIGRATIONS, Path::new(":memory:"), false).unwrap();
        assert_eq!(
            outcome,
            Outcome {
                from: 0,
                to: latest(MIGRATIONS)
            }
        );
        assert_eq!(version(&conn), latest(MIGRATIONS));
        let app: i32 = conn
            .pragma_query_value(None, "application_id", |r| r.get(0))
            .unwrap();
        assert_eq!(app, APPLICATION_ID);
    }

    #[test]
    fn migrating_twice_does_nothing_the_second_time() {
        let mut conn = fresh();
        migrate(&mut conn, MIGRATIONS, Path::new(":memory:"), false).unwrap();
        let outcome = migrate(&mut conn, MIGRATIONS, Path::new(":memory:"), false).unwrap();
        assert_eq!(outcome.from, outcome.to);
    }

    #[test]
    fn a_v1_database_is_migrated_forward_and_keeps_its_data() {
        let mut conn = fresh();
        migrate(&mut conn, MIGRATIONS, Path::new(":memory:"), false).unwrap();
        insert_board(&conn);

        let all = with_next(NEXT_ADD_COLUMN);
        let outcome = migrate(&mut conn, &all, Path::new(":memory:"), false).unwrap();
        assert_eq!(outcome, Outcome { from: 6, to: 7 });
        assert_eq!(version(&conn), 7);
        let (name, colour): (String, Option<String>) = conn
            .query_row("SELECT name, colour FROM boards", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((name.as_str(), colour), ("Work", None));
    }

    #[test]
    fn a_failing_migration_rolls_back_completely() {
        let mut conn = fresh();
        migrate(&mut conn, MIGRATIONS, Path::new(":memory:"), false).unwrap();
        insert_board(&conn);

        let err = migrate(
            &mut conn,
            &with_next(NEXT_BROKEN),
            Path::new(":memory:"),
            false,
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                Error::Migration {
                    version: 7,
                    name: "broken",
                    source: Some(_),
                    ..
                }
            ),
            "{err:?}"
        );
        assert!(
            std::error::Error::source(&err).is_some(),
            "the cause is kept"
        );
        assert_eq!(version(&conn), 6);
        // The half-applied ALTER TABLE was undone, and the data is intact.
        assert!(conn.prepare("SELECT colour FROM boards").is_err());
        let boards: i64 = conn
            .query_row("SELECT count(*) FROM boards", [], |r| r.get(0))
            .unwrap();
        assert_eq!(boards, 1);
        let fks: i64 = conn
            .pragma_query_value(None, "foreign_keys", |r| r.get(0))
            .unwrap();
        assert_eq!(fks, 1, "foreign keys are switched back on after a failure");
    }

    #[test]
    fn a_migration_leaving_dangling_references_is_refused() {
        let mut conn = fresh();
        migrate(&mut conn, MIGRATIONS, Path::new(":memory:"), false).unwrap();
        let dangling = Migration {
            version: 7,
            name: "orphan a list",
            sql: "INSERT INTO lists (id, board_id, name, list_view, list_sort, rank, created_at, updated_at)
                  VALUES (zeroblob(16), zeroblob(16), 'L', 'compact', 'manual', 1, 0, 0);",
            post: None,
        };
        let err = migrate(
            &mut conn,
            &with_next(dangling),
            Path::new(":memory:"),
            false,
        )
        .unwrap_err();
        assert!(
            matches!(err, Error::Migration { version: 7, .. }),
            "{err:?}"
        );
        assert_eq!(version(&conn), 6);
    }

    #[test]
    fn a_newer_database_is_refused_without_being_touched() {
        let mut conn = fresh();
        let all = with_next(NEXT_ADD_COLUMN);
        migrate(&mut conn, &all, Path::new(":memory:"), false).unwrap();
        insert_board(&conn);

        // The "old build" only knows the current version.
        let err = migrate(&mut conn, MIGRATIONS, Path::new(":memory:"), false).unwrap_err();
        assert!(
            matches!(
                err,
                Error::SchemaTooNew {
                    found: 7,
                    supported: 6
                }
            ),
            "{err:?}"
        );
        let message = err.to_string();
        assert!(
            message.contains("newer Snipdeck") && message.contains("update"),
            "{message}"
        );
        assert_eq!(version(&conn), 7);
    }

    #[test]
    fn someone_elses_sqlite_file_is_refused() {
        let mut conn = fresh();
        conn.execute_batch("CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT);")
            .unwrap();
        let err = migrate(&mut conn, MIGRATIONS, Path::new("notes.db"), false).unwrap_err();
        assert!(matches!(err, Error::ForeignDatabase { .. }), "{err:?}");
        // ... and was not modified.
        assert_eq!(version(&conn), 0);

        let mut other = fresh();
        other
            .execute_batch("PRAGMA user_version = 1; PRAGMA application_id = 42;")
            .unwrap();
        let err = migrate(&mut other, MIGRATIONS, Path::new("other.db"), false).unwrap_err();
        assert!(matches!(err, Error::ForeignDatabase { .. }), "{err:?}");
    }

    #[test]
    fn v2_and_v3_databases_upgrade_through_recent_and_labels() {
        let upto = |n: usize| MIGRATIONS[..n].to_vec();
        for start in [2usize, 3] {
            let mut conn = fresh();
            migrate(&mut conn, &upto(start), Path::new(":memory:"), false).unwrap();
            assert_eq!(version(&conn), u32::try_from(start).unwrap());
            insert_board(&conn);
            let outcome = migrate(&mut conn, MIGRATIONS, Path::new(":memory:"), false).unwrap();
            assert_eq!(
                outcome,
                Outcome {
                    from: u32::try_from(start).unwrap(),
                    to: 6
                }
            );
            assert_eq!(version(&conn), 6);
            for sql in [
                "SELECT label FROM items",
                "SELECT icon FROM lists",
                "SELECT hash FROM recent_entries",
            ] {
                assert!(conn.prepare(sql).is_ok(), "{sql}");
            }
            let boards: i64 = conn
                .query_row("SELECT count(*) FROM boards", [], |r| r.get(0))
                .unwrap();
            assert_eq!(boards, 1, "data kept");
        }
    }

    #[test]
    fn migrating_an_existing_file_leaves_a_backup_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snipdeck.db");
        let mut conn = Connection::open(&path).unwrap();
        migrate(&mut conn, MIGRATIONS, &path, true).unwrap();
        assert!(
            !dir.path().join("snipdeck.db.pre-v6.bak").exists(),
            "nothing to back up on creation"
        );
        insert_board(&conn);

        migrate(&mut conn, &with_next(NEXT_ADD_COLUMN), &path, true).unwrap();
        let backup = Connection::open(dir.path().join("snipdeck.db.pre-v6.bak")).unwrap();
        assert_eq!(version(&backup), 6);
        let boards: i64 = backup
            .query_row("SELECT count(*) FROM boards", [], |r| r.get(0))
            .unwrap();
        assert_eq!(boards, 1);
        assert!(
            backup.prepare("SELECT colour FROM boards").is_err(),
            "the copy predates the change"
        );
    }

    fn plain_copy_with_one_entry(path: &Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE recent_entries (text TEXT);
             INSERT INTO recent_entries VALUES ('left over');",
        )
        .unwrap();
    }

    fn entries(path: &Path) -> i64 {
        Connection::open(path)
            .unwrap()
            .query_row("SELECT count(*) FROM recent_entries", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn older_rollback_copies_lose_their_plain_recent_entries_and_a_broken_one_is_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snipdeck.db");
        let mut conn = Connection::open(&path).unwrap();
        migrate(&mut conn, MIGRATIONS, &path, true).unwrap();
        let v3 = dir.path().join("snipdeck.db.pre-v3.bak");
        let v4 = dir.path().join("snipdeck.db.pre-v4.bak");
        plain_copy_with_one_entry(&v3);
        std::fs::write(&v4, b"not a database, and held by nobody").unwrap();

        // A library already at the newest version still cleans the copies of earlier upgrades.
        migrate(&mut conn, MIGRATIONS, &path, true).unwrap();
        assert_eq!(entries(&v3), 0, "scrubbed");
        assert!(!v4.exists(), "a copy that cannot be scrubbed is deleted");
        assert_eq!(version(&conn), 6, "and the open went on");
    }

    #[test]
    fn a_scrubbed_copy_uses_no_journal_file_and_a_failed_scrub_does_not_fail_the_open() {
        let dir = tempfile::tempdir().unwrap();
        let copy = dir.path().join("copy.bak");
        plain_copy_with_one_entry(&copy);
        scrub_recent(&copy.to_string_lossy()).unwrap();
        assert_eq!(entries(&copy), 0);
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(
            leftovers.len(),
            1,
            "no -wal or -journal left: {leftovers:?}"
        );

        let broken = dir.path().join("broken.bak");
        std::fs::write(&broken, b"garbage garbage garbage garbage").unwrap();
        assert!(scrub_recent(&broken.to_string_lossy()).is_err());
        scrub_or_delete(&broken);
        assert!(!broken.exists());
    }

    #[test]
    fn a_failed_backup_keeps_the_previous_rollback_copy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snipdeck.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE t (x); PRAGMA user_version = 6;")
            .unwrap();
        let old = dir.path().join("snipdeck.db.pre-v6.bak");
        std::fs::write(&old, b"the previous copy").unwrap();
        // A directory sits where the new copy is built, so the copy cannot be made.
        std::fs::create_dir(dir.path().join("snipdeck.db.pre-v6.bak.tmp")).unwrap();
        assert!(write_backup(&conn, &path, 6).is_err());
        assert_eq!(std::fs::read(&old).unwrap(), b"the previous copy");

        std::fs::remove_dir(dir.path().join("snipdeck.db.pre-v6.bak.tmp")).unwrap();
        write_backup(&conn, &path, 6).unwrap();
        assert_eq!(version(&Connection::open(&old).unwrap()), 6);
        assert!(!dir.path().join("snipdeck.db.pre-v6.bak.tmp").exists());
    }

    /// A real file written by the released version-1 schema (see `tests/fixture.rs`).
    const V1_FIXTURE: &[u8] = include_bytes!("../tests/fixtures/v1.db");

    #[test]
    fn the_v1_fixture_file_migrates_forward_keeping_every_row_and_leaving_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snipdeck.db");
        std::fs::write(&path, V1_FIXTURE).unwrap();
        let mut conn = Connection::open(&path).unwrap();
        assert_eq!(version(&conn), 1);

        // A table rebuild, the migration shape that needs foreign keys off: widen the
        // `list_view` check by copying `lists` into a new table.
        let rebuild = Migration {
            version: 7,
            name: "rebuild lists",
            sql: "CREATE TABLE lists_new (
                      id BLOB NOT NULL PRIMARY KEY CHECK (length(id) = 16),
                      board_id BLOB NOT NULL REFERENCES boards (id) ON DELETE CASCADE,
                      name TEXT NOT NULL CHECK (length(name) > 0),
                      accent TEXT, list_view TEXT NOT NULL, list_sort TEXT NOT NULL,
                      rank INTEGER NOT NULL, created_at INTEGER NOT NULL,
                      updated_at INTEGER NOT NULL, deleted_at INTEGER, icon TEXT,
                      UNIQUE (board_id, rank)) STRICT;
                  INSERT INTO lists_new SELECT * FROM lists;
                  DROP TABLE lists;
                  ALTER TABLE lists_new RENAME TO lists;",
            post: None,
        };
        let counts = |conn: &Connection| -> Vec<i64> {
            [
                "boards",
                "lists",
                "items",
                "item_formats",
                "tags",
                "item_tags",
            ]
            .iter()
            .map(|t| {
                conn.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))
                    .unwrap()
            })
            .collect()
        };
        let before = counts(&conn);
        assert_eq!(before, [1, 1, 3, 4, 1, 1]);
        let outcome = migrate(&mut conn, &with_next(rebuild), &path, true).unwrap();
        assert_eq!(outcome, Outcome { from: 1, to: 7 });
        assert_eq!(counts(&conn), before);
        let items_still_reference_the_list: i64 = conn
            .query_row(
                "SELECT count(*) FROM items i JOIN lists l ON l.id = i.list_id",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(items_still_reference_the_list, 3);
        assert!(dir.path().join("snipdeck.db.pre-v1.bak").is_file());
    }
}
