//! Items made from outside: Ctrl+V with text on the clipboard. The text lands at the top of the
//! list the window shows as one history step, with a toast that offers Undo and Edit. Text that
//! looks like a secret is never saved without the user looking at it: the editor opens on it with
//! Sensitive on instead.

use snipdeck_core::{ItemId, Placement};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};

use super::history_ctl::item_targets;
use super::{Controller, Library, Toast, ToastKind, ToastMessage, Undo, failed, neutral};
use crate::{
    capture::{self, Captured},
    history::{Action, Change},
    host::ClipboardText,
    item_editor::Origin,
    paste_create::{kind_of_text, looks_secret},
};

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// Ctrl+V in the Panel or the Main window with no text field focused: makes an item from the
    /// text on the clipboard in the list `origin` shows (a lone web address becomes a Link).
    /// Nothing is written to the clipboard.
    pub fn paste_create(&mut self, origin: Origin) {
        self.main_fresh();
        let text = self.services.clipboard.read_text();
        let Some(text) = text.filter(|t| !t.trim().is_empty()) else {
            self.show_toast(neutral(ToastMessage::NothingToSave));
            return;
        };
        let Some(list) = self.new_item_list(origin) else {
            self.show_toast(neutral(ToastMessage::NoListToSave));
            return;
        };
        if looks_secret(&text).is_some() {
            self.open_new_item_secret(list, &text);
            return;
        }
        let Ok(Some(destination)) = capture::default_list(&self.store, Some(list)) else {
            self.show_toast(failed());
            return;
        };
        let saved = capture::capture_as(
            &mut self.store,
            &destination,
            Some(&text),
            kind_of_text(&text),
            Placement::First,
            self.clock.now(),
        );
        match saved {
            Ok(Captured::Saved { item, list }) => {
                let step =
                    self.record_step(Action::Paste, vec![Change::Restore(item_targets(&[item]))]);
                self.show_added_toast(list, 1, step, item);
                self.library_changed();
            }
            Ok(Captured::Empty) => self.show_toast(neutral(ToastMessage::NothingToSave)),
            Ok(Captured::TooLarge) => self.show_toast(neutral(ToastMessage::TooLargeToSave)),
            Err(_) => self.show_toast(failed()),
        }
    }

    /// "Added to Code" with Undo (the history step) and Edit (`first`, the new item).
    fn show_added_toast(&mut self, list: String, count: usize, step: Option<u64>, first: ItemId) {
        self.show_toast(Toast {
            kind: ToastKind::Success,
            message: ToastMessage::AddedTo { list, count },
        });
        self.undo = Some(Undo {
            seq: self.toast_seq,
            step,
            adds: Some(first),
        });
    }
}
