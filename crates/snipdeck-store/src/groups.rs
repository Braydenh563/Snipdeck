//! [`GroupRepo`] for [`Store`]: named runs of contiguous items inside a list.
//!
//! A group is a row in `item_groups` plus the `items.group_id` of its members. It has no rank:
//! the members' ranks are the order, and the header is drawn above the first live member.
//!
//! A group can sit inside another of the same list (`item_groups.parent`, at most
//! [`MAX_GROUP_DEPTH`] levels). `items.group_id` is always the *innermost* group; the run of a
//! group is every live item whose group is it or one nested in it, and that run is contiguous.
//! A nested group's run is therefore a contiguous part of its parent's.
//! Rules that keep the runs contiguous (among live items):
//! - an item inserted or moved `After` a member joins that member's (innermost) group; anywhere
//!   else (`First`, `Last`, after a non-member, another list) it belongs to no group;
//! - a restored item that would split a run leaves the group; one that lies inside another
//!   group's run joins that group;
//! - a run moves as a whole, never into another group's run, and a nested group's run never out
//!   of its parent's;
//! - deleting a group (keeping its items) lifts its members and its nested groups to its own
//!   parent;
//! - a group with no member row and no group nested in it is deleted; a group with no *live*
//!   member is simply not listed, and comes back when a trashed member is restored.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, params};
use snipdeck_core::{
    Group, GroupId, ItemId, ListAccent, ListId, MAX_GROUP_DEPTH, Placement, Timestamp,
};
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
    /// The items stay in the list, in the group's own parent (or none); the groups nested in
    /// it move up to that parent too.
    Keep,
    /// The items of the group and of every group nested in it go to the trash at this time and
    /// keep their group, so restoring them brings the headers back.
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
    /// Its own live members (the items whose group it is), top to bottom.
    pub items: Vec<ItemId>,
    /// Its whole run: its members and those of every group nested in it, top to bottom. The
    /// same as `items` for a group with none nested in it.
    pub run: Vec<ItemId>,
    /// How many groups it is nested in (0 for a top-level group).
    pub depth: usize,
}

/// Deletes the groups no item row refers to and no group is nested in, until none is left to
/// delete (a parent whose last nested group went is empty in its turn).
pub(crate) fn prune(conn: &Connection) -> Result<()> {
    while conn.execute(
        "DELETE FROM item_groups WHERE NOT EXISTS
           (SELECT 1 FROM items WHERE items.group_id = item_groups.id)
         AND NOT EXISTS
           (SELECT 1 FROM item_groups child WHERE child.parent = item_groups.id)",
        [],
    )? > 0
    {}
    Ok(())
}

type Raw = [u8; 16];

/// One list's live items in order with their groups, and how its groups nest.
struct Layout {
    /// Live items top to bottom, each with its innermost group.
    items: Vec<(Raw, Option<Raw>)>,
    parent: HashMap<Raw, Option<Raw>>,
}

