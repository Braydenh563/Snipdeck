//! [`BoardRepo`] and [`ListRepo`] for [`Store`].

use rusqlite::{OptionalExtension, params};
use snipdeck_core::{Board, BoardId, List, ListId, Name, Placement};

use crate::codec::{
    from_nanos, id_bytes, id_from, nanos, not_found, raw_placement, require_board, require_list,
    require_new,
};
use crate::rank::{self, BOARDS, LISTS};
use crate::store::begin;
use crate::{BoardRepo, ListRepo, Result, Store};

struct BoardRow {
    id: Vec<u8>,
    name: String,
    created_at: i64,
    updated_at: i64,
}

impl BoardRow {
    const COLUMNS: &'static str = "id, name, created_at, updated_at";

    fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            created_at: row.get(2)?,
            updated_at: row.get(3)?,
        })
    }

    fn into_board(self) -> Result<Board> {
        Ok(Board {
            id: id_from(&self.id)?,
            name: Name::new(&self.name)?,
            created_at: from_nanos(self.created_at)?,
            updated_at: from_nanos(self.updated_at)?,
        })
    }
}

impl BoardRepo for Store {
    fn boards(&self) -> Result<Vec<Board>> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {} FROM boards WHERE deleted_at IS NULL ORDER BY rank",
            BoardRow::COLUMNS
        ))?;
        let rows = stmt.query_map([], BoardRow::read)?;
        rows.map(|row| row?.into_board()).collect()
    }

    fn board(&self, id: BoardId) -> Result<Board> {
        let row = self
            .conn
            .query_row(
                &format!(
                    "SELECT {} FROM boards WHERE id = ?1 AND deleted_at IS NULL",
                    BoardRow::COLUMNS
                ),
                [id_bytes(id)],
                BoardRow::read,
            )
            .optional()?
            .ok_or_else(|| not_found(id))?;
        row.into_board()
    }

    fn insert_board(&mut self, board: &Board, at: Placement<BoardId>) -> Result<()> {
        let (created, updated) = (nanos(board.created_at)?, nanos(board.updated_at)?);
        let tx = begin(&mut self.conn)?;
        require_new(&tx, "boards", board.id)?;
        let rank = rank::place(&tx, BOARDS, None, raw_placement(at), None)?;
        tx.execute(
            "INSERT INTO boards (id, name, rank, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id_bytes(board.id), board.name.as_str(), rank, created, updated],
        )?;
        Ok(tx.commit()?)
    }

    fn update_board(&mut self, board: &Board) -> Result<()> {
        let (created, updated) = (nanos(board.created_at)?, nanos(board.updated_at)?);
        let tx = begin(&mut self.conn)?;
        require_board(&tx, board.id)?;
        tx.execute(
            "UPDATE boards SET name = ?2, created_at = ?3, updated_at = ?4 WHERE id = ?1",
            params![id_bytes(board.id), board.name.as_str(), created, updated],
        )?;
        Ok(tx.commit()?)
    }

    fn move_board(&mut self, id: BoardId, at: Placement<BoardId>) -> Result<()> {
        let tx = begin(&mut self.conn)?;
        require_board(&tx, id)?;
        let bytes = id_bytes(id);
        let rank = rank::place(&tx, BOARDS, None, raw_placement(at), Some(bytes))?;
        tx.execute(
            "UPDATE boards SET rank = ?2 WHERE id = ?1",
            params![bytes, rank],
        )?;
        Ok(tx.commit()?)
    }
}

struct ListRow {
    id: Vec<u8>,
    board_id: Vec<u8>,
    name: String,
    accent: Option<String>,
    view: String,
    sort: String,
    created_at: i64,
    updated_at: i64,
}

impl ListRow {
    const COLUMNS: &'static str =
        "id, board_id, name, accent, list_view, list_sort, created_at, updated_at";

    fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            board_id: row.get(1)?,
            name: row.get(2)?,
            accent: row.get(3)?,
            view: row.get(4)?,
            sort: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    }

    fn into_list(self) -> Result<List> {
        Ok(List {
            id: id_from(&self.id)?,
            board_id: id_from(&self.board_id)?,
            name: Name::new(&self.name)?,
            accent: self.accent.as_deref().map(str::parse).transpose()?,
            view: self.view.parse()?,
            sort: self.sort.parse()?,
            created_at: from_nanos(self.created_at)?,
            updated_at: from_nanos(self.updated_at)?,
        })
    }
}

