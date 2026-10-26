//! Table definitions: the schema as of each released version.
//!
//! The columns follow the storage conventions in `docs/architecture.md`; `docs/store.md` has the
//! table-by-table overview. Constants here are **frozen** once released.

/// Version 1 of the schema. **Frozen**: once released, never edit this text; add a migration.
pub(crate) const V1: &str = r#"
PRAGMA application_id = 1397638219; -- 0x534E444B "SNDK", see migrations::APPLICATION_ID

-- Boards, lists and items are ordered by `rank`: sparse integers, unique within the parent
-- (see docs/architecture.md, "Order belongs to storage"). A trashed row keeps its rank, so a
-- restore puts it back where it was. `deleted_at` is set only on the record that was trashed;
-- records inside a trashed parent are hidden because their parent is, not because they carry
-- the flag themselves.

CREATE TABLE boards (
    id         BLOB    NOT NULL PRIMARY KEY CHECK (length(id) = 16),
    name       TEXT    NOT NULL CHECK (length(name) > 0),
    rank       INTEGER NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER
) STRICT;

CREATE INDEX boards_trashed ON boards (deleted_at) WHERE deleted_at IS NOT NULL;

CREATE TABLE lists (
    id         BLOB    NOT NULL PRIMARY KEY CHECK (length(id) = 16),
    board_id   BLOB    NOT NULL REFERENCES boards (id) ON DELETE CASCADE,
    name       TEXT    NOT NULL CHECK (length(name) > 0),
    accent     TEXT    CHECK (accent IS NULL OR accent IN
                   ('blue', 'teal', 'green', 'amber', 'orange', 'red', 'rose', 'violet', 'slate')),
    list_view  TEXT    NOT NULL CHECK (list_view IN ('compact', 'comfortable')),
    list_sort  TEXT    NOT NULL CHECK (list_sort IN ('manual', 'most-used', 'recently-used')),
    rank       INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    -- Also the index on the `board_id` foreign key.
    UNIQUE (board_id, rank)
) STRICT;

CREATE INDEX lists_trashed ON lists (deleted_at) WHERE deleted_at IS NOT NULL;

-- `seq` is an internal row number that never changes (an `INTEGER PRIMARY KEY` is exempt from
-- the renumbering VACUUM may do to other tables); it is the full-text index's row id. Every
-- other table refers to items by the 16-byte `id`.
CREATE TABLE items (
    seq          INTEGER PRIMARY KEY,
    id           BLOB    NOT NULL UNIQUE CHECK (length(id) = 16),
    list_id      BLOB    NOT NULL REFERENCES lists (id) ON DELETE CASCADE,
    title        TEXT    NOT NULL,
    kind         TEXT    NOT NULL CHECK (kind IN
                     ('text', 'template', 'code', 'link', 'image', 'file')),
    preview      TEXT    NOT NULL,
    pinned       INTEGER NOT NULL CHECK (pinned IN (0, 1)),
    sensitive    INTEGER NOT NULL CHECK (sensitive IN (0, 1)),
    always_plain INTEGER NOT NULL CHECK (always_plain IN (0, 1)),
    use_count    INTEGER NOT NULL CHECK (use_count BETWEEN 0 AND 4294967295),
    last_used_at INTEGER,
    source_app   TEXT,
    rank         INTEGER NOT NULL,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    deleted_at   INTEGER,
    -- A sensitive item keeps no plaintext outside its content (architecture.md).
    CHECK (sensitive = 0 OR preview = ''),
    -- Also the index on the `list_id` foreign key.
    UNIQUE (list_id, rank)
) STRICT;

CREATE INDEX items_trashed ON items (deleted_at) WHERE deleted_at IS NOT NULL;

