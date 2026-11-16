//! The app-wide undo and redo history: what the Main window and the Panel share.
//!
//! One step is one user action (a bulk action over many items is still one step). A step holds
//! the changes it made, in order, each of them reversible: undoing a step applies the inverse
//! of every change in reverse order, redoing applies them again in the original order. This
//! module is pure data; the controller applies the changes to the library
//! (`controller::history_ctl`).
//!
//! Recorded: add, paste, drop, edit, delete, restore, move, reorder, colour, pin, tag and
//! duplicate of items; group changes and folds; the rename and delete of a tag; and the creation,
//! edit, move and delete of lists and boards. The text of a sensitive item is never copied here,
//! so an edit that changes it is recorded as a [`Change::Barrier`]: Undo takes the marker off
//! the list and says why nothing changed; the steps before it are the next Undo.
//!
//! The history lives in memory: it holds up to [`HISTORY_LIMIT`] steps, ends with the app (it is
//! not written to disk, because a step holds the text an edit replaced, and that text must go
//! when the item is deleted for good) and loses the steps a purge makes impossible
//! ([`History::retain`]). A lock keeps it: the steps hold no sensitive text.

use std::collections::VecDeque;

use snipdeck_core::{
    Board, BoardId, Group, GroupId, Item, ItemId, List, ListId, Placement, Tag, Trigger,
};
use snipdeck_store::Trashable;

/// How many steps are kept; the oldest goes first.
pub const HISTORY_LIMIT: usize = 100;

/// What a step did, for the words of the Undo and Redo buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// A new item made in the editor or the quick-add.
    Add,
    /// An item made by pasting from the clipboard.
    Paste,
    /// An item made by dropping something on a window.
    Drop,
    /// An item changed in the editor.
    Edit,
    /// Items moved to the Trash.
    Delete,
    /// Items moved to another list.
    Move,
    /// An item moved within its list.
    Reorder,
    /// A colour label set or cleared.
    Colour,
    /// Items pinned or unpinned.
    Pin,
    /// A tag ticked or unticked.
    Tag,
    /// Items duplicated.
    Duplicate,
    /// A group made, renamed, folded or ungrouped.
    Group,
    /// Items (or a list or board) brought back from the Trash.
    Restore,
    /// A list made, edited, sorted, moved or deleted.
    List,
    /// A board made, renamed, moved or deleted.
    Board,
}

impl Action {
    /// The number `ToastTexts.action-word` takes.
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Self::Add => 0,
            Self::Paste => 1,
            Self::Drop => 2,
            Self::Edit => 3,
            Self::Delete => 4,
            Self::Move => 5,
            Self::Reorder => 6,
            Self::Colour => 7,
            Self::Pin => 8,
            Self::Tag => 9,
            Self::Duplicate => 10,
            Self::Group => 11,
            Self::Restore => 12,
            Self::List => 13,
            Self::Board => 14,
        }
    }
}

/// An item as it was (or will be) after a change: its fields, and its text when the change
/// touched the content.
#[derive(Debug, Clone, PartialEq)]
pub struct Snap {
    /// The item's fields.
    pub item: Item,
    /// The text content to write back; `None` leaves the content alone.
    pub text: Option<String>,
    /// The item's formats as they were before a text edit dropped some (HTML, RTF), to write
    /// back instead of `text` when their blobs are still there; `None` writes `text` alone.
    pub formats: Option<snipdeck_core::Formats>,
    /// The trigger to write back (`Some(None)` clears it); `None` leaves it alone.
    pub trigger: Option<Option<Trigger>>,
}

/// Where an item sits: its list and its place in it.
pub type Spot = (ListId, Placement<ItemId>);

/// A stored record the history can write back whole: a tag, a list or a board.
#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    /// A tag (its name).
    Tag(Tag),
    /// A list (name, icon, accent, sort).
    List(List),
    /// A board (name).
    Board(Board),
}

