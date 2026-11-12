//! Items made from outside: Ctrl+V with files, a picture or text on the clipboard, and files
//! dropped on a window. One path for both ([`classify`] says what the pieces are). The items land
//! at the top of the list as one history step, with a toast that offers Undo and Edit. Text that
//! looks like a secret is never saved without the user looking at it: the editor opens on it with
//! Sensitive on instead.

use std::path::PathBuf;

use snipdeck_core::{FileEntry, Format, Formats, Item, ItemId, ItemKind, ListId, Placement};
use snipdeck_platform::{
    pipeline::{Clipboard, Keyboard, Timing},
    win_formats::{ClipFiles, MAX_DROP_FILES},
};

use snipdeck_store::ItemRepo;

use super::history_ctl::item_targets;
use super::{Controller, Library, Toast, ToastKind, ToastMessage, Undo, failed, neutral};
use crate::{
    capture::{self, Captured},
    drop_read::{ReadDone, ReadJob},
    history::{Action, Change},
    host::ClipboardText,
    item_editor::{self, Origin},
    paste_create::{Clip, Piece, classify, looks_secret},
};

/// An item whose bytes are stored and which only needs its row.
struct Made {
    item: Item,
    formats: Formats,
}

/// A drop on its way in: the files still to read, and the items made from those that were read.
/// The controller starts it ([`Controller::drop_begin`]), a [`ReadJob`] reads the next files
/// (off the UI thread), and the controller takes the result ([`Controller::drop_read_done`]);
/// the last result inserts the rows, as one history step.
pub struct DropRun {
    list: ListId,
    now: snipdeck_core::Timestamp,
    /// Not yet read, in drop order.
    todo: Vec<PathBuf>,
    /// Stored, in drop order.
    ready: Vec<Made>,
    skipped: usize,
}

