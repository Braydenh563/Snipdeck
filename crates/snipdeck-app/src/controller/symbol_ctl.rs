//! The Symbols tab's half of the [`Controller`]: using a symbol (paste or copy), adding it to a
//! list, copying or saving an icon, and the recents. Slint-free: the tab's callbacks call these
//! and draw the toast each one raises.
//!
//! A symbol is not an item: it has no [`ItemId`](snipdeck_core::ItemId), is never counted in the
//! store, and its text goes through the same clipboard pipeline a snippet uses
//! ([`snipdeck_platform::pipeline::use_item`]) with the same refocus-and-wait as an item.

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    rc::Rc,
};

use snipdeck_core::{ItemKind, UseMode};
use snipdeck_platform::{
    file_dialog::{FilePicker, SaveKind},
    payload::{Data, Payload},
    pipeline::{self, Clipboard, Env, Keyboard, Timing},
    route::Route,
};
use snipdeck_symbols::{Catalog, Kind, Recents, SymbolId};

use super::{Controller, Effect, Library, PanelMenu, REFOCUS_WAIT, UseResult, failed};
use crate::{
    host::ClipboardText,
    item_editor::Origin,
    toast::{self, Toast, ToastKind, ToastMessage},
    usecase::UseError,
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

/// One row of the context menu of a Symbols cell, in the order it is drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolMenuEntry {
    /// Paste the symbol into the Target (an icon is copied as a picture instead).
    Paste,
    /// Copy the symbol.
    Copy,
    /// Save the symbol as an item in the list the Panel shows.
    AddToList {
        /// The name of that list; `None` when the Panel shows no list.
        list: Option<String>,
        /// Emoji and kaomoji can be added; an icon is not text (the row is greyed).
        enabled: bool,
    },
    /// A line between groups.
    Separator,
    /// Icons only: the drawing's SVG text onto the clipboard.
    CopySvg,
    /// Icons only: the picture onto the clipboard.
    CopyPng,
    /// Icons only: save the SVG file (only where the machine has a file dialog).
    SaveAsSvg,
    /// Icons only: save a PNG file (only where the machine has a file dialog).
    SaveAsPng,
}

