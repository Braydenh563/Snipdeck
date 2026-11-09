//! Items made from outside: Ctrl+V with files, a picture or text on the clipboard, and files
//! dropped on a window. One path for both ([`classify`] says what the pieces are). The items land
//! at the top of the list as one history step, with a toast that offers Undo and Edit. Text that
//! looks like a secret is never saved without the user looking at it: the editor opens on it with
//! Sensitive on instead.

use std::path::PathBuf;

use snipdeck_core::{FileEntry, Format, Formats, Item, ItemId, ItemKind, ListId, Placement};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::{Batch, BlobStore, ItemRepo};

use super::history_ctl::item_targets;
use super::{Controller, Library, Toast, ToastKind, ToastMessage, Undo, failed, neutral};
use crate::{
    capture::MAX_TEXT_BYTES,
    history::{Action, Change},
    host::ClipboardText,
    item_editor::{self, Origin},
    paste_create::{Clip, Piece, classify, looks_secret},
};

/// The title of an item made from a picture on the clipboard (a file keeps its own name).
const PASTED_IMAGE_TITLE: &str = "Pasted image";

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// Ctrl+V in the Panel or the Main window with no text field focused: makes items from what
    /// is on the clipboard in the list `origin` shows. Files (copied in Explorer) win over text,
    /// and text over a picture. Nothing is written to the clipboard.
    pub fn paste_create(&mut self, origin: Origin) {
        self.main_fresh();
        let mut clip = Clip {
            files: self
                .services
                .clipboard
                .read_files()
                .into_iter()
                .map(PathBuf::from)
                .collect(),
            ..Clip::default()
        };
        if clip.files.is_empty() {
            clip.text = self.services.clipboard.read_text();
            if clip.text.as_deref().is_none_or(|t| t.trim().is_empty()) {
                clip.png = self.services.clipboard.read_png();
            }
        }
        let list = self.new_item_list(origin);
        self.create_from(&clip, list, Action::Paste);
    }

    /// Files were dropped on a window: makes items from them in `list` (the list under the
    /// pointer; `None`, or a list that is gone, is the list the window shows).
    pub fn drop_create(&mut self, paths: &[String], origin: Origin, list: Option<ListId>) {
        self.main_fresh();
        self.drop_end();
        let clip = Clip {
            files: paths.iter().map(PathBuf::from).collect(),
            ..Clip::default()
        };
        let target = list
            .filter(|l| self.store.list(*l).is_ok())
            .or_else(|| self.new_item_list(origin));
        self.create_from(&clip, target, Action::Drop);
    }

    /// Files are being dragged over a window and are over `list` (`None`: the list the window
    /// shows). The window says "Drop to add to <list>" until the drag ends.
    pub fn drop_hover(&mut self, origin: Origin, list: Option<ListId>) {
        let target = list
            .filter(|l| self.store.list(*l).is_ok())
            .or_else(|| self.new_item_list(origin));
        let Some(name) = target
            .and_then(|l| self.store.list(l).ok())
            .map(|l| l.name.as_str().to_owned())
        else {
            return;
        };
        if self.drop_hint == target {
            return;
        }
        self.drop_hint = target;
        self.show_toast(neutral(ToastMessage::DropTo { list: name }));
    }

    /// The list a drag over a window would add to, while one is over it.
    #[must_use]
    pub fn drop_hint(&self) -> Option<ListId> {
        self.drop_hint
    }

    /// The drag left the window or ended in a drop: the hint goes (another toast stays).
    pub fn drop_end(&mut self) {
        if self.drop_hint.take().is_some()
            && self
                .toast
                .as_ref()
                .is_some_and(|t| matches!(t.toast.message, ToastMessage::DropTo { .. }))
        {
            self.toast = None;
        }
    }

    fn create_from(&mut self, clip: &Clip, list: Option<ListId>, action: Action) {
        let pieces = classify(clip);
        if pieces.is_empty() {
            self.show_toast(neutral(ToastMessage::NothingToSave));
            return;
        }
        let Some(list) = list else {
            self.show_toast(neutral(ToastMessage::NoListToSave));
            return;
        };
        if let [Piece::Text { text, .. }] = pieces.as_slice() {
            if looks_secret(text).is_some() {
                self.open_new_item_secret(list, text);
                return;
            }
            if text.len() > MAX_TEXT_BYTES {
                self.show_toast(neutral(ToastMessage::TooLargeToSave));
                return;
            }
        }
        let now = self.clock.now();
        let mut skipped = 0_usize;
        let mut made: Vec<ItemId> = Vec::new();
        // One file is read at a time and stored at once, so a drop of many large files never
        // holds them all in memory. The first piece must end up on top: insert last to first.
        let done = self.store.atomically(|batch| {
            for piece in pieces.iter().rev() {
                match insert_piece(batch, piece, list, now) {
                    Ok(Some(id)) => made.push(id),
                    Ok(None) => skipped += 1,
                    Err(e) => return Err(e),
                }
            }
            Ok(())
        });
        if done.is_err() {
            self.show_toast(failed());
            return;
        }
        made.reverse();
        let Some(first) = made.first().copied() else {
            self.show_toast(neutral(ToastMessage::FilesNotAdded));
            return;
        };
        let name = self
            .store
            .list(list)
            .map(|l| l.name.as_str().to_owned())
            .unwrap_or_default();
        let step = self.record_step(action, vec![Change::Restore(item_targets(&made))]);
        let message = if skipped > 0 {
            ToastMessage::AddedSomeFiles { list: name }
        } else {
            ToastMessage::AddedTo {
                list: name,
                count: made.len(),
            }
        };
        self.show_added_toast(message, step, first);
        self.library_changed();
    }

    /// "Added to Code" with Undo (the history step) and Edit (`first`, the top new item).
    fn show_added_toast(&mut self, message: ToastMessage, step: Option<u64>, first: ItemId) {
        self.show_toast(Toast {
            kind: ToastKind::Success,
            message,
        });
        self.undo = Some(Undo {
            seq: self.toast_seq,
            step,
            adds: Some(first),
        });
    }
}