impl DropRun {
    /// The job that reads the files not yet read; they leave this drop until its result is
    /// handed back.
    pub fn next_job(&mut self) -> ReadJob {
        ReadJob::new(std::mem::take(&mut self.todo))
    }
}

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
        let mut clip = Clip::default();
        match self.services.clipboard.read_files() {
            ClipFiles::TooMany => {
                self.show_toast(neutral(ToastMessage::TooManyFiles));
                return;
            }
            ClipFiles::Files(paths) => clip.files = paths.into_iter().map(PathBuf::from).collect(),
        }
        if clip.files.is_empty() {
            let rich = self.services.clipboard.read_rich();
            (clip.text, clip.html, clip.rtf) = (rich.text, rich.html, rich.rtf);
            if clip.text.as_deref().is_none_or(|t| t.trim().is_empty()) {
                clip.png = self.services.clipboard.read_png();
            }
        }
        let list = self.new_item_list(origin);
        self.create_from(&clip, list, Action::Paste);
    }

    /// Files were dropped on a window: makes items from them in `list`, reading them on the
    /// calling thread (the windows read them off the UI thread with [`drop_begin`](Self::drop_begin)
    /// instead). Windows tells winit where the files came from but not where on the window, so
    /// the callers pass `None`: the list the window shows (so does a `list` that is gone).
    pub fn drop_create(&mut self, paths: &[String], origin: Origin, list: Option<ListId>) {
        let Some(mut run) = self.drop_begin(paths, origin, list) else {
            return;
        };
        loop {
            let done = run.next_job().run();
            match self.drop_read_done(run, done) {
                Some(next) => run = next,
                None => break,
            }
        }
    }

    /// Files were dropped on a window: checks them and names the list they go to. `None` when
    /// there is nothing to do (a toast says why); otherwise the drop, to be read with
    /// [`DropRun::next_job`] and finished with [`drop_read_done`](Self::drop_read_done).
    pub fn drop_begin(
        &mut self,
        paths: &[String],
        origin: Origin,
        list: Option<ListId>,
    ) -> Option<DropRun> {
        self.main_fresh();
        self.drop_end();
        if paths.len() > MAX_DROP_FILES {
            self.show_toast(neutral(ToastMessage::TooManyFiles));
            return None;
        }
        if paths.is_empty() {
            self.show_toast(neutral(ToastMessage::NothingToSave));
            return None;
        }
        let Some(list) = list
            .filter(|l| self.store.list(*l).is_ok())
            .or_else(|| self.new_item_list(origin))
        else {
            self.show_toast(neutral(ToastMessage::NoListToSave));
            return None;
        };
        Some(DropRun {
            list,
            now: self.clock.now(),
            todo: paths.iter().map(PathBuf::from).collect(),
            ready: Vec::new(),
            skipped: 0,
        })
    }

    /// The files `done` were read for `run`: their bytes are stored. Returns the drop again when
    /// files are left to read; otherwise the rows go in, as one history step, and it is over.
    pub fn drop_read_done(&mut self, mut run: DropRun, done: ReadDone) -> Option<DropRun> {
        for source in done.read {
            let Some(source) = source else {
                run.skipped += 1;
                continue;
            };
            let Ok(made) = self.made_from_source(source, run.list, run.now) else {
                self.show_toast(failed());
                return None;
            };
            run.ready.push(made);
        }
        run.todo.extend(done.rest);
        if !run.todo.is_empty() {
            return Some(run);
        }
        self.insert_made(&run.ready, run.skipped, run.list, Action::Drop);
        None
    }

    /// Files are being dragged over a window: the list the window shows (or `list`, when given
    /// and it exists) is named in "Drop to add to {list}" until the drag ends.
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
        if let [
            Piece::Text {
                kind,
                text,
                html,
                rtf,
            },
        ] = pieces.as_slice()
        {
            // A secret never goes in unseen, and its formatting is not kept.
            if looks_secret(text).is_some() {
                self.open_new_item_secret(list, text);
                return;
            }
            self.create_text(list, (text, html.as_deref(), rtf.as_deref()), *kind, action);
            return;
        }
        // Files and a picture: the bytes are read and stored one at a time (a drop of many large
        // files never holds them all in memory), then the rows go in together in one short
        // transaction. The first piece must end up on top: insert last to first.
        let now = self.clock.now();
        let mut skipped = 0_usize;
        let mut ready: Vec<Made> = Vec::new();
        for piece in &pieces {
            match self.prepare(piece, list, now) {
                Ok(Some(made)) => ready.push(made),
                Ok(None) => skipped += 1,
                Err(_) => {
                    self.show_toast(failed());
                    return;
                }
            }
        }
        self.insert_made(&ready, skipped, list, action);
    }

    /// Inserts the items made from files or a picture (`ready`, in order) at the top of `list`,
    /// in one short transaction with the first on top, and says so.
    fn insert_made(&mut self, ready: &[Made], skipped: usize, list: ListId, action: Action) {
        if ready.is_empty() {
            self.show_toast(neutral(ToastMessage::FilesNotAdded));
            return;
        }
        let done = self.store.atomically(|batch| {
            for made in ready.iter().rev() {
                batch.insert_item(&made.item, &made.formats, Placement::First)?;
            }
            Ok(())
        });
        if done.is_err() {
            self.show_toast(failed());
            return;
        }
        let made: Vec<ItemId> = ready.iter().map(|m| m.item.id).collect();
        self.finish_create(list, &made, skipped, action);
    }

    /// One text (or a link) as a new item, through the same save as quick capture.
    fn create_text(
        &mut self,
        list: ListId,
        (text, html, rtf): (&str, Option<&str>, Option<&[u8]>),
        kind: ItemKind,
        action: Action,
    ) {
        let Ok(Some(destination)) = capture::default_list(&self.store, Some(list)) else {
            self.show_toast(failed());
            return;
        };
        let saved = capture::capture_with(
            &mut self.store,
            &destination,
            Some(text),
            (html, rtf),
            kind,
            Placement::First,
            self.clock.now(),
        );
        match saved {
            Ok(Captured::Saved { item, .. }) => self.finish_create(list, &[item], 0, action),
            Ok(Captured::Empty) => self.show_toast(neutral(ToastMessage::NothingToSave)),
            Ok(Captured::TooLarge) => self.show_toast(neutral(ToastMessage::TooLargeToSave)),
            Err(_) => self.show_toast(failed()),
        }
    }

    /// The history step and the toast for the items just made (`made` is top first).
    fn finish_create(&mut self, list: ListId, made: &[ItemId], skipped: usize, action: Action) {
        let Some(first) = made.first().copied() else {
            return;
        };
        let name = self
            .store
            .list(list)
            .map(|l| l.name.as_str().to_owned())
            .unwrap_or_default();
        let step = self.record_step(action, vec![Change::Restore(item_targets(made))]);
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

    /// Reads and stores the bytes of `piece`: the item and its content, ready to insert.
    /// `Ok(None)` when it cannot be used: a file that is missing, a folder, empty, too large or
    /// unreadable, or a picture that is over the attachment limit or not a PNG.
    fn prepare(
        &mut self,
        piece: &Piece,
        list: ListId,
        now: snipdeck_core::Timestamp,
    ) -> snipdeck_store::Result<Option<Made>> {
        let (item, format) = match piece {
            // Text goes through `create_text`; `classify` never mixes it with attachments.
            Piece::Text { .. } => return Ok(None),
            Piece::Image(png) => {
                let too_big = u64::try_from(png.len())
                    .map_or(true, |n| n > item_editor::max_attachment_bytes());
                if too_big || !item_editor::is_png(png) {
                    return Ok(None);
                }
                let item = Item::new(
                    list,
                    capture::PASTED_IMAGE_TITLE.to_owned(),
                    ItemKind::Image,
                    now,
                );
                (item, Format::Png(self.store.put(png.clone())?))
            }
            Piece::File(path) => {
                let Ok(source) = item_editor::read_source(ItemKind::File, &path.to_string_lossy())
                else {
                    return Ok(None);
                };
                return self.made_from_source(source, list, now).map(Some);
            }
        };
        Ok(Some(Made {
            item,
            formats: Formats::new(vec![format])?,
        }))
    }

    /// Stores the bytes of a file that was read and makes its item: a PNG file becomes an Image
    /// item, any other a File item.
    fn made_from_source(
        &mut self,
        source: item_editor::Source,
        list: ListId,
        now: snipdeck_core::Timestamp,
    ) -> snipdeck_store::Result<Made> {
        let kind = if item_editor::is_png(&source.bytes) {
            ItemKind::Image
        } else {
            ItemKind::File
        };
        let item = Item::new(list, source.name.clone(), kind, now);
        let content = self.store.put(source.bytes)?;
        let format = if kind == ItemKind::Image {
            Format::Png(content)
        } else {
            Format::Files(vec![FileEntry::new(&source.name, content)?])
        };
        Ok(Made {
            item,
            formats: Formats::new(vec![format])?,
        })
    }
}
