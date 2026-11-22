//! The Colours tab: the current colour, using it (paste or copy a format), the recents, and saving
//! colours as Text items (to the home "Colours" list or any list). The maths is
//! [`snipdeck_colour`]; this file only joins it to the clipboard, the settings and the library.
//! Docs: `docs/reviews/colour-spec.md` (D2, D6a, D7, D9).

use snipdeck_colour::{
    ColourRecents, Format, Rgba8, Scheme, css_variables, literal, saved_text, title_for,
};
use snipdeck_core::{
    Content, Format as ItemFormat, Formats, Inline, Item, ItemId, ItemKind, List, ListId, Name,
    Placement, UseMode,
};
use snipdeck_platform::{
    payload::{Data, Payload},
    pipeline::{Clipboard, Keyboard, Timing},
};
use snipdeck_store::Trashable;

use super::{Controller, Effect, HomeCache, Library, UseResult, failed, success};
use crate::host::ClipboardText;
use crate::{
    capture::Destination,
    toast::{self, Toast, ToastKind, ToastMessage},
    usecase::UseError,
};

/// How many saved colours the tab's "Saved" strip shows.
const SAVED_STRIP: usize = 24;

/// Where a colour is saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveTarget {
    /// The home list, made on the first save.
    Home,
    /// Any list (the chooser of "Save to…").
    List(ListId),
    /// A new list at the end of the board the Panel shows, named by the translated "New list".
    NewList,
}

/// One list "Save to…" offers: any list of any board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveList {
    /// The list.
    pub id: ListId,
    /// Its name.
    pub name: String,
    /// The name of its board.
    pub board: String,
    /// More than one board exists, so the board is worth showing next to the name.
    pub several_boards: bool,
}

/// What the Colours tab shows, as plain data.
#[derive(Debug, Clone, PartialEq)]
pub struct ColourView {
    /// The colour being looked at.
    pub current: Rgba8,
    /// The colour it was before the last change (a click on it restores it).
    pub previous: Option<Rgba8>,
    /// The second colour of the contrast section.
    pub other: Rgba8,
    /// The default format (never `Name`).
    pub format: Format,
    /// The scheme laid out.
    pub scheme: Scheme,
    /// Recently used, newest first (at most 24).
    pub recents: Vec<Rgba8>,
    /// The newest colours of the home list, newest first.
    pub saved: Vec<Rgba8>,
    /// The picker is open.
    pub picker_open: bool,
    /// Wheel or square.
    pub style: snipdeck_colour::PickerStyle,
    /// The channel set the Values row edits.
    pub input_model: snipdeck_colour::InputModel,
    /// The home list exists, so "View list" has somewhere to go.
    pub has_saved_list: bool,
}

