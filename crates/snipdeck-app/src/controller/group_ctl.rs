//! Groups in the Panel and the Main window: make one from the marked rows (Ctrl+G), collapse and expand,
//! rename, ungroup, delete and move. Both windows use these paths; a [`Surface`] says whose rows an index
//! points at. Each change that is not a view setting is one history step.

use snipdeck_core::{Group, ItemId, ListId, Name, Placement};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::{GroupDelete, GroupRepo, GroupRun};

use super::{Controller, Library, failed};
use crate::main_window::Key as MainKey;
use crate::{
    history::{Action, Change},
    host::ClipboardText,
};

/// What a shown row is, for stepping a group past its neighbours.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RowMark {
    Header(snipdeck_core::GroupId),
    Member,
    Loose(ItemId),
    Other,
}

/// Whose rows a group operation reads: the Panel's or the Main window's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Surface {
    Panel,
    Main,
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// Ctrl+G, or "Group..." in the item menu: the marked rows (else row `cursor`) become one
    /// group named by the window's "Group" text, and its header starts renaming. Only a list in manual order can
    /// hold groups; others offer to switch.
    pub fn group_marked(&mut self, cursor: usize) {
        self.group_marked_on(Surface::Panel, cursor);
    }

    /// Ctrl+G in the Main window: the selected rows (else the row at `cursor`) become one group.
    pub fn main_group_marked(&mut self, cursor: usize) {
        self.main_fresh();
        self.group_marked_on(Surface::Main, cursor);
    }

    fn group_marked_on(&mut self, s: Surface, cursor: usize) {
        let Some(list) = self.group_list(s) else {
            return;
        };
        if !self.group_reorderable(s) {
            self.offer_manual_for_group(s);
            return;
        }
        let Ok(name) = Name::new(&self.group_name) else {
            return;
        };
        let members = self.group_targets(s, cursor);
        if members.is_empty() {
            // On a header with nothing marked there is nothing to gather: rename its group.
            if let Some(run) = self.group_row(s, cursor) {
                self.start_group_rename(run.group.id);
            }
            return;
        }
        let group = Group::new(list, name);
        let order = self.group_order(s, list);
        // Each member's group now: Undo puts every one back exactly, whatever the moves did.
        let mut changes: Vec<Change> = members
            .iter()
            .filter_map(|id| {
                self.group_of_item(s, *id).map(|r| Change::Regroup {
                    item: *id,
                    group: Some(r.group),
                })
            })
            .collect();
        let made = self.store.atomically(|b| {
            // The members gather under the topmost: each one that is not already right below
            // the one before moves there, as a recorded move, so Undo puts it back.
            let ordered: Vec<ItemId> = order
                .iter()
                .filter(|i| members.contains(i))
                .copied()
                .collect();
            let mut now = order.clone();
            for pair in ordered.windows(2) {
                let at = now.iter().position(|i| *i == pair[1]);
                if at.and_then(|n| n.checked_sub(1)).and_then(|n| now.get(n)) == Some(&pair[0]) {
                    continue;
                }
                super::history_ctl::move_recorded(
                    b,
                    pair[1],
                    list,
                    Placement::After(pair[0]),
                    &mut changes,
                )?;
                now.retain(|i| *i != pair[1]);
                if let Some(n) = now.iter().position(|i| *i == pair[0]) {
                    now.insert(n + 1, pair[1]);
                }
            }
            b.create_group(&group, &ordered)?;
            changes.push(Change::Group {
                group: group.clone(),
                members: ordered,
            });
            Ok(())
        });
        if made.is_err() {
            self.show_toast(failed());
            return;
        }
        self.record_step(Action::Group, changes);
        self.group_clear_marks(s);
        if !self.group_refresh(s) {
            return;
        }
        if let Some(at) = self.header_row(s, group.id) {
            self.group_cursor(s, at);
            self.start_group_rename(group.id);
        }
    }

    /// Sets the name a new group starts with (the translated "Group").
    pub fn set_group_name(&mut self, name: &str) {
        name.clone_into(&mut self.group_name);
    }

    /// Asks the window to open the inline rename of group `id`'s header. The request follows the
    /// group, not a row number, and ends with [`group_rename_ended`](Self::group_rename_ended).
    pub(super) fn start_group_rename(&mut self, id: snipdeck_core::GroupId) {
        self.rename_group = Some(id);
        self.rename_seq += 1;
    }

    /// The inline rename closed (accepted or abandoned): the request is spent.
    pub fn group_rename_ended(&mut self) {
        self.rename_group = None;
    }

    /// F2 on a header in the Main window: its name opens for editing in place.
    pub fn main_group_rename_start(&mut self, index: usize) {
        self.main_fresh();
        if let Some(run) = self.group_row(Surface::Main, index) {
            self.start_group_rename(run.group.id);
        }
    }

    /// A header's menu row was chosen.
    pub(super) fn choose_group_entry(&mut self, id: ItemId, entry: super::ItemMenuEntry) {
        let Some(at) = self.model.rows().iter().position(|r| r.id == id) else {
            return;
        };
        match entry {
            super::ItemMenuEntry::RenameGroup => {
                if let Some(run) = self.group_row(Surface::Panel, at) {
                    self.start_group_rename(run.group.id);
                }
            }
            super::ItemMenuEntry::Ungroup => self.group_ungroup(at),
            super::ItemMenuEntry::MoveUp { enabled } => {
                if enabled {
                    self.group_move(at, -1);
                }
            }
            super::ItemMenuEntry::MoveDown { enabled } => {
                if enabled {
                    self.group_move(at, 1);
                }
            }
            _ => self.group_delete(at),
        }
    }

    // ---- The two windows' rows -------------------------------------------------------------------

    /// The group whose header is row `index`.
    fn group_row(&self, s: Surface, index: usize) -> Option<GroupRun> {
        match s {
            Surface::Panel => self.model.group_at(index),
            Surface::Main => self.main.group_at(index),
        }
    }

    /// The group item `id` is in.
    fn group_of_item(&self, s: Surface, id: ItemId) -> Option<GroupRun> {
        match s {
            Surface::Panel => self.model.group_of(id).cloned(),
            Surface::Main => self.main.group_of(id).cloned(),
        }
    }

    /// The list the rows belong to (the Main window: the open list; other sections have no groups).
    fn group_list(&self, s: Surface) -> Option<ListId> {
        match s {
            Surface::Panel => self.model.list().map(|l| l.id),
            Surface::Main => self.main.open_list().map(|l| l.id),
        }
    }

    fn group_reorderable(&self, s: Surface) -> bool {
        match s {
            Surface::Panel => self.model.reorderable(),
            Surface::Main => self.main.reorderable(),
        }
    }

    /// The items an action on row `cursor` applies to.
    fn group_targets(&self, s: Surface, cursor: usize) -> Vec<ItemId> {
        match s {
            Surface::Panel => self.model.targets(cursor),
            Surface::Main => self
                .main
                .targets(cursor)
                .into_iter()
                .filter_map(|k| match k {
                    MainKey::Item(id) => Some(id),
                    _ => None,
                })
                .collect(),
        }
    }

    fn group_order(&self, s: Surface, list: ListId) -> Vec<ItemId> {
        match s {
            Surface::Panel => self.model.manual_order(),
            Surface::Main => self.main.manual_order(list),
        }
    }

    /// Where group `id`'s header is among the rows.
    fn header_row(&self, s: Surface, id: snipdeck_core::GroupId) -> Option<usize> {
        match s {
            Surface::Panel => self
                .model
                .rows()
                .iter()
                .position(|r| r.header.as_ref().is_some_and(|h| h.group == id)),
            Surface::Main => self
                .main
                .keys()
                .iter()
                .position(|k| *k == MainKey::Group(id)),
        }
    }

    fn group_cursor(&mut self, s: Surface, at: usize) {
        match s {
            Surface::Panel => self.model.select_row(at),
            Surface::Main => self.main.set_cursor(at),
        }
    }

    /// What each shown row is, for stepping a group past its neighbours.
    fn row_marks(&self, s: Surface) -> Vec<RowMark> {
        match s {
            Surface::Panel => self
                .model
                .rows()
                .iter()
                .map(|r| match &r.header {
                    Some(h) => RowMark::Header(h.group),
                    None if r.grouped => RowMark::Member,
                    None if r.is_item() => RowMark::Loose(r.id),
                    None => RowMark::Other,
                })
                .collect(),
            Surface::Main => self
                .main
                .keys()
                .iter()
                .map(|k| match k {
                    MainKey::Group(g) => RowMark::Header(*g),
                    MainKey::Item(id) if self.main.group_of(*id).is_some() => RowMark::Member,
                    MainKey::Item(id) => RowMark::Loose(*id),
                    _ => RowMark::Other,
                })
                .collect(),
        }
    }


    fn group_clear_marks(&mut self, s: Surface) {
        match s {
            Surface::Panel => self.model.clear_marks(),
            Surface::Main => self.main.clear_selection(),
        }
    }

    /// Reads the library again after a change. A failure shows the notice.
    fn group_refresh(&mut self, s: Surface) -> bool {
        if self.refresh_panel().is_err() {
            self.show_toast(failed());
            return false;
        }
        if s == Surface::Main {
            self.main_fresh();
        }
        true
    }

    /// A refused group change in a list that sorts itself: offers manual order (a search or a non-list section
    /// has nothing to offer).
    fn offer_manual_for_group(&mut self, s: Surface) {
        match s {
            Surface::Panel => self.offer_manual_for_panel(),
            Surface::Main => {
                if !self.main.searching()
                    && let Some(list) = self.main.open_list()
                    && list.sort != snipdeck_core::ListSort::Manual
                {
                    let id = list.id;
                    self.offer_manual_order(id);
                }
            }
        }
    }

    /// The row to drop group `index` on to move it one place (`delta` of 1 or -1): the next
    /// block (a group's header or a loose row) in that direction. `None` at the ends.
    fn group_step_target(&self, s: Surface, index: usize, delta: isize) -> Option<usize> {
        let run = self.group_row(s, index)?;
        let list = self.group_list(s)?;
        let order = self.group_order(s, list);
        let head = order.iter().position(|i| run.items.first() == Some(i))?;
        let tail = head + run.items.len().saturating_sub(1);
        let next = if delta > 0 {
            order.get(tail + 1)
        } else {
            order.get(head.checked_sub(1)?)
        }?;
        let group = self.group_of_item(s, *next).map(|g| g.group.id);
        self.row_marks(s).iter().position(|m| match (m, group) {
            (RowMark::Header(h), Some(g)) => *h == g,
            (RowMark::Loose(id), None) => id == next,
            _ => false,
        })
    }

    /// Whether the group whose header is row `index` can move `delta` places (1 or -1).
    pub(crate) fn can_move_group(&self, index: usize, delta: isize) -> bool {
        self.model.reorderable() && self.group_step_target(Surface::Panel, index, delta).is_some()
    }

    /// Alt+Up / Alt+Down on a header, or "Move up / down" in its menu: the whole group moves one
    /// place past the next group or loose row (the keyboard way to drag a group). One history
    /// step; the cursor stays on the header and the new place is announced.
    pub(crate) fn group_move(&mut self, index: usize, delta: isize) {
        self.group_move_on(Surface::Panel, index, delta);
    }

    /// [`group_move`](Self::group_move) for either window's rows.
    pub(super) fn group_move_on(&mut self, s: Surface, index: usize, delta: isize) {
        let Some(group) = self.group_row(s, index).map(|r| r.group.id) else {
            return;
        };
        if !self.group_reorderable(s) {
            self.offer_manual_for_group(s);
            return;
        }
        let Some(onto) = self.group_step_target(s, index, delta) else {
            return;
        };
        self.group_drop_on(s, index, onto);
        let marks = self.row_marks(s);
        let Some(at) = self.header_row(s, group) else {
            return;
        };
        self.group_cursor(s, at);
        let blocks: Vec<bool> = marks
            .iter()
            .filter(|m| !matches!(m, RowMark::Member))
            .map(|m| matches!(m, RowMark::Header(g) if *g == group))
            .collect();
        if let Some(place) = blocks.iter().position(|mine| *mine) {
            self.announce_placed(place + 1, blocks.len());
        }
    }

    /// A click on a header's chevron, or Left / Right on it: collapses or expands the group.
    /// `collapsed` of `None` toggles. Not a history step: it is a view setting.
    pub fn group_collapse(&mut self, index: usize, collapsed: Option<bool>) {
        self.group_collapse_on(Surface::Panel, index, collapsed);
    }

    /// The Main window's header: a click, or Left / Right.
    pub fn main_group_collapse(&mut self, index: usize, collapsed: Option<bool>) {
        self.main_fresh();
        self.group_collapse_on(Surface::Main, index, collapsed);
    }

    fn group_collapse_on(&mut self, s: Surface, index: usize, collapsed: Option<bool>) {
        let Some(run) = self.group_row(s, index) else {
            return;
        };
        let want = collapsed.unwrap_or(!run.group.collapsed);
        if want == run.group.collapsed {
            return;
        }
        let mut group = run.group;
        group.collapsed = want;
        if self.store.update_group(&group).is_err() {
            self.show_toast(failed());
            return;
        }
        if !self.group_refresh(s) {
            return;
        }
        // The cursor stays on the header.
        if let Some(at) = self.header_row(s, group.id) {
            self.group_cursor(s, at);
        }
    }

    /// The inline rename of header `index` was accepted. An empty or unchanged name does nothing.
    pub fn group_rename(&mut self, index: usize, name: &str) {
        self.group_rename_on(Surface::Panel, index, name);
    }

    /// The inline rename of a Main window header was accepted.
    pub fn main_group_rename(&mut self, index: usize, name: &str) {
        self.main_fresh();
        self.group_rename_on(Surface::Main, index, name);
    }

    fn group_rename_on(&mut self, s: Surface, index: usize, name: &str) {
        let Some(run) = self.group_row(s, index) else {
            return;
        };
        let Ok(name) = Name::new(name) else {
            return;
        };
        if name == run.group.name {
            return;
        }
        let before = run.group;
        let mut after = before.clone();
        after.name = name;
        if self.store.update_group(&after).is_err() {
            self.show_toast(failed());
            return;
        }
        self.record_step(Action::Group, vec![Change::Rename { before, after }]);
        self.group_refresh(s);
    }

    /// Ungroup: the header goes, the items stay. One Undo step.
    pub fn group_ungroup(&mut self, index: usize) {
        self.group_ungroup_on(Surface::Panel, index);
    }

    /// Ungroup, from the Main window's header menu.
    pub fn main_group_ungroup(&mut self, index: usize) {
        self.main_fresh();
        self.group_ungroup_on(Surface::Main, index);
    }

    fn group_ungroup_on(&mut self, s: Surface, index: usize) {
        let Some(run) = self.group_row(s, index) else {
            return;
        };
        let gone = self
            .store
            .atomically(|b| b.delete_group(run.group.id, GroupDelete::Keep));
        if gone.is_err() {
            self.show_toast(failed());
            return;
        }
        self.record_step(
            Action::Group,
            vec![Change::Ungroup {
                group: run.group,
                members: run.items,
            }],
        );
        self.group_refresh(s);
    }

    /// Delete group: the header and its items go to the Trash, with the usual Undo toast (one
    /// step; restoring the items brings the header back).
    pub fn group_delete(&mut self, index: usize) {
        let Some(run) = self.group_row(Surface::Panel, index) else {
            return;
        };
        let items: Vec<ItemId> = run.items;
        self.trash_with_undo(&items);
    }

    /// Delete group from the Main window: the same, with the same Undo toast.
    pub fn main_group_delete(&mut self, index: usize) {
        self.main_fresh();
        let Some(run) = self.group_row(Surface::Main, index) else {
            return;
        };
        self.trash_with_undo(&run.items);
        self.main_fresh();
    }

    /// A drag that involves a header: a header dropped on a row moves the whole group there; an
    /// item dropped on a header joins that group at its top. Returns whether it was one of these.
    /// Both are one history step.
    pub(super) fn group_drop(&mut self, from: usize, onto: usize) -> bool {
        self.group_drop_on(Surface::Panel, from, onto)
    }

    /// What row `index` stands for in a drag: a header, or an item (`real`: not a "Deleted · Undo" ghost).
    fn drag_row(&self, s: Surface, index: usize) -> Option<DragRow> {
        match s {
            Surface::Panel => {
                let row = self.model.rows().into_iter().nth(index)?;
                Some(if row.header.is_some() {
                    DragRow::Header
                } else {
                    DragRow::Item {
                        id: row.id,
                        real: row.is_item(),
                    }
                })
            }
            Surface::Main => match self.main.keys().get(index)? {
                MainKey::Group(_) => Some(DragRow::Header),
                MainKey::Item(id) => Some(DragRow::Item {
                    id: *id,
                    real: true,
                }),
                _ => None,
            },
        }
    }

    /// [`group_drop`](Self::group_drop) for either window's rows.
    pub(super) fn group_drop_on(&mut self, s: Surface, from: usize, onto: usize) -> bool {
        let (Some(a), Some(b)) = (self.drag_row(s, from), self.drag_row(s, onto)) else {
            return false;
        };
        if a != DragRow::Header && b != DragRow::Header {
            return false;
        }
        let Some(list) = self.group_list(s) else {
            self.offer_manual_for_group(s);
            return true;
        };
        if !self.group_reorderable(s) {
            self.offer_manual_for_group(s);
            return true;
        }
        let order = self.group_order(s, list);
        let target_id = match b {
            DragRow::Item { id, .. } => Some(id),
            DragRow::Header => None,
        };
        let done = match (self.group_row(s, from), self.group_row(s, onto)) {
            (Some(run), target) => {
                if target_id.is_some_and(|id| run.items.contains(&id))
                    || target.as_ref().is_some_and(|t| t.group.id == run.group.id)
                {
                    return true;
                }
                let down = from < onto;
                let anchor = match &target {
                    Some(t) if down => t.items.last().copied(),
                    Some(t) => t.items.first().copied(),
                    None => target_id,
                };
                let Some(anchor) = anchor else {
                    return true;
                };
                let to = if down {
                    Placement::After(anchor)
                } else {
                    // Above the anchor's group (or the anchor), outside the dragged run.
                    let top = self
                        .group_of_item(s, anchor)
                        .and_then(|g| g.items.first().copied())
                        .unwrap_or(anchor);
                    let Some(to) = place_before(&order, top, &run.items) else {
                        return true;
                    };
                    to
                };
                let Some(back) = place_before(&order, run.items[0], &run.items) else {
                    return true;
                };
                let moved = self.store.move_run(run.group.id, to);
                if moved.is_ok() {
                    self.record_step(
                        Action::Reorder,
                        vec![Change::MoveRun {
                            group: run.group.id,
                            from: back,
                            to,
                        }],
                    );
                }
                moved
            }
            (None, Some(t)) => match a {
                DragRow::Item { id, real: true } => self.join_dropped(s, id, &t),
                _ => return true,
            },
            _ => return true,
        };
        if done.is_err() {
            self.show_toast(failed());
        } else {
            self.library_changed();
            if s == Surface::Main {
                self.main_fresh();
            }
        }
        true
    }

    /// Item `id` dropped on the header of `target`: it joins the group at its top (one step).
    fn join_dropped(
        &mut self,
        s: Surface,
        id: ItemId,
        target: &snipdeck_store::GroupRun,
    ) -> snipdeck_store::Result<()> {
        let from = super::history_ctl::spot_of(&self.store, id)?;
        let old = self.group_of_item(s, id);
        self.store
            .join_group(id, target.group.id, snipdeck_store::GroupEnd::First)?;
        self.record_step(
            Action::Reorder,
            vec![Change::Join {
                item: id,
                group: target.group.id,
                from,
                from_group: old.map(|r| r.group),
            }],
        );
        Ok(())
    }
}

/// What a drag starts on or lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragRow {
    Header,
    Item { id: ItemId, real: bool },
}

/// The placement directly above `anchor` in `order`, not counting the `run` being moved.
/// `None` when `anchor` is not in `order`.
fn place_before(order: &[ItemId], anchor: ItemId, run: &[ItemId]) -> Option<Placement<ItemId>> {
    let at = order.iter().position(|i| *i == anchor)?;
    Some(
        order[..at]
            .iter()
            .rev()
            .find(|i| !run.contains(i))
            .map_or(Placement::First, |p| Placement::After(*p)),
    )
}
