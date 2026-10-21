//! Items made from outside: Ctrl+V with something on the clipboard, and things dropped on a
//! window (text, a picture, files from Explorer, a link from a browser). One path for both: the
//! pieces come from [`classify`], text that looks like a secret is never saved without the user
//! looking at it, everything else lands at the top of the chosen list as one history step, with
//! a toast that offers Undo and Edit.

use snipdeck_core::{
    Content, FileEntry, Format, Formats, Inline, Item, ItemId, ItemKind, ListId, Placement,
};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::{BlobStore, ItemRepo, ListRepo};

use super::history_ctl::item_targets;
use super::{Controller, Effect, Library, Toast, ToastKind, ToastMessage, failed};
use crate::{
    capture::MAX_TEXT_BYTES,
    history::{Action, Change},
    host::ClipboardText,
    item_editor::{self, Origin},
    paste_create::{Clip, Piece, classify, looks_secret},
};

/// What one piece turns into once read: the item and its content.
struct Made {
    item: Item,
    formats: Formats,
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// Ctrl+V in the Panel or the Main window with no text field focused: makes items from what
    /// is on the clipboard in the list `origin` shows. Returns the effects the host applies
    /// (the editor opens for text that looks like a secret).
    pub fn paste_create(&mut self, origin: Origin) -> Vec<Effect> {
        self.main_fresh();
        let clip = self.services.clipboard.read_clip();
        let Some(list) = self.new_item_list(origin) else {
            self.show_toast(Toast {
                kind: ToastKind::Neutral,
                message: ToastMessage::NoListToSave,
            });
            return Vec::new();
        };
        self.create_from(&clip, list, Action::Paste)
    }

    /// Something was dropped on a window: makes items from `clip` in `list` (the list under the
    /// pointer; `None` is the list the window shows).
    pub fn drop_create(
        &mut self,
        clip: &Clip,
        origin: Origin,
        list: Option<ListId>,
    ) -> Vec<Effect> {
        self.main_fresh();
        self.drop_hint = None;
        let target = list.filter(|l| self.store.list(*l).is_ok());
        let Some(list) = target.or_else(|| self.new_item_list(origin)) else {
            self.show_toast(Toast {
                kind: ToastKind::Neutral,
                message: ToastMessage::NoListToSave,
            });
            return Vec::new();
        };
        self.create_from(clip, list, Action::Drop)
    }

    /// Something is being dragged over a window and is over `list` (or over no list: the list
    /// the window shows). The window shows "Drop to add to <list>" until the drag ends.
    pub fn drop_hover(&mut self, origin: Origin, list: Option<ListId>) {
        let target = list
            .filter(|l| self.store.list(*l).is_ok())
            .or_else(|| self.new_item_list(origin));
        self.drop_hint = target
            .and_then(|l| self.store.list(l).ok())
            .map(|l| l.name.as_str().to_owned());
    }

    /// The drag left the window or was dropped: the highlight goes.
    pub fn drop_end(&mut self) {
        self.drop_hint = None;
    }

    fn create_from(&mut self, clip: &Clip, list: ListId, action: Action) -> Vec<Effect> {
        let pieces = classify(clip);
        if pieces.is_empty() {
            self.show_toast(Toast {
                kind: ToastKind::Neutral,
                message: ToastMessage::NothingToSave,
            });
            return Vec::new();
        }
        // A secret is never saved without a look: the editor opens with it and Sensitive on.
        if let [Piece::Text { text, .. }] = pieces.as_slice()
            && looks_secret(text).is_some()
        {
            return self.ask_about_secret(list, text);
        }
        let now = self.clock.now();
        let mut skipped = 0_usize;
        let mut ready = Vec::new();
        for piece in &pieces {
            match read_piece(piece, list, now) {
                Ok(Read::Made(made)) => ready.push(made),
                Ok(Read::TooLarge) => {
                    self.show_toast(Toast {
                        kind: ToastKind::Neutral,
                        message: ToastMessage::TooLargeToSave,
                    });
                    return Vec::new();
                }
                Err(()) => skipped += 1,
            }
        }
        if ready.is_empty() {
            self.show_toast(if skipped > 0 {
                failed()
            } else {
                Toast {
                    kind: ToastKind::Neutral,
                    message: ToastMessage::NothingToSave,
                }
            });
            return Vec::new();
        }
        let ids: Vec<ItemId> = ready.iter().map(|m| m.item.id).collect();
        // The first piece ends up on top: insert last to first, each at the head.
        let done = self.store.atomically(|b| {
            for made in ready.iter().rev() {
                let content = made.formats.clone();
                b.insert_item(&made.item, &content, Placement::First)?;
            }
            Ok(())
        });
        if done.is_err() {
            self.show_toast(failed());
            return Vec::new();
        }
        let step = self.record_step(action, vec![Change::Restore(item_targets(&ids))]);
        self.library_changed();
        if self.refresh_panel().is_err() {
            self.show_toast(failed());
            return Vec::new();
        }
        let name = self
            .store
            .list(list)
            .map(|l| l.name.as_str().to_owned())
            .unwrap_or_default();
        self.show_added_toast(name, ids.len(), step, ids.first().copied());
        Vec::new()
    }

