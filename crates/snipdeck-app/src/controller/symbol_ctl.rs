//! The Symbols tab's half of the [`Controller`]: using a symbol (paste or copy), adding it to a
//! list, copying or saving an icon, and the recents. Slint-free: the tab's callbacks call these
//! and draw the toast each one raises.
//!
//! A symbol is not an item: it has no [`ItemId`](snipdeck_core::ItemId), is never counted in the
//! store, and its text goes through the same clipboard pipeline a snippet uses
//! ([`snipdeck_platform::pipeline::use_item`]) with the same refocus-and-wait as an item.

use std::path::PathBuf;

use snipdeck_core::{ItemKind, UseMode};
use snipdeck_platform::{
    file_dialog::SaveKind,
    payload::{Data, Payload},
    pipeline::{self, Clipboard, Env, Keyboard, Timing},
    route::Route,
};
use snipdeck_symbols::{Catalog, Kind, Recents, SymbolId};

use super::{Controller, Library, REFOCUS_WAIT, UseResult, failed};
use crate::{
    host::ClipboardText,
    item_editor::Origin,
    toast::{self, Toast, ToastKind, ToastMessage},
};

/// The size, in pixels, of an icon copied to the clipboard as a picture.
pub const ICON_COPY_PX: u32 = 64;

/// What "Save as..." writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconExport {
    /// The SVG drawing, `currentColor` kept.
    Svg,
    /// A PNG of `px` x `px` pixels in the text colour.
    Png {
        /// Width and height in pixels (the menu offers 24, 32, 48, 64, 128, 256).
        px: u32,
        /// The text colour of the theme in force, as `copy_icon_png` takes it.
        rgba: [u8; 4],
    },
}