impl Record {
    /// The list or board this is, for a purge that makes the step impossible.
    fn stored(&self) -> Option<Trashable> {
        match self {
            Self::Tag(_) => None,
            Self::List(l) => Some(Trashable::List(l.id)),
            Self::Board(b) => Some(Trashable::Board(b.id)),
        }
    }
}

/// One reversible change to the library.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// These are (or become) trashed. A creation is recorded as [`Change::Restore`]: undoing it
    /// trashes the new item, redoing brings it back.
    Trash(Vec<Trashable>),
    /// These are (or become) live again.
    Restore(Vec<Trashable>),
    /// An item's fields went from `before` to `after`.
    Update {
        /// The state undo returns to.
        before: Box<Snap>,
        /// The state redo returns to.
        after: Box<Snap>,
    },
    /// An item went from one place to another.
    Move {
        /// The item.
        id: ItemId,
        /// Where undo puts it.
        from: Spot,
        /// Where redo puts it.
        to: Spot,
    },
    /// A group exists, holding these items (they gather under the topmost). Undoing a
    /// creation is [`Change::Ungroup`]; undoing an ungroup is this.
    Group {
        /// The group, with the name and collapsed flag it has.
        group: Group,
        /// Its members, top to bottom.
        members: Vec<ItemId>,
    },
    /// The group is gone and its items stay in the list ungrouped.
    Ungroup {
        /// The group as it was.
        group: Group,
        /// Its members, top to bottom.
        members: Vec<ItemId>,
    },
    /// A whole group went from one place in its list to another (members stay together).
    MoveRun {
        /// The group.
        group: GroupId,
        /// Where undo puts it.
        from: Placement<ItemId>,
        /// Where redo puts it.
        to: Placement<ItemId>,
    },
    /// An item joined a group at its top (dragged onto the header). Undoing it is
    /// [`Change::Leave`].
    Join {
        /// The item.
        item: ItemId,
        /// The group it joined.
        group: GroupId,
        /// Where it was before.
        from: Spot,
        /// The group it was in before, if any: Undo restores exactly this membership.
        from_group: Option<Group>,
    },
    /// The inverse of [`Change::Join`]: the item goes back to where it was.
    Leave {
        /// The item.
        item: ItemId,
        /// The group it leaves.
        group: GroupId,
        /// Where it goes back to.
        from: Spot,
        /// The group it goes back into, if any (`None`: it ends up in no group).
        from_group: Option<Group>,
    },
    /// An item's group is set to exactly this one (`None`: no group), wherever it sits. Recorded
    /// before moves that would otherwise re-derive the membership from the row above; it
    /// is its own inverse, because the state it restores is the state that preceded the step.
    Regroup {
        /// The item.
        item: ItemId,
        /// The group it belongs to (made again if it was emptied since), or none.
        group: Option<Group>,
    },
    /// A group was renamed or folded.
    Rename {
        /// The group before.
        before: Group,
        /// The group after.
        after: Group,
    },
    /// A tag, list or board went from one state to another (a rename, an icon, a sort).
    Rewrite {
        /// The state undo returns to.
        before: Record,
        /// The state redo returns to.
        after: Record,
    },
    /// A list went from one place among the lists to another (maybe on another board).
    MoveList {
        /// The list.
        id: ListId,
        /// Where undo puts it.
        from: (BoardId, Placement<ListId>),
        /// Where redo puts it.
        to: (BoardId, Placement<ListId>),
    },
    /// A board went from one place among the boards to another.
    MoveBoard {
        /// The board.
        id: BoardId,
        /// Where undo puts it.
        from: Placement<BoardId>,
        /// Where redo puts it.
        to: Placement<BoardId>,
    },
    /// The tag is deleted; `items` are the ones that carried it (the Trash's too). Undoing a
    /// deletion is [`Change::MakeTag`].
    DropTag {
        /// The tag as it was.
        tag: Tag,
        /// The items that carried it.
        items: Vec<ItemId>,
    },
    /// The tag exists and is on `items`.
    MakeTag {
        /// The tag.
        tag: Tag,
        /// The items that carry it.
        items: Vec<ItemId>,
    },
    /// An edit of this sensitive item that cannot be undone: its text is not kept. Undo takes the
    /// marker off the list and says so; it changes nothing else.
    Barrier(ItemId),
}

