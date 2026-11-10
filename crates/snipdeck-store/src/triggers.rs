//! [`TriggerRepo`] for [`Store`]: the typed codes that stand for items.
//!
//! One row per item in `triggers`, the code being the primary key (case counts). A row stays
//! with its item through the trash, so undoing a trash keeps the code, but only rows of live
//! items (item, list and board) count: [`triggers`] and [`trigger_holder`] leave the others
//! out, and giving a code to another item takes it from a trashed holder. A restored item
//! thus keeps its code if nobody took it meanwhile, and has none if someone did. Purging an
//! item deletes the row with it (foreign key), and turning an item Sensitive deletes it
//! ([`free_item`]). These are plain statements, not SQL triggers, so a later migration that
//! rebuilds `items` or `lists` has nothing to recreate.

use rusqlite::{Connection, OptionalExtension, params};
use snipdeck_core::{ItemId, Trigger};

use crate::Error;
use crate::Result;
use crate::batch::Ctx;
use crate::codec::{id_bytes, id_from, require_item};

/// Rows whose item, list and board are all live.
const LIVE: &str = "item_id IN (SELECT i.id FROM items i
       JOIN lists l ON l.id = i.list_id JOIN boards b ON b.id = l.board_id
      WHERE i.deleted_at IS NULL AND l.deleted_at IS NULL AND b.deleted_at IS NULL)";

fn stored(text: &str) -> Result<Trigger> {
    Ok(Trigger::new(text)?)
}

/// Every trigger of a live item and its item, sorted by trigger. A row that does not read
/// back as a valid trigger is skipped (and reported on stderr), not allowed to fail a whole
/// export.
pub(crate) fn triggers(conn: &Connection) -> Result<Vec<(Trigger, ItemId)>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT trigger, item_id FROM triggers WHERE {LIVE} ORDER BY trigger"
    ))?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (text, item) = row?;
        match (stored(&text), id_from(&item)) {
            (Ok(trigger), Ok(item)) => out.push((trigger, item)),
            _ => skip_bad_row(),
        }
    }
    Ok(out)
}

#[allow(
    clippy::print_stderr,
    reason = "the store has no logger; this is a rare corruption note"
)]
fn skip_bad_row() {
    eprintln!("snipdeck: skipped a stored trigger that is not valid");
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
        &format!("SELECT item_id FROM triggers WHERE trigger = ?1 AND {LIVE}"),
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

/// Sets or clears the item's code. Returns the codes of other live items that this one is a
/// prefix of or has as a prefix: typing the longer one fires the shorter first, so the longer
/// one can never be reached.
pub(crate) fn set_trigger(
    cx: &mut Ctx<'_>,
    item: ItemId,
    trigger: Option<&Trigger>,
) -> Result<Vec<Trigger>> {
    let conn = cx.conn;
    require_item(conn, item)?;
    let id = id_bytes(item);
    free_item(conn, id)?;
    let Some(trigger) = trigger else {
        return Ok(Vec::new());
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
    // A trashed item's claim to the code gives way.
    conn.execute(
        "DELETE FROM triggers WHERE trigger = ?1",
        [trigger.as_str()],
    )?;
    conn.execute(
        "INSERT INTO triggers (trigger, item_id) VALUES (?1, ?2)",
        params![trigger.as_str(), id],
    )?;
    Ok(triggers(conn)?
        .into_iter()
        .filter(|(other, owner)| {
            *owner != item
                && (other.as_str().starts_with(trigger.as_str())
                    || trigger.as_str().starts_with(other.as_str()))
        })
        .map(|(other, _)| other)
        .collect())
}
