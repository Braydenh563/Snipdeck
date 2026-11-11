//! Groups in the Panel: make one from the marked rows (Ctrl+G), collapse and expand, rename,
//! ungroup and delete. Each change that is not a view setting is one history step.

use snipdeck_core::{Group, ItemId, Name, Placement};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::{GroupDelete, GroupRepo};

use super::{Controller, Library, failed};
use crate::{
    history::{Action, Change},
    host::ClipboardText,
};

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
        let Some(list) = self.model.list().map(|l| l.id) else {
            return;
        };
        if !self.model.reorderable() {
            self.offer_manual_for_panel();
            return;
        }
        let Ok(name) = Name::new(&self.group_name) else {
            return;
        };
        let members = self.model.targets(cursor);
        if members.is_empty() {
            // On a header with nothing marked there is nothing to gather: rename its group.
            if let Some(run) = self.group_row(cursor) {
                self.start_group_rename(run.group.id);
            }
            return;
        }
        let group = Group::new(list, name);
        let order = self.model.manual_order();
        let mut changes = Vec::new();
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
        self.model.clear_marks();
        if self.refresh_panel().is_err() {
            self.show_toast(failed());
            return;
        }
        let at = self
            .model
            .rows()
            .iter()
            .position(|r| r.header.as_ref().is_some_and(|h| h.group == group.id));
        if let Some(at) = at {
            self.model.select_row(at);
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

    /// A header's menu row was chosen.
    pub(super) fn choose_group_entry(&mut self, id: ItemId, entry: super::ItemMenuEntry) {
        let Some(at) = self.model.rows().iter().position(|r| r.id == id) else {
            return;
        };
        match entry {
            super::ItemMenuEntry::RenameGroup => {
                if let Some(run) = self.group_row(at) {
                    self.start_group_rename(run.group.id);
                }
            }
            super::ItemMenuEntry::Ungroup => self.group_ungroup(at),
            _ => self.group_delete(at),
        }
    }

    /// The group whose header is row `index`.
    fn group_row(&self, index: usize) -> Option<snipdeck_store::GroupRun> {
        self.model.group_at(index)
    }

    /// A click on a header's chevron, or Left / Right on it: collapses or expands the group.
    /// `collapsed` of `None` toggles. Not a history step: it is a view setting.
    pub fn group_collapse(&mut self, index: usize, collapsed: Option<bool>) {
        let Some(run) = self.group_row(index) else {
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
        if self.refresh_panel().is_err() {
            self.show_toast(failed());
        }
        // The cursor stays on the header.
        if let Some(at) = self
            .model
            .rows()
            .iter()
            .position(|r| r.header.as_ref().is_some_and(|h| h.group == group.id))
        {
            self.model.select_row(at);
        }
    }

    /// The inline rename of header `index` was accepted. An empty or unchanged name does nothing.
    pub fn group_rename(&mut self, index: usize, name: &str) {
        let Some(run) = self.group_row(index) else {
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
        if self.refresh_panel().is_err() {
            self.show_toast(failed());
        }
    }

    /// Ungroup: the header goes, the items stay. One Undo step.
    pub fn group_ungroup(&mut self, index: usize) {
        let Some(run) = self.group_row(index) else {
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
        if self.refresh_panel().is_err() {
            self.show_toast(failed());
        }
    }

    /// Delete group: the header and its items go to the Trash, with the usual Undo toast (one
    /// step; restoring the items brings the header back).
    pub fn group_delete(&mut self, index: usize) {
        let Some(run) = self.group_row(index) else {
            return;
        };
        let items: Vec<ItemId> = run.items;
        self.trash_with_undo(&items);
    }

    /// A drag that involves a header: a header dropped on a row moves the whole group there; an
    /// item dropped on a header joins that group at its top. Returns whether it was one of these.
    /// Both are one history step.
    pub(super) fn group_drop(&mut self, from: usize, onto: usize) -> bool {
        let rows = self.model.rows();
        let (Some(a), Some(b)) = (rows.get(from), rows.get(onto)) else {
            return false;
        };
        if a.header.is_none() && b.header.is_none() {
            return false;
        }
        if !self.model.reorderable() {
            self.offer_manual_for_panel();
            return true;
        }
        let order = self.model.manual_order();
        let done = match (self.group_row(from), self.group_row(onto)) {
            (Some(run), target) => {
                if run.items.contains(&b.id)
                    || target.as_ref().is_some_and(|t| t.group.id == run.group.id)
                {
                    return true;
                }
                let down = from < onto;
                let anchor = match &target {
                    Some(t) if down => t.items.last().copied(),
                    Some(t) => t.items.first().copied(),
                    None => Some(b.id),
                };
                let Some(anchor) = anchor else {
                    return true;
                };
                let to = if down {
                    Placement::After(anchor)
                } else {
                    // Above the anchor's group (or the anchor), outside the dragged run.
                    let top = self
                        .model
                        .group_of(anchor)
                        .and_then(|g| g.items.first().copied())
                        .unwrap_or(anchor);
                    place_before(&order, top, &run.items)
                };
                let back = place_before(&order, run.items[0], &run.items);
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
            (None, Some(t)) if a.is_item() => self.join_dropped(a.id, &t),
            _ => return true,
        };
        if done.is_err() {
            self.show_toast(failed());
        } else {
            self.library_changed();
        }
        true
    }

    /// Item `id` dropped on the header of `target`: it joins the group at its top (one step).
    fn join_dropped(
        &mut self,
        id: ItemId,
        target: &snipdeck_store::GroupRun,
    ) -> snipdeck_store::Result<()> {
        let from = super::history_ctl::spot_of(&self.store, id)?;
        let old = self.model.group_of(id).cloned();
        self.store
            .join_group(id, target.group.id, snipdeck_store::GroupEnd::First)?;
        self.record_step(
            Action::Reorder,
            vec![Change::Join {
                item: id,
                group: target.group.id,
                from,
                from_group: old.as_ref().map(|r| r.group.id),
                was_first: old.as_ref().is_some_and(|r| r.items.first() == Some(&id)),
            }],
        );
        Ok(())
    }
}

/// The placement directly above `anchor` in `order`, not counting the `run` being moved.
fn place_before(order: &[ItemId], anchor: ItemId, run: &[ItemId]) -> Placement<ItemId> {
    let at = order.iter().position(|i| *i == anchor).unwrap_or(0);
    order[..at]
        .iter()
        .rev()
        .find(|i| !run.contains(i))
        .map_or(Placement::First, |p| Placement::After(*p))
}