/// What choosing a row of the symbol menu leaves for the host to do.
#[derive(Debug)]
pub enum SymbolChoice {
    /// Done; these effects follow (the item use path's: close the Panel after a paste, say).
    Done(Vec<Effect>),
    /// "Save as...": run [`prepare_icon_save`](Controller::prepare_icon_save) for this icon,
    /// ask the dialog with no borrow held, then `finish_icon_save`.
    Save(SymbolId, IconExport),
    /// Nothing was chosen (the row is not there, or greyed).
    None,
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
        match pipeline::use_item(&mut env, &payload, mode, &route) {
            Ok(outcome) => {
                self.note_activity();
                self.show_toast(toast::after_use(outcome, &route, pasted_into.as_deref()));
                self.remember_symbol(id);
                UseResult::Done(outcome)
            }
            Err(error) => {
                self.show_toast(toast::after_error(&UseError::Platform(error)));
                UseResult::Failed
            }
        }
    }

    /// "Add to list": saves an emoji or kaomoji as a Text item at the end of the list `origin`
    /// shows (the same path as the quick-add box, so its toast with Undo and Edit appears, and
    /// "no list" says so). Returns whether an item was added. Icons are not text: they get the
    /// "Icons can't be added to a list yet" toast and `false`.
    pub fn add_symbol_to_list(&mut self, id: &SymbolId, origin: Origin) -> bool {
        let Some(text) = Catalog::get().get_symbol(id).and_then(|s| s.text.clone()) else {
            if matches!(id, SymbolId::Icon(_)) {
                self.show_toast(Toast {
                    kind: ToastKind::Neutral,
                    message: ToastMessage::IconNotAddable,
                });
            }
            return false;
        };
        let added = self.quick_add(&text, Some(ItemKind::Text), false, origin);
        if added {
            self.remember_symbol(id);
        }
        added
    }

    /// A click or Enter on a symbol: `mode` pastes or copies an emoji or kaomoji; an icon is
    /// copied as a picture in `ink` whichever the mode (it has no text).
    pub fn activate_symbol(&mut self, id: &SymbolId, mode: UseMode, ink: [u8; 4]) -> UseResult {
        if matches!(id, SymbolId::Icon(_)) {
            if self.copy_icon_png(id, ink) {
                UseResult::Done(snipdeck_core::UseOutcome::Copied)
            } else {
                UseResult::Failed
            }
        } else {
            self.use_symbol(id, mode)
        }
    }

    /// Right-click, the Menu key or Shift+F10 on a symbol: opens its context menu.
    pub fn open_symbol_menu(&mut self, id: SymbolId) {
        self.symbol_menu = Some(id);
        self.menu = PanelMenu::Symbol;
    }

    /// Whether the symbol menu is the one open.
    #[must_use]
    pub fn symbol_menu_open(&self) -> bool {
        self.menu == PanelMenu::Symbol && self.symbol_menu.is_some()
    }

    /// The rows of the symbol menu for `id`.
    #[must_use]
    pub fn symbol_menu_entries(&self, id: &SymbolId) -> Vec<SymbolMenuEntry> {
        let icon = matches!(id, SymbolId::Icon(_));
        let list = self
            .model
            .tabs()
            .iter()
            .find(|t| t.active)
            .map(|t| t.name.clone());
        let mut entries = vec![
            SymbolMenuEntry::Paste,
            SymbolMenuEntry::Copy,
            SymbolMenuEntry::AddToList {
                list,
                enabled: !icon,
            },
        ];
        if icon {
            entries.extend([
                SymbolMenuEntry::Separator,
                SymbolMenuEntry::CopySvg,
                SymbolMenuEntry::CopyPng,
            ]);
            if self.file_picker().is_some() {
                entries.extend([SymbolMenuEntry::SaveAsSvg, SymbolMenuEntry::SaveAsPng]);
            }
        }
        entries
    }

    /// The open symbol menu's rows, for the view.
    pub(super) fn symbol_menu_view(&self) -> Option<Vec<SymbolMenuEntry>> {
        let id = self
            .symbol_menu
            .as_ref()
            .filter(|_| self.menu == PanelMenu::Symbol)?;
        Some(self.symbol_menu_entries(id))
    }

    /// The user chose row `index` of the symbol menu (`ink` is the theme's text colour, for a
    /// picture). The menu closes first. "Save as" is handed back, not run: its dialog must run
    /// with no borrow of the controller held.
    pub fn choose_symbol_menu(&mut self, index: usize, ink: [u8; 4]) -> SymbolChoice {
        let Some(id) = self.symbol_menu.take() else {
            return SymbolChoice::None;
        };
        let entry = self.symbol_menu_entries(&id).get(index).cloned();
        self.menu = PanelMenu::None;
        let Some(entry) = entry else {
            return SymbolChoice::None;
        };
        match entry {
            SymbolMenuEntry::Paste | SymbolMenuEntry::Copy => {
                let mode = if entry == SymbolMenuEntry::Paste {
                    UseMode::Paste
                } else {
                    UseMode::Copy
                };
                let used = self.activate_symbol(&id, mode, ink);
                SymbolChoice::Done(self.after_use(&used))
            }
            SymbolMenuEntry::AddToList { enabled: true, .. } => {
                self.add_symbol_to_list(&id, Origin::Panel);
                SymbolChoice::Done(Vec::new())
            }
            SymbolMenuEntry::CopySvg => {
                self.copy_icon_svg(&id);
                SymbolChoice::Done(Vec::new())
            }
            SymbolMenuEntry::CopyPng => {
                self.copy_icon_png(&id, ink);
                SymbolChoice::Done(Vec::new())
            }
            SymbolMenuEntry::SaveAsSvg => SymbolChoice::Save(id, IconExport::Svg),
            SymbolMenuEntry::SaveAsPng => SymbolChoice::Save(
                id,
                IconExport::Png {
                    px: ICON_COPY_PX,
                    rgba: ink,
                },
            ),
            SymbolMenuEntry::AddToList { enabled: false, .. } | SymbolMenuEntry::Separator => {
                SymbolChoice::None
            }
        }
    }

    /// "Copy SVG": the icon's markup as text, `fill="currentColor"` kept. `false` for anything
    /// that is not an icon, or when the clipboard is busy (a toast says so).
    pub fn copy_icon_svg(&mut self, id: &SymbolId) -> bool {
        let Some(svg) = Catalog::get().icon_svg(id) else {
            return false;
        };
        self.copy_symbol_data(id, Data::Text(svg))
    }

    /// "Copy PNG": the icon as a `ICON_COPY_PX` picture in `rgba` (the text colour of the
    /// theme in force), transparent behind. `false` for anything that is not an icon, or when
    /// the clipboard is busy.
    pub fn copy_icon_png(&mut self, id: &SymbolId, rgba: [u8; 4]) -> bool {
        let Some(png) = Catalog::get().icon_png(id, ICON_COPY_PX, rgba) else {
            return false;
        };
        self.copy_symbol_data(id, Data::Png(png))
    }

    /// First half of "Save as...": renders the file and finds the dialog. The caller then asks
    /// `IconSave::ask` with the controller released (the Save dialog runs its own message loop,
    /// during which timers and hotkeys reach the controller) and hands the answer to
    /// [`finish_icon_save`](Self::finish_icon_save).
    ///
    /// # Errors
    ///
    /// `SaveOutcome::Unavailable` when the machine has no file dialog, `SaveOutcome::Failed`
    /// when `id` is not an icon or cannot be rendered.
    pub fn prepare_icon_save(
        &self,
        id: &SymbolId,
        export: IconExport,
    ) -> Result<IconSave, SaveOutcome> {
        let SymbolId::Icon(name) = id else {
            return Err(SaveOutcome::Failed);
        };
        let Some(picker) = self.file_picker() else {
            return Err(SaveOutcome::Unavailable);
        };
        let catalog = Catalog::get();
        let (kind, bytes) = match export {
            IconExport::Svg => (SaveKind::Svg, catalog.icon_svg(id).map(String::into_bytes)),
            IconExport::Png { px, rgba } => (SaveKind::Png, catalog.icon_png(id, px, rgba)),
        };
        let Some(bytes) = bytes else {
            return Err(SaveOutcome::Failed);
        };
        Ok(IconSave {
            id: id.clone(),
            name: name.clone(),
            kind,
            bytes,
            picker,
        })
    }

    /// Second half of "Save as...": writes the file the dialog answered with (the kind's extension
    /// appended unless the name already ends with it), toasts "Saved" and remembers the icon. A cancel writes nothing
    /// and says nothing; a dialog or write error is logged and toasted.
    pub fn finish_icon_save(
        &mut self,
        save: &IconSave,
        answer: Result<Option<PathBuf>, snipdeck_platform::error::Error>,
    ) -> SaveOutcome {
        let path = match answer {
            Ok(Some(path)) => with_extension_if_missing(path, save.kind.extension()),
            Ok(None) => return SaveOutcome::Cancelled,
            Err(error) => {
                crate::trace!("the Save dialog failed: {error}");
                self.show_toast(failed());
                return SaveOutcome::Failed;
            }
        };
        if let Err(error) = write_new_file(&path, &save.bytes) {
            crate::trace!("writing the saved icon failed: {error}");
            self.show_toast(failed());
            return SaveOutcome::Failed;
        }
        let file = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        self.show_toast(Toast {
            kind: ToastKind::Success,
            message: ToastMessage::IconSaved { name: file },
        });
        self.remember_symbol(&save.id);
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
        if let Err(error) = self.services.clipboard.write_persistent(&payload) {
            self.show_toast(toast::after_error(&UseError::Platform(error)));
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

/// An icon the user is saving: rendered, with the dialog that will ask where. Holds no borrow
/// of the controller.
pub struct IconSave {
    id: SymbolId,
    name: String,
    kind: SaveKind,
    bytes: Vec<u8>,
    picker: Rc<dyn FilePicker>,
}

impl IconSave {
    /// Shows the Save dialog (the icon's name filled in) and waits. `owner` is the native window
    /// handle the dialog belongs to. Call it with no borrow of the controller held.
    ///
    /// # Errors
    ///
    /// The dialog could not be shown.
    pub fn ask(
        &self,
        owner: Option<isize>,
    ) -> Result<Option<PathBuf>, snipdeck_platform::error::Error> {
        self.picker.save(self.kind, &self.name, owner)
    }
}

/// `path` with `.ext` appended unless its name already ends with it (any case). A name the user
/// accepted is never rewritten: "logo.v2" becomes "logo.v2.svg", "x.svg" stays.
fn with_extension_if_missing(path: PathBuf, ext: &str) -> PathBuf {
    let ends_with = path
        .extension()
        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case(ext));
    if ends_with {
        return path;
    }
    let mut name = path.into_os_string();
    name.push(".");
    name.push(ext);
    PathBuf::from(name)
}

/// Writes `bytes` to a temporary file next to `path`, then renames it over `path`: a failure
/// leaves the old file (if any) whole.
fn write_new_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let folder = path.parent().filter(|p| !p.as_os_str().is_empty());
    let folder = folder.unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_name()
        .map_or_else(|| "icon".to_owned(), |n| n.to_string_lossy().into_owned());
    let (mut file, temp) = crate::settings_store::create_temp(folder, &stem)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    match written.and_then(|()| fs::rename(&temp, path)) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temp);
            Err(error)
        }
    }
}