/// Stores `piece` as a new item at the top of `list`. `Ok(None)` when it cannot be used (a file
/// that is missing, a folder, too large, empty, not readable, or a picture that is not a PNG).
fn insert_piece(
    batch: &mut Batch<'_>,
    piece: &Piece,
    list: ListId,
    now: snipdeck_core::Timestamp,
) -> snipdeck_store::Result<Option<ItemId>> {
    let (mut item, format) = match piece {
        Piece::Text { kind, text } => {
            let mut item = Item::new(list, String::new(), *kind, now);
            item.preview = Item::preview_of(text, false);
            (item, Format::Text(batch.put(text.as_bytes().to_vec())?))
        }
        Piece::Image(png) => {
            if !item_editor::is_png(png) {
                return Ok(None);
            }
            let item = Item::new(list, PASTED_IMAGE_TITLE.to_owned(), ItemKind::Image, now);
            (item, Format::Png(batch.put(png.clone())?))
        }
        Piece::File(path) => {
            let Ok(source) = item_editor::read_source(ItemKind::File, &path.to_string_lossy())
            else {
                return Ok(None);
            };
            let kind = if item_editor::is_png(&source.bytes) {
                ItemKind::Image
            } else {
                ItemKind::File
            };
            let item = Item::new(list, source.name.clone(), kind, now);
            let content = batch.put(source.bytes)?;
            let format = if kind == ItemKind::Image {
                Format::Png(content)
            } else {
                Format::Files(vec![FileEntry::new(&source.name, content)?])
            };
            (item, format)
        }
    };
    item.updated_at = now;
    let formats = Formats::new(vec![format])?;
    batch.insert_item(&item, &formats, Placement::First)?;
    Ok(Some(item.id))
}