impl Layout {
    fn load(conn: &Connection, list: Raw) -> Result<Self> {
        let mut stmt = conn.prepare_cached(
            "SELECT id, group_id FROM items WHERE list_id = ?1 AND deleted_at IS NULL ORDER BY rank",
        )?;
        let items = stmt
            .query_map([list], |r| {
                Ok((r.get::<_, Raw>(0)?, r.get::<_, Option<Raw>>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut stmt =
            conn.prepare_cached("SELECT id, parent FROM item_groups WHERE list_id = ?1")?;
        let mut parent = stmt
            .query_map([list], |r| {
                Ok((r.get::<_, Raw>(0)?, r.get::<_, Option<Raw>>(1)?))
            })?
            .collect::<rusqlite::Result<HashMap<_, _>>>()?;
        // A parent that is not a group of this list is no parent.
        let known: Vec<Raw> = parent.keys().copied().collect();
        for id in known {
            if let Some(Some(up)) = parent.get(&id).copied()
                && !parent.contains_key(&up)
            {
                parent.insert(id, None);
            }
        }
        Ok(Self { items, parent })
    }

    /// `group` and the groups above it, innermost first. A stored loop ends where it closes.
    fn chain(&self, group: Raw) -> Vec<Raw> {
        let mut out = vec![group];
        let mut at = group;
        while let Some(Some(up)) = self.parent.get(&at).copied() {
            if out.contains(&up) {
                break;
            }
            out.push(up);
            at = up;
        }
        out
    }

    fn depth(&self, group: Raw) -> usize {
        self.chain(group).len() - 1
    }

    /// How many levels `group` and the groups nested in it take.
    fn height(&self, group: Raw) -> usize {
        let mut level = vec![group];
        let mut levels = 0;
        while !level.is_empty() && levels < 32 {
            levels += 1;
            level = self
                .parent
                .iter()
                .filter(|(_, up)| up.is_some_and(|u| level.contains(&u)))
                .map(|(g, _)| *g)
                .collect();
        }
        levels
    }

    fn in_run(&self, item_group: Option<Raw>, group: Raw) -> bool {
        item_group.is_some_and(|g| self.chain(g).contains(&group))
    }

    /// The indices into `items` of `group`'s run.
    fn run_indices(&self, group: Raw) -> Vec<usize> {
        (0..self.items.len())
            .filter(|i| self.in_run(self.items[*i].1, group))
            .collect()
    }

    /// `group`'s live run, top to bottom.
    fn run(&self, group: Raw) -> Vec<Raw> {
        self.run_indices(group)
            .into_iter()
            .map(|i| self.items[i].0)
            .collect()
    }

    /// Whether `group`'s run has no live item of another group between its first and last.
    fn contiguous(&self, group: Raw) -> bool {
        let at = self.run_indices(group);
        match (at.first(), at.last()) {
            (Some(first), Some(last)) => last - first + 1 == at.len(),
            _ => true,
        }
    }
}

/// Fails unless the run of each of `groups` (and of the groups above them) is still one block.
fn require_runs(conn: &Connection, list: Raw, groups: &[Raw]) -> Result<()> {
    let layout = Layout::load(conn, list)?;
    for group in groups {
        for g in layout.chain(*group) {
            if !layout.contiguous(g) {
                return Err(Error::Invalid(
                    "the rows would split a group's run".to_owned(),
                ));
            }
        }
    }
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

/// After a restore: an item whose group's live run it would split leaves that group (and
/// the ones above that run it splits); one lying inside another group's run joins that group
/// rather than cut it in two.
pub(crate) fn heal_restored(conn: &Connection, item: Raw) -> Result<()> {
    let Some((list, mut group)) = conn
        .query_row(
            "SELECT list_id, group_id FROM items WHERE id = ?1",
            [item],
            |r| Ok((r.get::<_, Raw>(0)?, r.get::<_, Option<Raw>>(1)?)),
        )
        .optional()?
    else {
        return Ok(());
    };
    let layout = Layout::load(conn, list)?;
    let before = group;
    if let Some(own) = group {
        let chain = layout.chain(own);
        // The outermost group whose run the item splits: it leaves that one for the one above.
        if let Some(at) = chain.iter().rposition(|g| !layout.contiguous(*g)) {
            group = layout.parent.get(&chain[at]).copied().flatten();
        }
    }
    if let Some(at) = layout.items.iter().position(|(id, _)| *id == item) {
        // Lying strictly inside the run of the others: the deepest such group takes the item.
        let mut inside: Option<(usize, Raw)> = None;
        for g in layout.parent.keys() {
            let others: Vec<usize> = layout
                .run_indices(*g)
                .into_iter()
                .filter(|i| *i != at)
                .collect();
            if let (Some(first), Some(last)) = (others.first(), others.last())
                && *first < at
                && at < *last
            {
                let depth = layout.depth(*g);
                if inside.is_none_or(|(d, _)| depth > d) {
                    inside = Some((depth, *g));
                }
            }
        }
        if let Some((_, g)) = inside
            && !layout.in_run(group, g)
        {
            group = Some(g);
        }
    }
    if group != before {
        conn.execute(
            "UPDATE items SET group_id = ?2 WHERE id = ?1",
            params![item, group],
        )?;
    }
    prune(conn)
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
    let layout = Layout::load(conn, id_bytes(list))?;
    let mut stmt = conn
        .prepare_cached("SELECT id, name, collapsed, colour FROM item_groups WHERE list_id = ?1")?;
    let rows = stmt.query_map([id_bytes(list)], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, bool>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    })?;
    let mut runs: Vec<(usize, GroupRun)> = Vec::new();
    for row in rows {
        let (id, name, collapsed, colour) = row?;
        let group: GroupId = id_from(&id)?;
        let raw = id_bytes(group);
        let at = layout.run_indices(raw);
        let Some(first) = at.first().copied() else {
            continue;
        };
        let parent = layout
            .parent
            .get(&raw)
            .copied()
            .flatten()
            .map(|p| id_from::<GroupId>(&p))
            .transpose()?;
        let ids = |own: bool| -> Result<Vec<ItemId>> {
            at.iter()
                .map(|i| &layout.items[*i])
                .filter(|e| !own || e.1 == Some(raw))
                .map(|e| id_from(&e.0))
                .collect()
        };
        runs.push((
            first,
            GroupRun {
                group: Group {
                    id: group,
                    list_id: list,
                    name: stored_name(&name)?,
                    collapsed,
                    colour: colour.as_deref().map(str::parse).transpose()?,
                    parent,
                },
                items: ids(true)?,
                run: ids(false)?,
                depth: layout.depth(raw),
            },
        ));
    }
    // List order of the first item; a group before the groups nested in it, which start at it.
    runs.sort_by_key(|(first, run)| (*first, run.depth));
    Ok(runs.into_iter().map(|(_, run)| run).collect())
}

/// Whether `parent` may hold a group (a moved one, if `moved`) that is `height` levels tall.
fn require_parent_room(
    layout: &Layout,
    moved: Option<Raw>,
    parent: Raw,
    height: usize,
) -> Result<()> {
    let cycle = moved.is_some_and(|m| layout.chain(parent).contains(&m));
    if cycle || layout.depth(parent) + 1 + height > MAX_GROUP_DEPTH {
        return Err(Error::GroupParent);
    }
    Ok(())
}

pub(crate) fn create_group(cx: &mut Ctx<'_>, group: &Group, members: &[ItemId]) -> Result<()> {
    let conn = cx.conn;
    require_list(conn, group.list_id)?;
    require_new(conn, "item_groups", group.id)?;
    let list = id_bytes(group.list_id);
    let layout = Layout::load(conn, list)?;
    let parent = group.parent.map(id_bytes);
    if let Some(parent) = parent {
        if !layout.parent.contains_key(&parent) {
            return Err(Error::GroupParent);
        }
        require_parent_room(&layout, None, parent, 1)?;
    }
    // The members in list order; a stray or missing one fails the whole call.
    let wanted: Vec<[u8; 16]> = members.iter().map(|m| id_bytes(*m)).collect();
    let ordered: Vec<[u8; 16]> = layout
        .items
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| wanted.contains(id))
        .collect();
    if ordered.is_empty() {
        return Err(Error::NotFound {
            entity: "group member",
            id: "(none)".to_owned(),
        });
    }
    if let Some(stray) = members.iter().find(|m| !ordered.contains(&id_bytes(**m))) {
        return Err(not_found(*stray));
    }
    // A group made inside another takes only that group's own members.
    if let Some(parent) = parent
        && layout
            .items
            .iter()
            .any(|(id, g)| ordered.contains(id) && *g != Some(parent))
    {
        return Err(Error::Invalid(
            "a nested group takes members of its parent only".to_owned(),
        ));
    }
    let mut touched: Vec<Raw> = layout
        .items
        .iter()
        .filter(|(id, _)| ordered.contains(id))
        .filter_map(|(_, g)| *g)
        .collect();
    conn.execute(
        "INSERT INTO item_groups (id, list_id, name, collapsed, colour, parent)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            id_bytes(group.id),
            list,
            group.name.as_str(),
            group.collapsed,
            group.colour.map(ListAccent::key),
            parent
        ],
    )?;
    gather(conn, list, id_bytes(group.id), &ordered)?;
    touched.push(id_bytes(group.id));
    require_runs(conn, list, &touched)?;
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

/// Makes `group` again where it was, for undo: `members` become its own members and `children`
/// are nested in it again, all without moving a row (they were one block before the group
/// went, and nothing has moved since). The group may have no member of its own.
pub(crate) fn restore_group(
    cx: &mut Ctx<'_>,
    group: &Group,
    members: &[ItemId],
    children: &[GroupId],
) -> Result<()> {
    let conn = cx.conn;
    require_list(conn, group.list_id)?;
    require_new(conn, "item_groups", group.id)?;
    let list = id_bytes(group.list_id);
    let layout = Layout::load(conn, list)?;
    let parent = group
        .parent
        .map(id_bytes)
        .filter(|p| layout.parent.contains_key(p));
    for child in children {
        if group_list(conn, *child)? != list {
            return Err(not_found(*child));
        }
    }
    for member in members {
        require_item(conn, *member)?;
    }
    conn.execute(
        "INSERT INTO item_groups (id, list_id, name, collapsed, colour, parent)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            id_bytes(group.id),
            list,
            group.name.as_str(),
            group.collapsed,
            group.colour.map(ListAccent::key),
            parent
        ],
    )?;
    for member in members {
        conn.execute(
            "UPDATE items SET group_id = ?2 WHERE id = ?1",
            params![id_bytes(*member), id_bytes(group.id)],
        )?;
    }
    for child in children {
        conn.execute(
            "UPDATE item_groups SET parent = ?2 WHERE id = ?1",
            params![id_bytes(*child), id_bytes(group.id)],
        )?;
    }
    let layout = Layout::load(conn, list)?;
    let own = id_bytes(group.id);
    if layout.height(own) + layout.depth(own) > MAX_GROUP_DEPTH {
        return Err(Error::GroupParent);
    }
    require_runs(conn, list, &[own])?;
    prune(conn)
}

/// Overwrites the group's name, collapsed flag and colour; its place in the nesting is
/// [`reparent_group`]'s.
pub(crate) fn update_group(cx: &mut Ctx<'_>, group: &Group) -> Result<()> {
    let list = group_list(cx.conn, group.id)?;
    if list != id_bytes(group.list_id) {
        return Err(not_found(group.id));
    }
    cx.conn.execute(
        "UPDATE item_groups SET name = ?2, collapsed = ?3, colour = ?4 WHERE id = ?1",
        params![
            id_bytes(group.id),
            group.name.as_str(),
            group.collapsed,
            group.colour.map(ListAccent::key)
        ],
    )?;
    Ok(())
}

pub(crate) fn delete_group(cx: &mut Ctx<'_>, id: GroupId, what: GroupDelete) -> Result<()> {
    let conn = cx.conn;
    group_list(conn, id)?;
    match what {
        GroupDelete::Keep => {
            let up: Option<Raw> = conn.query_row(
                "SELECT parent FROM item_groups WHERE id = ?1",
                [id_bytes(id)],
                |r| r.get(0),
            )?;
            conn.execute(
                "UPDATE items SET group_id = ?2 WHERE group_id = ?1",
                params![id_bytes(id), up],
            )?;
            conn.execute(
                "UPDATE item_groups SET parent = ?2 WHERE parent = ?1",
                params![id_bytes(id), up],
            )?;
            conn.execute("DELETE FROM item_groups WHERE id = ?1", [id_bytes(id)])?;
        }
        GroupDelete::TrashItems(at) => {
            conn.execute(
                "WITH RECURSIVE sub(id) AS (
                     SELECT ?1 UNION SELECT g.id FROM item_groups g JOIN sub ON g.parent = sub.id)
                 UPDATE items SET deleted_at = ?2
                  WHERE group_id IN (SELECT id FROM sub) AND deleted_at IS NULL",
                params![id_bytes(id), nanos(at)?],
            )?;
        }
    }
    prune(conn)
}

/// A place among the live items.
#[derive(Debug, Clone, Copy)]
enum Slot {
    First,
    Last,
    After(Raw),
}

fn slot_of(conn: &Connection, at: Placement<ItemId>) -> Result<Slot> {
    Ok(match raw_placement(at) {
        Placement::First => Slot::First,
        Placement::Last => Slot::Last,
        Placement::After(anchor) => {
            require_item(conn, ItemId::from_uuid(anchor))?;
            Slot::After(*anchor.as_bytes())
        }
    })
}

/// Moves the run of group `id` to `slot` among the live items outside it. The place is
/// corrected to a legal one: never inside the run of a group it is not in (it goes after
/// that run), and a nested group never out of its parent's run.
fn place_run(conn: &Connection, id: GroupId, list: Raw, slot: Slot) -> Result<()> {
    let layout = Layout::load(conn, list)?;
    let raw = id_bytes(id);
    let members = layout.run(raw);
    let Some(&first) = members.first() else {
        return Err(not_found(id));
    };
    let others: Vec<(Raw, Option<Raw>)> = layout
        .items
        .iter()
        .filter(|(m, _)| !members.contains(m))
        .copied()
        .collect();
    // Where the run is now, as a place among the others.
    let now = layout
        .items
        .iter()
        .take_while(|(m, _)| !members.contains(m))
        .count();
    let mut at = match slot {
        Slot::First => 0,
        Slot::Last => others.len(),
        Slot::After(anchor) => {
            if members.contains(&anchor) {
                return Err(not_found(ItemId::from_uuid(Uuid::from_bytes(anchor))));
            }
            others
                .iter()
                .position(|(m, _)| *m == anchor)
                .map_or(now, |p| p + 1)
        }
    };
    let span = |group: Raw| -> Option<(usize, usize)> {
        let found: Vec<usize> = others
            .iter()
            .enumerate()
            .filter(|(_, (_, g))| layout.in_run(*g, group))
            .map(|(i, _)| i)
            .collect();
        Some((*found.first()?, *found.last()?))
    };
    if let Some(parent) = layout.parent.get(&raw).copied().flatten() {
        at = match span(parent) {
            Some((lo, hi)) => at.clamp(lo, hi + 1),
            None => now,
        };
    }
    let mine = layout.chain(raw);
    let outside: Vec<Raw> = layout
        .parent
        .keys()
        .copied()
        .filter(|g| !mine.contains(g))
        .collect();
    while let Some((_, hi)) = outside
        .iter()
        .filter_map(|g| span(*g))
        .filter(|(lo, hi)| *lo < at && at <= *hi)
        .max_by_key(|(lo, hi)| hi - lo)
    {
        at = hi + 1;
    }
    let to = match at.checked_sub(1).and_then(|i| others.get(i)) {
        Some((anchor, _)) => Placement::After(Uuid::from_bytes(*anchor)),
        None => Placement::First,
    };
    let rank = rank::place(conn, ITEMS, Some(list), to, Some(first))?;
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

pub(crate) fn move_run(cx: &mut Ctx<'_>, id: GroupId, at: Placement<ItemId>) -> Result<()> {
    let conn = cx.conn;
    let list = group_list(conn, id)?;
    let slot = slot_of(conn, at)?;
    place_run(conn, id, list, slot)?;
    require_runs(conn, list, &[id_bytes(id)])
}

/// Nests group `id` in `parent` (`None`: at the top level) and moves its whole run to `at`,
/// corrected to a place inside the new parent's run. Fails with [`Error::GroupParent`] when the
/// parent is not a group of the list with something live in it, is the group itself or one
/// of its own, or when the nesting would be deeper than [`MAX_GROUP_DEPTH`] levels.
pub(crate) fn reparent_group(
    cx: &mut Ctx<'_>,
    id: GroupId,
    parent: Option<GroupId>,
    at: Placement<ItemId>,
) -> Result<()> {
    let conn = cx.conn;
    let list = group_list(conn, id)?;
    let layout = Layout::load(conn, list)?;
    if let Some(parent) = parent {
        let parent = id_bytes(parent);
        if !layout.parent.contains_key(&parent) || layout.run(parent).is_empty() {
            return Err(Error::GroupParent);
        }
        require_parent_room(
            &layout,
            Some(id_bytes(id)),
            parent,
            layout.height(id_bytes(id)),
        )?;
    }
    let slot = slot_of(conn, at)?;
    conn.execute(
        "UPDATE item_groups SET parent = ?2 WHERE id = ?1",
        params![id_bytes(id), parent.map(id_bytes)],
    )?;
    place_run(conn, id, list, slot)?;
    require_runs(conn, list, &[id_bytes(id)])?;
    prune(conn)
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
    let layout = Layout::load(conn, list)?;
    let others: Vec<[u8; 16]> = layout
        .run(id_bytes(group))
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

pub(crate) fn set_item_group(cx: &mut Ctx<'_>, item: ItemId, group: Option<GroupId>) -> Result<()> {
    let conn = cx.conn;
    require_item(conn, item)?;
    let item_bytes = id_bytes(item);
    if let Some(group) = group {
        let list = group_list(conn, group)?;
        let in_list: bool = conn.query_row(
            "SELECT list_id = ?2 FROM items WHERE id = ?1",
            params![item_bytes, list],
            |r| r.get(0),
        )?;
        if !in_list {
            return Err(not_found(item));
        }
    }
    conn.execute(
        "UPDATE items SET group_id = ?2 WHERE id = ?1",
        params![item_bytes, group.map(id_bytes)],
    )?;
    prune(conn)
}
