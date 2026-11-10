//! [`GroupRepo`] for [`Store`]: named runs of contiguous items inside a list.
//!
//! A group is a row in `item_groups` plus the `items.group_id` of its members. It has no rank:
//! the members' ranks are the order, and the header is drawn above the first live member.
//! Rules that keep the run contiguous (among live items):
//! - an item inserted or moved `After` a member joins that member's group; anywhere else
//!   (`First`, `Last`, after a non-member, another list) it belongs to no group;
//! - a restored item that would split its group's run leaves the group;
//! - a group with no member row at all is deleted; a group with no *live* member is simply
//!   not listed, and comes back when a trashed member is restored.

use rusqlite::{Connection, OptionalExtension, params};
use snipdeck_core::{Group, GroupId, ItemId, ListId, Placement, Timestamp};
use uuid::Uuid;

use crate::batch::Ctx;
use crate::codec::{
    id_bytes, id_from, nanos, not_found, raw_placement, require_item, require_list, require_new,
    stored_name,
};
use crate::rank::{self, ITEMS};
use crate::{Error, Result};

/// What deleting a group does to its items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupDelete {
    /// The items stay in the list, ungrouped.
    Keep,
    /// The items go to the trash at this time and keep their group, so restoring them brings
    /// the header back.
    TrashItems(Timestamp),
}

/// Which end of a group an item joins at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupEnd {
    /// Directly above the group's first live member.
    First,
    /// Directly below its last live member.
    Last,
}

/// A group and its live members, in list order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupRun {
    /// The group.
    pub group: Group,
    /// Its live members, top to bottom.
    pub items: Vec<ItemId>,
}

/// Deletes the groups no item row refers to.
pub(crate) fn prune(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM item_groups WHERE NOT EXISTS
           (SELECT 1 FROM items WHERE items.group_id = item_groups.id)",
        [],
    )?;
    Ok(())
}

/// The group a new or moved item joins when placed at `at`: its anchor's.
pub(crate) fn inherited(conn: &Connection, at: Placement<Uuid>) -> Result<Option<[u8; 16]>> {
    let Placement::After(anchor) = at else {
        return Ok(None);
    };
    Ok(conn
        .query_row(
            "SELECT group_id FROM items WHERE id = ?1",
            [*anchor.as_bytes()],
            |r| r.get::<_, Option<Vec<u8>>>(0),
        )
        .optional()?
        .flatten()
        .and_then(|bytes| <[u8; 16]>::try_from(bytes).ok()))
}

/// After a restore: an item whose group's live run it would split leaves the group.
pub(crate) fn heal_restored(conn: &Connection, item: [u8; 16]) -> Result<()> {
    let split: bool = conn.query_row(
        "SELECT EXISTS (
           SELECT 1 FROM items me JOIN items other
             ON other.list_id = me.list_id AND other.deleted_at IS NULL
            AND other.group_id IS NOT me.group_id
            AND other.rank BETWEEN
                (SELECT MIN(m.rank) FROM items m WHERE m.group_id = me.group_id AND m.deleted_at IS NULL)
            AND (SELECT MAX(m.rank) FROM items m WHERE m.group_id = me.group_id AND m.deleted_at IS NULL)
           WHERE me.id = ?1 AND me.group_id IS NOT NULL)",
        [item],
        |r| r.get(0),
    )?;
    if split {
        conn.execute("UPDATE items SET group_id = NULL WHERE id = ?1", [item])?;
    }
    // Lying inside another group's run: join that run rather than cut it in two.
    conn.execute(
        "UPDATE items SET group_id = (
             SELECT h.group_id FROM items h, items me
              WHERE me.id = ?1 AND h.list_id = me.list_id AND h.deleted_at IS NULL
                AND h.group_id IS NOT NULL AND h.group_id IS NOT me.group_id AND h.id != me.id
              GROUP BY h.group_id HAVING MIN(h.rank) < me.rank AND MAX(h.rank) > me.rank
              LIMIT 1)
         WHERE id = ?1 AND EXISTS (
             SELECT 1 FROM items h, items me
              WHERE me.id = ?1 AND h.list_id = me.list_id AND h.deleted_at IS NULL
                AND h.group_id IS NOT NULL AND h.group_id IS NOT me.group_id AND h.id != me.id
              GROUP BY h.group_id HAVING MIN(h.rank) < me.rank AND MAX(h.rank) > me.rank)",
        [item],
    )?;
    prune(conn)
}

