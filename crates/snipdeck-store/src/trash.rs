//! [`TrashRepo`] for [`Store`].
//!
//! Cascade rules:
//! - `trash` sets `deleted_at` on the one record named. What is inside it (a list's items, a
//!   board's lists and their items) is hidden because its parent is, and keeps its own flag
//!   (usually none).
//! - `restore` clears the record's flag and those of its ancestors, so restoring an item inside
//!   a trashed list brings the list (and board) back. Records inside that were trashed on
//!   their own stay in the trash. [`Store::restore_with`] also enforces a live-board limit
//!   and reports what it revived.
//! - `trashed` lists the records whose parent is not itself trashed: one entry per deletion
//!   the user made.
//! - `purge` permanently deletes records trashed before the cutoff; for a list or board that
//!   includes everything inside it, whatever the inner records' own timestamps.

use rusqlite::{OptionalExtension, params};
use snipdeck_core::{BlobRef, BoardId, Feature, ItemId, ListId, Timestamp};

use crate::codec::{
    exists, from_nanos, id_bytes, id_from, nanos, not_found, require_board, require_item,
    require_list,
};
use crate::store::{begin, checkpoint};
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
        self.restore_with(target, None).map(drop)
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
        // Purged items may have held plaintext of sensitive items: scrub blobs, then fold the
        // WAL so their page images do not outlive them (review H1). Both are best effort
        // after the commit (review M4).
        self.release_blobs(&candidates, true);
        if entries > 0 {
            checkpoint(&self.conn);
        }
        usize::try_from(entries).map_err(|_| Error::Corrupt("negative count".to_owned()))
    }
}

impl Store {
    /// [`TrashRepo::restore`] that refuses to bring a board back when `max_live_boards`
    /// boards are already live, and says what it brought back (review M1).
    ///
    /// Restoring a list or item inside a trashed board revives the board too, so the Free
    /// board limit cannot be bypassed by trashing a board, creating another and restoring the
    /// first. Storage stays free of entitlements: the app passes the limit
    /// (`Some(Entitlements::FREE_BOARD_LIMIT)` on Free, `None` on Pro). Over the limit it fails
    /// with `Error::Core(ProRequired(Feature::UnlimitedBoards))`, the upsell path, and changes
    /// nothing.
    ///
    /// Returns the records whose trash flag was cleared: the target (unless it was live),
    /// then its list and board if they were trashed. Empty if everything was live already.
    pub fn restore_with(
        &mut self,
        target: Trashable,
        max_live_boards: Option<usize>,
    ) -> Result<Vec<Trashable>> {
        let tx = begin(&mut self.conn)?;
        let mut revived = Vec::new();
        let board = match target {
            Trashable::Item(id) => {
                let (deleted, list): (Option<i64>, Vec<u8>) = tx
                    .query_row(
                        "SELECT deleted_at, list_id FROM items WHERE id = ?1",
                        [id_bytes(id)],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?
                    .ok_or_else(|| not_found(id))?;
                if deleted.is_some() {
                    revived.push(target);
                }
                list_and_board(&tx, id_from(&list)?, &mut revived)?
            }
            Trashable::List(id) => {
                if !exists(&tx, "lists", id_bytes(id))? {
                    return Err(not_found(id));
                }
                list_and_board(&tx, id, &mut revived)?
            }
            Trashable::Board(id) => {
                if !exists(&tx, "boards", id_bytes(id))? {
                    return Err(not_found(id));
                }
                id
            }
        };
        let board_trashed: bool = tx.query_row(
            "SELECT deleted_at IS NOT NULL FROM boards WHERE id = ?1",
            [id_bytes(board)],
            |r| r.get(0),
        )?;
        if board_trashed {
            if let Some(max) = max_live_boards {
                let live: i64 = tx.query_row(
                    "SELECT count(*) FROM boards WHERE deleted_at IS NULL",
                    [],
                    |r| r.get(0),
                )?;
                if usize::try_from(live).unwrap_or(usize::MAX) >= max {
                    return Err(snipdeck_core::Error::ProRequired(Feature::UnlimitedBoards).into());
                }
            }
            revived.push(Trashable::Board(board));
        }
        for record in &revived {
            let (table, id) = match *record {
                Trashable::Board(id) => ("boards", id_bytes(id)),
                Trashable::List(id) => ("lists", id_bytes(id)),
                Trashable::Item(id) => ("items", id_bytes(id)),
            };
            tx.execute(
                &format!("UPDATE {table} SET deleted_at = NULL WHERE id = ?1"),
                [id],
            )?;
        }
        tx.commit()?;
        Ok(revived)
    }
}

/// Notes the list as revived if it is trashed, and returns its board.
fn list_and_board(
    conn: &rusqlite::Connection,
    list: ListId,
    revived: &mut Vec<Trashable>,
) -> Result<BoardId> {
    let (deleted, board): (Option<i64>, Vec<u8>) = conn.query_row(
        "SELECT deleted_at, board_id FROM lists WHERE id = ?1",
        [id_bytes(list)],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if deleted.is_some() {
        revived.push(Trashable::List(list));
    }
    id_from(&board)
}