impl Change {
    /// The change that undoes this one.
    #[must_use]
    pub fn inverse(&self) -> Self {
        match self {
            Self::Trash(t) => Self::Restore(t.clone()),
            Self::Restore(t) => Self::Trash(t.clone()),
            Self::Update { before, after } => Self::Update {
                before: after.clone(),
                after: before.clone(),
            },
            Self::Move { id, from, to } => Self::Move {
                id: *id,
                from: *to,
                to: *from,
            },
            Self::Group { group, members } => Self::Ungroup {
                group: group.clone(),
                members: members.clone(),
            },
            Self::Ungroup { group, members } => Self::Group {
                group: group.clone(),
                members: members.clone(),
            },
            Self::MoveRun { group, from, to } => Self::MoveRun {
                group: *group,
                from: *to,
                to: *from,
            },
            Self::Join {
                item,
                group,
                from,
                from_group,
            } => Self::Leave {
                item: *item,
                group: *group,
                from: *from,
                from_group: from_group.clone(),
            },
            Self::Leave {
                item,
                group,
                from,
                from_group,
            } => Self::Join {
                item: *item,
                group: *group,
                from: *from,
                from_group: from_group.clone(),
            },
            Self::Regroup { item, group } => Self::Regroup {
                item: *item,
                group: group.clone(),
            },
            Self::Rename { before, after } => Self::Rename {
                before: after.clone(),
                after: before.clone(),
            },
            Self::Rewrite { before, after } => Self::Rewrite {
                before: after.clone(),
                after: before.clone(),
            },
            Self::MoveList { id, from, to } => Self::MoveList {
                id: *id,
                from: *to,
                to: *from,
            },
            Self::MoveBoard { id, from, to } => Self::MoveBoard {
                id: *id,
                from: *to,
                to: *from,
            },
            Self::DropTag { tag, items } => Self::MakeTag {
                tag: tag.clone(),
                items: items.clone(),
            },
            Self::MakeTag { tag, items } => Self::DropTag {
                tag: tag.clone(),
                items: items.clone(),
            },
            Self::Barrier(id) => Self::Barrier(*id),
        }
    }

    /// The items, lists and boards this change names (and the siblings it places next to): what
    /// a later step must not touch for this one to be taken back alone, and what a purge must
    /// not have removed for it to be applied.
    fn names(&self, out: &mut Names) {
        let spot = |out: &mut Names, (list, at): &Spot| {
            out.stored.push(Trashable::List(*list));
            if let Placement::After(id) = at {
                out.items.push(*id);
            }
        };
        let after = |out: &mut Names, at: &Placement<ItemId>| {
            if let Placement::After(id) = at {
                out.items.push(*id);
            }
        };
        match self {
            Self::Trash(t) | Self::Restore(t) => {
                for target in t {
                    out.stored.push(*target);
                    if let Trashable::Item(id) = target {
                        out.items.push(*id);
                    }
                }
            }
            Self::Update { before, after } => {
                out.items.extend([before.item.id, after.item.id]);
            }
            Self::Move { id, from, to } => {
                out.items.push(*id);
                spot(out, from);
                spot(out, to);
            }
            Self::Group { members, .. } | Self::Ungroup { members, .. } => {
                out.items.extend(members);
            }
            Self::MoveRun { from, to, .. } => {
                after(out, from);
                after(out, to);
            }
            Self::Join { item, from, .. } | Self::Leave { item, from, .. } => {
                out.items.push(*item);
                spot(out, from);
            }
            Self::Regroup { item, .. } | Self::Barrier(item) => out.items.push(*item),
            Self::Rename { .. } | Self::DropTag { .. } | Self::MakeTag { .. } => {}
            Self::Rewrite { before, after } => {
                out.stored.extend(before.stored());
                out.stored.extend(after.stored());
            }
            Self::MoveList { id, .. } => out.stored.push(Trashable::List(*id)),
            Self::MoveBoard { id, .. } => out.stored.push(Trashable::Board(*id)),
        }
        if let Self::DropTag { items, .. } | Self::MakeTag { items, .. } = self {
            // Tag links are applied to the items still stored: a purged one is not needed.
            out.carriers.extend(items);
        }
    }
}