-- One row per format, `idx` being its preference order. Exactly one of `inline_bytes` or
-- (`blob_hash`, `len`) holds the payload, except for `files`, whose payloads are `item_files`.
CREATE TABLE item_formats (
    item_id      BLOB    NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    idx          INTEGER NOT NULL CHECK (idx >= 0),
    kind         TEXT    NOT NULL CHECK (kind IN ('text', 'html', 'rtf', 'image/png', 'files')),
    inline_bytes BLOB,
    blob_hash    BLOB    CHECK (blob_hash IS NULL OR length(blob_hash) = 32),
    len          INTEGER CHECK (len IS NULL OR len >= 0),
    PRIMARY KEY (item_id, idx),
    UNIQUE (item_id, kind),
    CHECK (
        (kind = 'files' AND inline_bytes IS NULL AND blob_hash IS NULL AND len IS NULL)
        OR (kind <> 'files' AND (
            (inline_bytes IS NOT NULL AND blob_hash IS NULL AND len IS NULL)
            OR (inline_bytes IS NULL AND blob_hash IS NOT NULL AND len IS NOT NULL)))
    )
) STRICT, WITHOUT ROWID;

CREATE INDEX item_formats_blob ON item_formats (blob_hash) WHERE blob_hash IS NOT NULL;

-- The files of an item's `files` format, in order.
CREATE TABLE item_files (
    item_id      BLOB    NOT NULL,
    format_kind  TEXT    NOT NULL DEFAULT 'files' CHECK (format_kind = 'files'),
    idx          INTEGER NOT NULL CHECK (idx >= 0),
    name         TEXT    NOT NULL CHECK (length(name) > 0),
    inline_bytes BLOB,
    blob_hash    BLOB    CHECK (blob_hash IS NULL OR length(blob_hash) = 32),
    len          INTEGER CHECK (len IS NULL OR len >= 0),
    PRIMARY KEY (item_id, idx),
    FOREIGN KEY (item_id, format_kind) REFERENCES item_formats (item_id, kind) ON DELETE CASCADE,
    CHECK (
        (inline_bytes IS NOT NULL AND blob_hash IS NULL AND len IS NULL)
        OR (inline_bytes IS NULL AND blob_hash IS NOT NULL AND len IS NOT NULL)
    )
) STRICT, WITHOUT ROWID;

CREATE INDEX item_files_blob ON item_files (blob_hash) WHERE blob_hash IS NOT NULL;

