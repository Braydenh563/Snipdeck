//! [`TriggerRepo`] for [`Store`]: the typed codes that stand for items.
//!
//! One row per item in `triggers`, the code being the primary key (case counts). A code is
//! freed (by [`free_item`], [`free_list`], [`free_board`]) when its item, list or board goes to
//! the trash or the item turns Sensitive, so a restored item comes back without its code, and
//! any row that exists belongs to a live, non-sensitive item. These are plain statements, not
//! SQL triggers, so a later migration that rebuilds `items` or `lists` has nothing to recreate.

use rusqlite::{Connection, OptionalExtension, params};
use snipdeck_core::{ItemId, Trigger};

use crate::Error;
use crate::Result;
use crate::batch::Ctx;
use crate::codec::{id_bytes, id_from, require_item};

fn stored(text: &str) -> Result<Trigger> {
    Ok(Trigger::new(text)?)
}

/// Every trigger and its item, sorted by trigger.
pub(crate) fn triggers(conn: &Connection) -> Result<Vec<(Trigger, ItemId)>> {
    let mut stmt = conn.prepare_cached("SELECT trigger, item_id FROM triggers ORDER BY trigger")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (text, item) = row?;
        out.push((stored(&text)?, id_from(&item)?));
    }
    Ok(out)
}

pub(crate) fn trigger_of(conn: &Connection, item: ItemId) -> Result<Option<Trigger>> {
    require_item(conn, item)?;
    conn.query_row(
        "SELECT trigger FROM triggers WHERE item_id = ?1",
        [id_bytes(item)],
        |r| r.get::<_, String>(0),
    )
    .optional()?
    .map(|text| stored(&text))
    .transpose()
}

pub(crate) fn trigger_holder(conn: &Connection, trigger: &Trigger) -> Result<Option<ItemId>> {
    conn.query_row(
        "SELECT item_id FROM triggers WHERE trigger = ?1",
        [trigger.as_str()],
        |r| r.get::<_, Vec<u8>>(0),
    )
    .optional()?
    .map(|bytes| id_from(&bytes))
    .transpose()
}

/// Frees the code of one item.
pub(crate) fn free_item(conn: &Connection, item: [u8; 16]) -> Result<()> {
    conn.execute("DELETE FROM triggers WHERE item_id = ?1", [item])?;
    Ok(())
}

/// Frees the codes of every item in a list.
pub(crate) fn free_list(conn: &Connection, list: [u8; 16]) -> Result<()> {
    conn.execute(
        "DELETE FROM triggers WHERE item_id IN (SELECT id FROM items WHERE list_id = ?1)",
        [list],
    )?;
    Ok(())
}

/// Frees the codes of every item in a board.
pub(crate) fn free_board(conn: &Connection, board: [u8; 16]) -> Result<()> {
    conn.execute(
        "DELETE FROM triggers WHERE item_id IN
           (SELECT i.id FROM items i JOIN lists l ON l.id = i.list_id WHERE l.board_id = ?1)",
        [board],
    )?;
    Ok(())
}

pub(crate) fn set_trigger(cx: &mut Ctx<'_>, item: ItemId, trigger: Option<&Trigger>) -> Result<()> {
    let conn = cx.conn;
    require_item(conn, item)?;
    let id = id_bytes(item);
    free_item(conn, id)?;
    let Some(trigger) = trigger else {
        return Ok(());
    };
    let sensitive: bool =
        conn.query_row("SELECT sensitive FROM items WHERE id = ?1", [id], |r| {
            r.get(0)
        })?;
    if sensitive {
        return Err(Error::SensitiveTrigger);
    }
    // Both checks run after the delete: the caller's transaction (a savepoint here) rolls the
    // delete back when this fails, so a refused change keeps the old trigger.
    if trigger_holder(conn, trigger)?.is_some() {
        return Err(Error::TriggerTaken(trigger.as_str().to_owned()));
    }
    conn.execute(
        "INSERT INTO triggers (trigger, item_id) VALUES (?1, ?2)",
        params![trigger.as_str(), id],
    )?;
    Ok(())
}
