//! The app-wide undo and redo: recording what an action changed, and applying a step back or
//! forward. The Main window and the Panel share one history (Ctrl+Z, Ctrl+Y, Ctrl+Shift+Z and
//! the two header buttons); the toast's Undo button takes back the step it was shown for.

use snipdeck_core::{Item, ItemId, ListId, Placement, Timestamp};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::{Batch, ItemRepo, TrashRepo, Trashable};

use super::{Controller, Library, Toast, ToastKind, ToastMessage, failed};
use crate::{
    history::{Action, Change, Snap, Spot, Step},
    host::ClipboardText,
};

/// What the Undo and Redo buttons show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HistoryView {
    /// What Undo would take back; `None` disables the button.
    pub undo: Option<Action>,
    /// What Redo would bring back; `None` disables the button.
    pub redo: Option<Action>,
}

/// An item's fields as a history snapshot (the content is left alone).
#[must_use]
pub(super) fn snap(item: &Item) -> Snap {
    Snap {
        item: item.clone(),
        text: None,
    }
}

/// Where item `id` sits now: its list and the sibling before it.
///
/// # Errors
///
/// The item or its list could not be read.
pub(super) fn spot_of<R: ItemRepo>(repo: &R, id: ItemId) -> snipdeck_store::Result<Spot> {
    let list = repo.item(id)?.list_id;
    let items = repo.items(list)?;
    let at = items.iter().position(|i| i.id == id);
    let placement = match at.and_then(|n| n.checked_sub(1)).and_then(|n| items.get(n)) {
        Some(before) => Placement::After(before.id),
        None => Placement::First,
    };
    Ok((list, placement))
}

/// Moves `id` to `to` and records the change that goes with it.
///
/// # Errors
///
/// The move failed (nothing is recorded).
pub(super) fn move_recorded<R: ItemRepo>(
    repo: &mut R,
    id: ItemId,
    list: ListId,
    at: Placement<ItemId>,
    changes: &mut Vec<Change>,
) -> snipdeck_store::Result<()> {
    let from = spot_of(repo, id)?;
    repo.move_item(id, list, at)?;
    changes.push(Change::Move {
        id,
        from,
        to: (list, at),
    });
    Ok(())
}

/// Writes `after` over the stored item (with `item` as it was) and records the change, when
/// anything differs.
///
/// # Errors
///
/// The write failed.
pub(super) fn update_recorded<R: ItemRepo>(
    repo: &mut R,
    before: &Item,
    after: &Item,
    changes: &mut Vec<Change>,
) -> snipdeck_store::Result<()> {
    repo.update_item(after, None)?;
    changes.push(Change::Update {
        before: Box::new(snap(before)),
        after: Box::new(snap(after)),
    });
    Ok(())
}

/// Applies one change inside a transaction.
fn apply(
    b: &mut Batch<'_>,
    change: &Change,
    now: Timestamp,
    board_limit: Option<usize>,
) -> snipdeck_store::Result<()> {
    match change {
        Change::Trash(targets) => {
            for t in targets {
                b.trash(*t, now)?;
            }
        }
        Change::Restore(targets) => {
            for t in targets {
                b.restore_with(*t, board_limit)?;
            }
        }
        Change::Update { after, .. } => {
            let formats = match after.text.as_deref() {
                Some(text) => Some(snipdeck_core::Formats::new(vec![
                    snipdeck_core::Format::Text(snipdeck_store::BlobStore::put(
                        b,
                        text.as_bytes().to_vec(),
                    )?),
                ])?),
                None => None,
            };
            b.update_item(&after.item, formats.as_ref())?;
        }
        Change::Move { id, to, .. } => b.move_item(*id, to.0, to.1)?,
    }
    Ok(())
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// What the Undo and Redo buttons show now.
    #[must_use]
    pub fn history_view(&self) -> HistoryView {
        HistoryView {
            undo: self.history.next_undo(),
            redo: self.history.next_redo(),
        }
    }

    /// Records a finished action as one step. Returns the step's id.
    pub(super) fn record_step(&mut self, action: Action, changes: Vec<Change>) -> Option<u64> {
        self.history.record(action, changes)
    }

    /// Forgets the whole history (the library changed in a way steps cannot follow).
    pub(super) fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Ctrl+Z, or the Undo button: takes back the latest step.
    pub fn undo(&mut self) {
        self.undo_step(None);
    }

    /// Ctrl+Y or Ctrl+Shift+Z, or the Redo button: brings back the step Undo took back last.
    pub fn redo(&mut self) {
        self.main_fresh();
        let Some(step) = self.history.take_redo() else {
            return;
        };
        let now = self.clock.now();
        let limit = self.board_limit();
        let done = self.store.atomically(|b| {
            for change in &step.changes {
                apply(b, change, now, limit)?;
            }
            Ok(())
        });
        self.library_changed();
        let action = step.action;
        match done {
            Ok(()) => {
                self.history.redone(step);
                self.show_toast(Toast {
                    kind: ToastKind::Neutral,
                    message: ToastMessage::Redone { action },
                });
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    /// Takes back step `id` (the latest when `None`).
    pub(super) fn undo_step(&mut self, id: Option<u64>) {
        self.main_fresh();
        let Some(step) = self.history.take_undo(id) else {
            return;
        };
        let now = self.clock.now();
        let limit = self.board_limit();
        let changes = step.undo_changes();
        let done = self.store.atomically(|b| {
            for change in &changes {
                apply(b, change, now, limit)?;
            }
            Ok(())
        });
        match done {
            Ok(()) => {
                let message = restored_count(&step).map_or(
                    ToastMessage::Undone {
                        action: step.action,
                    },
                    |count| ToastMessage::Restored { count },
                );
                self.history.undone(step);
                self.show_toast(Toast {
                    kind: ToastKind::Success,
                    message,
                });
                self.library_changed();
            }
            Err(error) => {
                self.show_toast(super::main_ctl::restore_failed(&error));
                self.refresh_logged("a failed undo");
            }
        }
    }
}

/// How many Trash entries a delete step took, so its undo says "Restored 3 items".
fn restored_count(step: &Step) -> Option<usize> {
    if step.action != Action::Delete {
        return None;
    }
    Some(
        step.changes
            .iter()
            .map(|c| match c {
                Change::Trash(t) => t.len(),
                _ => 0,
            })
            .sum(),
    )
}

/// The trash targets of items.
pub(super) fn item_targets(items: &[ItemId]) -> Vec<Trashable> {
    items.iter().map(|id| Trashable::Item(*id)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_delete_step_counts_its_trash_entries() {
        let step = Step {
            id: 1,
            action: Action::Delete,
            changes: vec![Change::Trash(item_targets(&[
                ItemId::generate(),
                ItemId::generate(),
            ]))],
        };
        assert_eq!(restored_count(&step), Some(2));
        let other = Step {
            action: Action::Pin,
            ..step
        };
        assert_eq!(restored_count(&other), None);
    }
}