/// What a step names, for [`Step::names`].
#[derive(Debug, Default)]
struct Names {
    items: Vec<ItemId>,
    carriers: Vec<ItemId>,
    stored: Vec<Trashable>,
}

/// One user action: what it was and the changes it made, in order.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// Identifies the step (the toast's Undo names it).
    pub id: u64,
    /// What the user did.
    pub action: Action,
    /// The changes, as they were made.
    pub changes: Vec<Change>,
}

impl Step {
    /// The changes that take this step back, in the order to apply them.
    #[must_use]
    pub fn undo_changes(&self) -> Vec<Change> {
        self.changes.iter().rev().map(Change::inverse).collect()
    }

    /// An edit that cannot be taken back (see [`Change::Barrier`]).
    #[must_use]
    pub fn is_barrier(&self) -> bool {
        !self.changes.is_empty() && self.changes.iter().all(|c| matches!(c, Change::Barrier(_)))
    }

    /// A creation: every change brings items to life, so taking it back only trashes them. These
    /// are the items (the toast then offers to delete them for good).
    #[must_use]
    pub fn made(&self) -> Vec<Trashable> {
        if !matches!(
            self.action,
            Action::Add | Action::Paste | Action::Drop | Action::Duplicate
        ) {
            return Vec::new();
        }
        let mut made = Vec::new();
        for change in &self.changes {
            match change {
                Change::Restore(t) => made.extend(t.iter().copied()),
                _ => return Vec::new(),
            }
        }
        made
    }

    fn names(&self) -> Names {
        let mut names = Names::default();
        for change in &self.changes {
            change.names(&mut names);
        }
        names
    }

    /// Whether `other` names an item this step names (or sits beside): then the two cannot be
    /// taken back in either order without one undoing part of the other.
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        let (mine, theirs) = (self.names(), other.names());
        let all = |n: &Names| {
            n.items
                .iter()
                .chain(&n.carriers)
                .copied()
                .collect::<Vec<_>>()
        };
        let theirs_all = all(&theirs);
        all(&mine).iter().any(|id| theirs_all.contains(id))
            || mine.stored.iter().any(|t| {
                matches!(t, Trashable::List(_) | Trashable::Board(_)) && theirs.stored.contains(t)
            })
    }

    /// Every item, list and board the step needs to still be in the library.
    #[must_use]
    pub fn needs(&self) -> Vec<Trashable> {
        let names = self.names();
        names
            .stored
            .into_iter()
            .chain(names.items.into_iter().map(Trashable::Item))
            .collect()
    }
}

/// The undo and redo stacks.
#[derive(Debug, Default)]
pub struct History {
    undo: VecDeque<Step>,
    redo: Vec<Step>,
    next_id: u64,
}

impl History {
    /// An empty history.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a new action: it becomes the next undo and the redo stack is cleared. A step
    /// with no changes is not recorded. Returns the step's id.
    pub fn record(&mut self, action: Action, changes: Vec<Change>) -> Option<u64> {
        if changes.is_empty() {
            return None;
        }
        self.next_id += 1;
        let id = self.next_id;
        self.redo.clear();
        self.undo.push_back(Step {
            id,
            action,
            changes,
        });
        while self.undo.len() > HISTORY_LIMIT {
            self.undo.pop_front();
        }
        Some(id)
    }

