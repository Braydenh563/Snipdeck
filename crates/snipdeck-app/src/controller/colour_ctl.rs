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

use super::{Controller, Effect, Library, UseResult, failed};
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
}

/// What the Colours tab shows, as plain data.
#[derive(Debug, Clone, PartialEq)]
pub struct ColourView {
    /// The colour being looked at.
    pub current: Rgba8,
    /// The second colour of the contrast section.
    pub other: Rgba8,
    /// The default format (never `Name`).
    pub format: Format,
    /// The scheme laid out.
    pub scheme: Scheme,
    /// Recently used, newest first (at most 24).
    pub recents: Vec<Rgba8>,
    /// The first colours of the home list, in list order.
    pub saved: Vec<Rgba8>,
    /// The picker is open.
    pub picker_open: bool,
    /// Wheel or square.
    pub style: snipdeck_colour::PickerStyle,
    /// The channel set the Values row edits.
    pub input_model: snipdeck_colour::InputModel,
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
            other: self.colour_other,
            format: self.settings.colour_format,
            scheme: self.settings.colour_scheme,
            recents: ColourRecents::from_saved(&self.settings.colours_recent).list(),
            saved: self.saved_colours().into_iter().map(|(_, c)| c).collect(),
            picker_open: self.settings.colour_adjust_open,
            style: self.settings.colour_picker_style,
            input_model: self.settings.colour_input_model,
        }
    }

    /// Sets the words the home list is named in the language shown ("Colours"); the controller
    /// has no `@tr`.
    pub fn set_colours_list_name(&mut self, name: &str) {
        name.clone_into(&mut self.colours_list_name);
    }

    /// Makes `c` the colour being looked at. Not a use: no recents push.
    pub fn set_colour(&mut self, c: Rgba8) {
        self.colour = c;
        self.update_settings(|s| s.colour_current = c.to_hex());
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
        let result = self.use_text(snipdeck_colour::format(c, f), mode);
        if matches!(result, UseResult::Done(_)) {
            self.remember_colour(c);
        }
        result
    }

    /// Takes `c` out of the recents.
    pub fn forget_recent_colour(&mut self, c: Rgba8) {
        let mut recents = ColourRecents::from_saved(&self.settings.colours_recent);
        recents.remove(c);
        let saved = recents.to_saved();
        self.update_settings(|s| s.colours_recent = saved);
    }

    /// Pushes `c` to the front of the recents, stamped by the injected clock.
    fn remember_colour(&mut self, c: Rgba8) {
        let now = self.clock.now().as_millisecond();
        let mut recents = ColourRecents::from_saved(&self.settings.colours_recent);
        recents.push(c, now);
        let saved = recents.to_saved();
        self.update_settings(|s| s.colours_recent = saved);
    }

    /// A colour came back from the screen eyedropper: it becomes current and recent, is copied in
    /// the default format when the setting says so (never pasted: after an overlay the Target is
    /// not trustworthy), and the Panel shows the tab.
    pub(crate) fn colour_picked(&mut self, c: Rgba8, hdr: bool) -> Vec<Effect> {
        self.set_colour(c);
        self.remember_colour(c);
        if self.settings.colour_copy_on_pick {
            self.use_text(
                snipdeck_colour::format(c, self.settings.colour_format),
                UseMode::Copy,
            );
        }
        if hdr {
            self.show_toast(Toast {
                kind: ToastKind::Neutral,
                message: ToastMessage::EyedropHdr { hex: c.to_hex() },
            });
        }
        vec![Effect::ShowPanel, Effect::ShowColours]
    }

    // ---- Saving ----------------------------------------------------------------------------

    /// Saves `c` as a Text item at the end of `to` (the home list is made first when needed),
    /// with the default format's text, or the hex when that format would round the colour, and
    /// the keyword as its title when it is exactly one. The usual "Saved to {list}" toast with
    /// Undo and Edit appears. Returns whether it was saved.
    pub fn save_colour(&mut self, c: Rgba8, to: SaveTarget) -> bool {
        let destination = match to {
            SaveTarget::List(id) => match self.store.list(id) {
                Ok(list) => Destination {
                    id,
                    name: list.name.as_str().to_owned(),
                },
                Err(_) => {
                    self.show_toast(failed());
                    return false;
                }
            },
            SaveTarget::Home => {
                let Some(d) = self.home_list() else {
                    return false;
                };
                d
            }
        };
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
                Formats::new(vec![ItemFormat::Text(Content::Inline(inline))]).map_err(|_| ())
            })
            .and_then(|formats| {
                self.store
                    .insert_item(&item, &formats, Placement::Last)
                    .map_err(|_| ())
            });
        if stored.is_err() {
            self.show_toast(failed());
            return false;
        }
        self.remember_colour(c);
        if self.library_changed() {
            self.model.select_item(id);
        }
        self.show_saved_toast(
            Toast {
                kind: ToastKind::Success,
                message: ToastMessage::Saved {
                    list: destination.name,
                },
            },
            id,
        );
        true
    }

    /// The home list: the remembered one while it exists, else a new list at the end of the board
    /// the Panel shows (or the first board). `None` (and a toast) when the library has no board.
    fn home_list(&mut self) -> Option<Destination> {
        let remembered = self
            .settings
            .colour_save_list
            .as_deref()
            .and_then(|text| text.parse::<ListId>().ok())
            .and_then(|id| self.store.list(id).ok());
        if let Some(list) = remembered {
            return Some(Destination {
                id: list.id,
                name: list.name.as_str().to_owned(),
            });
        }
        let board = self
            .model
            .board()
            .filter(|b| {
                self.store
                    .boards()
                    .is_ok_and(|all| all.iter().any(|x| x.id == *b))
            })
            .or_else(|| {
                self.store
                    .boards()
                    .ok()
                    .and_then(|all| all.first().map(|b| b.id))
            });
        let Some(board) = board else {
            self.show_toast(super::neutral(ToastMessage::NoListToSave));
            return None;
        };
        let made = Name::new(&self.colours_list_name)
            .map_err(|_| ())
            .and_then(|name| {
                let list = List::new(board, name, self.clock.now());
                self.store
                    .insert_list(&list, Placement::Last)
                    .map(|()| list)
                    .map_err(|_| ())
            });
        if let Ok(list) = made {
            let id = list.id.to_string();
            self.update_settings(|s| s.colour_save_list = Some(id));
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

    /// The lists of the board the Panel shows, for the "Save to…" chooser.
    #[must_use]
    pub fn colour_save_targets(&self) -> Vec<(ListId, String)> {
        self.model
            .tabs()
            .into_iter()
            .map(|t| (t.id, t.name))
            .collect()
    }

    /// The colour items of the home list, in list order, at most 24.
    #[must_use]
    pub fn saved_colours(&self) -> Vec<(ItemId, Rgba8)> {
        let Some(list) = self
            .settings
            .colour_save_list
            .as_deref()
            .and_then(|text| text.parse::<ListId>().ok())
        else {
            return Vec::new();
        };
        self.store
            .items(list)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|i| colour_of(i).map(|c| (i.id, c)))
                    .take(SAVED_STRIP)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// "Saved colours": the Panel shows the home list.
    pub fn open_saved_colours(&mut self) -> Vec<Effect> {
        let Some(list) = self
            .settings
            .colour_save_list
            .as_deref()
            .and_then(|text| text.parse::<ListId>().ok())
        else {
            return Vec::new();
        };
        if let Ok(l) = self.store.list(list)
            && self.model.select_board(&self.store, l.board_id).is_ok()
            && self.model.select_list(&self.store, list).is_ok()
        {
            return vec![Effect::ShowPanel];
        }
        Vec::new()
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
