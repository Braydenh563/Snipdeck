//! Sparse integer ranks (docs/architecture.md, "Order belongs to storage").
//!
//! Boards, lists and items each have an `INTEGER` rank, unique within their parent. A new rank
//! is computed from its neighbours; when no integer is left between two neighbours the parent's
//! children are renumbered [`STEP`] apart in the same transaction and the placement is retried.
//! Trashed rows keep their ranks, so they count as neighbours: a restored row returns to
//! exactly where it was.

use rusqlite::{Connection, OptionalExtension};
use snipdeck_core::Placement;
use uuid::Uuid;

use crate::{Error, Result};

/// Distance between neighbours after a renumber, and the step for `First` and `Last`.
pub(crate) const STEP: i64 = 1 << 16;

/// Ranks stay within plus or minus this, so arithmetic on them cannot overflow.
const LIMIT: i64 = 1 << 60;

/// While renumbering, rows are parked at `PARK - rank`: far outside `[-LIMIT, LIMIT]`, and the
/// mapping is one-to-one, so the unique index is never violated halfway through.
const PARK: i64 = -(1 << 62);

/// A table whose rows are ordered, and the column naming their parent (none for boards).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Scope {
    pub table: &'static str,
    pub parent: Option<&'static str>,
    /// What an anchor that is missing is called in the error.
    pub entity: &'static str,
}

pub(crate) const BOARDS: Scope = Scope {
    table: "boards",
    parent: None,
    entity: "board",
};
pub(crate) const LISTS: Scope = Scope {
    table: "lists",
    parent: Some("board_id"),
    entity: "list",
};
pub(crate) const ITEMS: Scope = Scope {
    table: "items",
    parent: Some("list_id"),
    entity: "item",
};

impl Scope {
    fn filter(self) -> String {
        match self.parent {
            Some(column) => format!("{column} = ?1"),
            None => "?1 IS NULL".to_owned(),
        }
    }
}

/// The rank for a row placed at `at` among the children of `parent`.
///
/// `exclude` is the row being moved: it is ignored when looking for neighbours. A
/// `Placement::After` anchor must be a live row of the same parent; anchoring a row after
/// itself keeps its rank.
pub(crate) fn place(
    conn: &Connection,
    scope: Scope,
    parent: Option<[u8; 16]>,
    at: Placement<Uuid>,
    exclude: Option<[u8; 16]>,
) -> Result<i64> {
    // A renumber always leaves room, so the second attempt cannot fail for lack of space.
    for _ in 0..2 {
        if let Some(rank) = try_place(conn, scope, parent, at, exclude)? {
            return Ok(rank);
        }
        renumber(conn, scope, parent)?;
    }
    Err(Error::Corrupt(format!(
        "no room for a {} after renumbering",
        scope.entity
    )))
}

/// `None` means there is no room and the parent needs renumbering.
fn try_place(
    conn: &Connection,
    scope: Scope,
    parent: Option<[u8; 16]>,
    at: Placement<Uuid>,
    exclude: Option<[u8; 16]>,
) -> Result<Option<i64>> {
    let (table, filter) = (scope.table, scope.filter());
    let in_range = |rank: Option<i64>| rank.filter(|r| (-LIMIT..=LIMIT).contains(r));
    match at {
        Placement::First => {
            let min: Option<i64> = conn.query_row(
                &format!("SELECT MIN(rank) FROM {table} WHERE {filter} AND id IS NOT ?2"),
                rusqlite::params![parent, exclude],
                |r| r.get(0),
            )?;
            Ok(in_range(match min {
                None => Some(STEP),
                Some(min) => min.checked_sub(STEP),
            }))
        }
        Placement::Last => {
            let max: Option<i64> = conn.query_row(
                &format!("SELECT MAX(rank) FROM {table} WHERE {filter} AND id IS NOT ?2"),
                rusqlite::params![parent, exclude],
                |r| r.get(0),
            )?;
            Ok(in_range(match max {
                None => Some(STEP),
                Some(max) => max.checked_add(STEP),
            }))
        }
        Placement::After(anchor) => {
            let anchor_bytes = *anchor.as_bytes();
            let anchor_rank: i64 = conn
                .query_row(
                    &format!(
                        "SELECT rank FROM {table} WHERE {filter} AND id = ?2 AND deleted_at IS NULL"
                    ),
                    rusqlite::params![parent, anchor_bytes],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or_else(|| Error::NotFound {
                    entity: scope.entity,
                    id: anchor.to_string(),
                })?;
            if exclude == Some(anchor_bytes) {
                return Ok(Some(anchor_rank));
            }
            let next: Option<i64> = conn.query_row(
                &format!(
                    "SELECT MIN(rank) FROM {table} WHERE {filter} AND id IS NOT ?2 AND rank > ?3"
                ),
                rusqlite::params![parent, exclude, anchor_rank],
                |r| r.get(0),
            )?;
            Ok(match next {
                None => in_range(anchor_rank.checked_add(STEP)),
                // The gap is `next - anchor_rank`; both are within `LIMIT`, so no overflow.
                Some(next) if next - anchor_rank >= 2 => {
                    Some(anchor_rank + (next - anchor_rank) / 2)
                }
                Some(_) => None,
            })
        }
    }
}

/// Renumbers every child of `parent` to `STEP`, `2 * STEP`, ... in the current order.
fn renumber(conn: &Connection, scope: Scope, parent: Option<[u8; 16]>) -> Result<()> {
    let (table, filter) = (scope.table, scope.filter());
    // Phase 1 reverses the order and moves everything out of the way ...
    conn.execute(
        &format!("UPDATE {table} SET rank = {PARK} - rank WHERE {filter}"),
        rusqlite::params![parent],
    )?;
    // ... phase 2 numbers the rows, reading the reversed order back.
    conn.execute(
        &format!(
            "UPDATE {table} SET rank = s.next
             FROM (SELECT id, ROW_NUMBER() OVER (ORDER BY rank DESC) * {STEP} AS next
                   FROM {table} WHERE {filter}) AS s
             WHERE {table}.id = s.id"
        ),
        rusqlite::params![parent],
    )?;
    Ok(())
}
