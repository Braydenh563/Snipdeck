//! Multi-select for the Main window.
//!
//! Slint has no loops, so the window cannot walk its rows to select a range. It reports what the user did
//! (`select-requested(index, mode)`, `select-all-requested`, `clear-selection-requested`) and the host answers by
//! writing `rows[i].selected` and `selected-count`. [`Selection`] is that logic, free of Slint; [`bind`] wires a
//! window and its row model to one.

use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

use slint::{ComponentHandle, Model, VecModel};

use crate::{MainRow, MainWindow, SelectMode};

/// Which rows are selected, plus the anchor a Shift range starts from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    selected: BTreeSet<usize>,
    anchor: usize,
}

impl Selection {
    /// An empty selection.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies a click or a key on row `index` of `len` rows.
    ///
    /// `Replace` selects only that row (and makes it the anchor), `Toggle` flips it (Ctrl+click, Ctrl+Space) and
    /// `Extend` selects everything from the anchor to it (Shift+click, Shift+arrow). An index past the end is
    /// ignored.
    pub fn select(&mut self, index: usize, mode: SelectMode, len: usize) {
        if index >= len {
            return;
        }
        match mode {
            SelectMode::Replace => {
                self.selected.clear();
                self.selected.insert(index);
                self.anchor = index;
            }
            SelectMode::Toggle => {
                if !self.selected.remove(&index) {
                    self.selected.insert(index);
                }
                self.anchor = index;
            }
            SelectMode::Extend => {
                let anchor = self.anchor.min(len - 1);
                let (lo, hi) = if anchor <= index {
                    (anchor, index)
                } else {
                    (index, anchor)
                };
                self.selected = (lo..=hi).collect();
            }
        }
    }

    /// Selects all `len` rows (Ctrl+A).
    pub fn select_all(&mut self, len: usize) {
        self.selected = (0..len).collect();
    }

    /// Deselects everything (Esc, a new view).
    pub fn clear(&mut self) {
        self.selected.clear();
        self.anchor = 0;
    }

    /// Drops rows that no longer exist after the list changed to `len` rows.
    pub fn clamp(&mut self, len: usize) {
        self.selected.retain(|&i| i < len);
        self.anchor = self.anchor.min(len.saturating_sub(1));
    }

    /// Whether row `index` is selected.
    #[must_use]
    pub fn contains(&self, index: usize) -> bool {
        self.selected.contains(&index)
    }

    /// How many rows are selected.
    #[must_use]
    pub fn len(&self) -> usize {
        self.selected.len()
    }

    /// Whether nothing is selected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    /// The selected row indices, ascending.
    pub fn indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.selected.iter().copied()
    }

    /// Writes the selection into `rows[i].selected`, touching only rows that change (a model reset would
    /// rebuild every visible card and lose the scroll position). Returns the number of selected rows.
    pub fn apply(&self, rows: &VecModel<MainRow>) -> usize {
        for i in 0..rows.row_count() {
            if let Some(mut row) = rows.row_data(i) {
                let want = self.contains(i);
                if row.selected != want {
                    row.selected = want;
                    rows.set_row_data(i, row);
                }
            }
        }
        self.len()
    }
}

/// Connects `window`'s selection callbacks to a new [`Selection`] over `rows` and returns it, so the host can
/// read the selected indices (for the bulk actions) and reset it when the list changes.
///
/// After replacing the rows or changing the section, call [`Selection::clear`] or [`Selection::clamp`], then
/// [`Selection::apply`] and `set_selected_count` yourself; this only handles the user's own gestures.
pub fn bind(window: &MainWindow, rows: &Rc<VecModel<MainRow>>) -> Rc<RefCell<Selection>> {
    let selection = Rc::new(RefCell::new(Selection::new()));
    let weak = window.as_weak();
    let refresh = {
        let selection = selection.clone();
        let rows = rows.clone();
        move || {
            let count = selection.borrow().apply(&rows);
            if let Some(window) = weak.upgrade() {
                window.set_selected_count(i32::try_from(count).unwrap_or(i32::MAX));
            }
        }
    };
    {
        let selection = selection.clone();
        let rows = rows.clone();
        let refresh = refresh.clone();
        window.on_select_requested(move |index, mode| {
            if let Ok(index) = usize::try_from(index) {
                selection.borrow_mut().select(index, mode, rows.row_count());
                refresh();
            }
        });
    }
    {
        let selection = selection.clone();
        let rows = rows.clone();
        let refresh = refresh.clone();
        window.on_select_all_requested(move || {
            selection.borrow_mut().select_all(rows.row_count());
            refresh();
        });
    }
    {
        let selection = selection.clone();
        window.on_clear_selection_requested(move || {
            selection.borrow_mut().clear();
            refresh();
        });
    }
    selection
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picked(sel: &Selection) -> Vec<usize> {
        sel.indices().collect()
    }

    #[test]
    fn replace_selects_one_row_and_sets_the_anchor() {
        let mut sel = Selection::new();
        sel.select(2, SelectMode::Replace, 5);
        sel.select(4, SelectMode::Replace, 5);
        assert_eq!(picked(&sel), [4]);
    }

    #[test]
    fn toggle_adds_and_removes_without_touching_the_rest() {
        let mut sel = Selection::new();
        sel.select(1, SelectMode::Replace, 5);
        sel.select(3, SelectMode::Toggle, 5);
        assert_eq!(picked(&sel), [1, 3]);
        sel.select(1, SelectMode::Toggle, 5);
        assert_eq!(picked(&sel), [3]);
    }

    #[test]
    fn extend_selects_the_range_from_the_anchor_in_both_directions() {
        let mut sel = Selection::new();
        sel.select(2, SelectMode::Replace, 8);
        sel.select(5, SelectMode::Extend, 8);
        assert_eq!(picked(&sel), [2, 3, 4, 5]);
        // The anchor stays at 2: extending back past it flips the range.
        sel.select(0, SelectMode::Extend, 8);
        assert_eq!(picked(&sel), [0, 1, 2]);
    }

    #[test]
    fn extend_after_a_toggle_starts_from_the_toggled_row() {
        let mut sel = Selection::new();
        sel.select(0, SelectMode::Replace, 8);
        sel.select(6, SelectMode::Toggle, 8);
        sel.select(4, SelectMode::Extend, 8);
        assert_eq!(picked(&sel), [4, 5, 6]);
    }

    #[test]
    fn out_of_range_indices_are_ignored_and_clamp_drops_vanished_rows() {
        let mut sel = Selection::new();
        sel.select(9, SelectMode::Replace, 5);
        assert!(sel.is_empty());
        sel.select_all(5);
        assert_eq!(sel.len(), 5);
        sel.clamp(3);
        assert_eq!(picked(&sel), [0, 1, 2]);
        sel.clear();
        assert!(sel.is_empty());
    }
}