    /// The action Undo would take back.
    #[must_use]
    pub fn next_undo(&self) -> Option<Action> {
        self.undo.back().map(|s| s.action)
    }

    /// The id of the step Undo would take back.
    #[must_use]
    pub fn next_undo_id(&self) -> Option<u64> {
        self.undo.back().map(|s| s.id)
    }

    /// The action Redo would bring back.
    #[must_use]
    pub fn next_redo(&self) -> Option<Action> {
        self.redo.last().map(|s| s.action)
    }

    /// The step Undo would take back: the latest, or step `id` (the toast's). The step stays
    /// on the stack until [`History::undone`] or [`History::drop_undo`] says what became of it,
    /// so a failed undo loses nothing. A step that a later step overlaps (see
    /// [`Step::overlaps`]) is not offered by id: undoing it alone would undo part of the later
    /// one too.
    #[must_use]
    pub fn peek_undo(&self, id: Option<u64>) -> Option<&Step> {
        match id {
            None => self.undo.back(),
            Some(id) => {
                let at = self.undo.iter().position(|s| s.id == id)?;
                let step = self.undo.get(at)?;
                self.undo
                    .iter()
                    .skip(at + 1)
                    .all(|later| !later.overlaps(step))
                    .then_some(step)
            }
        }
    }

    /// Whether step `id` can still be undone alone: it is on the stack and no later step
    /// overlaps it.
    #[must_use]
    pub fn can_undo(&self, id: u64) -> bool {
        self.peek_undo(Some(id)).is_some()
    }

    /// The step Redo would bring back; it stays until [`History::redone`] or
    /// [`History::drop_redo`].
    #[must_use]
    pub fn peek_redo(&self) -> Option<&Step> {
        self.redo.last()
    }

    /// Step `id` was undone: it moves to the redo stack (a step that cannot be redone, an edit
    /// of a sensitive item, is forgotten).
    pub fn undone(&mut self, id: u64) {
        if let Some(at) = self.undo.iter().position(|s| s.id == id)
            && let Some(step) = self.undo.remove(at)
            && !step.is_barrier()
        {
            self.redo.push(step);
        }
    }

    /// Step `id` can never be undone: forget it.
    pub fn drop_undo(&mut self, id: u64) {
        self.undo.retain(|s| s.id != id);
    }

    /// The step on top of the redo stack was redone: Undo can take it back again.
    pub fn redone(&mut self) {
        if let Some(step) = self.redo.pop() {
            self.undo.push_back(step);
            while self.undo.len() > HISTORY_LIMIT {
                self.undo.pop_front();
            }
        }
    }

    /// The step on top of the redo stack can never be redone: forget it.
    pub fn drop_redo(&mut self) {
        self.redo.pop();
    }

    /// Keeps the steps `keep` accepts, in both stacks (a purge forgets the ones it broke).
    pub fn retain(&mut self, mut keep: impl FnMut(&Step) -> bool) {
        self.undo.retain(&mut keep);
        self.redo.retain(keep);
    }