/// How a "Save as..." ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    /// The file was written here.
    Saved(PathBuf),
    /// The user closed the dialog.
    Cancelled,
    /// The machine has no file dialog (the toast is not raised: the menu should not be offered).
    Unavailable,
    /// The dialog or the write failed (a toast says so); the symbol was not used.
    Failed,
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// Uses an emoji or kaomoji: pastes it into the Target, or copies it (`mode` Copy, or no
    /// Target), raises the usual toast and remembers the symbol. Icons have no text:
    /// [`UseResult::Failed`], nothing written, no toast.
    pub fn use_symbol(&mut self, id: &SymbolId, mode: UseMode) -> UseResult {
        let Some(text) = Catalog::get().get_symbol(id).and_then(|s| s.text.clone()) else {
            return UseResult::Failed;
        };
        let Some(payload) = Payload::new(vec![Data::Text(text)]) else {
            return UseResult::Failed;
        };
        let route = self.model.route();
        let pasted_into = self.model.target().map(|t| t.app.clone());
        if mode != UseMode::Copy && matches!(route, Route::Paste { .. }) && self.target.refocus() {
            self.services.timing.sleep(REFOCUS_WAIT);
        }
        let mut env = Env {
            clipboard: &mut self.services.clipboard,
            keyboard: &mut self.services.keyboard,
            timing: &mut self.services.timing,
        };
        if let Ok(outcome) = pipeline::use_item(&mut env, &payload, mode, &route) {
            self.note_activity();
            self.show_toast(toast::after_use(outcome, &route, pasted_into.as_deref()));
            self.remember_symbol(id);
            UseResult::Done(outcome)
        } else {
            self.show_toast(clipboard_busy());
            UseResult::Failed
        }
    }

    /// "Add to list": saves an emoji or kaomoji as a Text item at the end of the list `origin`
    /// shows (the same path as the quick-add box, so its toast with Undo and Edit appears, and
    /// "no list" says so). Returns whether an item was added. Icons are not text: `false`.
    pub fn add_symbol_to_list(&mut self, id: &SymbolId, origin: Origin) -> bool {
        let Some(text) = Catalog::get().get_symbol(id).and_then(|s| s.text.clone()) else {
            return false;
        };
        let added = self.quick_add(&text, Some(ItemKind::Text), false, origin);
        if added {
            self.remember_symbol(id);
        }
        added
    }

    /// "Copy SVG": the icon's markup as text, `fill="currentColor"` kept. `false` for anything
    /// that is not an icon, or when the clipboard is busy (a toast says so).
    pub fn copy_icon_svg(&mut self, id: &SymbolId) -> bool {
        let Some(svg) = Catalog::get().icon_svg(id) else {
            return false;
        };
        self.copy_symbol_data(id, Data::Text(svg))
    }

    /// "Copy PNG": the icon as a [`ICON_COPY_PX`] picture in `rgba` (the text colour of the
    /// theme in force), transparent behind. `false` for anything that is not an icon, or when
    /// the clipboard is busy.
    pub fn copy_icon_png(&mut self, id: &SymbolId, rgba: [u8; 4]) -> bool {
        let Some(png) = Catalog::get().icon_png(id, ICON_COPY_PX, rgba) else {
            return false;
        };
        self.copy_symbol_data(id, Data::Png(png))
    }

    /// "Save as...": asks where with the system Save dialog (the icon's name filled in), writes
    /// the SVG or PNG there and remembers the icon. `owner` is the native window handle the
    /// dialog belongs to. A cancel writes nothing and says nothing.
    pub fn save_icon(
        &mut self,
        id: &SymbolId,
        export: IconExport,
        owner: Option<isize>,
    ) -> SaveOutcome {
        let SymbolId::Icon(name) = id else {
            return SaveOutcome::Failed;
        };
        let Some(picker) = self.file_picker() else {
            return SaveOutcome::Unavailable;
        };
        let catalog = Catalog::get();
        let (kind, bytes) = match export {
            IconExport::Svg => (SaveKind::Svg, catalog.icon_svg(id).map(String::into_bytes)),
            IconExport::Png { px, rgba } => (SaveKind::Png, catalog.icon_png(id, px, rgba)),
        };
        let Some(bytes) = bytes else {
            return SaveOutcome::Failed;
        };
        let path = match picker.save(kind, name, owner) {
            Ok(Some(path)) => path,
            Ok(None) => return SaveOutcome::Cancelled,
            Err(_) => {
                self.show_toast(failed());
                return SaveOutcome::Failed;
            }
        };
        if std::fs::write(&path, bytes).is_err() {
            self.show_toast(failed());
            return SaveOutcome::Failed;
        }
        self.remember_symbol(id);
        SaveOutcome::Saved(path)
    }

    /// The symbols used last, newest first (only one kind when given). Reads the saved list the
    /// first time the Symbols tab asks, dropping ids the catalog no longer has.
    #[must_use]
    pub fn symbol_recents(&self, kind: Option<Kind>) -> Vec<SymbolId> {
        if self.settings.symbols_recent.is_empty() {
            return Vec::new();
        }
        Recents::from_saved(&self.settings.symbols_recent, Catalog::get()).list(kind)
    }

    fn copy_symbol_data(&mut self, id: &SymbolId, data: Data) -> bool {
        let Some(payload) = Payload::new(vec![data]) else {
            return false;
        };
        if self.services.clipboard.write_persistent(&payload).is_err() {
            self.show_toast(clipboard_busy());
            return false;
        }
        self.note_activity();
        self.show_toast(Toast {
            kind: ToastKind::Success,
            message: ToastMessage::Copied,
        });
        self.remember_symbol(id);
        true
    }

    /// Pushes `id` to the front of the saved recents, stamped by the injected clock, to be
    /// written after the usual quiet time.
    fn remember_symbol(&mut self, id: &SymbolId) {
        let now = self.clock.now().as_millisecond();
        let mut recents = Recents::from_saved(&self.settings.symbols_recent, Catalog::get());
        recents.push(id.clone(), now);
        let saved = recents.to_saved();
        self.update_settings(|s| s.symbols_recent = saved);
    }
}

fn clipboard_busy() -> Toast {
    Toast {
        kind: ToastKind::Danger,
        message: ToastMessage::ClipboardBusy,
    }
}
