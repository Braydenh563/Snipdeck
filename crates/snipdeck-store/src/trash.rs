//! [`TrashRepo`] for [`Store`].
//!
//! Cascade rules:
//! - `trash` sets `deleted_at` on the one record named. What is inside it (a list's items, a
//!   board's lists and their items) is hidden because its parent is, and keeps its own flag
//!   (usually none).
//! - `restore` clears the record's flag and those of its ancestors, so restoring an item inside
//!   a trashed list brings the list (and board) back. Records inside that were trashed on
//!   their own stay in the trash.
//! - `trashed` lists the records whose parent is not itself trashed: one entry per deletion
//!   the user made.
//! - `purge` permanently deletes records trashed before the cutoff; for a list or board that
//!   includes everything inside it, whatever the inner records' own timestamps.

use rusqlite::params;
use snipdeck_core::{BlobRef, BoardId, ItemId, ListId, Timestamp};

use crate::codec::{
    exists, from_nanos, id_bytes, id_from, nanos, not_found, require_board, require_item,
    require_list,
};
use crate::store::begin;
use crate::{Error, Result, Store, TrashEntry, TrashRepo, Trashable};

/// One row per trash entry: kind (0 board, 1 list, 2 item), id, label, `deleted_at`.
const ENTRIES: &str = "
    SELECT 0 AS kind, id, name AS label, deleted_at FROM boards WHERE deleted_at IS NOT NULL
    UNION ALL
    SELECT 1, l.id, l.name, l.deleted_at
      FROM lists l JOIN boards b ON b.id = l.board_id
     WHERE l.deleted_at IS NOT NULL AND b.deleted_at IS NULL
    UNION ALL
    SELECT 2, i.id, CASE WHEN i.title <> '' THEN i.title ELSE i.preview END, i.deleted_at
      FROM items i JOIN lists l ON l.id = i.list_id JOIN boards b ON b.id = l.board_id
     WHERE i.deleted_at IS NOT NULL AND l.deleted_at IS NULL AND b.deleted_at IS NULL";

impl TrashRepo for Store {
    fn trash(&mut self, target: Trashable, at: Timestamp) -> Result<()> {
        let at = nanos(at)?;
        let tx = begin(&mut self.conn)?;
        let (table, id) = match target {
            Trashable::Board(id) => {
                require_board(&tx, id)?;
                ("boards", id_bytes(id))
            }
            Trashable::List(id) => {
                require_list(&tx, id)?;
                ("lists", id_bytes(id))
            }
            Trashable::Item(id) => {
                require_item(&tx, id)?;
                ("items", id_bytes(id))
            }
        };
        tx.execute(
            &format!("UPDATE {table} SET deleted_at = ?2 WHERE id = ?1"),
            params![id, at],
        )?;
        Ok(tx.commit()?)
    }

    fn restore(&mut self, target: Trashable) -> Result<()> {
        let tx = begin(&mut self.conn)?;
        match target {
            Trashable::Item(id) => {
                let bytes = id_bytes(id);
                if !exists(&tx, "items", bytes)? {
                    return Err(not_found(id));
                }
                tx.execute("UPDATE items SET deleted_at = NULL WHERE id = ?1", [bytes])?;
                restore_list_chain(&tx, "SELECT list_id FROM items WHERE id = ?1", bytes)?;
            }
            Trashable::List(id) => {
                let bytes = id_bytes(id);
                if !exists(&tx, "lists", bytes)? {
                    return Err(not_found(id));
                }
                restore_list_chain(&tx, "SELECT ?1", bytes)?;
            }
            Trashable::Board(id) => {
                let bytes = id_bytes(id);
                if !exists(&tx, "boards", bytes)? {
                    return Err(not_found(id));
                }
                tx.execute("UPDATE boards SET deleted_at = NULL WHERE id = ?1", [bytes])?;
            }
        }
        Ok(tx.commit()?)
    }

    fn trashed(&self) -> Result<Vec<TrashEntry>> {
        let mut stmt = self
            .conn
            .prepare_cached(&format!("{ENTRIES} ORDER BY deleted_at DESC, kind, id"))?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        rows.map(|row| {
            let (kind, id, label, deleted_at) = row?;
            let target = match kind {
                0 => Trashable::Board(id_from::<BoardId>(&id)?),
                1 => Trashable::List(id_from::<ListId>(&id)?),
                _ => Trashable::Item(id_from::<ItemId>(&id)?),
            };
            Ok(TrashEntry {
                target,
                label,
                deleted_at: from_nanos(deleted_at)?,
            })
        })
        .collect()
    }

    fn purge(&mut self, cutoff: Timestamp) -> Result<usize> {
        let cutoff = nanos(cutoff)?;
        let tx = begin(&mut self.conn)?;
        let entries: i64 = tx.query_row(
            &format!("SELECT count(*) FROM ({ENTRIES}) WHERE deleted_at < ?1"),
            [cutoff],
            |r| r.get(0),
        )?;

        // Blobs of everything about to disappear: the items trashed themselves, and those in
        // lists or boards trashed before the cutoff. They are deleted below if nothing else
        // uses them.
        let doomed = "SELECT id FROM items WHERE deleted_at < ?1 OR list_id IN
                        (SELECT id FROM lists WHERE deleted_at < ?1 OR board_id IN
                          (SELECT id FROM boards WHERE deleted_at < ?1))";
        let mut candidates = Vec::new();
        {
            let mut stmt = tx.prepare(&format!(
                "SELECT blob_hash FROM item_formats WHERE blob_hash IS NOT NULL AND item_id IN ({doomed})
                 UNION SELECT blob_hash FROM item_files WHERE blob_hash IS NOT NULL AND item_id IN ({doomed})"
            ))?;
            for hash in stmt.query_map([cutoff], |r| r.get::<_, Vec<u8>>(0))? {
                let hash = <[u8; 32]>::try_from(hash?)
                    .map_err(|_| Error::Corrupt("a blob hash is not 32 bytes".to_owned()))?;
                candidates.push(BlobRef::from_bytes(hash));
            }
        }

        // Foreign keys cascade from boards to lists to items to their content, and the
        // `items_fts_delete` trigger drops the search rows.
        tx.execute("DELETE FROM items WHERE deleted_at < ?1", [cutoff])?;
        tx.execute("DELETE FROM lists WHERE deleted_at < ?1", [cutoff])?;
        tx.execute("DELETE FROM boards WHERE deleted_at < ?1", [cutoff])?;
        tx.commit()?;
        self.release_blobs(candidates)?;
        usize::try_from(entries).map_err(|_| Error::Corrupt("negative count".to_owned()))
    }
}

/// Clears the trash flag of a list and of its board. `list_sql` yields the list's id.
fn restore_list_chain(conn: &rusqlite::Connection, list_sql: &str, arg: [u8; 16]) -> Result<()> {
    conn.execute(
        &format!(
            "UPDATE boards SET deleted_at = NULL
             WHERE id = (SELECT board_id FROM lists WHERE id = ({list_sql}))"
        ),
        [arg],
    )?;
    conn.execute(
        &format!("UPDATE lists SET deleted_at = NULL WHERE id = ({list_sql})"),
        [arg],
    )?;
    Ok(())
}