fn live_members(conn: &Connection, group: [u8; 16]) -> Result<Vec<[u8; 16]>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id FROM items WHERE group_id = ?1 AND deleted_at IS NULL ORDER BY rank",
    )?;
    let rows = stmt.query_map([group], |r| r.get::<_, [u8; 16]>(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The group's list, or `NotFound`.
fn group_list(conn: &Connection, id: GroupId) -> Result<[u8; 16]> {
    conn.query_row(
        "SELECT list_id FROM item_groups WHERE id = ?1",
        [id_bytes(id)],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| not_found(id))
}

pub(crate) fn groups_of(conn: &Connection, list: ListId) -> Result<Vec<GroupRun>> {
    require_list(conn, list)?;
    let mut stmt = conn.prepare_cached(
        "SELECT g.id, g.name, g.collapsed, i.id FROM items i
           JOIN item_groups g ON g.id = i.group_id
          WHERE i.list_id = ?1 AND i.deleted_at IS NULL ORDER BY i.rank",
    )?;
    let rows = stmt.query_map([id_bytes(list)], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, bool>(2)?,
            r.get::<_, Vec<u8>>(3)?,
        ))
    })?;
    let mut runs: Vec<GroupRun> = Vec::new();
    for row in rows {
        let (group, name, collapsed, item) = row?;
        let group: GroupId = id_from(&group)?;
        let item: ItemId = id_from(&item)?;
        match runs.iter_mut().find(|run| run.group.id == group) {
            Some(run) => run.items.push(item),
            None => runs.push(GroupRun {
                group: Group {
                    id: group,
                    list_id: list,
                    name: stored_name(&name)?,
                    collapsed,
                },
                items: vec![item],
            }),
        }
    }
    Ok(runs)
}

pub(crate) fn create_group(cx: &mut Ctx<'_>, group: &Group, members: &[ItemId]) -> Result<()> {
    let conn = cx.conn;
    require_list(conn, group.list_id)?;
    require_new(conn, "item_groups", group.id)?;
    let list = id_bytes(group.list_id);
    // The members in list order; a stray or missing one fails the whole call.
    let mut ordered = Vec::new();
    {
        let mut stmt = conn.prepare_cached(
            "SELECT id FROM items WHERE list_id = ?1 AND deleted_at IS NULL ORDER BY rank",
        )?;
        let wanted: Vec<[u8; 16]> = members.iter().map(|m| id_bytes(*m)).collect();
        for row in stmt.query_map([list], |r| r.get::<_, [u8; 16]>(0))? {
            let id = row?;
            if wanted.contains(&id) {
                ordered.push(id);
            }
        }
        if ordered.is_empty() {
            return Err(Error::NotFound {
                entity: "group member",
                id: "(none)".to_owned(),
            });
        }
        if let Some(stray) = members.iter().find(|m| !ordered.contains(&id_bytes(**m))) {
            return Err(not_found(*stray));
        }
    }
    conn.execute(
        "INSERT INTO item_groups (id, list_id, name, collapsed) VALUES (?1, ?2, ?3, ?4)",
        params![
            id_bytes(group.id),
            list,
            group.name.as_str(),
            group.collapsed
        ],
    )?;
    gather(conn, list, id_bytes(group.id), &ordered)?;
    prune(conn)
}

/// Makes `ordered` (in list order) members of `group` and stacks them below the first.
fn gather(conn: &Connection, list: [u8; 16], group: [u8; 16], ordered: &[[u8; 16]]) -> Result<()> {
    let mut prev: Option<[u8; 16]> = None;
    for id in ordered {
        conn.execute(
            "UPDATE items SET group_id = ?2 WHERE id = ?1",
            params![id, group],
        )?;
        if let Some(prev) = prev {
            let at = Placement::After(Uuid::from_bytes(prev));
            let rank = rank::place(conn, ITEMS, Some(list), at, Some(*id))?;
            conn.execute(
                "UPDATE items SET rank = ?2 WHERE id = ?1",
                params![id, rank],
            )?;
        }
        prev = Some(*id);
    }
    Ok(())
}

pub(crate) fn update_group(cx: &mut Ctx<'_>, group: &Group) -> Result<()> {
    let list = group_list(cx.conn, group.id)?;
    if list != id_bytes(group.list_id) {
        return Err(not_found(group.id));
    }
    cx.conn.execute(
        "UPDATE item_groups SET name = ?2, collapsed = ?3 WHERE id = ?1",
        params![id_bytes(group.id), group.name.as_str(), group.collapsed],
    )?;
    Ok(())
}

