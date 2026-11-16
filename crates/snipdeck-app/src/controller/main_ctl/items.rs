//! Item actions and the Trash with its Undo.

use crate::history::{Action, Change};
use snipdeck_core::template::transform::Transform;
use snipdeck_core::{Item, ItemId, Name, Placement, Tag};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::{ItemRepo, TrashRepo, Trashable};

use super::super::history_ctl::{item_targets, move_recorded, update_recorded};
use super::super::{
    Controller, Library, Toast, ToastKind, ToastMessage, UseResult, change_failed, failed, success,
};
use super::{MainAction, TagRenameError, neutral_toast};
use crate::{
    host::ClipboardText,
    main_window::{Key, NameError, Section},
};

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    // ---- Item actions --------------------------------------------------------------------------

    /// Runs `action` on the selection (when more than one row is selected) or on row `index`.
    pub fn main_action(&mut self, action: MainAction, index: usize) -> Option<UseResult> {
        self.main_fresh();
        let targets = self.main.targets(index);
        if targets.is_empty() {
            return None;
        }
        let items: Vec<ItemId> = targets
            .iter()
            .filter_map(|k| match k {
                Key::Item(id) => Some(*id),
                _ => None,
            })
            .collect();
        match action {
            MainAction::Use => {
                let id = items.first().copied()?;
                return Some(self.use_item(id, snipdeck_core::UseMode::Paste));
            }
            MainAction::UsePlain => {
                let id = items.first().copied()?;
                return Some(self.use_item(id, snipdeck_core::UseMode::PastePlain));
            }
            // Every selected row, joined as `copy_items` does for Ctrl+C and the Panel's bulk bar.
            MainAction::Copy => return self.copy_items(&items),
            MainAction::Edit => {
                if let Some(id) = items.first() {
                    self.open_item_editor(*id);
                }
            }
            MainAction::Pin => self.pin(&items),
            MainAction::Duplicate => self.duplicate(&items),
            MainAction::Move | MainAction::Tag | MainAction::PasteAs => {
                self.main.set_chooser(targets);
            }
            MainAction::Delete => self.trash_items(&items),
            MainAction::Restore => self.restore(&targets),
            MainAction::DeleteForever => self.delete_forever(&targets),
        }
        None
    }

    /// The *Paste as…* chooser picked choice `index` (in the order of [`Transform::ALL`]): the
    /// item it was opened for is pasted through it, as plain text with the caret at the end.
    pub fn main_paste_as(&mut self, index: usize) -> Option<UseResult> {
        self.main_fresh();
        let id = self.main.chooser().iter().find_map(|k| match k {
            Key::Item(id) => Some(*id),
            _ => None,
        });
        self.main.set_chooser(Vec::new());
        let transform = Transform::ALL.get(index).copied()?;
        Some(self.use_item_as(id?, transform))
    }

    /// Ctrl+C on the Main list: the selection (when more than one row is selected) or row
    /// `index`, copied by `copy_items`.
    pub fn main_copy_selection(&mut self, index: usize) -> Option<UseResult> {
        self.main_fresh();
        let ids: Vec<ItemId> = self
            .main
            .targets(index)
            .iter()
            .filter_map(|k| match k {
                Key::Item(id) => Some(*id),
                _ => None,
            })
            .collect();
        if ids.is_empty() {
            return None;
        }
        self.copy_items(&ids)
    }

    pub(super) fn pin(&mut self, items: &[ItemId]) {
        let now = self.clock.now();
        let pin = !items
            .iter()
            .all(|id| self.main.item(*id).is_some_and(|i| i.pinned));
        let mut changes = Vec::new();
        let done = self.store.atomically(|b| {
            changes.clear();
            for id in items {
                let before = b.item(*id)?;
                let mut item = before.clone();
                item.pinned = pin;
                item.updated_at = now;
                update_recorded(b, &before, &item, &mut changes)?;
            }
            Ok(())
        });
        if done.is_ok() {
            self.record_step(Action::Pin, changes);
        }
        self.finish(&done, Some(success(ToastMessage::Pinned(pin))));
    }

    /// Each item again, directly after itself: same title, content, tags and flags; pinned off and
    /// use history cleared, like a new item.
    /// A sensitive item behind a locked library is not copied: the unlock window is asked for and
    /// the toast says why nothing was done, like a copy of the whole library.
    pub(in super::super) fn duplicate(&mut self, items: &[ItemId]) {
        let now = self.clock.now();
        let locked = self.library_locked();
        let mut made = Vec::new();
        let done = self.store.atomically(|b| {
            made.clear();
            for id in items {
                let original = b.item(*id)?;
                if locked && original.sensitive {
                    return Err(snipdeck_store::Error::EncryptionLocked);
                }
                let formats = b.formats(*id)?;
                let mut copy =
                    Item::new(original.list_id, original.title.clone(), original.kind, now);
                copy.preview = original.preview;
                copy.tags = original.tags;
                copy.sensitive = original.sensitive;
                copy.always_plain = original.always_plain;
                copy.label = original.label;
                copy.source_app = original.source_app;
                b.insert_item(&copy, &formats, Placement::After(*id))?;
                made.push(copy.id);
            }
            Ok(())
        });
        if done.is_ok() && !made.is_empty() {
            self.record_step(
                Action::Duplicate,
                vec![Change::Restore(item_targets(&made))],
            );
        }
        if matches!(done, Err(snipdeck_store::Error::EncryptionLocked)) {
            self.request_unlock();
            self.show_toast(Toast {
                kind: ToastKind::Neutral,
                message: ToastMessage::UnlockFirst,
            });
            return;
        }
        self.finish(
            &done,
            (!made.is_empty()).then(|| success(ToastMessage::Duplicated)),
        );
    }

    pub(in super::super) fn trash_items(&mut self, items: &[ItemId]) {
        if items.is_empty() {
            return;
        }
        let now = self.clock.now();
        let done = self.store.atomically(|b| {
            for id in items {
                b.trash(Trashable::Item(*id), now)?;
            }
            Ok(())
        });
        if done.is_ok() {
            let step = self.record_step(Action::Delete, vec![Change::Trash(item_targets(items))]);
            self.show_undo_toast(
                Toast {
                    kind: ToastKind::Neutral,
                    message: ToastMessage::ItemsTrashed { count: items.len() },
                },
                step,
            );
            self.library_changed();
        } else {
            self.show_toast(failed());
        }
    }

    /// Brings `targets` back from the Trash.
    pub(super) fn restore(&mut self, targets: &[Key]) {
        let done = self.store.atomically(|b| {
            for entry in targets.iter().filter_map(|k| k.trashable()) {
                b.restore_reporting(entry)?;
            }
            Ok(())
        });
        if done.is_ok() {
            self.show_toast(Toast {
                kind: ToastKind::Success,
                message: ToastMessage::Restored {
                    count: targets.len(),
                },
            });
            self.library_changed();
        } else {
            self.show_toast(failed());
            self.refresh_logged("a failed restore");
        }
    }

    pub(super) fn delete_forever(&mut self, targets: &[Key]) {
        // Steps may name what goes for good.
        self.clear_history();
        let done = self.store.atomically(|b| {
            for entry in targets.iter().filter_map(|k| k.trashable()) {
                b.purge_entry(entry)?;
            }
            Ok(())
        });
        if done.is_ok() {
            self.show_toast(neutral_toast(ToastMessage::DeletedForever {
                count: targets.len(),
            }));
            self.library_changed();
        } else {
            self.show_toast(failed());
            self.refresh_logged("a failed delete");
        }
    }

    /// Ends an action: a failure is a toast; a success shows `confirmation` (if any) and then
    /// reads the library again, so a refresh that fails has the last word.
    pub(super) fn finish(
        &mut self,
        done: &snipdeck_store::Result<()>,
        confirmation: Option<Toast>,
    ) {
        match done {
            Ok(()) => {
                if let Some(toast) = confirmation {
                    self.show_toast(toast);
                }
                self.library_changed();
            }
            Err(e) => self.show_toast(change_failed(e)),
        }
    }

    /// The *Move to list* chooser picked list `index`: the rows it was opened for go to its end.
    pub fn main_move_to(&mut self, index: usize) {
        self.main_fresh();
        let Some(target) = self.main.move_targets().into_iter().nth(index) else {
            return;
        };
        let (list, name) = (target.list, target.label);
        let items: Vec<ItemId> = self
            .main
            .chooser()
            .iter()
            .filter_map(|k| match k {
                Key::Item(id) => Some(*id),
                _ => None,
            })
            .collect();
        if items.is_empty() {
            return;
        }
        let mut changes = Vec::new();
        let done = self.store.atomically(|b| {
            changes.clear();
            for id in &items {
                move_recorded(b, *id, list, Placement::Last, &mut changes)?;
            }
            Ok(())
        });
        self.main.set_chooser(Vec::new());
        match done {
            Ok(()) => {
                self.record_step(Action::Move, changes);
                self.show_toast(neutral_toast(ToastMessage::ItemsMoved {
                    count: items.len(),
                    list: name,
                }));
                self.library_changed();
            }
            Err(e) => self.show_toast(change_failed(&e)),
        }
    }

    /// The *Tags* chooser ticked (`on`) or unticked tag `index` for the rows it was opened for.
    pub fn main_toggle_tag(&mut self, index: usize, on: bool) {
        self.main_fresh();
        let Some(tag) = self.main.tags().get(index).map(|t| t.id) else {
            return;
        };
        let items: Vec<ItemId> = self
            .main
            .chooser()
            .iter()
            .filter_map(|k| match k {
                Key::Item(id) => Some(*id),
                _ => None,
            })
            .collect();
        self.tag_items(&items, tag, on, false);
    }

    /// A drag of row `row` (with the rest of the selection, when it is one of several) ended on
    /// the sidebar's Trash: the items go to the Trash, with its Undo toast. Dropping in the Trash
    /// itself does nothing.
    pub fn main_drop_on_trash(&mut self, row: usize) {
        self.main_fresh();
        if self.main.section() == Section::Trash {
            return;
        }
        let items = self.main.dragged_items(row);
        self.trash_items(&items);
    }

    /// A drag of row `row` (with the rest of the selection) ended on sidebar tag `tag` (an index
    /// of the tags): the items get the tag, and a toast says how many were reached.
    pub fn main_drop_on_tag(&mut self, row: usize, tag: usize) {
        self.main_fresh();
        if self.main.section() == Section::Trash {
            return;
        }
        let Some(id) = self.main.tags().get(tag).map(|t| t.id) else {
            return;
        };
        let items = self.main.dragged_items(row);
        self.tag_items(&items, id, true, true);
    }

    /// Adds (`on`) or removes `tag` on `items` as one Undo step. `always_say` shows the toast for a
    /// single item too (a drop has no tick to show it).
    fn tag_items(
        &mut self,
        items: &[ItemId],
        tag: snipdeck_core::TagId,
        on: bool,
        always_say: bool,
    ) {
        let now = self.clock.now();
        let mut changes = Vec::new();
        let done = self.store.atomically(|b| {
            changes.clear();
            for id in items {
                let before = b.item(*id)?;
                let mut item = before.clone();
                let touched = if on {
                    item.tags.insert(tag)
                } else {
                    item.tags.remove(&tag)
                };
                if touched {
                    item.updated_at = now;
                    update_recorded(b, &before, &item, &mut changes)?;
                }
            }
            Ok(())
        });
        // A change to several items says how many it reached (MAIN-24); one item shows its tick.
        let confirmation = (changes.len() > 1 || (always_say && !changes.is_empty())).then(|| {
            neutral_toast(ToastMessage::ItemsTagged {
                count: changes.len(),
                added: on,
            })
        });
        if done.is_ok() {
            self.record_step(Action::Tag, changes);
        }
        self.finish(&done, confirmation);
    }

    /// The Main window's close button. Returns whether Snipdeck should quit with it: when
    /// "Keep running in the tray" is off, or when nothing could bring a window back (no tray
    /// icon and the Panel hidden). Otherwise the window just hides.
    #[must_use]
    pub fn main_window_closed(&self) -> bool {
        !self.settings.close_to_tray || (!self.session.tray_available && !self.visible)
    }

    /// Renames tag `index` (in the order of [`MainScreen::tags`](crate::controller::MainScreen::tags)) to `name`. A name another tag
    /// has is not applied: [`TagRenameError::Merge`] says which tag it is and how many items would move,
    /// and the Tag manager asks before [`Self::main_merge_tag`]. A tag that is no longer there is
    /// left alone.
    ///
    /// # Errors
    ///
    /// Why the name was refused, or [`TagRenameError::Failed`] when the library could not be
    /// written.
    pub fn main_rename_tag(&mut self, index: usize, name: &str) -> Result<(), TagRenameError> {
        self.main_fresh();
        let name = Name::new(name).map_err(|error| {
            NameError::of(&error).map_or(TagRenameError::Failed, TagRenameError::Name)
        })?;
        let Some(tag) = self.main.tags().get(index).map(|t| t.id) else {
            return Ok(());
        };
        match self.store.update_tag(&Tag { id: tag, name }) {
            Ok(()) => {
                self.library_changed();
                Ok(())
            }
            Err(snipdeck_store::Error::TagExists(name)) => {
                let target = Name::new(&name)
                    .ok()
                    .and_then(|n| self.store.find_tag(&n).ok().flatten())
                    .and_then(|t| self.main.tags().iter().position(|m| m.id == t.id));
                let count = self.main.side_tags().get(index).map_or(0, |s| s.count);
                target.map_or(Err(TagRenameError::Exists), |into| {
                    Err(TagRenameError::Merge { into, count })
                })
            }
            Err(_) => {
                self.show_toast(failed());
                Err(TagRenameError::Failed)
            }
        }
    }

    /// Merges tag `from` into tag `into` (indices of [`MainScreen::tags`](crate::controller::MainScreen::tags)): its items
    /// carry `into` instead and `from` is removed. Two stale or equal indices do nothing.
    pub fn main_merge_tag(&mut self, from: usize, into: usize) {
        self.main_fresh();
        let tags = self.main.tags();
        let (Some(from), Some(into)) = (tags.get(from).map(|t| t.id), tags.get(into).map(|t| t.id))
        else {
            return;
        };
        if from == into {
            return;
        }
        let done = self.store.merge_tag(from, into).map(drop);
        self.finish(&done, None);
    }

    /// Adds a tag no item carries yet, from the Tag manager's "New tag" field.
    ///
    /// # Errors
    ///
    /// [`TagRenameError::Name`] for a bad name, [`TagRenameError::Exists`] when a tag has the name
    /// (ignoring case), [`TagRenameError::Failed`] when the library could not be written.
    pub fn main_new_tag(&mut self, name: &str) -> Result<(), TagRenameError> {
        let name = Name::new(name).map_err(|error| {
            NameError::of(&error).map_or(TagRenameError::Failed, TagRenameError::Name)
        })?;
        match self.store.insert_tag(&Tag::new(name)) {
            Ok(()) => {
                self.library_changed();
                Ok(())
            }
            Err(snipdeck_store::Error::TagExists(_)) => Err(TagRenameError::Exists),
            Err(_) => {
                self.show_toast(failed());
                Err(TagRenameError::Failed)
            }
        }
    }

    /// Deletes tag `index` after the window's confirmation (its items stay).
    pub fn main_delete_tag(&mut self, index: usize) {
        self.main_fresh();
        let Some((tag, name)) = self
            .main
            .tags()
            .get(index)
            .map(|t| (t.id, t.name.as_str().to_owned()))
        else {
            return;
        };
        let done = self.store.delete_tag(tag);
        self.finish(
            &done,
            Some(neutral_toast(ToastMessage::TagDeleted { name })),
        );
    }

    // ---- The Trash and Undo --------------------------------------------------------------------

    /// The toast's Undo: brings back what the delete moved to the Trash.
    pub fn main_undo(&mut self) {
        let Some(undo) = self.undo.take() else {
            return;
        };
        if self.toast.as_ref().is_none_or(|t| t.seq != undo.seq) {
            return;
        }
        // A step is taken back with the history; an add without one (a quick-add, or a step the
        // history has since forgotten) goes to the Trash.
        if let Some(step) = undo.step.filter(|s| self.history.can_undo(*s)) {
            // The old toast goes, a new one replaces it.
            self.undo_step(Some(step));
        } else if let Some(id) = undo.adds {
            let now = self.clock.now();
            let made = undo.list;
            let done = self.store.atomically(|b| {
                b.trash(Trashable::Item(id), now)?;
                made.map_or(Ok(()), |list| b.trash(Trashable::List(list), now))
            });
            if let Some(list) = made {
                let gone = list.to_string();
                if self.settings.colour_save_list.as_deref() == Some(gone.as_str()) {
                    self.update_settings(|s| s.colour_save_list = None);
                }
            }
            self.toast = None;
            if done.is_err() {
                self.show_toast(failed());
                self.refresh_logged("a failed undo");
            } else {
                self.library_changed();
            }
        }
    }

    /// "Empty Trash" after the window's confirmation.
    pub fn main_empty_trash(&mut self) {
        self.clear_history();
        let Some(cutoff) = self.trash_cutoff(0) else {
            return;
        };
        // Everything trashed up to now, including this very second.
        let cutoff = snipdeck_core::Timestamp::from_second(cutoff.as_second().saturating_add(1))
            .unwrap_or(cutoff);
        match self.store.purge(cutoff) {
            Ok(_) => {
                self.show_toast(neutral_toast(ToastMessage::TrashEmptied));
                self.library_changed();
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    /// Deletes what has been in the Trash longer than [`TRASH_DAYS`](crate::main_window::TRASH_DAYS)
    /// (the host calls this at start-up). Returns how many entries went.
    ///
    /// # Errors
    ///
    /// Why the Trash could not be purged (the maintenance run tells the user).
    pub fn purge_expired_trash(&mut self) -> Result<usize, snipdeck_store::Error> {
        let Some(cutoff) = self.trash_cutoff(i64::from(crate::main_window::TRASH_DAYS)) else {
            return Ok(0);
        };
        let purged = self.store.purge(cutoff)?;
        if purged > 0 {
            self.library_changed();
        }
        Ok(purged)
    }

    pub(super) fn trash_cutoff(&self, days: i64) -> Option<snipdeck_core::Timestamp> {
        let now = self.clock.now().as_second();
        snipdeck_core::Timestamp::from_second(now.checked_sub(days.checked_mul(86_400)?)?).ok()
    }
}