    fn ask_about_secret(&mut self, list: ListId, text: &str) -> Vec<Effect> {
        if self.raise_item_editor() {
            return vec![Effect::NewItem];
        }
        match self.new_slot(list) {
            Ok(mut slot) => {
                slot.fields.content = text.to_owned();
                slot.fields.sensitive = true;
                self.install_item_editor(slot);
                self.show_toast(Toast {
                    kind: ToastKind::Neutral,
                    message: ToastMessage::LooksSecret,
                });
                vec![Effect::NewItem]
            }
            Err(_) => {
                self.show_toast(failed());
                Vec::new()
            }
        }
    }

    /// "Added to Code" with Undo (the history step) and Edit (the first new item).
    fn show_added_toast(
        &mut self,
        list: String,
        count: usize,
        step: Option<u64>,
        first: Option<ItemId>,
    ) {
        self.show_undo_toast(
            Toast {
                kind: ToastKind::Success,
                message: ToastMessage::AddedTo { list, count },
            },
            step,
        );
        if let Some(active) = self.toast.as_mut() {
            active.edit = first;
        }
    }

    /// The toast's Edit button: the editor on the item the toast is about. The host opens the
    /// editor for the returned effect.
    pub fn toast_edit(&mut self) -> Vec<Effect> {
        let Some(active) = self.toast.as_mut() else {
            return Vec::new();
        };
        active.edit.take().map(Effect::EditItem).into_iter().collect()
    }
}

/// A piece read from where it came from.
enum Read {
    Made(Made),
    TooLarge,
}

/// Reads `piece` into an item for `list`. `Err` when a file cannot be used (missing, a folder,
/// too large, empty); text over the limit is [`Read::TooLarge`].
fn read_piece(
    piece: &Piece,
    list: ListId,
    now: snipdeck_core::Timestamp,
) -> Result<Read, ()> {
    match piece {
        Piece::Text { kind, text } => {
            if text.len() > MAX_TEXT_BYTES {
                return Ok(Read::TooLarge);
            }
            let mut item = Item::new(list, String::new(), *kind, now);
            item.preview = Item::preview_of(text, false);
            let formats = Formats::new(vec![Format::Text(content_of(text.as_bytes())?)])
                .map_err(|_| ())?;
            Ok(Read::Made(Made { item, formats }))
        }
        Piece::Image(png) => {
            if !item_editor::is_png(png) {
                return Err(());
            }
            let item = Item::new(list, "Pasted image".to_owned(), ItemKind::Image, now);
            let formats = Formats::new(vec![Format::Png(content_of(png)?)]).map_err(|_| ())?;
            Ok(Read::Made(Made { item, formats }))
        }
        Piece::File(path) => {
            let source = item_editor::read_source(ItemKind::File, &path.to_string_lossy())
                .map_err(|_| ())?;
            let kind = if item_editor::is_png(&source.bytes) {
                ItemKind::Image
            } else {
                ItemKind::File
            };
            let item = Item::new(list, source.name.clone(), kind, now);
            let content = content_of(&source.bytes)?;
            let format = if kind == ItemKind::Image {
                Format::Png(content)
            } else {
                Format::Files(vec![FileEntry::new(&source.name, content).map_err(|_| ())?])
            };
            let formats = Formats::new(vec![format]).map_err(|_| ())?;
            Ok(Read::Made(Made { item, formats }))
        }
    }
}

/// Inline content for small bytes; larger bytes need the store's blob folder, so they are put
/// when the item is inserted (see `Made`). Small enough bytes only here.
fn content_of(bytes: &[u8]) -> Result<Content, ()> {
    Inline::new(bytes.to_vec()).map(Content::Inline).map_err(|_| ())
}
