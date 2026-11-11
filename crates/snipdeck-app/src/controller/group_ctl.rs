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
            return;
        }
        let group = Group::new(list, name);
        let made = self.store.atomically(|b| b.create_group(&group, &members));
        if made.is_err() {
            self.show_toast(failed());
            return;
        }
        self.record_step(
            Action::Group,
            vec![Change::Group {
                group: group.clone(),
                members,
            }],
        );
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
            self.rename_row = Some(at);
            self.rename_seq += 1;
        }
    }

    /// Sets the name a new group starts with (the translated "Group").
    pub fn set_group_name(&mut self, name: &str) {
        name.clone_into(&mut self.group_name);
    }

    /// A header's menu row was chosen.
    pub(super) fn choose_group_entry(&mut self, id: ItemId, entry: super::ItemMenuEntry) {
        let Some(at) = self.model.rows().iter().position(|r| r.id == id) else {
            return;
        };
        match entry {
            super::ItemMenuEntry::RenameGroup => {
                self.rename_row = Some(at);
                self.rename_seq += 1;
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
                // Onto an item: after it. Onto a header: before that group.
                let placement = match target {
                    Some(t) => t
                        .items
                        .first()
                        .and_then(|i| order.iter().position(|o| o == i))
                        .and_then(|p| p.checked_sub(1))
                        .map_or(Placement::First, |p| Placement::After(order[p])),
                    None => Placement::After(b.id),
                };
                if run.items.contains(&b.id) {
                    return true;
                }
                self.store.move_run(run.group.id, placement)
            }
            (None, Some(t)) if a.is_item() => {
                self.store
                    .join_group(a.id, t.group.id, snipdeck_store::GroupEnd::First)
            }
            _ => return true,
        };
        if done.is_err() {
            self.show_toast(failed());
        } else {
            self.library_changed();
        }
        true
    }
}
