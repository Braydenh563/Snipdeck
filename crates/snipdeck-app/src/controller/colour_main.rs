//! Copy a saved colour in any format from where it is stored: the Main window's Colour block and
//! its "Paste as…" for a colour row, and the Picker's "Paste as" and "Open in Colours" for a
//! colour result. The colour is read from the item (a Text or Code item whose whole text is one
//! strict literal, never a Sensitive one), written by [`snipdeck_colour`] and sent through the
//! same use path as the Panel's Paste as, so the toast, the recents and the use count are the
//! same. Docs: `docs/reviews/colour-spec.md` (D6a).

use snipdeck_colour::{Format, Rgba8};
use snipdeck_core::{ItemId, UseMode};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};

use super::{Controller, Library, UseResult, colour_ctl::colour_of};
use crate::{
    host::ClipboardText,
    main_window::{Key, Section},
};

/// The Colour block under the Main window's list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColourBlock {
    /// The colour, for the swatch.
    pub colour: Rgba8,
    /// Every format in [`Format::ALL`] order with the colour written that way.
    pub formats: Vec<(Format, String)>,
}

/// The colour written in every format, in [`Format::ALL`] order.
#[must_use]
pub fn colour_formats(c: Rgba8) -> Vec<(Format, String)> {
    Format::ALL
        .iter()
        .map(|f| (*f, snipdeck_colour::format(c, *f)))
        .collect()
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    // ---- Main window ---------------------------------------------------------------------------

    /// The colour item on Main row `index` (not in the Trash).
    fn main_colour_row(&self, index: usize) -> Option<(ItemId, Rgba8)> {
        if self.main.section() == Section::Trash {
            return None;
        }
        let Key::Item(id) = *self.main.keys().get(index)? else {
            return None;
        };
        let item = self.main.item(id)?;
        colour_of(item).map(|c| (id, c))
    }

    /// The Colour block for the cursor row: shown while exactly one row (or none) is selected and
    /// the cursor is on a colour item.
    pub(super) fn main_colour_block(&self) -> Option<ColourBlock> {
        if self.main.selected_count() > 1 {
            return None;
        }
        let (_, c) = self.main_colour_row(self.main.cursor())?;
        Some(ColourBlock {
            colour: c,
            formats: colour_formats(c),
        })
    }

    /// The formats row `index` offers in "Paste as…" (empty when it is not a colour row).
    #[must_use]
    pub fn main_colour_choices(&self, index: usize) -> Vec<(Format, String)> {
        self.main_colour_row(index)
            .map(|(_, c)| colour_formats(c))
            .unwrap_or_default()
    }

    /// A Colour block button: copies the colour on row `index` written as `format_index` (an
    /// index into [`Format::ALL`]). One click copies; the usual "Copied" toast appears.
    pub fn main_colour_copy(&mut self, index: usize, format_index: usize) -> Option<UseResult> {
        self.main_fresh();
        let (id, c) = self.main_colour_row(index)?;
        let format = Format::ALL.get(format_index).copied()?;
        Some(self.use_colour_item(id, c, format, UseMode::Copy))
    }

    /// The "Paste as…" chooser of a colour row picked format `format_index`: pasted, or copied
    /// when Shift was held. The chooser was opened for the item `main_action(PasteAs)` stored.
    pub fn main_colour_paste_as(&mut self, format_index: usize, copy: bool) -> Option<UseResult> {
        self.main_fresh();
        let id = self.main.chooser().iter().find_map(|k| match k {
            Key::Item(id) => Some(*id),
            _ => None,
        });
        self.main.set_chooser(Vec::new());
        let id = id?;
        let mode = if copy { UseMode::Copy } else { UseMode::Paste };
        Some(self.use_colour_item_as(id, format_index, mode))
    }

    /// "Open in Colours" on Main row `index`: the Panel comes forward on the Colours tab with the
    /// colour current. Nothing happens while the tab is switched off.
    pub fn main_open_in_colours(&mut self, index: usize) {
        self.main_fresh();
        if !self.settings.show_colours_tab {
            return;
        }
        if let Some((id, _)) = self.main_colour_row(index) {
            let effects = self.open_item_in_colours(id);
            self.pending.extend(effects);
        }
    }

    // ---- Both windows --------------------------------------------------------------------------

    /// Pastes or copies colour item `id` written as `format_index` (an index into
    /// [`Format::ALL`]). Fails when the item is no longer a colour.
    pub fn use_colour_item_as(
        &mut self,
        id: ItemId,
        format_index: usize,
        mode: UseMode,
    ) -> UseResult {
        let (Some(c), Some(format)) = (
            self.colour_of_item(id),
            Format::ALL.get(format_index).copied(),
        ) else {
            return UseResult::Failed;
        };
        self.use_colour_item(id, c, format, mode)
    }

    // ---- The Picker ----------------------------------------------------------------------------

    /// The colour on Picker result row `index`.
    fn picker_colour(&self, index: usize) -> Option<Rgba8> {
        let id = self.picker.as_ref()?.item_at(index)?;
        self.colour_of_item(id)
    }

    /// The formats the Picker's "Paste as" offers for result row `index` (empty for a row that is
    /// not a colour).
    #[must_use]
    pub fn picker_colour_choices(&self, index: usize) -> Vec<(Format, String)> {
        self.picker_colour(index)
            .map(colour_formats)
            .unwrap_or_default()
    }

    /// "Open in Colours" on Picker result row `index`: the Picker closes without using anything
    /// and the Panel comes forward on the Colours tab. False (and the Picker stays) when the row
    /// is not a colour or the tab is switched off.
    pub fn picker_open_in_colours(&mut self, index: usize) -> bool {
        if !self.settings.show_colours_tab {
            return false;
        }
        let Some(id) = self.picker.as_ref().and_then(|p| p.item_at(index)) else {
            return false;
        };
        if self.colour_of_item(id).is_none() {
            return false;
        }
        self.picker = None;
        let effects = self.open_item_in_colours(id);
        self.pending.extend(effects);
        true
    }
}
