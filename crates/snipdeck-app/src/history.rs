//! The app-wide undo and redo history: what the Main window and the Panel share.
//!
//! One step is one user action (a bulk action over many items is still one step). A step holds
//! the changes it made, in order, each of them reversible: undoing a step applies the inverse
//! of every change in reverse order, redoing applies them again in the original order. This
//! module is pure data; the controller applies the changes to the library
//! (`controller::history_ctl`).
//!
//! Only item changes are recorded: add, paste, drop, edit, delete, move, reorder, colour, pin,
//! tag and duplicate. The text of a sensitive item is never copied here, so an edit that
//! changes it is not recorded.

use std::collections::VecDeque;

use snipdeck_core::{Item, ItemId, ListId, Placement};
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
}

/// Where an item sits: its list and its place in it.
pub type Spot = (ListId, Placement<ItemId>);

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
        }
    }
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

    /// Takes the step to undo off the stack (the caller applies it, then calls
    /// [`History::undone`]). `id` picks a particular step (the toast's); `None` the latest.
    pub fn take_undo(&mut self, id: Option<u64>) -> Option<Step> {
        match id {
            None => self.undo.pop_back(),
            Some(id) => {
                let at = self.undo.iter().position(|s| s.id == id)?;
                self.undo.remove(at)
            }
        }
    }

    /// Takes the step to redo off the stack.
    pub fn take_redo(&mut self) -> Option<Step> {
        self.redo.pop()
    }

    /// `step` was undone: Redo can bring it back.
    pub fn undone(&mut self, step: Step) {
        self.redo.push(step);
    }

    /// `step` was redone: Undo can take it back again (the redo stack stays).
    pub fn redone(&mut self, step: Step) {
        self.undo.push_back(step);
        while self.undo.len() > HISTORY_LIMIT {
            self.undo.pop_front();
        }
    }

    /// Forgets everything (the library changed under it: a restore from a backup, a lock).
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
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
        let step = h.take_undo(None).unwrap();
        assert_eq!(h.next_undo(), Some(Action::Add));
        h.undone(step);
        assert_eq!(h.next_redo(), Some(Action::Delete));
        let again = h.take_redo().unwrap();
        h.redone(again);
        assert_eq!(h.next_undo(), Some(Action::Delete));
        assert_eq!(h.next_redo(), None);
    }

    #[test]
    fn a_new_action_clears_redo() {
        let mut h = History::new();
        h.record(Action::Add, trash(1));
        let step = h.take_undo(None).unwrap();
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
        let step = h.take_undo(None).unwrap();
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
        let step = h.take_undo(Some(first)).unwrap();
        assert_eq!(step.action, Action::Delete);
        assert_eq!(h.next_undo(), Some(Action::Pin));
        assert!(h.take_undo(Some(first)).is_none());
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
