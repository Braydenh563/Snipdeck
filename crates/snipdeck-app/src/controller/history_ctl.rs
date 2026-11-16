//! The app-wide undo and redo: recording what an action changed, and applying a step back or
//! forward. The Main window and the Panel share one history (Ctrl+Z, Ctrl+Y, Ctrl+Shift+Z and
//! the two header buttons); the toast's Undo button takes back the step it was shown for.

use snipdeck_core::{Group, Item, ItemId, ListId, Placement, Tag, Timestamp};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::{
    Batch, BoardRepo, GroupDelete, GroupEnd, GroupRepo, ItemRepo, ListRepo, TagRepo, TrashRepo,
    Trashable, TriggerRepo,
};

use super::{Controller, Library, Toast, ToastKind, ToastMessage, failed};
use crate::{
    history::{Action, Change, Record, Snap, Spot, Step},
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
        formats: None,
        trigger: None,
    }
}

/// Whether the bytes of every text, HTML, RTF and picture format in `formats` can still be read
/// (blob files go when nothing refers to them for long enough).
fn readable(b: &impl snipdeck_store::BlobStore, formats: &snipdeck_core::Formats) -> bool {
    use snipdeck_core::Format;
    formats.iter().all(|format| match format {
        Format::Text(c) | Format::Html(c) | Format::Rtf(c) | Format::Png(c) => b.read(c).is_ok(),
        Format::Files(files) => files.iter().all(|f| b.read(f.content()).is_ok()),
    })
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
fn apply(b: &mut Batch<'_>, change: &Change, now: Timestamp) -> snipdeck_store::Result<()> {
    match change {
        Change::Trash(targets) => {
            for t in targets {
                b.trash(*t, now)?;
            }
        }
        Change::Restore(targets) => {
            for t in targets {
                b.restore_reporting(*t)?;
            }
        }
        Change::Update { after, .. } => {
            // A tag deleted since stays gone: the item takes the rest of the snapshot.
            let mut item = after.item.clone();
            let live: std::collections::BTreeSet<_> = b.tags()?.into_iter().map(|t| t.id).collect();
            item.tags.retain(|t| live.contains(t));
            let formats = match (after.formats.as_ref(), after.text.as_deref()) {
                // The formats the edit dropped, while all their bytes are still stored.
                (Some(kept), _) if readable(b, kept) => Some(kept.clone()),
                (_, Some(text)) => Some(snipdeck_core::Formats::new(vec![
                    snipdeck_core::Format::Text(snipdeck_store::BlobStore::put(
                        b,
                        text.as_bytes().to_vec(),
                    )?),
                ])?),
                (_, None) => None,
            };
            b.update_item(&item, formats.as_ref())?;
            if let Some(trigger) = &after.trigger {
                // A code another item took in the meantime stays with that item.
                match b.set_trigger(after.item.id, trigger.as_ref()) {
                    Ok(_)
                    | Err(
                        snipdeck_store::Error::TriggerTaken(_)
                        | snipdeck_store::Error::SensitiveTrigger,
                    ) => {}
                    Err(e) => return Err(e),
                }
            }
        }
        Change::Move { id, to, .. } => b.move_item(*id, to.0, to.1)?,
        Change::Group { group, members } => b.create_group(group, members)?,
        Change::Ungroup { group, .. } => b.delete_group(group.id, GroupDelete::Keep)?,
        Change::Rename { after, .. } => b.update_group(after)?,
        Change::MoveRun { group, to, .. } => b.move_run(*group, *to)?,
        Change::Join { item, group, .. } => b.join_group(*item, *group, GroupEnd::First)?,
        Change::Leave {
            item,
            from,
            from_group,
            ..
        } => {
            b.move_item(*item, from.0, from.1)?;
            regroup(b, *item, from_group.as_ref())?;
        }
        Change::Regroup { item, group } => regroup(b, *item, group.as_ref())?,
        Change::Rewrite { after, .. } => match after {
            Record::Tag(tag) => b.update_tag(tag)?,
            Record::List(list) => b.update_list(list)?,
            Record::Board(board) => b.update_board(board)?,
        },
        Change::MoveList { id, to, .. } => b.move_list(*id, to.0, to.1)?,
        Change::MoveBoard { id, to, .. } => b.move_board(*id, *to)?,
        Change::DropTag { tag, .. } => match b.delete_tag(tag.id) {
            Ok(()) | Err(snipdeck_store::Error::NotFound { .. }) => {}
            Err(e) => return Err(e),
        },
        Change::MakeTag { tag, items } => make_tag(b, tag, items)?,
        Change::Barrier(_) => {}
    }
    Ok(())
}

/// Brings `tag` back on `items` (the ones still stored). A tag another one took the name of in the
/// meantime is not made twice: the items take that one.
fn make_tag(b: &mut Batch<'_>, tag: &Tag, items: &[ItemId]) -> snipdeck_store::Result<()> {
    let id = match b.insert_tag(tag) {
        Ok(()) | Err(snipdeck_store::Error::AlreadyExists { .. }) => tag.id,
        Err(snipdeck_store::Error::TagExists(_)) => b
            .tags()?
            .into_iter()
            .find(|t| {
                t.id == tag.id || t.name.as_str().to_lowercase() == tag.name.as_str().to_lowercase()
            })
            .map_or(tag.id, |t| t.id),
        Err(e) => return Err(e),
    };
    b.link_tag(id, items)
}

/// Whether a store error will repeat however often the step is tried again (the thing it names is
/// gone), as opposed to a busy or full disk that may clear.
fn permanent(error: &snipdeck_store::Error) -> bool {
    use snipdeck_store::Error as E;
    matches!(
        error,
        E::NotFound { .. }
            | E::AlreadyExists { .. }
            | E::TagExists(_)
            | E::Invalid(_)
            | E::Core(_)
            | E::BlobCorrupt { .. }
    )
}

/// Puts `item` in exactly `group` (made again, with the item alone, if it was emptied and
/// pruned since) or in none, leaving its position alone.
fn regroup<B: GroupRepo>(
    b: &mut B,
    item: ItemId,
    group: Option<&Group>,
) -> snipdeck_store::Result<()> {
    match group {
        None => b.set_item_group(item, None),
        Some(g) => b
            .set_item_group(item, Some(g.id))
            .or_else(|_| b.create_group(g, &[item])),
    }
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
        let id = self.history.record(action, changes);
        // A toast's Undo for an older step goes once this one touches the same things: taking
        // the older step back alone would take part of this one back with it.
        if let Some(step) = self.undo.as_ref().and_then(|u| u.step)
            && !self.history.can_undo(step)
        {
            self.undo = None;
        }
        id
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
        let Some(step) = self.history.peek_redo().cloned() else {
            return;
        };
        let now = self.clock.now();
        let done = self.store.atomically(|b| {
            for change in &step.changes {
                apply(b, change, now)?;
            }
            Ok(())
        });
        self.library_changed();
        let action = step.action;
        match done {
            Ok(()) => {
                self.history.redone();
                self.show_toast(Toast {
                    kind: ToastKind::Neutral,
                    message: ToastMessage::Redone { action },
                });
            }
            Err(e) if permanent(&e) => {
                // What the step needs is gone for good: it cannot come back, and says so.
                self.history.drop_redo();
                self.show_toast(Toast {
                    kind: ToastKind::Danger,
                    message: ToastMessage::HistoryStepLost,
                });
            }
            // Busy, full disk and the like: the step stays for another try.
            Err(_) => self.show_toast(failed()),
        }
    }

    /// Takes back step `id` (the latest when `None`). The step leaves the stack only when it was
    /// taken back, or can never be (what it needs is gone); a failure that may pass keeps it.
    pub(super) fn undo_step(&mut self, id: Option<u64>) {
        self.main_fresh();
        let Some(step) = self.history.peek_undo(id).cloned() else {
            return;
        };
        if step.is_barrier() {
            // The text of a sensitive item is not kept: the edit stays, and so does what is
            // before it (Undo says why it stopped).
            self.history.drop_undo(step.id);
            self.show_toast(Toast {
                kind: ToastKind::Neutral,
                message: ToastMessage::UndoStopped,
            });
            return;
        }
        let now = self.clock.now();
        let changes = step.undo_changes();
        let done = self.store.atomically(|b| {
            for change in &changes {
                apply(b, change, now)?;
            }
            Ok(())
        });
        match done {
            Ok(()) => {
                let made = step.made();
                let message = restored_count(&step).map_or(
                    ToastMessage::Undone {
                        action: step.action,
                    },
                    |count| ToastMessage::Restored { count },
                );
                self.history.undone(step.id);
                self.show_toast(Toast {
                    kind: ToastKind::Success,
                    message,
                });
                if !made.is_empty() {
                    self.undo = Some(super::Undo {
                        seq: self.toast_seq,
                        step: None,
                        adds: None,
                        list: None,
                        purge: made,
                    });
                }
                self.library_changed();
            }
            Err(e) => {
                if permanent(&e) {
                    self.history.drop_undo(step.id);
                    self.show_toast(Toast {
                        kind: ToastKind::Danger,
                        message: ToastMessage::HistoryStepLost,
                    });
                } else {
                    self.show_toast(super::failed());
                }
                self.refresh_logged("a failed undo");
            }
        }
    }

    /// Forgets the steps that need something a purge removed (an item, a list or a board), in
    /// both stacks; the rest of the history stays.
    pub(super) fn prune_history(&mut self) {
        let store = &self.store;
        self.history.retain(|step| {
            step.needs()
                .iter()
                .all(|t| store.is_stored(*t).unwrap_or(true))
        });
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