    /// Forgets everything (the library changed under it: a restore from a backup).
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// How many steps Redo can bring back.
    #[must_use]
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// How many steps Undo can take back.
    #[must_use]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trash(n: usize) -> Vec<Change> {
        (0..n)
            .map(|_| Change::Trash(vec![Trashable::Item(ItemId::generate())]))
            .collect()
    }

    #[test]
    fn the_latest_step_is_the_first_undone_and_redo_returns_it() {
        let mut h = History::new();
        h.record(Action::Add, trash(1));
        h.record(Action::Delete, trash(1));
        assert_eq!(h.next_undo(), Some(Action::Delete));
        let step = h.peek_undo(None).unwrap().id;
        assert_eq!(h.next_undo(), Some(Action::Delete), "peeking takes nothing");
        h.undone(step);
        assert_eq!(h.next_undo(), Some(Action::Add));
        assert_eq!(h.next_redo(), Some(Action::Delete));
        assert!(h.peek_redo().is_some());
        h.redone();
        assert_eq!(h.next_undo(), Some(Action::Delete));
        assert_eq!(h.next_redo(), None);
    }

    #[test]
    fn a_new_action_clears_redo() {
        let mut h = History::new();
        h.record(Action::Add, trash(1));
        let step = h.peek_undo(None).unwrap().id;
        h.undone(step);
        assert!(h.next_redo().is_some());
        h.record(Action::Edit, trash(1));
        assert_eq!(h.next_redo(), None);
    }

    #[test]
    fn a_step_without_changes_is_not_recorded() {
        let mut h = History::new();
        assert_eq!(h.record(Action::Move, Vec::new()), None);
        assert_eq!(h.undo_len(), 0);
    }

    #[test]
    fn a_bulk_step_is_one_step_and_undoes_in_reverse_order() {
        let mut h = History::new();
        let (a, b) = (ItemId::generate(), ItemId::generate());
        h.record(
            Action::Delete,
            vec![
                Change::Trash(vec![Trashable::Item(a)]),
                Change::Trash(vec![Trashable::Item(b)]),
            ],
        );
        assert_eq!(h.undo_len(), 1);
        let step = h.peek_undo(None).unwrap();
        assert_eq!(
            step.undo_changes(),
            vec![
                Change::Restore(vec![Trashable::Item(b)]),
                Change::Restore(vec![Trashable::Item(a)]),
            ]
        );
    }

    #[test]
    fn the_history_keeps_the_newest_steps_up_to_its_limit() {
        let mut h = History::new();
        for _ in 0..HISTORY_LIMIT + 10 {
            h.record(Action::Pin, trash(1));
        }
        assert_eq!(h.undo_len(), HISTORY_LIMIT);
        const { assert!(HISTORY_LIMIT >= 50) };
    }

    #[test]
    fn a_step_can_be_taken_by_id_from_the_middle() {
        let mut h = History::new();
        let first = h.record(Action::Delete, trash(1)).unwrap();
        h.record(Action::Pin, trash(1));
        // The two steps touch different items, so the older one can be taken by its id.
        let step = h.peek_undo(Some(first)).unwrap();
        assert_eq!(step.action, Action::Delete);
        h.undone(first);
        assert_eq!(h.next_undo(), Some(Action::Pin));
        assert!(h.peek_undo(Some(first)).is_none());
    }

    #[test]
    fn a_step_a_later_step_overlaps_cannot_be_taken_alone() {
        let mut h = History::new();
        let item = ItemId::generate();
        let first = h
            .record(
                Action::Delete,
                vec![Change::Trash(vec![Trashable::Item(item)])],
            )
            .unwrap();
        let second = h
            .record(
                Action::Pin,
                vec![Change::Trash(vec![Trashable::Item(item)])],
            )
            .unwrap();
        assert!(!h.can_undo(first), "the later step touches the same item");
        assert!(h.can_undo(second));
        h.drop_undo(second);
        assert!(h.can_undo(first), "and it can again once that step is gone");
    }

    #[test]
    fn a_barrier_leaves_the_undo_list_without_reaching_redo() {
        let mut h = History::new();
        h.record(Action::Pin, trash(1));
        let barrier = h
            .record(Action::Edit, vec![Change::Barrier(ItemId::generate())])
            .unwrap();
        assert!(h.peek_undo(None).unwrap().is_barrier());
        h.undone(barrier);
        assert_eq!(h.next_redo(), None, "it cannot be redone");
        assert_eq!(h.next_undo(), Some(Action::Pin), "the older step is next");
    }

    #[test]
    fn inverting_twice_gives_the_change_back() {
        let id = ItemId::generate();
        let list = ListId::generate();
        let change = Change::Move {
            id,
            from: (list, Placement::First),
            to: (list, Placement::Last),
        };
        assert_eq!(change.inverse().inverse(), change);
    }
}
