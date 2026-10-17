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
    /// The version the database is at after this step; versions are 1, 2, 3 without gaps.
    pub version: u32,
    /// Short description, for error messages.
    pub name: &'static str,
    /// SQL run inside a transaction. Do not put `BEGIN`/`COMMIT` or `PRAGMA user_version` in it.
    pub sql: &'static str,
}

/// Every migration, oldest first. Append only: a released entry is never edited or removed.
pub(crate) const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "initial schema",
    sql: schema::V1,
}];

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
        let fail = |reason: String| Error::Migration {
            version: step.version,
            name: step.name,
            reason,
        };
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| fail(e.to_string()))?;
        tx.execute_batch(step.sql)
            .map_err(|e| fail(e.to_string()))?;
        let dangling: Option<String> = tx
            .query_row(
                "SELECT \"table\" FROM pragma_foreign_key_check LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| fail(e.to_string()))?;
        if let Some(table) = dangling {
            return Err(fail(format!(
                "rows in `{table}` refer to rows that do not exist"
            )));
        }
        // `PRAGMA` cannot bind a parameter; `version` is a number from our own list.
        tx.execute_batch(&format!("PRAGMA user_version = {};", step.version))
            .map_err(|e| fail(e.to_string()))?;
        tx.commit().map_err(|e| fail(e.to_string()))?;
    }
    Ok(())
}

/// Copies the database to `<file>.pre-v<from>.bak`, replacing an older copy.
fn write_backup(conn: &Connection, path: &Path, from: u32) -> Result<()> {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".pre-v{from}.bak"));
    let target = std::path::PathBuf::from(name);
    match std::fs::remove_file(&target) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let target = target.to_string_lossy().into_owned();
    conn.execute("VACUUM INTO ?1", [target])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    const V2_ADD_COLUMN: Migration = Migration {
        version: 2,
        name: "add boards.colour",
        sql: "ALTER TABLE boards ADD COLUMN colour TEXT;",
    };

    const V2_BROKEN: Migration = Migration {
        version: 2,
        name: "broken",
        sql: "ALTER TABLE boards ADD COLUMN colour TEXT; THIS IS NOT SQL;",
    };

    fn with_v2(step: Migration) -> Vec<Migration> {
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

        let all = with_v2(V2_ADD_COLUMN);
        let outcome = migrate(&mut conn, &all, Path::new(":memory:"), false).unwrap();
        assert_eq!(outcome, Outcome { from: 1, to: 2 });
        assert_eq!(version(&conn), 2);
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

        let err =
            migrate(&mut conn, &with_v2(V2_BROKEN), Path::new(":memory:"), false).unwrap_err();
        assert!(
            matches!(
                err,
                Error::Migration {
                    version: 2,
                    name: "broken",
                    ..
                }
            ),
            "{err:?}"
        );
        assert_eq!(version(&conn), 1);
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
            version: 2,
            name: "orphan a list",
            sql: "INSERT INTO lists (id, board_id, name, list_view, list_sort, rank, created_at, updated_at)
                  VALUES (zeroblob(16), zeroblob(16), 'L', 'compact', 'manual', 1, 0, 0);",
        };
        let err = migrate(&mut conn, &with_v2(dangling), Path::new(":memory:"), false).unwrap_err();
        assert!(
            matches!(err, Error::Migration { version: 2, .. }),
            "{err:?}"
        );
        assert_eq!(version(&conn), 1);
    }

    #[test]
    fn a_newer_database_is_refused_without_being_touched() {
        let mut conn = fresh();
        let all = with_v2(V2_ADD_COLUMN);
        migrate(&mut conn, &all, Path::new(":memory:"), false).unwrap();
        insert_board(&conn);

        // The "old build" only knows version 1.
        let err = migrate(&mut conn, MIGRATIONS, Path::new(":memory:"), false).unwrap_err();
        assert!(
            matches!(
                err,
                Error::SchemaTooNew {
                    found: 2,
                    supported: 1
                }
            ),
            "{err:?}"
        );
        let message = err.to_string();
        assert!(
            message.contains("newer Snipdeck") && message.contains("update"),
            "{message}"
        );
        assert_eq!(version(&conn), 2);
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
    fn migrating_an_existing_file_leaves_a_backup_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snipdeck.db");
        let mut conn = Connection::open(&path).unwrap();
        migrate(&mut conn, MIGRATIONS, &path, true).unwrap();
        assert!(
            !dir.path().join("snipdeck.db.pre-v1.bak").exists(),
            "nothing to back up on creation"
        );
        insert_board(&conn);

        migrate(&mut conn, &with_v2(V2_ADD_COLUMN), &path, true).unwrap();
        let backup = Connection::open(dir.path().join("snipdeck.db.pre-v1.bak")).unwrap();
        assert_eq!(version(&backup), 1);
        let boards: i64 = backup
            .query_row("SELECT count(*) FROM boards", [], |r| r.get(0))
            .unwrap();
        assert_eq!(boards, 1);
        assert!(
            backup.prepare("SELECT colour FROM boards").is_err(),
            "the copy predates the change"
        );
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
            version: 2,
            name: "rebuild lists",
            sql: "CREATE TABLE lists_new (
                      id BLOB NOT NULL PRIMARY KEY CHECK (length(id) = 16),
                      board_id BLOB NOT NULL REFERENCES boards (id) ON DELETE CASCADE,
                      name TEXT NOT NULL CHECK (length(name) > 0),
                      accent TEXT, list_view TEXT NOT NULL, list_sort TEXT NOT NULL,
                      rank INTEGER NOT NULL, created_at INTEGER NOT NULL,
                      updated_at INTEGER NOT NULL, deleted_at INTEGER,
                      UNIQUE (board_id, rank)) STRICT;
                  INSERT INTO lists_new SELECT * FROM lists;
                  DROP TABLE lists;
                  ALTER TABLE lists_new RENAME TO lists;",
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
        let outcome = migrate(&mut conn, &with_v2(rebuild), &path, true).unwrap();
        assert_eq!(outcome, Outcome { from: 1, to: 2 });
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