/// The colour `text` (`#rrggbb[aa]`) names, else `fallback`.
pub(super) fn colour_or(text: &str, fallback: &str) -> Rgba8 {
    Rgba8::from_hex(text)
        .or_else(|| Rgba8::from_hex(fallback))
        .unwrap_or(Rgba8::rgb(0x28, 0x70, 0xd4))
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// Everything the Colours tab draws.
    #[must_use]
    pub fn colour_view(&self) -> ColourView {
        ColourView {
            current: self.colour,
            previous: self.colour_previous,
            other: self.colour_other,
            format: self.settings.colour_format,
            scheme: self.settings.colour_scheme,
            recents: ColourRecents::from_saved(&self.settings.colours_recent).list(),
            saved: self.saved_colours().into_iter().map(|(_, c)| c).collect(),
            has_saved_list: self.home_list_id().is_some(),
            picker_open: self.settings.colour_adjust_open,
            style: self.settings.colour_picker_style,
            input_model: self.settings.colour_input_model,
        }
    }

    /// Sets the words the home list is named in the language shown ("Colours"); the controller
    /// has no `@tr`.
    pub fn set_colours_list_name(&mut self, name: &str) {
        name.clone_into(&mut self.colours_list_name);
        self.clear_colour_caches();
    }

    /// Makes `c` the colour being looked at. Not a use: no recents push.
    pub fn set_colour(&mut self, c: Rgba8) {
        if c != self.colour {
            self.colour_previous = Some(self.colour);
        }
        self.colour_gesture_from = None;
        self.colour = c;
        self.update_settings(|s| s.colour_current = c.to_hex());
    }

    /// Like [`set_colour`](Self::set_colour) for a drag in progress: the colour changes in memory
    /// only, with no settings update per pointer event. [`settle_colour`](Self::settle_colour)
    /// remembers it when the pointer is released.
    pub fn set_colour_live(&mut self, c: Rgba8) {
        self.colour_gesture_from.get_or_insert(self.colour);
        self.colour = c;
    }

    /// The pointer was released: the colour being looked at is remembered.
    pub fn settle_colour(&mut self) {
        let c = self.colour;
        if let Some(from) = self.colour_gesture_from.take()
            && from != c
        {
            self.colour_previous = Some(from);
        }
        self.update_settings(|s| s.colour_current = c.to_hex());
    }

    /// A click on the previous colour: it becomes current, and the colour it replaces becomes the
    /// previous one (so a second click goes back).
    pub fn restore_previous_colour(&mut self) {
        if let Some(previous) = self.colour_previous {
            self.set_colour(previous);
        }
    }

    /// Ctrl+V on the tab: the first colour literal in the clipboard text becomes current
    /// (`background: #ff0000;` and `"rgb(1, 2, 3)"` work). False when the clipboard has none.
    pub fn paste_colour(&mut self) -> bool {
        let found = self
            .services
            .clipboard
            .read_text()
            .and_then(|text| snipdeck_colour::extract(&text));
        let Some((c, _)) = found else {
            self.show_toast(Toast {
                kind: ToastKind::Neutral,
                message: ToastMessage::NoColourOnClipboard,
            });
            return false;
        };
        self.set_colour(c);
        true
    }

    /// Sets the second colour of the contrast section.
    pub fn set_contrast_with(&mut self, c: Rgba8) {
        self.colour_other = c;
        self.update_settings(|s| s.colour_contrast_with = c.to_hex());
    }

    /// The Swap button: current and other change places.
    pub fn swap_contrast(&mut self) {
        let (a, b) = (self.colour, self.colour_other);
        self.set_colour(b);
        self.set_contrast_with(a);
    }

    /// The default format. `Name` is lossy and is ignored.
    pub fn set_colour_format(&mut self, f: Format) {
        if f.can_be_default() {
            self.update_settings(|s| s.colour_format = f);
        }
    }

    /// The scheme the tab lays out.
    pub fn set_colour_scheme(&mut self, scheme: Scheme) {
        self.update_settings(|s| s.colour_scheme = scheme);
    }

    /// The picker disclosure was opened or closed.
    pub fn set_colour_adjust_open(&mut self, open: bool) {
        self.update_settings(|s| s.colour_adjust_open = open);
    }

    /// Square or wheel.
    pub fn set_colour_picker_style(&mut self, style: snipdeck_colour::PickerStyle) {
        self.update_settings(|s| s.colour_picker_style = style);
    }

    /// The channel set of the Values row.
    pub fn set_colour_input_model(&mut self, model: snipdeck_colour::InputModel) {
        self.update_settings(|s| s.colour_input_model = model);
    }

    /// Pastes or copies `c` written as `f` (a format row, a swatch's copy, Paste as), raises the
    /// usual toast and remembers the colour.
    pub fn use_colour(&mut self, c: Rgba8, f: Format, mode: UseMode) -> UseResult {
        // The Name row copies the keyword only when it is exactly `c`; the nearest one is another
        // colour (`use_nearest_name` is the explicit way to switch to it).
        let Some(text) = snipdeck_colour::copy_text(c, f) else {
            let near = snipdeck_colour::nearest_name(c);
            self.show_toast(Toast {
                kind: ToastKind::Neutral,
                message: ToastMessage::NameNotExact {
                    name: near.name.to_owned(),
                    hex: near.rgb.to_hex(),
                },
            });
            return UseResult::Failed;
        };
        let result = self.use_text(text, mode);
        if matches!(result, UseResult::Done(_)) {
            self.remember_colour(c);
        }
        result
    }

    /// "More formats…": the current colour as one of the code formats, copied (never pasted).
    pub fn copy_extra_format(&mut self, i: usize) -> UseResult {
        let Some(f) = snipdeck_colour::ExtraFormat::ALL.get(i) else {
            return UseResult::Failed;
        };
        let c = self.colour;
        let result = self.use_text(snipdeck_colour::extra_text(c, *f), UseMode::Copy);
        if matches!(result, UseResult::Done(_)) {
            self.remember_colour(c);
        }
        result
    }

    /// "Use royalblue (#4169e1)": the nearest keyword's colour replaces the current one. An explicit
    /// change of colour, never part of a copy.
    pub fn use_nearest_name(&mut self) {
        let named = snipdeck_colour::nearest_name(self.colour);
        self.set_colour(Rgba8 {
            a: self.colour.a,
            ..named.rgb
        });
    }

    /// Takes `c` out of the recents.
    pub fn forget_recent_colour(&mut self, c: Rgba8) {
        let mut recents = ColourRecents::from_saved(&self.settings.colours_recent);
        recents.remove(c);
        let saved = recents.to_saved();
        self.update_settings(|s| s.colours_recent = saved);
    }

    /// Forgets every recently used emoji, symbol and colour ("Clear recently used emoji and
    /// colours" in Settings), and says so. Search text is never stored, so there is none to clear.
    pub fn picker_recents_clear(&mut self) {
        self.update_settings(|s| {
            s.colours_recent.clear();
            s.symbols_recent.clear();
        });
        self.show_toast(success(ToastMessage::PickerRecentsCleared));
    }

    /// Pushes `c` to the front of the recents, stamped by the injected clock.
    fn remember_colour(&mut self, c: Rgba8) {
        let now = self.clock.now().as_millisecond();
        let mut recents = ColourRecents::from_saved(&self.settings.colours_recent);
        recents.push(c, now);
        let saved = recents.to_saved();
        self.update_settings(|s| s.colours_recent = saved);
    }

    /// A colour came back from the screen eyedropper. For the contrast check's other colour
    /// (`eyedrop_for_other`) it only sets that colour: no recent, no copy. Otherwise it becomes
    /// current and recent (so do the `also` colours collected on the way, oldest first), is copied
    /// in the default format when the setting says so (never pasted: after an overlay the Target is
    /// not trustworthy), and the Panel shows the tab. A copy and the HDR note are one toast.
    pub(crate) fn colour_picked(&mut self, c: Rgba8, hdr: bool, also: &[Rgba8]) -> Vec<Effect> {
        if std::mem::take(&mut self.eyedrop_for_other) {
            self.set_contrast_with(c);
            return self.back_from_eyedropper();
        }
        for earlier in also {
            self.remember_colour(*earlier);
        }
        self.set_colour(c);
        self.remember_colour(c);
        let mut copied = false;
        if self.settings.colour_copy_on_pick {
            let f = self.settings.colour_format;
            let text = snipdeck_colour::copy_text(c, f).unwrap_or_else(|| c.to_hex());
            copied = matches!(self.use_text(text, UseMode::Copy), UseResult::Done(_));
        }
        if hdr {
            let hex = c.to_hex();
            self.show_toast(Toast {
                kind: ToastKind::Neutral,
                message: if copied {
                    ToastMessage::EyedropHdrCopied { hex }
                } else {
                    ToastMessage::EyedropHdr { hex }
                },
            });
        }
        self.back_from_eyedropper()
    }

    // ---- Saving ----------------------------------------------------------------------------

    /// Saves `c` as a Text item at the end of `to` (the home list is made first when needed),
    /// with the default format's text, or the hex when that format would round the colour, and
    /// the keyword as its title when it is exactly one. The usual "Saved to {list}" toast with
    /// Undo and Edit appears. Returns whether it was saved.
    pub fn save_colour(&mut self, c: Rgba8, to: SaveTarget) -> bool {
        let destination = match to {
            SaveTarget::List(id) => {
                let Ok(list) = self.store.list(id) else {
                    self.show_toast(failed());
                    return false;
                };
                Destination {
                    id,
                    name: list.name.as_str().to_owned(),
                }
            }
            SaveTarget::NewList => {
                let Some(d) = self.new_list_for_save() else {
                    return false;
                };
                d
            }
            SaveTarget::Home => {
                let Some(d) = self.home_list() else {
                    return false;
                };
                d
            }
        };
        if let Some(existing) = self.saved_colour_in(destination.id, c) {
            self.show_toast(Toast {
                kind: ToastKind::Neutral,
                message: ToastMessage::AlreadySaved {
                    list: destination.name,
                },
            });
            if self.library_changed() {
                self.model.select_item(existing);
            }
            return false;
        }
        let made_list = self.colour_made_list.take();
        let text = saved_text(c, self.settings.colour_format);
        let mut item = Item::new(
            destination.id,
            title_for(c),
            ItemKind::Text,
            self.clock.now(),
        );
        item.preview = Item::preview_of(&text, false);
        let id = item.id;
        let stored = Inline::new(text.into_bytes())
            .map_err(|_| ())
            .and_then(|inline| {
                if take_injected_failure() {
                    Err(())
                } else {
                    Ok(inline)
                }
            })
            .and_then(|inline| {
                Formats::new(vec![ItemFormat::Text(Content::Inline(inline))]).map_err(|_| ())
            })
            .and_then(|formats| {
                self.store
                    .insert_item(&item, &formats, Placement::Last)
                    .map_err(|_| ())
            });
        if stored.is_err() {
            // A list made for this save would stay empty with nothing to Undo it, so it goes to the
            // Trash again (as an Undo of the first save does), and the next save makes it anew.
            if let Some(list) = made_list {
                let gone = list.to_string();
                // The toast below says the save failed; a failed trash is only logged by the refresh.
                let _ = self.store.trash(Trashable::List(list), self.clock.now());
                if self.settings.colour_save_list.as_deref() == Some(gone.as_str()) {
                    self.update_settings(|s| s.colour_save_list = None);
                }
            }
            self.library_changed();
            self.show_toast(failed());
            return false;
        }
        self.remember_colour(c);
        if self.library_changed() {
            self.model.select_item(id);
        }
        self.show_saved_toast_made(
            Toast {
                kind: ToastKind::Success,
                message: ToastMessage::Saved {
                    list: destination.name,
                },
            },
            id,
            made_list,
        );
        true
    }

    /// The home list that exists now: the remembered one, else the first list on any board that
    /// carries the home name (an import, or settings lost, leaves such a list behind).
    fn home_list_id(&self) -> Option<ListId> {
        if let HomeCache::Known(kept) = self.colour_home_cache.get() {
            return kept;
        }
        let found = self.find_home_list_id();
        self.colour_home_cache.set(HomeCache::Known(found));
        found
    }

    /// Forgets what the colour view kept: the saved colours and the home list.
    pub(super) fn clear_colour_caches(&self) {
        self.colour_saved_cache.take();
        self.colour_home_cache.set(HomeCache::Unknown);
    }

    fn find_home_list_id(&self) -> Option<ListId> {
        let remembered = self
            .settings
            .colour_save_list
            .as_deref()
            .and_then(|text| text.parse::<ListId>().ok())
            .and_then(|id| self.store.list(id).ok());
        if let Some(list) = remembered {
            return Some(list.id);
        }
        if self.colours_list_name.is_empty() {
            return None;
        }
        self.store.boards().ok()?.iter().find_map(|board| {
            self.store
                .lists(board.id)
                .ok()?
                .into_iter()
                .find(|l| l.name.as_str() == self.colours_list_name)
                .map(|l| l.id)
        })
    }

    /// The home list: the one [`home_list_id`](Self::home_list_id) finds, else a new list at the
    /// end of the board the Panel shows (or the first board). `None` (and a toast) when the
    /// library has no board.
    fn home_list(&mut self) -> Option<Destination> {
        if let Some(list) = self.home_list_id().and_then(|id| self.store.list(id).ok()) {
            let id = list.id.to_string();
            if self.settings.colour_save_list.as_deref() != Some(id.as_str()) {
                self.update_settings(|s| s.colour_save_list = Some(id));
            }
            return Some(Destination {
                id: list.id,
                name: list.name.as_str().to_owned(),
            });
        }
        let name = self.colours_list_name.clone();
        let made = self.make_list_for_save(&name)?;
        let id = made.id.to_string();
        self.update_settings(|s| s.colour_save_list = Some(id));
        self.clear_colour_caches();
        Some(made)
    }

    /// A new list for "Save to… New list…": named by the translated "New list" and the first
    /// free number, at the end of the board the Panel shows.
    fn new_list_for_save(&mut self) -> Option<Destination> {
        let board = self.save_board()?;
        let taken: Vec<String> = self
            .store
            .lists(board)
            .ok()?
            .iter()
            .map(|l| l.name.as_str().to_owned())
            .collect();
        let name =
            crate::starter::numbered_name(&self.starter.new_list, taken.iter().map(String::as_str));
        self.make_list_for_save(&name)
    }

    /// The board a list made for a save goes on: the one the Panel shows, else the first. A
    /// toast says so when the library has none.
    fn save_board(&mut self) -> Option<snipdeck_core::BoardId> {
        let boards = self.store.boards().ok().unwrap_or_default();
        let shown = self
            .model
            .board()
            .filter(|b| boards.iter().any(|x| x.id == *b))
            .or_else(|| boards.first().map(|b| b.id));
        if shown.is_none() {
            self.show_toast(super::neutral(ToastMessage::NoListToSave));
        }
        shown
    }

    /// Inserts a list called `name` at the end of the save board. A toast on failure.
    fn make_list_for_save(&mut self, name: &str) -> Option<Destination> {
        let board = self.save_board()?;
        let made = Name::new(name).map_err(|_| ()).and_then(|name| {
            let list = List::new(board, name, self.clock.now());
            self.store
                .insert_list(&list, Placement::Last)
                .map(|()| list)
                .map_err(|_| ())
        });
        if let Ok(list) = made {
            self.colour_made_list = Some(list.id);
            self.main.mark_stale();
            Some(Destination {
                id: list.id,
                name: list.name.as_str().to_owned(),
            })
        } else {
            self.show_toast(failed());
            None
        }
    }

    /// The colour an item holds when its whole trimmed text is one strict colour literal (a Text
    /// or Code item that is not sensitive). Read from the preview, so no blob is opened.
    #[must_use]
    pub fn colour_of_item(&self, id: ItemId) -> Option<Rgba8> {
        let item = self.store.item(id).ok()?;
        colour_of(&item)
    }

    /// Every list of every board, in board and list order, for the "Save to…" chooser.
    #[must_use]
    pub fn colour_save_targets(&self) -> Vec<SaveList> {
        let boards = self.store.boards().unwrap_or_default();
        let several_boards = boards.len() > 1;
        boards
            .iter()
            .flat_map(|board| {
                self.store
                    .lists(board.id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(move |l| SaveList {
                        id: l.id,
                        name: l.name.as_str().to_owned(),
                        board: board.name.as_str().to_owned(),
                        several_boards,
                    })
            })
            .collect()
    }

    /// Sets the title an item made from a picture without a file name gets, translated by the
    /// window layer.
    pub fn set_pasted_image_title(&mut self, title: &str) {
        title.clone_into(&mut self.pasted_image_title);
    }

    /// The colour items of the home list, newest first (a save appends), at most 24.
    #[must_use]
    pub fn saved_colours(&self) -> Vec<(ItemId, Rgba8)> {
        if let Some(kept) = self.colour_saved_cache.borrow().as_ref() {
            return kept.clone();
        }
        let found = self.read_saved_colours();
        *self.colour_saved_cache.borrow_mut() = Some(found.clone());
        found
    }

    /// The item of `list` that already holds exactly `c` (same RGBA).
    fn saved_colour_in(&self, list: ListId, c: Rgba8) -> Option<ItemId> {
        self.store
            .items(list)
            .ok()?
            .iter()
            .find(|i| colour_of(i) == Some(c))
            .map(|i| i.id)
    }

    /// Takes saved colour `id` to the Trash, with the usual Undo toast.
    pub fn delete_saved_colour(&mut self, id: ItemId) {
        self.trash_items(&[id]);
    }

    /// "Edit or rename…": the item editor opens on saved colour `id`.
    pub fn edit_saved_colour(&mut self, id: ItemId) -> Vec<Effect> {
        vec![Effect::EditItem(id)]
    }

    /// Renames saved colour `id`. An empty name restores the default title.
    pub fn rename_saved_colour(&mut self, id: ItemId, title: &str) {
        let Ok(mut item) = self.store.item(id) else {
            self.show_toast(failed());
            return;
        };
        let Some(c) = colour_of(&item) else {
            return;
        };
        let title = title.trim();
        item.title = if title.is_empty() {
            title_for(c)
        } else {
            title.to_owned()
        };
        item.updated_at = self.clock.now();
        if self.store.update_item(&item, None).is_ok() {
            self.library_changed();
        } else {
            self.show_toast(failed());
        }
    }

    fn read_saved_colours(&self) -> Vec<(ItemId, Rgba8)> {
        let Some(list) = self.home_list_id() else {
            return Vec::new();
        };
        self.store
            .items(list)
            .map(|items| {
                items
                    .iter()
                    .rev()
                    .filter_map(|i| colour_of(i).map(|c| (i.id, c)))
                    .take(SAVED_STRIP)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether the "Colours" list exists.
    #[must_use]
    pub fn has_saved_list(&self) -> bool {
        self.home_list_id().is_some()
    }

    /// "Saved colours": the Panel shows the home list, and the Colours tab steps aside.
    pub fn open_saved_colours(&mut self) -> Vec<Effect> {
        let Some(list) = self.home_list_id() else {
            return Vec::new();
        };
        if let Ok(l) = self.store.list(list)
            && self.model.select_board(&self.store, l.board_id).is_ok()
            && self.model.select_list(&self.store, list).is_ok()
        {
            return vec![Effect::ShowPanel, Effect::HideColours];
        }
        Vec::new()
    }

    /// Pastes or copies colour item `id` written as `f` (Paste as): like [`Self::use_colour`] and the
    /// item counts as used, so Most used and Recently used move.
    pub fn use_colour_item(&mut self, id: ItemId, c: Rgba8, f: Format, mode: UseMode) -> UseResult {
        let result = self.use_colour(c, f, mode);
        if matches!(result, UseResult::Done(_)) {
            let now = self.clock.now();
            if let Err(error) = self.store.record_use(id, now) {
                crate::trace!("counting a colour paste failed: {error}");
            } else {
                self.index_cache.note_use(id);
            }
            self.queue.used(id);
            self.refresh_logged("a paste");
        }
        result
    }

    /// "Open in Colours": the item's colour becomes current and the Panel shows the tab.
    pub fn open_item_in_colours(&mut self, id: ItemId) -> Vec<Effect> {
        let Some(c) = self.colour_of_item(id) else {
            return Vec::new();
        };
        self.set_colour(c);
        vec![Effect::ShowPanel, Effect::ShowColours]
    }

    /// The list holds at least one colour item.
    pub(crate) fn list_has_colours(&self, list: ListId) -> bool {
        self.store
            .items(list)
            .is_ok_and(|items| items.iter().any(|i| colour_of(i).is_some()))
    }

    /// "Copy as CSS variables": the colour items of `list` as a `:root { … }` block on the
    /// clipboard, in list order. Returns false (copying nothing) when the list holds none.
    pub fn copy_list_as_css(&mut self, list: ListId) -> bool {
        let Ok(items) = self.store.items(list) else {
            return false;
        };
        let pairs: Vec<(String, String)> = items
            .iter()
            .filter(|i| colour_of(i).is_some())
            .map(|i| (i.title.clone(), i.preview.trim().to_owned()))
            .collect();
        if pairs.is_empty() {
            return false;
        }
        let refs: Vec<(&str, &str)> = pairs
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let Some(payload) = Payload::new(vec![Data::Text(css_variables(&refs))]) else {
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
        true
    }
}

/// The colour a Text or Code item holds, if its whole text is one strict literal.
pub(super) fn colour_of(item: &Item) -> Option<Rgba8> {
    if item.sensitive || !matches!(item.kind, ItemKind::Text | ItemKind::Code) {
        return None;
    }
    literal(&item.preview)
}

#[cfg(test)]
thread_local! {
    static FAIL_INSERT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Makes the next colour save fail where its item would be stored (the store has no such hook).
#[cfg(test)]
pub(crate) fn fail_next_insert() {
    FAIL_INSERT.with(|f| f.set(true));
}

#[cfg(test)]
fn take_injected_failure() -> bool {
    FAIL_INSERT.with(|f| f.replace(false))
}

#[cfg(not(test))]
const fn take_injected_failure() -> bool {
    false
}