impl ListRepo for Store {
    fn lists(&self, board: BoardId) -> Result<Vec<List>> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {} FROM lists WHERE board_id = ?1 AND deleted_at IS NULL
               AND EXISTS (SELECT 1 FROM boards b WHERE b.id = ?1 AND b.deleted_at IS NULL)
             ORDER BY rank",
            ListRow::COLUMNS
        ))?;
        let rows = stmt.query_map([id_bytes(board)], ListRow::read)?;
        rows.map(|row| row?.into_list()).collect()
    }

    fn list(&self, id: ListId) -> Result<List> {
        let row = self
            .conn
            .query_row(
                &format!(
                    "SELECT {} FROM lists WHERE id = ?1 AND deleted_at IS NULL
                       AND EXISTS (SELECT 1 FROM boards b
                                   WHERE b.id = lists.board_id AND b.deleted_at IS NULL)",
                    ListRow::COLUMNS
                ),
                [id_bytes(id)],
                ListRow::read,
            )
            .optional()?
            .ok_or_else(|| not_found(id))?;
        row.into_list()
    }

    fn insert_list(&mut self, list: &List, at: Placement<ListId>) -> Result<()> {
        let (created, updated) = (nanos(list.created_at)?, nanos(list.updated_at)?);
        let tx = begin(&mut self.conn)?;
        require_new(&tx, "lists", list.id)?;
        require_board(&tx, list.board_id)?;
        let board = id_bytes(list.board_id);
        let rank = rank::place(&tx, LISTS, Some(board), raw_placement(at), None)?;
        tx.execute(
            "INSERT INTO lists (id, board_id, name, accent, list_view, list_sort, rank, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id_bytes(list.id),
                board,
                list.name.as_str(),
                list.accent.map(snipdeck_core::ListAccent::key),
                list.view.key(),
                list.sort.key(),
                rank,
                created,
                updated
            ],
        )?;
        Ok(tx.commit()?)
    }

    fn update_list(&mut self, list: &List) -> Result<()> {
        let (created, updated) = (nanos(list.created_at)?, nanos(list.updated_at)?);
        let tx = begin(&mut self.conn)?;
        require_list(&tx, list.id)?;
        let (id, board) = (id_bytes(list.id), id_bytes(list.board_id));
        let current: Vec<u8> =
            tx.query_row("SELECT board_id FROM lists WHERE id = ?1", [id], |r| {
                r.get(0)
            })?;
        if current != board {
            // A new parent: the list goes to the end of it.
            require_board(&tx, list.board_id)?;
            let rank = rank::place(&tx, LISTS, Some(board), Placement::Last, Some(id))?;
            tx.execute(
                "UPDATE lists SET board_id = ?2, rank = ?3 WHERE id = ?1",
                params![id, board, rank],
            )?;
        }
        tx.execute(
            "UPDATE lists SET name = ?2, accent = ?3, list_view = ?4, list_sort = ?5,
                    created_at = ?6, updated_at = ?7 WHERE id = ?1",
            params![
                id,
                list.name.as_str(),
                list.accent.map(snipdeck_core::ListAccent::key),
                list.view.key(),
                list.sort.key(),
                created,
                updated
            ],
        )?;
        Ok(tx.commit()?)
    }

    fn move_list(&mut self, id: ListId, board: BoardId, at: Placement<ListId>) -> Result<()> {
        let tx = begin(&mut self.conn)?;
        require_list(&tx, id)?;
        require_board(&tx, board)?;
        let (bytes, board_bytes) = (id_bytes(id), id_bytes(board));
        let rank = rank::place(
            &tx,
            LISTS,
            Some(board_bytes),
            raw_placement(at),
            Some(bytes),
        )?;
        tx.execute(
            "UPDATE lists SET board_id = ?2, rank = ?3 WHERE id = ?1",
            params![bytes, board_bytes, rank],
        )?;
        Ok(tx.commit()?)
    }
}
