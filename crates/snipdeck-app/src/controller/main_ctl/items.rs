//! Item actions and the Trash with its Undo.

use crate::history::{Action, Change, Membership, Record};
use snipdeck_core::template::transform::Transform;
use snipdeck_core::{Item, ItemId, Name, Placement, Tag};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::{GroupRepo, ItemRepo, TagRepo, TrashRepo, Trashable};

use super::super::history_ctl::{item_targets, move_recorded, update_recorded};
use super::super::{
    Controller, Library, Toast, ToastKind, ToastMessage, UseResult, change_failed, failed, success,
};
use super::{MainAction, TagRenameError, neutral_toast};
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
            MainAction::OpenLink => {
                if let Some(id) = items.first() {
                    self.open_link(*id);
                }
            }
            MainAction::Pin => self.pin(&items),
            MainAction::Duplicate => self.duplicate(&items),
            MainAction::Move | MainAction::Tag | MainAction::PasteAs => {
                self.group_move.clear();
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

    /// Opens Link item `id` in the system's handler. Only `link_open::ALLOWED_SCHEMES` are opened,
    /// whatever the item holds; anything else (or an address the system cannot open) is a toast.
    fn open_link(&mut self, id: ItemId) {
        let is_link = self
            .main
            .item(id)
            .is_some_and(|i| i.kind == snipdeck_core::ItemKind::Link);
        if !is_link {
            return;
        }
        let text = crate::usecase::item_text(&self.store, id).ok().flatten();
        let opened = match (&text, self.editor_env.link_opener.clone()) {
            (Some(text), Some(opener)) => {
                crate::link_open::open_with(text, |target| opener(target)).is_ok()
            }
            _ => false,
        };
        if !opened {
            self.show_toast(Toast {
                kind: ToastKind::Danger,
                message: ToastMessage::LinkNotOpened,
            });
        }
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
        let format = self.copy_title.clone();
        let copy_title = |title: &str| crate::starter::copy_title(&format, title);
        let done = self.store.atomically(|b| {
            made.clear();
            for id in items {
                let original = b.item(*id)?;
                if locked && original.sensitive {
                    return Err(snipdeck_store::Error::EncryptionLocked);
                }
                let formats = b.formats(*id)?;
                let mut copy = Item::new(
                    original.list_id,
                    copy_title(&original.title),
                    original.kind,
                    now,
                );
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
        // The cursor moves to the copy, so Enter or F2 acts on it.
        if let Some(copy) = made.first() {
            self.main_fresh();
            self.follow_moved_row(*copy);
        }
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

    /// Brings `targets` back from the Trash, as a step Undo can put back.
    pub(super) fn restore(&mut self, targets: &[Key]) {
        let entries: Vec<Trashable> = targets.iter().filter_map(|k| k.trashable()).collect();
        let done = self.store.atomically(|b| {
            for entry in &entries {
                b.restore_reporting(*entry)?;
            }
            Ok(())
        });
        if done.is_ok() {
            let step = self.record_step(Action::Restore, vec![Change::Restore(entries)]);
            self.show_undo_toast(
                Toast {
                    kind: ToastKind::Success,
                    message: ToastMessage::Restored {
                        count: targets.len(),
                    },
                },
                step,
            );
            self.library_changed();
        } else {
            self.show_toast(failed());
            self.refresh_logged("a failed restore");
        }
    }

    pub(super) fn delete_forever(&mut self, targets: &[Key]) {
        let entries: Vec<Trashable> = targets.iter().filter_map(|k| k.trashable()).collect();
        self.purge_entries(&entries);
    }

    /// Removes `entries` from the Trash for good. The history forgets only the steps that needed
    /// them; the rest stays.
    pub(in crate::controller) fn purge_entries(&mut self, entries: &[Trashable]) {
        let done = self.store.atomically(|b| {
            for entry in entries {
                b.purge_entry(*entry)?;
            }
            Ok(())
        });
        if done.is_ok() {
            self.prune_history();
            self.show_toast(neutral_toast(ToastMessage::DeletedForever {
                count: entries.len(),
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
        let Some(list) = self.main.lists().get(index) else {
            return;
        };
        let (list, name) = (list.id, list.name.as_str().to_owned());
        let carried = std::mem::take(&mut self.group_move);
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
        // Each item's group now, recorded first so Undo (which runs the steps backwards) puts the
        // membership back last, whatever the moves did to it.
        let regroups: Vec<Change> = items
            .iter()
            .filter_map(|id| {
                let from = self.store.item(*id).ok()?.list_id;
                let runs = self.store.groups_of(from).ok()?;
                let run = runs.iter().find(|r| r.items.contains(id))?;
                Some(Change::Regroup {
                    item: *id,
                    group: Membership::of(&runs, run.group.id),
                })
            })
            .collect();
        let mut changes = Vec::new();
        let done = self.store.atomically(|b| {
            changes.clear();
            changes.extend(regroups.iter().cloned());
            for id in &items {
                move_recorded(b, *id, list, Placement::Last, &mut changes)?;
            }
            // A moved group stays a group, with the groups nested in it: made again in the new
            // list with the same names, folds and colours. The outermost holds every item, each
            // nested one its own.
            let mut made: Vec<(snipdeck_core::GroupId, snipdeck_core::GroupId)> = Vec::new();
            for old in &carried {
                let mut group = snipdeck_core::Group::new(list, old.group.name.clone());
                group.collapsed = old.group.collapsed;
                group.colour = old.group.colour;
                group.parent = old
                    .group
                    .parent
                    .and_then(|p| made.iter().find(|(o, _)| *o == p).map(|(_, n)| *n));
                let members = if group.parent.is_some() {
                    old.items.clone()
                } else {
                    items.clone()
                };
                b.create_group(&group, &members)?;
                made.push((old.group.id, group.id));
                changes.push(Change::Group {
                    group,
                    members,
                    children: Vec::new(),
                });
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
        let mut changes = Vec::new();
        let done = self.store.atomically(|b| {
            changes.clear();
            for id in &items {
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
        if done.is_ok() {
            self.record_step(Action::Tag, changes);
        }
        self.finish(&done, None);
    }

    /// The Main window's close button. Returns whether Snipdeck should quit with it: when
    /// "Keep running in the tray" is off, or when nothing could bring a window back (no tray
    /// icon and the Panel hidden). Otherwise the window just hides.
    #[must_use]
    pub fn main_window_closed(&self) -> bool {
        !self.settings.close_to_tray || (!self.session.tray_available && !self.visible)
    }

    /// Renames tag `index` (in the order of [`MainScreen::tags`](crate::controller::MainScreen::tags)) to `name`. A name another tag
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
        let before = self.stored_tag(tag);
        let after = Tag {
            id: tag,
            name,
            colour: before.as_ref().and_then(|b| b.colour),
            parent: before.as_ref().and_then(|b| b.parent),
        };
        match self.store.update_tag(&after) {
            Ok(()) => {
                if let Some(before) = before.filter(|b| *b != after) {
                    self.record_step(
                        Action::Tag,
                        vec![Change::Rewrite {
                            before: Record::Tag(before),
                            after: Record::Tag(after),
                        }],
                    );
                }
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

    /// Colours tag `index` (`None` takes the colour off): one of the eight item colours, shown as
    /// a dot beside the tag wherever it is listed.
    pub fn main_set_tag_colour(&mut self, index: usize, colour: Option<snipdeck_core::ItemLabel>) {
        self.main_fresh();
        let Some(tag) = self.main.tags().get(index).map(|t| t.id) else {
            return;
        };
        let Some(before) = self.stored_tag(tag) else {
            return;
        };
        let after = Tag {
            colour,
            ..before.clone()
        };
        if after == before {
            return;
        }
        match self.store.update_tag(&after) {
            Ok(()) => {
                self.record_step(
                    Action::Tag,
                    vec![Change::Rewrite {
                        before: Record::Tag(before),
                        after: Record::Tag(after),
                    }],
                );
                self.library_changed();
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    /// Nests tag `index` under tag `under` (both in the order of
    /// [`MainScreen::tags`](crate::controller::MainScreen::tags)), or takes it back to the top
    /// level with `None`. Its own nested tags go with it. One history step. A tag cannot go
    /// under itself or under one of its own nested tags; that is left alone.
    pub fn main_nest_tag(&mut self, index: usize, under: Option<usize>) {
        self.main_fresh();
        let Some(tag) = self.main.tags().get(index).map(|t| t.id) else {
            return;
        };
        let parent = match under {
            None => None,
            Some(i) => {
                let Some(parent) = self.main.tags().get(i).map(|t| t.id) else {
                    return;
                };
                if !self.main.tag_tree().can_nest(tag, parent) {
                    return;
                }
                Some(parent)
            }
        };
        let Some(before) = self.stored_tag(tag) else {
            return;
        };
        let after = Tag {
            parent,
            ..before.clone()
        };
        if after == before {
            return;
        }
        match self.store.update_tag(&after) {
            Ok(()) => {
                self.record_step(
                    Action::Tag,
                    vec![Change::Rewrite {
                        before: Record::Tag(before),
                        after: Record::Tag(after),
                    }],
                );
                self.library_changed();
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    /// Merges tag `index` into the tag named `into` (the clash a rename met): every item that
    /// carried the first carries the second, and the first is deleted. One history step, so Undo
    /// brings the tag back on exactly the items it was on.
    ///
    /// # Errors
    ///
    /// [`TagRenameError::Failed`] when no other tag has that name or the library could not be
    /// written.
    pub fn main_merge_tag(&mut self, index: usize, into: &str) -> Result<(), TagRenameError> {
        self.main_fresh();
        let key = into.trim().to_lowercase();
        let Some(source) = self.main.tags().get(index).map(|t| t.id) else {
            return Ok(());
        };
        let Some(target) = self
            .main
            .tags()
            .iter()
            .find(|t| t.id != source && t.name.as_str().to_lowercase() == key)
            .map(|t| (t.id, t.name.as_str().to_owned()))
        else {
            return Err(TagRenameError::Failed);
        };
        let Some(stored) = self.stored_tag(source) else {
            return Ok(());
        };
        let from = stored.name.as_str().to_owned();
        let now = self.clock.now();
        let carriers = self.store.tag_items(source).unwrap_or_default();
        let already = self.store.tag_items(target.0).unwrap_or_default();
        let tree = self.main.tag_tree().clone();
        let mut changes = Vec::new();
        let done = self.store.atomically(|b| {
            changes.clear();
            // The items in the Trash are not rewritten: they gain the target by a link of their own
            // (so a restore keeps the merged tag), and Undo takes it off again and brings the
            // source back on them.
            let mut kept = Vec::new();
            for id in &carriers {
                match b.item(*id) {
                    Ok(before) => {
                        let mut item = before.clone();
                        item.tags.remove(&source);
                        item.tags.insert(target.0);
                        item.updated_at = now;
                        update_recorded(b, &before, &item, &mut changes)?;
                    }
                    Err(snipdeck_store::Error::NotFound { .. }) => kept.push(*id),
                    Err(e) => return Err(e),
                }
            }
            let gain: Vec<_> = kept
                .iter()
                .copied()
                .filter(|id| !already.contains(id))
                .collect();
            if !gain.is_empty() {
                b.link_tag(target.0, &gain)?;
                changes.push(Change::LinkTag {
                    tag: target.0,
                    items: gain,
                });
            }
            // The tags nested under it follow the merge: they go under the target, except the
            // target itself and the tags above it, which go where the merged tag was.
            lift_children(b, &stored, &mut changes, |child| {
                if child == target.0 || tree.is_under(target.0, child) {
                    stored.parent
                } else {
                    Some(target.0)
                }
            })?;
            b.delete_tag(source)?;
            changes.push(Change::DropTag {
                tag: stored.clone(),
                items: kept,
            });
            Ok(())
        });
        if done.is_err() {
            self.show_toast(failed());
            return Err(TagRenameError::Failed);
        }
        let step = self.record_step(Action::Tag, changes);
        self.show_undo_toast(
            neutral_toast(ToastMessage::TagMerged {
                from,
                into: target.1,
            }),
            step,
        );
        self.library_changed();
        Ok(())
    }

    /// Deletes tag `index` after the window's confirmation (its items stay).
    pub fn main_delete_tag(&mut self, index: usize) {
        self.main_fresh();
        let Some(tag) = self.main.tags().get(index).map(|t| t.id) else {
            return;
        };
        let Some(stored) = self.stored_tag(tag) else {
            return;
        };
        let items = self.store.tag_items(tag).unwrap_or_default();
        // The tags nested under it move up to where it was; Undo puts them back under it.
        let mut changes = Vec::new();
        let done = self.store.atomically(|b| {
            changes.clear();
            lift_children(b, &stored, &mut changes, |_| stored.parent)?;
            b.delete_tag(tag)?;
            changes.push(Change::DropTag {
                tag: stored.clone(),
                items: items.clone(),
            });
            Ok(())
        });
        match done {
            Ok(()) => {
                let name = stored.name.as_str().to_owned();
                let step = self.record_step(Action::Tag, changes);
                self.show_undo_toast(neutral_toast(ToastMessage::TagDeleted { name }), step);
                self.library_changed();
            }
            Err(e) => self.show_toast(change_failed(&e)),
        }
    }

    /// Tag `id` as stored.
    fn stored_tag(&self, id: snipdeck_core::TagId) -> Option<Tag> {
        self.store.tags().ok()?.into_iter().find(|t| t.id == id)
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
            if done.is_err() {
                self.toast = None;
                self.show_toast(failed());
                self.refresh_logged("a failed undo");
            } else {
                // The item is in the Trash for 30 days: the toast offers to remove it for good.
                self.show_toast(Toast {
                    kind: ToastKind::Success,
                    message: ToastMessage::Undone {
                        action: Action::Add,
                    },
                });
                let mut purge = vec![Trashable::Item(id)];
                purge.extend(made.map(Trashable::List));
                self.undo = Some(super::super::Undo {
                    seq: self.toast_seq,
                    step: None,
                    adds: None,
                    list: None,
                    purge,
                });
                self.library_changed();
            }
        }
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
                self.prune_history();
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
            self.prune_history();
            self.library_changed();
        }
        Ok(purged)
    }

    pub(super) fn trash_cutoff(&self, days: i64) -> Option<snipdeck_core::Timestamp> {
        let now = self.clock.now().as_second();
        snipdeck_core::Timestamp::from_second(now.checked_sub(days.checked_mul(86_400)?)?).ok()
    }
}

/// Moves the tags nested directly under `source` to the parent `to` names for each, recording
/// every move as a step change so Undo can put it back.
fn lift_children(
    b: &mut snipdeck_store::Batch<'_>,
    source: &Tag,
    changes: &mut Vec<Change>,
    to: impl Fn(snipdeck_core::TagId) -> Option<snipdeck_core::TagId>,
) -> snipdeck_store::Result<()> {
    for before in b
        .tags()?
        .into_iter()
        .filter(|t| t.parent == Some(source.id))
    {
        let after = Tag {
            parent: to(before.id),
            ..before.clone()
        };
        b.update_tag(&after)?;
        changes.push(Change::Rewrite {
            before: Record::Tag(before),
            after: Record::Tag(after),
        });
    }
    Ok(())
}
