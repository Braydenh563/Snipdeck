//! Item actions and the Trash with its Undo.

use snipdeck_core::{Feature, Item, ItemId, Name, Placement, Tag};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::{ItemRepo, TrashRepo, Trashable};

use super::super::{Controller, Library, Toast, ToastKind, ToastMessage, UseResult, failed};
use super::{MainAction, TagRenameError, neutral_toast, restore_failed};
use crate::{
    host::ClipboardText,
    main_window::{Key, NameError},
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
            MainAction::Use | MainAction::Copy => {
                let mode = if action == MainAction::Use {
                    snipdeck_core::UseMode::Paste
                } else {
                    snipdeck_core::UseMode::Copy
                };
                let id = items.first().copied()?;
                return Some(self.use_item(id, mode));
            }
            MainAction::Edit => {
                if let Some(id) = items.first() {
                    self.open_item_editor(*id);
                }
            }
            MainAction::Pin => self.pin(&items),
            MainAction::Duplicate => self.duplicate(&items),
            MainAction::Move | MainAction::Tag => self.main.set_chooser(targets),
            MainAction::Delete => self.trash_items(&items),
            MainAction::Restore => self.restore(&targets),
            MainAction::DeleteForever => self.delete_forever(&targets),
        }
        None
    }

    pub(super) fn pin(&mut self, items: &[ItemId]) {
        let now = self.clock.now();
        let pin = !items
            .iter()
            .all(|id| self.main.item(*id).is_some_and(|i| i.pinned));
        let done = self.store.atomically(|b| {
            for id in items {
                let mut item = b.item(*id)?;
                item.pinned = pin;
                item.updated_at = now;
                b.update_item(&item, None)?;
            }
            Ok(())
        });
        self.finish(&done);
    }

    /// Each item again, directly after itself: same title, content, tags and flags; pinned off and
    /// use history cleared, like a new item.
    /// A sensitive item behind a locked library is not copied: the unlock window is asked for and
    /// the toast says why nothing was done, like a copy of the whole library.
    pub(in super::super) fn duplicate(&mut self, items: &[ItemId]) {
        let now = self.clock.now();
        let locked = self.library_locked();
        let done = self.store.atomically(|b| {
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
            }
            Ok(())
        });
        if matches!(done, Err(snipdeck_store::Error::EncryptionLocked)) {
            self.request_unlock();
            self.show_toast(Toast {
                kind: ToastKind::Neutral,
                message: ToastMessage::UnlockFirst,
            });
            return;
        }
        self.finish(&done);
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
            let targets = items.iter().map(|id| Trashable::Item(*id)).collect();
            self.library_changed();
            self.show_undo_toast(
                Toast {
                    kind: ToastKind::Neutral,
                    message: ToastMessage::ItemsTrashed { count: items.len() },
                },
                targets,
            );
        } else {
            self.show_toast(failed());
        }
    }

    /// Brings `targets` back from the Trash. A board comes back only while the plan allows
    /// another one.
    pub(super) fn restore(&mut self, targets: &[Key]) {
        let limit = self.board_limit();
        let done = self.store.atomically(|b| {
            for key in targets {
                b.restore_with(Trashable::from(*key), limit)?;
            }
            Ok(())
        });
        self.library_changed();
        match done {
            Ok(()) => self.show_toast(Toast {
                kind: ToastKind::Success,
                message: ToastMessage::Restored {
                    count: targets.len(),
                },
            }),
            Err(error) => self.show_toast(restore_failed(&error)),
        }
    }

    pub(super) fn delete_forever(&mut self, targets: &[Key]) {
        let done = self.store.atomically(|b| {
            for key in targets {
                b.purge_entry(Trashable::from(*key))?;
            }
            Ok(())
        });
        self.library_changed();
        match done {
            Ok(()) => self.show_toast(neutral_toast(ToastMessage::DeletedForever {
                count: targets.len(),
            })),
            Err(_) => self.show_toast(failed()),
        }
    }

    /// The Free plan's live-board limit, for restoring a board; `None` on Supporter.
    pub(super) fn board_limit(&self) -> Option<usize> {
        (!self.entitlements.allows(Feature::UnlimitedBoards))
            .then_some(snipdeck_core::Entitlements::FREE_BOARD_LIMIT)
    }

    /// Ends an action: a failure is a toast, a success is a changed library.
    pub(super) fn finish(&mut self, done: &snipdeck_store::Result<()>) {
        match done {
            Ok(()) => self.library_changed(),
            Err(_) => self.show_toast(failed()),
        }
    }

    /// The *Move to list* chooser picked list `index`: the rows it was opened for go to its end.
    pub fn main_move_to(&mut self, index: usize) {
        self.main_fresh();
        let Some(list) = self.main.lists().get(index) else {
            return;
        };
        let (list, name) = (list.id, list.name.as_str().to_owned());
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
        let done = self.store.atomically(|b| {
            for id in &items {
                b.move_item(*id, list, Placement::Last)?;
            }
            Ok(())
        });
        self.main.set_chooser(Vec::new());
        match done {
            Ok(()) => {
                self.library_changed();
                self.show_toast(neutral_toast(ToastMessage::ItemsMoved {
                    count: items.len(),
                    list: name,
                }));
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    /// The *Tags* chooser ticked (`on`) or unticked tag `index` for the rows it was opened for.
    pub fn main_toggle_tag(&mut self, index: usize, on: bool) {
        self.main_fresh();
        let Some(tag) = self.main.tags().get(index).map(|t| t.id) else {
            return;
        };
        let now = self.clock.now();
        let items: Vec<ItemId> = self
            .main
            .chooser()
            .iter()
            .filter_map(|k| match k {
                Key::Item(id) => Some(*id),
                _ => None,
            })
            .collect();
        let done = self.store.atomically(|b| {
            for id in &items {
                let mut item = b.item(*id)?;
                let changed = if on {
                    item.tags.insert(tag)
                } else {
                    item.tags.remove(&tag)
                };
                if changed {
                    item.updated_at = now;
                    b.update_item(&item, None)?;
                }
            }
            Ok(())
        });
        self.finish(&done);
    }

    /// The Main window's close button. Returns whether Snipdeck should quit with it: when
    /// "Keep running in the tray" is off, or when nothing could bring a window back (no tray
    /// icon and the Panel hidden). Otherwise the window just hides.
    #[must_use]
    pub fn main_window_closed(&self) -> bool {
        !self.settings.close_to_tray || (!self.session.tray_available && !self.visible)
    }

    /// Renames tag `index` (in the order of [`MainScreen::tags`]) to `name`. A name another tag
    /// has is refused with [`TagRenameError::Exists`] and no toast: the Tag manager shows the
    /// reason beside the field. A tag that is no longer there is left alone.
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
        let Some(tag) = self.main.tags().get(index).map(|t| t.id) else {
            return;
        };
        let done = self.store.delete_tag(tag);
        self.finish(&done);
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
        let keys: Vec<Key> = undo.targets.iter().map(|t| Key::from(*t)).collect();
        // The delete is undone: the old toast goes, a "Restored" toast replaces it.
        self.restore(&keys);
    }

    /// "Empty Trash" after the window's confirmation.
    pub fn main_empty_trash(&mut self) {
        let Some(cutoff) = self.trash_cutoff(0) else {
            return;
        };
        // Everything trashed up to now, including this very second.
        let cutoff = snipdeck_core::Timestamp::from_second(cutoff.as_second().saturating_add(1))
            .unwrap_or(cutoff);
        match self.store.purge(cutoff) {
            Ok(_) => {
                self.library_changed();
                self.show_toast(neutral_toast(ToastMessage::TrashEmptied));
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    /// Deletes what has been in the Trash longer than [`TRASH_DAYS`](crate::main_window::TRASH_DAYS)
    /// (the host calls this at start-up). Returns how many entries went.
    pub fn purge_expired_trash(&mut self) -> usize {
        let Some(cutoff) = self.trash_cutoff(i64::from(crate::main_window::TRASH_DAYS)) else {
            return 0;
        };
        let purged = self.store.purge(cutoff).unwrap_or(0);
        if purged > 0 {
            self.library_changed();
        }
        purged
    }

    pub(super) fn trash_cutoff(&self, days: i64) -> Option<snipdeck_core::Timestamp> {
        let now = self.clock.now().as_second();
        snipdeck_core::Timestamp::from_second(now.checked_sub(days.checked_mul(86_400)?)?).ok()
    }
}
