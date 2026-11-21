//! Groups in the Panel and the Main window: make one from the marked rows (Ctrl+G), collapse and expand,
//! rename, ungroup, delete and move. Both windows use these paths; a [`Surface`] says whose rows an index
//! points at. Each change that is not a view setting is one history step.

use snipdeck_core::{Group, GroupId, GroupTree, ItemId, ListId, MAX_GROUP_DEPTH, Name, Placement};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::{GroupDelete, GroupRepo, GroupRun};

use super::{Controller, Library, failed, neutral};
use crate::main_window::Key as MainKey;
use crate::toast::ToastMessage;
use crate::{
    history::{Action, Change, Membership},
    host::ClipboardText,
};

/// What a shown row is, for stepping a group past its neighbours.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RowMark {
    Header(GroupId),
    Item(ItemId),
    Other,
}

/// One block among the siblings of a group: a group (with all it holds) or an item that is a
/// member of the parent itself (a loose item at the top level).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Block {
    Group(GroupId),
    Item(ItemId),
}

/// A window's request to open a header's inline rename, and a counter that tells the window it
/// is a new request.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct Rename {
    pub(super) group: Option<snipdeck_core::GroupId>,
    pub(super) seq: u64,
}

/// What the Main window's "Nest under…" chooser is offering: the group it moves and where it may go.
#[derive(Debug, Clone)]
pub(super) struct NestChooser {
    group: GroupId,
    /// The choices in the order shown: `None` is the top level.
    options: Vec<Option<GroupId>>,
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
    /// Ctrl+G, or "Group…" in the item menu: the marked rows (else row `cursor`) become one
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
            self.show_toast(neutral(ToastMessage::GroupNeedsList));
            return;
        };
        // Under a filter the rows show no headers: a group made now would be invisible.
        if s == Surface::Main
            && self.main.open_list().is_some()
            && !self.main.searching()
            && !self.main.shows_groups()
        {
            self.show_toast(neutral(ToastMessage::GroupColourFilter));
            return;
        }
        if !self.group_reorderable(s) {
            self.offer_manual_for_group(s);
            return;
        }
        // A window text that is no valid name (empty, too long) must not stop the group: it takes
        // the plain word, and its header starts renaming anyway.
        let Ok(name) = Name::new(&self.group_name).or_else(|_| Name::new("Group")) else {
            return;
        };
        let members = self.group_targets(s, cursor);
        if members.is_empty() {
            // On a header with nothing marked there is nothing to gather: rename its group.
            if let Some(run) = self.group_row(s, cursor) {
                self.start_group_rename(s, run.group.id);
            } else {
                self.show_toast(neutral(ToastMessage::GroupNothing));
            }
            return;
        }
        let mut group = Group::new(list, name);
        let order = self.group_order(s, list);
        // Each member's group now: Undo puts every one back exactly, whatever the moves did.
        let Ok(runs) = self.store.groups_of(list) else {
            self.show_toast(failed());
            return;
        };
        // Rows that all sit in one group make a group nested in it (a mixed selection makes one at
        // the top level, as it always did).
        let parents: Vec<Option<GroupId>> = members
            .iter()
            .map(|id| {
                runs.iter()
                    .find(|r| r.items.contains(id))
                    .map(|r| r.group.id)
            })
            .collect();
        if let Some(Some(parent)) = parents.first().copied()
            && parents.iter().all(|p| *p == Some(parent))
        {
            let tree = GroupTree::new(runs.iter().map(|r| &r.group));
            if tree.depth(parent) + 2 > MAX_GROUP_DEPTH {
                self.show_toast(neutral(ToastMessage::GroupTooDeep));
                return;
            }
            group.parent = Some(parent);
        }
        let mut changes: Vec<Change> = members
            .iter()
            .filter_map(|id| {
                let run = runs.iter().find(|r| r.items.contains(id))?;
                Some(Change::Regroup {
                    item: *id,
                    group: Membership::of(&runs, run.group.id),
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
                children: Vec::new(),
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
            self.start_group_rename(s, group.id);
        }
    }

    /// Sets the name a new group starts with (the translated "Group").
    pub fn set_group_name(&mut self, name: &str) {
        name.clone_into(&mut self.group_name);
    }

    /// Asks the window to open the inline rename of group `id`'s header. The request follows the
    /// group, not a row number, and ends with [`group_rename_ended`](Self::group_rename_ended).
    pub(super) fn start_group_rename(&mut self, s: Surface, id: snipdeck_core::GroupId) {
        let r = self.rename_of(s);
        r.group = Some(id);
        r.seq += 1;
    }

    fn rename_of(&mut self, s: Surface) -> &mut Rename {
        match s {
            Surface::Panel => &mut self.rename_panel,
            Surface::Main => &mut self.rename_main,
        }
    }

    /// The Panel's inline rename closed (accepted or abandoned): the request is spent.
    pub fn group_rename_ended(&mut self) {
        self.rename_panel.group = None;
    }

    /// The Main window's inline rename closed (accepted, abandoned or lost the focus).
    pub fn main_group_rename_ended(&mut self) {
        self.rename_main.group = None;
    }

    /// F2 on a header in the Main window: its name opens for editing in place.
    pub fn main_group_rename_start(&mut self, index: usize) {
        self.main_fresh();
        if let Some(run) = self.group_row(Surface::Main, index) {
            self.start_group_rename(Surface::Main, run.group.id);
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
                    self.start_group_rename(Surface::Panel, run.group.id);
                }
            }
            super::ItemMenuEntry::Ungroup => self.group_ungroup(at),
            super::ItemMenuEntry::ToggleGroup { .. } => self.group_collapse(at, None),
            super::ItemMenuEntry::GroupUp { enabled } => {
                if enabled {
                    self.group_move(at, -1);
                }
            }
            super::ItemMenuEntry::GroupDown { enabled } => {
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

    /// Every group of the shown list, folded ones too, in list order (a parent before the groups
    /// nested in it).
    fn group_runs(&self, s: Surface) -> Vec<GroupRun> {
        match s {
            Surface::Panel => self.model.groups().to_vec(),
            Surface::Main => self.main.all_groups().cloned().collect(),
        }
    }

    /// The groups nested directly in `id`.
    fn group_children(&self, s: Surface, id: GroupId) -> Vec<GroupId> {
        self.group_runs(s)
            .into_iter()
            .filter(|r| r.group.parent == Some(id))
            .map(|r| r.group.id)
            .collect()
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
                    None if r.is_item() => RowMark::Item(r.id),
                    None => RowMark::Other,
                })
                .collect(),
            Surface::Main => self
                .main
                .keys()
                .iter()
                .map(|k| match k {
                    MainKey::Group(g) => RowMark::Header(*g),
                    MainKey::Item(id) => RowMark::Item(*id),
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
                    && !list.sort.is_manual()
                {
                    let id = list.id;
                    self.offer_manual_order(id);
                } else {
                    // A search or a view that mixes lists has no order of its own to group in.
                    self.show_toast(neutral(ToastMessage::GroupNeedsList));
                }
            }
        }
    }

    /// The groups group `id` is in, outermost first, and `id` itself last.
    fn group_path(runs: &[GroupRun], id: GroupId) -> Vec<GroupId> {
        let tree = GroupTree::new(runs.iter().map(|r| &r.group));
        let mut path: Vec<GroupId> = tree.ancestors(id).collect();
        path.reverse();
        path.push(id);
        path
    }

    /// The groups item `id` is in, outermost first.
    fn item_path(runs: &[GroupRun], id: ItemId) -> Vec<GroupId> {
        runs.iter()
            .find(|r| r.items.contains(&id))
            .map_or_else(Vec::new, |r| Self::group_path(runs, r.group.id))
    }

    /// The blocks that sit side by side with group `id`, in order: the groups and the loose items
    /// directly in its parent (at the top level when it has none).
    fn sibling_blocks(&self, s: Surface, list: ListId, id: GroupId) -> Vec<Block> {
        let runs = self.group_runs(s);
        let path = Self::group_path(&runs, id);
        let level = path.len() - 1;
        let mut blocks: Vec<Block> = Vec::new();
        for item in self.group_order(s, list) {
            let at = Self::item_path(&runs, item);
            if at.len() < level || at[..level] != path[..level] {
                continue;
            }
            let block = at
                .get(level)
                .map_or(Block::Item(item), |g| Block::Group(*g));
            if blocks.last() != Some(&block) {
                blocks.push(block);
            }
        }
        blocks
    }

    /// The row to drop group `index` on to move it one place (`delta` of 1 or -1): the next
    /// block beside it (a group's header or a loose row) in that direction, within the group it
    /// is nested in. `None` at the ends.
    fn group_step_target(&self, s: Surface, index: usize, delta: isize) -> Option<usize> {
        let run = self.group_row(s, index)?;
        let list = self.group_list(s)?;
        let blocks = self.sibling_blocks(s, list, run.group.id);
        let at = blocks
            .iter()
            .position(|b| *b == Block::Group(run.group.id))?;
        let next = *blocks.get(at.checked_add_signed(delta)?)?;
        self.row_marks(s).iter().position(|m| match (m, next) {
            (RowMark::Header(h), Block::Group(g)) => *h == g,
            (RowMark::Item(id), Block::Item(want)) => *id == want,
            _ => false,
        })
    }

    /// The group under the Main window's cursor and where it can move, `None` on an item.
    pub(super) fn main_cursor_group(&self) -> Option<crate::commands::CursorGroup> {
        let at = self.main.cursor();
        self.group_row(Surface::Main, at)?;
        let can = |delta| self.group_step_target(Surface::Main, at, delta).is_some();
        Some(crate::commands::CursorGroup {
            up: can(-1),
            down: can(1),
        })
    }

    /// The palette's "Move group up / down": the same move as Alt+Up / Alt+Down on the header at `index`.
    pub(super) fn main_move_group(&mut self, index: usize, delta: isize) {
        self.main_fresh();
        self.group_move_on(Surface::Main, index, delta);
    }

    /// Whether the group whose header is row `index` can move `delta` places (1 or -1).
    pub(crate) fn can_move_group(&self, index: usize, delta: isize) -> bool {
        self.model.reorderable()
            && self
                .group_step_target(Surface::Panel, index, delta)
                .is_some()
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
        let Some(at) = self.header_row(s, group) else {
            return;
        };
        self.group_cursor(s, at);
        if let Some(list) = self.group_list(s) {
            let blocks = self.sibling_blocks(s, list, group);
            if let Some(place) = blocks.iter().position(|b| *b == Block::Group(group)) {
                self.announce_placed(place + 1, blocks.len());
            }
        }
    }

    /// A click on a header's chevron, or Left / Right on it: collapses or expands the group.
    /// `collapsed` of `None` toggles. A history step, like the rename: the fold is saved with the
    /// group, so Undo brings the old state back.
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
        let before = run.group;
        let mut group = before.clone();
        group.collapsed = want;
        if self.store.update_group(&group).is_err() {
            self.show_toast(failed());
            return;
        }
        self.record_step(
            Action::Group,
            vec![Change::Rename {
                before,
                after: group.clone(),
            }],
        );
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
        // Saving (Enter or the focus leaving) closes the editor, whatever came of the name.
        self.rename_of(s).group = None;
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

    /// The Main window's colour choice for header `index`: one of the nine accents, or none.
    /// The same colour again does nothing. A history step, like the rename.
    pub fn main_group_colour(&mut self, index: usize, colour: Option<snipdeck_core::ListAccent>) {
        self.main_fresh();
        let Some(run) = self.group_row(Surface::Main, index) else {
            return;
        };
        if colour == run.group.colour {
            return;
        }
        let before = run.group;
        let mut after = before.clone();
        after.colour = colour;
        if self.store.update_group(&after).is_err() {
            self.show_toast(failed());
            return;
        }
        self.record_step(Action::Group, vec![Change::Rename { before, after }]);
        self.group_refresh(Surface::Main);
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
        // The groups nested in it go up to its parent, as do its own members.
        let children = self.group_children(s, run.group.id);
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
                children,
            }],
        );
        self.group_refresh(s);
    }

    /// "Nest under…" on header `index` (or the palette's command): the chooser the window opens next
    /// lists where the group can go, the top level first when it is nested now.
    pub fn main_group_nest_start(&mut self, index: usize) {
        self.main_fresh();
        self.nest = self.group_row(Surface::Main, index).map(|run| {
            let runs = self.group_runs(Surface::Main);
            let tree = GroupTree::new(runs.iter().map(|r| &r.group));
            let mut options = Vec::new();
            if run.group.parent.is_some() {
                options.push(None);
            }
            options.extend(
                runs.iter()
                    .filter(|r| {
                        Some(r.group.id) != run.group.parent
                            && tree.can_nest(run.group.id, Some(r.group.id))
                    })
                    .map(|r| Some(r.group.id)),
            );
            NestChooser {
                group: run.group.id,
                options,
            }
        });
    }

    /// The "Nest under…" chooser's entries (empty while it is not open).
    pub(super) fn nest_choices(&self) -> Vec<super::main_ctl::NestChoice> {
        let Some(nest) = &self.nest else {
            return Vec::new();
        };
        let runs = self.group_runs(Surface::Main);
        nest.options
            .iter()
            .map(|option| {
                let run = option.and_then(|id| runs.iter().find(|r| r.group.id == id));
                super::main_ctl::NestChoice {
                    name: run.map(|r| r.group.name.as_str().to_owned()),
                    depth: run.map_or(0, |r| r.depth),
                    count: run.map_or(0, |r| r.run.len()),
                }
            })
            .collect()
    }

    /// The chooser picked entry `index`: the group, with everything in it, goes under that group
    /// (or to the top level). One undo step.
    pub fn main_group_nest_chosen(&mut self, index: usize) {
        self.main_fresh();
        let Some(nest) = self.nest.take() else {
            return;
        };
        if let Some(parent) = nest.options.get(index).copied() {
            self.group_nest(Surface::Main, nest.group, parent);
        }
    }

    /// The chooser closed without a pick.
    pub fn main_group_nest_cancel(&mut self) {
        self.nest = None;
    }

    /// Group `id` and all it holds go under `parent` (`None`: the top level), at the end of it.
    fn group_nest(&mut self, s: Surface, id: GroupId, parent: Option<GroupId>) {
        let runs = self.group_runs(s);
        let (Some(run), Some(list)) = (
            runs.iter().find(|r| r.group.id == id).cloned(),
            self.group_list(s),
        ) else {
            return;
        };
        let tree = GroupTree::new(runs.iter().map(|r| &r.group));
        if !tree.can_nest(id, parent) {
            self.show_toast(neutral(ToastMessage::GroupTooDeep));
            return;
        }
        let order = self.group_order(s, list);
        let Some(back) = place_before(&order, run.run[0], &run.run) else {
            return;
        };
        // The end of the group it goes into (else of the outermost group it leaves), outside
        // its own run; with nothing there, just above that group.
        let holder = match parent {
            Some(p) => runs.iter().find(|r| r.group.id == p),
            None => Self::group_path(&runs, id)
                .first()
                .and_then(|root| runs.iter().find(|r| r.group.id == *root)),
        };
        let last = holder.and_then(|h| h.run.iter().rev().find(|i| !run.run.contains(i)));
        let to = match (last, holder) {
            (Some(last), _) => Placement::After(*last),
            (None, Some(h)) if parent.is_none() => h
                .run
                .first()
                .and_then(|first| place_before(&order, *first, &run.run))
                .unwrap_or(back),
            _ => back,
        };
        if self.store.reparent_group(id, parent, to).is_err() {
            self.show_toast(failed());
            return;
        }
        self.record_step(
            Action::Group,
            vec![Change::Reparent {
                group: id,
                from: (run.group.parent, back),
                to: (parent, to),
            }],
        );
        if !self.group_refresh(s) {
            return;
        }
        if let Some(at) = self.header_row(s, id) {
            self.group_cursor(s, at);
        }
    }

    /// "Move group to list…": the chooser the window opens next holds the group's items, those of
    /// the groups nested in it included, so the list picked takes them all (the headers stay behind;
    /// a group is a run in one list, and is made again, nested as it was, in the list picked).
    pub fn main_group_move_start(&mut self, index: usize) {
        self.main_fresh();
        if let Some(run) = self.group_row(Surface::Main, index) {
            let targets = run.run.iter().map(|id| MainKey::Item(*id)).collect();
            self.main.set_chooser(targets);
            self.group_move = self.subtree(Surface::Main, run.group.id);
        }
    }

    /// Group `id` and the groups nested in it, parents first.
    fn subtree(&self, s: Surface, id: GroupId) -> Vec<GroupRun> {
        let runs = self.group_runs(s);
        let tree = GroupTree::new(runs.iter().map(|r| &r.group));
        let inside = tree.subtree(id);
        runs.into_iter()
            .filter(|r| inside.contains(&r.group.id))
            .collect()
    }

    /// Delete group: the header and its items go to the Trash, with the usual Undo toast (one
    /// step; restoring the items brings the header back).
    pub fn group_delete(&mut self, index: usize) {
        let Some(run) = self.group_row(Surface::Panel, index) else {
            return;
        };
        self.trash_with_undo(&run.run);
    }

    /// Delete group from the Main window: the same, with the same Undo toast. A folded group
    /// hides the rows it deletes, so the window asks first, with the count.
    pub fn main_group_delete(&mut self, index: usize) {
        self.main_fresh();
        let Some(run) = self.group_row(Surface::Main, index) else {
            return;
        };
        // Rows out of sight, or a whole branch of groups: ask first, with the count.
        let nested = !self.group_children(Surface::Main, run.group.id).is_empty();
        if (run.group.collapsed || nested) && !run.run.is_empty() {
            self.group_delete = Some(run.group.id);
            return;
        }
        self.trash_with_undo(&run.run);
        self.main_fresh();
    }

    /// The group the Main window asks about, with how many items go with it (its whole branch).
    pub(super) fn group_delete_view(&self) -> Option<(String, usize)> {
        let id = self.group_delete?;
        self.main
            .all_groups()
            .find(|r| r.group.id == id)
            .map(|r| (r.group.name.as_str().to_owned(), r.run.len()))
    }

    /// "Delete group" in the confirmation: the group and its items go to the Trash, with Undo.
    pub fn main_group_delete_confirm(&mut self) {
        self.main_fresh();
        let Some(id) = self.group_delete.take() else {
            return;
        };
        let items = self
            .main
            .all_groups()
            .find(|r| r.group.id == id)
            .map(|r| r.run.clone());
        if let Some(items) = items {
            self.trash_with_undo(&items);
            self.main_fresh();
        }
    }

    /// Cancel, Esc or a click on the scrim: the group stays.
    pub fn main_group_delete_cancel(&mut self) {
        self.group_delete = None;
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
        let runs = self.group_runs(s);
        let done = match (self.group_row(s, from), self.group_row(s, onto)) {
            (Some(run), target) => {
                if target_id.is_some_and(|id| run.run.contains(&id))
                    || target.as_ref().is_some_and(|t| t.group.id == run.group.id)
                {
                    return true;
                }
                let down = from < onto;
                // The block the drop lands beside, seen from the group's own level: a target
                // deeper in another group counts as that group, not as its member.
                let ancestors = {
                    let mut path = Self::group_path(&runs, run.group.id);
                    path.pop();
                    path
                };
                let (first, last) = match (&target, target_id) {
                    (Some(t), _) => (t.run.first().copied(), t.run.last().copied()),
                    (None, Some(id)) => {
                        let path = Self::item_path(&runs, id);
                        let shared = path
                            .iter()
                            .zip(&ancestors)
                            .take_while(|(a, b)| a == b)
                            .count();
                        let block = path
                            .get(shared)
                            .and_then(|g| runs.iter().find(|r| r.group.id == *g));
                        block.map_or((Some(id), Some(id)), |r| {
                            (r.run.first().copied(), r.run.last().copied())
                        })
                    }
                    (None, None) => (None, None),
                };
                let Some(anchor) = (if down { last } else { first }) else {
                    return true;
                };
                let to = if down {
                    Placement::After(anchor)
                } else {
                    // Above the anchor's block, outside the dragged run.
                    let Some(to) = place_before(&order, anchor, &run.run) else {
                        return true;
                    };
                    to
                };
                let Some(back) = place_before(&order, run.run[0], &run.run) else {
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

    /// Alt+Up / Alt+Down (and the item menu's Move up / down) on the item at Main row `from` when the
    /// open list shows groups: the item swaps places with the row beside it, taking that row's
    /// group (the rows above the first member are its header: moving past it leaves the group);
    /// a folded group is stepped over whole, an open group's header is joined at its top.
    /// One undo step, none when nothing changes. `false`: not a case for this path.
    pub(super) fn main_move_item_on(&mut self, from: usize, delta: isize) -> bool {
        if !self.main.shows_groups() || self.main.open_groups().is_empty() {
            return false;
        }
        let (Some(MainKey::Item(id)), Some(list)) = (
            self.main.keys().get(from).copied(),
            self.group_list(Surface::Main),
        ) else {
            return false;
        };
        let Some(beside) = from.checked_add_signed(delta) else {
            return true;
        };
        let order = self.group_order(Surface::Main, list);
        let old = self.group_of_item(Surface::Main, id);
        let (to, want) = match self.main.keys().get(beside).copied() {
            Some(MainKey::Item(n)) => {
                let want = self.group_of_item(Surface::Main, n).map(|r| r.group.id);
                let to = if delta < 0 {
                    place_before(&order, n, &[id])
                } else {
                    Some(Placement::After(n))
                };
                (to, want)
            }
            Some(MainKey::Group(g)) => {
                let Some(run) = self.main.group_at(beside) else {
                    return true;
                };
                if old.as_ref().is_some_and(|o| o.group.id == g) {
                    // Past its own header: out of the group (into the one it is nested in), above it.
                    (place_before(&order, id, &[id]), run.group.parent)
                } else if run.group.collapsed {
                    let to = if delta < 0 {
                        run.run
                            .first()
                            .and_then(|first| place_before(&order, *first, &[id]))
                    } else {
                        run.run.last().map(|last| Placement::After(*last))
                    };
                    (to, run.group.parent)
                } else {
                    // An open group's header below: the item joins it at its top.
                    let joined = self.join_dropped(Surface::Main, id, &run);
                    self.finish_item_move(joined.is_ok(), id);
                    return true;
                }
            }
            _ => return true,
        };
        let Some(to) = to else {
            return true;
        };
        self.place_item_in(Surface::Main, id, list, to, want, old.as_ref());
        true
    }

    /// Item `id` of `list` goes to `to`, and into the group `want` (none: out of any group), as one
    /// undo step (none when nothing changes), then the Main window follows it.
    fn place_item_in(
        &mut self,
        s: Surface,
        id: ItemId,
        list: ListId,
        to: Placement<ItemId>,
        want: Option<snipdeck_core::GroupId>,
        old: Option<&GroupRun>,
    ) {
        let before = self.store.groups_of(list).unwrap_or_default();
        let mut changes = vec![Change::Regroup {
            item: id,
            group: old.and_then(|r| Membership::of(&before, r.group.id)),
        }];
        let done = self.store.atomically(|b| {
            let spot = super::history_ctl::spot_of(b, id)?;
            super::history_ctl::move_recorded(b, id, list, to, &mut changes)?;
            let now = b.groups_of(list)?;
            let now = now
                .iter()
                .find(|r| r.items.contains(&id))
                .map(|r| r.group.clone());
            if now.as_ref().map(|g| g.id) != want {
                b.set_item_group(id, want)?;
                changes.push(Change::Regroup {
                    item: id,
                    group: want.and_then(|w| Membership::of(&b.groups_of(list).ok()?, w)),
                });
            }
            let after = b.groups_of(list)?;
            let group = after
                .iter()
                .find(|r| r.items.contains(&id))
                .map(|r| r.group.id);
            // The step is the net change: nothing moved, nothing to undo.
            Ok(spot != super::history_ctl::spot_of(b, id)? || group != old.map(|r| r.group.id))
        });
        if let Ok(true) = done {
            self.record_step(Action::Reorder, changes);
        }
        if s == Surface::Main {
            self.finish_item_move(done.is_ok(), id);
        } else if done.is_ok() {
            self.library_changed();
        } else {
            self.show_toast(failed());
        }
    }

    /// An item dragged from Main row `from` onto the top half of the header at row `onto`: it lands
    /// directly above that group, outside it (a drop on the lower half joins the group instead).
    /// `false`: not that case, so the caller treats it as an ordinary drop.
    pub(super) fn drop_above_group(&mut self, s: Surface, from: usize, onto: usize) -> bool {
        if self.drag_row(s, onto) != Some(DragRow::Header) {
            return false;
        }
        let Some(DragRow::Item { id, real: true }) = self.drag_row(s, from) else {
            return false;
        };
        let (Some(list), Some(run)) = (self.group_list(s), self.group_row(s, onto)) else {
            return false;
        };
        if !self.group_reorderable(s) {
            self.offer_manual_for_group(s);
            return true;
        }
        let order = self.group_order(s, list);
        let old = self.group_of_item(s, id);
        // A run is built from live members only (`groups_of`), so it always has a first item; were that
        // ever not so, the ordinary drop is the fallback rather than a drop that silently does nothing.
        let Some(to) = run
            .run
            .first()
            .and_then(|first| place_before(&order, *first, &[id]))
        else {
            return false;
        };
        // Above the group, outside it: in the group it is nested in, if any.
        self.place_item_in(s, id, list, to, run.group.parent, old.as_ref());
        true
    }

    /// After an item move in the Main window: refresh, follow the item and say where it is.
    fn finish_item_move(&mut self, ok: bool, id: ItemId) {
        if !ok {
            self.show_toast(failed());
            return;
        }
        self.library_changed();
        self.main_fresh();
        self.follow_moved_row(id);
        let at = self.main.cursor();
        self.announce_placed(at + 1, self.main.keys().len());
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
        let runs = self.group_runs(s);
        self.store
            .join_group(id, target.group.id, snipdeck_store::GroupEnd::First)?;
        self.record_step(
            Action::Reorder,
            vec![Change::Join {
                item: id,
                group: target.group.id,
                from,
                from_group: old.and_then(|r| Membership::of(&runs, r.group.id)),
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