pub(crate) fn delete_group(cx: &mut Ctx<'_>, id: GroupId, what: GroupDelete) -> Result<()> {
    let conn = cx.conn;
    group_list(conn, id)?;
    match what {
        GroupDelete::Keep => {
            conn.execute(
                "UPDATE items SET group_id = NULL WHERE group_id = ?1",
                [id_bytes(id)],
            )?;
        }
        GroupDelete::TrashItems(at) => {
            conn.execute(
                "DELETE FROM triggers WHERE item_id IN
                   (SELECT id FROM items WHERE group_id = ?1 AND deleted_at IS NULL)",
                [id_bytes(id)],
            )?;
            conn.execute(
                "UPDATE items SET deleted_at = ?2 WHERE group_id = ?1 AND deleted_at IS NULL",
                params![id_bytes(id), nanos(at)?],
            )?;
        }
    }
    prune(conn)
}

pub(crate) fn move_run(cx: &mut Ctx<'_>, id: GroupId, at: Placement<ItemId>) -> Result<()> {
    let conn = cx.conn;
    let list = group_list(conn, id)?;
    let members = live_members(conn, id_bytes(id))?;
    let Some(&first) = members.first() else {
        return Err(not_found(id));
    };
    let mut at = raw_placement(at);
    if let Placement::After(anchor) = at {
        let anchor_bytes = *anchor.as_bytes();
        require_item(conn, ItemId::from_uuid(anchor))?;
        if members.contains(&anchor_bytes) {
            return Err(not_found(ItemId::from_uuid(anchor)));
        }
        // Never land inside another group: go after its last live member.
        if let Some(other) = inherited(conn, at)?
            && let Some(last) = live_members(conn, other)?.last()
        {
            at = Placement::After(Uuid::from_bytes(*last));
        }
    }
    let rank = rank::place(conn, ITEMS, Some(list), at, Some(first))?;
    conn.execute(
        "UPDATE items SET rank = ?2 WHERE id = ?1",
        params![first, rank],
    )?;
    let mut prev = first;
    for member in &members[1..] {
        let at = Placement::After(Uuid::from_bytes(prev));
        let rank = rank::place(conn, ITEMS, Some(list), at, Some(*member))?;
        conn.execute(
            "UPDATE items SET rank = ?2 WHERE id = ?1",
            params![member, rank],
        )?;
        prev = *member;
    }
    Ok(())
}

pub(crate) fn join_group(
    cx: &mut Ctx<'_>,
    item: ItemId,
    group: GroupId,
    end: GroupEnd,
) -> Result<()> {
    let conn = cx.conn;
    require_item(conn, item)?;
    let list = group_list(conn, group)?;
    let item_bytes = id_bytes(item);
    let in_list: bool = conn.query_row(
        "SELECT list_id = ?2 FROM items WHERE id = ?1",
        params![item_bytes, list],
        |r| r.get(0),
    )?;
    if !in_list {
        return Err(not_found(item));
    }
    let others: Vec<[u8; 16]> = live_members(conn, id_bytes(group))?
        .into_iter()
        .filter(|m| *m != item_bytes)
        .collect();
    let at = match (end, others.first(), others.last()) {
        (GroupEnd::Last, _, Some(tail)) => Placement::After(Uuid::from_bytes(*tail)),
        (GroupEnd::First, Some(head), _) => {
            // Directly above the head: after whatever sorts just before it, else the top.
            let before: Option<[u8; 16]> = conn
                .query_row(
                    "SELECT id FROM items WHERE list_id = ?1 AND id IS NOT ?3
                       AND rank < (SELECT rank FROM items WHERE id = ?2)
                     ORDER BY rank DESC LIMIT 1",
                    params![list, head, item_bytes],
                    |r| r.get(0),
                )
                .optional()?;
            before.map_or(Placement::First, |b| Placement::After(Uuid::from_bytes(b)))
        }
        _ => return Err(not_found(group)),
    };
    let rank = rank::place(conn, ITEMS, Some(list), at, Some(item_bytes))?;
    conn.execute(
        "UPDATE items SET rank = ?2, group_id = ?3 WHERE id = ?1",
        params![item_bytes, rank, id_bytes(group)],
    )?;
    prune(conn)
}