-- `name_key` is the name lower-cased by Rust (SQLite's own folding is ASCII-only).
CREATE TABLE tags (
    id       BLOB NOT NULL PRIMARY KEY CHECK (length(id) = 16),
    name     TEXT NOT NULL CHECK (length(name) > 0),
    name_key TEXT NOT NULL UNIQUE CHECK (length(name_key) > 0)
) STRICT;

CREATE TABLE item_tags (
    item_id BLOB NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    tag_id  BLOB NOT NULL REFERENCES tags (id) ON DELETE CASCADE,
    PRIMARY KEY (item_id, tag_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX item_tags_tag ON item_tags (tag_id);

-- Full-content search for the main window. One row per item (rowid = items.seq), kept in step
-- by the item repository because the text may live in a blob file; only the delete is a
-- trigger, so purges and cascades can never leave a ghost row. Rows of trashed items stay in
-- the index and are filtered out at query time, which makes restoring free.
CREATE VIRTUAL TABLE item_fts USING fts5 (
    title,
    body,
    tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER items_fts_delete AFTER DELETE ON items
BEGIN
    DELETE FROM item_fts WHERE rowid = old.seq;
END;
"#;

/// Version 2 (reviews H1, M6, N5). **Frozen** once released.
///
/// * `items.encrypted`: every payload of the item (each `item_formats` and `item_files` row)
///   is an `SNENC2` envelope bound to the item, format and file (`docs/crypto.md`). Storage
///   records this instead of guessing from the bytes, so plaintext that happens to look like
///   an envelope is never mistaken for one. Only a sensitive item can be encrypted.
/// * `key_ring`: the one persisted `SNKEY1` key record, if encryption was set up.
/// * The full-text index deletes for real (`secure-delete`), and is merged once so entries
///   deleted under version 1 leave its segments.
/// * Tag keys are recomputed in Rust after this SQL (`tags::rekey`): full case folding and
///   NFC instead of lower-casing.
pub(crate) const V2: &str = r"
ALTER TABLE items ADD COLUMN encrypted INTEGER NOT NULL DEFAULT 0
    CHECK (encrypted IN (0, 1))
    CHECK (encrypted = 0 OR sensitive = 1);

-- At most one row. The record is opaque (wrapped by the OS key provider); the check only
-- keeps anything that is not a key record out.
CREATE TABLE key_ring (
    id     INTEGER NOT NULL PRIMARY KEY CHECK (id = 1),
    record BLOB    NOT NULL CHECK (length(record) > 6 AND substr(record, 1, 6) = CAST('SNKEY1' AS BLOB))
) STRICT;

INSERT INTO item_fts (item_fts, rank) VALUES ('secure-delete', 1);
INSERT INTO item_fts (item_fts) VALUES ('optimize');
";

/// Version 4: an item's colour label and a list's icon. Both are optional text keys, checked
/// against the sets `snipdeck-core` defines (adding a key later is a new migration).
pub(crate) const V4: &str = r"
ALTER TABLE items ADD COLUMN label TEXT
    CHECK (label IS NULL OR label IN
        ('blue', 'teal', 'green', 'amber', 'orange', 'red', 'violet', 'slate'));
ALTER TABLE lists ADD COLUMN icon TEXT
    CHECK (icon IS NULL OR icon IN
        ('folder', 'star', 'heart', 'briefcase', 'house', 'user', 'envelope-simple', 'chat-circle', 'phone', 'calendar-blank', 'shopping-cart', 'credit-card', 'code', 'terminal-window', 'book-open', 'lightbulb', 'flag', 'bookmark-simple', 'globe', 'map-pin', 'camera', 'music-note', 'wrench', 'rocket-launch'));
";

/// Version 6: the key record a replacement overwrote, kept until the next one (review B6), so
/// a wrong replacement can be undone by hand.
pub(crate) const V6: &str = r"
ALTER TABLE key_ring ADD COLUMN previous BLOB
    CHECK (previous IS NULL OR (length(previous) > 6 AND substr(previous, 1, 6) = CAST('SNKEY1' AS BLOB)));
";

#[cfg(test)]
mod tests {
    use rusqlite::{Connection, params};
    use snipdeck_core::{FormatKind, ItemKind, ListAccent, ListSort, ListView};

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        conn.execute_batch(super::V1).unwrap();
        conn
    }

    fn id(n: u8) -> Vec<u8> {
        vec![n; 16]
    }

    fn board(conn: &Connection, n: u8, rank: i64) -> rusqlite::Result<usize> {
        conn.execute(
            "INSERT INTO boards (id, name, rank, created_at, updated_at) VALUES (?1, 'B', ?2, 0, 0)",
            params![id(n), rank],
        )
    }

    fn list(conn: &Connection, n: u8, board: u8, rank: i64) -> rusqlite::Result<usize> {
        conn.execute(
            "INSERT INTO lists (id, board_id, name, accent, list_view, list_sort, rank, created_at, updated_at)
             VALUES (?1, ?2, 'L', NULL, 'compact', 'manual', ?3, 0, 0)",
            params![id(n), id(board), rank],
        )
    }

    fn item_with(
        conn: &Connection,
        n: u8,
        list: u8,
        rank: i64,
        (kind, pinned, sensitive, preview): (&str, i64, i64, &str),
    ) -> rusqlite::Result<usize> {
        conn.execute(
            "INSERT INTO items (id, list_id, title, kind, preview, pinned, sensitive, always_plain,
                                use_count, rank, created_at, updated_at)
             VALUES (?1, ?2, 't', ?3, ?4, ?5, ?6, 0, 0, ?7, 0, 0)",
            params![id(n), id(list), kind, preview, pinned, sensitive, rank],
        )
    }

    fn item(conn: &Connection, n: u8, list: u8, rank: i64) -> rusqlite::Result<usize> {
        item_with(conn, n, list, rank, ("text", 0, 0, ""))
    }

    fn tree(conn: &Connection) {
        board(conn, 1, 10).unwrap();
        list(conn, 2, 1, 10).unwrap();
        item(conn, 3, 2, 10).unwrap();
    }

    #[test]
    fn the_schema_applies_to_an_empty_database() {
        let conn = db();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'item_fts_%' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(
            tables,
            [
                "boards",
                "item_files",
                "item_formats",
                "item_fts",
                "item_tags",
                "items",
                "lists",
                "tags"
            ]
        );
    }

    #[test]
    fn every_stored_enum_key_is_accepted_and_unknown_keys_are_not() {
        let conn = db();
        board(&conn, 1, 1).unwrap();
        for (i, accent) in ListAccent::ALL.iter().enumerate() {
            conn.execute(
                "INSERT INTO lists (id, board_id, name, accent, list_view, list_sort, rank, created_at, updated_at)
                 VALUES (?1, ?2, 'L', ?3, 'compact', 'manual', ?4, 0, 0)",
                params![id(20 + u8::try_from(i).unwrap()), id(1), accent.key(), i64::try_from(i).unwrap()],
            )
            .unwrap();
        }
        for (i, view) in ListView::ALL.iter().enumerate() {
            conn.execute(
                "UPDATE lists SET list_view = ?1 WHERE id = ?2",
                params![view.key(), id(20 + u8::try_from(i).unwrap())],
            )
            .unwrap();
        }
        for (i, sort) in ListSort::ALL.iter().enumerate() {
            conn.execute(
                "UPDATE lists SET list_sort = ?1 WHERE id = ?2",
                params![sort.key(), id(20 + u8::try_from(i).unwrap())],
            )
            .unwrap();
        }
        for bad in [
            "UPDATE lists SET accent = 'pink'",
            "UPDATE lists SET list_view = 'huge'",
            "UPDATE lists SET list_sort = 'random'",
        ] {
            assert!(conn.execute(bad, []).is_err(), "{bad}");
        }
        for (i, kind) in ItemKind::ALL.iter().enumerate() {
            item_with(
                &conn,
                100 + u8::try_from(i).unwrap(),
                20,
                i64::try_from(i).unwrap(),
                (kind.key(), 0, 0, ""),
            )
            .unwrap();
        }
        assert!(item_with(&conn, 99, 20, 99, ("video", 0, 0, "")).is_err());
        let item_id = id(100);
        for (i, kind) in FormatKind::ALL.iter().enumerate() {
            let i = i64::try_from(i).unwrap();
            if *kind == FormatKind::Files {
                conn.execute(
                    "INSERT INTO item_formats (item_id, idx, kind) VALUES (?1, ?2, ?3)",
                    params![item_id, i, kind.key()],
                )
                .unwrap();
            } else {
                conn.execute("INSERT INTO item_formats (item_id, idx, kind, inline_bytes) VALUES (?1, ?2, ?3, x'00')", params![item_id, i, kind.key()]).unwrap();
            }
        }
        assert!(conn.execute("INSERT INTO item_formats (item_id, idx, kind, inline_bytes) VALUES (?1, 9, 'video', x'00')", params![item_id]).is_err());
    }

    #[test]
    fn ids_must_be_16_bytes() {
        let conn = db();
        let bad = conn.execute(
            "INSERT INTO boards (id, name, rank, created_at, updated_at) VALUES (x'0102', 'B', 1, 0, 0)",
            [],
        );
        assert!(bad.is_err());
    }

    #[test]
    fn names_must_not_be_empty() {
        let conn = db();
        let bad = conn.execute(
            "INSERT INTO boards (id, name, rank, created_at, updated_at) VALUES (?1, '', 1, 0, 0)",
            params![id(1)],
        );
        assert!(bad.is_err());
    }

    #[test]
    fn ranks_are_unique_per_parent_only() {
        let conn = db();
        board(&conn, 1, 5).unwrap();
        assert!(board(&conn, 2, 5).is_err(), "boards share one rank space");
        board(&conn, 2, 6).unwrap();
        list(&conn, 10, 1, 5).unwrap();
        assert!(list(&conn, 11, 1, 5).is_err());
        list(&conn, 11, 2, 5).unwrap(); // same rank under another board is fine
        item(&conn, 20, 10, 5).unwrap();
        assert!(item(&conn, 21, 10, 5).is_err());
        item(&conn, 21, 11, 5).unwrap();
    }

    #[test]
    fn flags_are_booleans_and_sensitive_items_have_no_preview() {
        let conn = db();
        board(&conn, 1, 1).unwrap();
        list(&conn, 2, 1, 1).unwrap();
        assert!(item_with(&conn, 3, 2, 1, ("text", 2, 0, "")).is_err());
        assert!(item_with(&conn, 3, 2, 1, ("text", 0, 1, "hunter2")).is_err());
        item_with(&conn, 3, 2, 1, ("text", 0, 1, "")).unwrap();
        item_with(&conn, 4, 2, 2, ("text", 1, 0, "visible")).unwrap();
    }

    #[test]
    fn use_count_fits_a_u32() {
        let conn = db();
        tree(&conn);
        conn.execute("UPDATE items SET use_count = 4294967295", [])
            .unwrap();
        assert!(
            conn.execute("UPDATE items SET use_count = 4294967296", [])
                .is_err()
        );
        assert!(conn.execute("UPDATE items SET use_count = -1", []).is_err());
    }

    #[test]
    fn foreign_keys_reject_orphans() {
        let conn = db();
        assert!(list(&conn, 2, 1, 1).is_err(), "list without board");
        board(&conn, 1, 1).unwrap();
        list(&conn, 2, 1, 1).unwrap();
        assert!(item(&conn, 3, 9, 1).is_err(), "item without list");
        item(&conn, 3, 2, 1).unwrap();
        assert!(
            conn.execute("INSERT INTO item_formats (item_id, idx, kind, inline_bytes) VALUES (?1, 0, 'text', x'00')", params![id(9)]).is_err(),
            "format without item"
        );
        assert!(
            conn.execute(
                "INSERT INTO item_tags (item_id, tag_id) VALUES (?1, ?2)",
                params![id(3), id(7)]
            )
            .is_err(),
            "tag link without tag"
        );
    }

    #[test]
    fn deleting_a_parent_cascades_to_everything_inside() {
        let conn = db();
        tree(&conn);
        conn.execute(
            "INSERT INTO item_formats (item_id, idx, kind) VALUES (?1, 0, 'files')",
            params![id(3)],
        )
        .unwrap();
        conn.execute("INSERT INTO item_files (item_id, idx, name, inline_bytes) VALUES (?1, 0, 'a.txt', x'61')", params![id(3)]).unwrap();
        conn.execute(
            "INSERT INTO tags (id, name, name_key) VALUES (?1, 'Tag', 'tag')",
            params![id(7)],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO item_tags (item_id, tag_id) VALUES (?1, ?2)",
            params![id(3), id(7)],
        )
        .unwrap();
        conn.execute("DELETE FROM boards", []).unwrap();
        for table in ["lists", "items", "item_formats", "item_files", "item_tags"] {
            let n: i64 = conn
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "{table}");
        }
        let tags: i64 = conn
            .query_row("SELECT count(*) FROM tags", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tags, 1, "tags outlive items");
    }

    #[test]
    fn deleting_a_tag_unlinks_it_but_keeps_the_items() {
        let conn = db();
        tree(&conn);
        conn.execute(
            "INSERT INTO tags (id, name, name_key) VALUES (?1, 'Tag', 'tag')",
            params![id(7)],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO item_tags (item_id, tag_id) VALUES (?1, ?2)",
            params![id(3), id(7)],
        )
        .unwrap();
        conn.execute("DELETE FROM tags", []).unwrap();
        let links: i64 = conn
            .query_row("SELECT count(*) FROM item_tags", [], |r| r.get(0))
            .unwrap();
        let items: i64 = conn
            .query_row("SELECT count(*) FROM items", [], |r| r.get(0))
            .unwrap();
        assert_eq!((links, items), (0, 1));
    }

    #[test]
    fn tag_names_are_unique_on_the_folded_key() {
        let conn = db();
        conn.execute(
            "INSERT INTO tags (id, name, name_key) VALUES (?1, 'Work', 'work')",
            params![id(1)],
        )
        .unwrap();
        assert!(
            conn.execute(
                "INSERT INTO tags (id, name, name_key) VALUES (?1, 'WORK', 'work')",
                params![id(2)]
            )
            .is_err()
        );
    }

    #[test]
    fn format_payloads_are_inline_xor_blob() {
        let conn = db();
        tree(&conn);
        let hash = vec![9u8; 32];
        let insert = |idx: i64,
                      kind: &str,
                      inline: Option<&[u8]>,
                      hash: Option<&[u8]>,
                      len: Option<i64>| {
            conn.execute(
                "INSERT INTO item_formats (item_id, idx, kind, inline_bytes, blob_hash, len) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id(3), idx, kind, inline, hash, len],
            )
        };
        assert!(insert(0, "text", None, None, None).is_err(), "no payload");
        assert!(
            insert(0, "text", Some(b"a"), Some(&hash), Some(70000)).is_err(),
            "both"
        );
        assert!(
            insert(0, "text", None, Some(&hash[..31]), Some(70000)).is_err(),
            "short hash"
        );
        assert!(
            insert(0, "text", None, Some(&hash), None).is_err(),
            "blob without length"
        );
        assert!(
            insert(0, "files", Some(b"a"), None, None).is_err(),
            "files carry no payload"
        );
        insert(0, "text", Some(b"a"), None, None).unwrap();
        insert(1, "image/png", None, Some(&hash), Some(70000)).unwrap();
        assert!(
            insert(2, "text", Some(b"b"), None, None).is_err(),
            "one text format per item"
        );
        assert!(
            insert(1, "html", Some(b"b"), None, None).is_err(),
            "one format per index"
        );
    }

    #[test]
    fn file_rows_need_a_files_format() {
        let conn = db();
        tree(&conn);
        let insert = || {
            conn.execute(
                "INSERT INTO item_files (item_id, idx, name, inline_bytes) VALUES (?1, 0, 'a.txt', x'61')",
                params![id(3)],
            )
        };
        assert!(insert().is_err(), "no files format yet");
        conn.execute("INSERT INTO item_formats (item_id, idx, kind, inline_bytes) VALUES (?1, 0, 'text', x'61')", params![id(3)]).unwrap();
        assert!(insert().is_err(), "a text format is not a files format");
        conn.execute(
            "INSERT INTO item_formats (item_id, idx, kind) VALUES (?1, 1, 'files')",
            params![id(3)],
        )
        .unwrap();
        insert().unwrap();
    }

    #[test]
    fn every_foreign_key_column_is_indexed() {
        let conn = db();
        let tables = ["lists", "items", "item_formats", "item_files", "item_tags"];
        let mut checked = 0;
        for table in tables {
            let mut fks = conn
                .prepare(&format!("PRAGMA foreign_key_list({table})"))
                .unwrap();
            let groups: Vec<(i64, String)> = fks
                .query_map([], |r| Ok((r.get(0)?, r.get(3)?)))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            for (group, column) in &groups {
                if groups.iter().filter(|(g, _)| g == group).count() > 1
                    && column != &groups.iter().find(|(g, _)| g == group).unwrap().1
                {
                    continue; // composite key: the first column leads the index
                }
                let mut covered = false;
                let mut indexes = conn
                    .prepare(&format!("PRAGMA index_list({table})"))
                    .unwrap();
                let names: Vec<String> = indexes
                    .query_map([], |r| r.get(1))
                    .unwrap()
                    .map(Result::unwrap)
                    .collect();
                for index in names {
                    let first: Option<String> = conn
                        .query_row(
                            &format!(
                                "SELECT name FROM pragma_index_info('{index}') WHERE seqno = 0"
                            ),
                            [],
                            |r| r.get(0),
                        )
                        .ok();
                    covered |= first.as_deref() == Some(column);
                }
                // `WITHOUT ROWID` tables are their own primary-key index.
                let pk_first: Option<String> = conn
                    .query_row(
                        &format!("SELECT name FROM pragma_table_info('{table}') WHERE pk = 1"),
                        [],
                        |r| r.get(0),
                    )
                    .ok();
                covered |= pk_first.as_deref() == Some(column);
                assert!(covered, "{table}.{column} has no index");
                checked += 1;
            }
        }
        assert!(
            checked >= 6,
            "expected to check every foreign key, checked {checked}"
        );
    }

    #[test]
    fn the_application_id_marks_the_file_as_snipdeck() {
        let conn = db();
        let app: i32 = conn
            .pragma_query_value(None, "application_id", |r| r.get(0))
            .unwrap();
        assert_eq!(app, crate::migrations::APPLICATION_ID);
    }
}
