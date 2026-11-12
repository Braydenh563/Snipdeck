//! The Panel's multi-selection and what acts on it: marking rows (Ctrl+click, Shift+click,
//! Ctrl+A, Shift+arrows), the bulk bar (Move to, Colour, Delete with Undo, Copy all, Clear) and
//! the item menu's second level (the lists to move to, the colours to give). Part of the
//! [`Controller`]; the window is the host's.

use snipdeck_core::{ItemId, ItemLabel, Placement, template::transform::Transform};
use snipdeck_platform::payload::{Data, Payload};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::{ItemRepo, TrashRepo, Trashable};

use super::{
    Controller, Library, PanelMenu, UseResult, failed, neutral,
    views::{ChooseEntry, ChooseLabel, ChooseMenu, ClearListView},
};
use crate::{
    host::ClipboardText,
    main_window::SelectMode,
    toast::{Toast, ToastKind, ToastMessage},
    usecase,
};

/// The colour labels in the order the editor and the Main window list them.
pub const LABELS: [ItemLabel; 8] = [
    ItemLabel::Blue,
    ItemLabel::Teal,
    ItemLabel::Green,
    ItemLabel::Amber,
    ItemLabel::Orange,
    ItemLabel::Red,
    ItemLabel::Violet,
    ItemLabel::Slate,
];

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    // ---- Marking ---------------------------------------------------------------------------------

    /// Ctrl+click (`Toggle`), Shift+click (`Extend`) or a plain click while a selection is on
    /// (`Replace`) on row `index`.
    ///
    /// In Select mode a plain click toggles the row, as Ctrl+click does: the checkbox is the click.
    pub fn panel_mark(&mut self, index: usize, mode: SelectMode) {
        let mode = if self.select_mode && mode == SelectMode::Replace {
            SelectMode::Toggle
        } else {
            mode
        };
        self.model.mark(index, mode);
    }

    /// Shift+Up / Shift+Down: the cursor moves by `delta` and the selection follows it.
    pub fn panel_extend_marks(&mut self, delta: isize) {
        self.model.extend_marks(delta);
    }

    /// Ctrl+A: every row shown.
    pub fn panel_mark_all(&mut self) {
        self.model.mark_all();
    }

    /// Esc, or the bulk bar's Clear: nothing is selected.
    pub fn panel_clear_marks(&mut self) {
        self.model.clear_marks();
    }

    // ---- The header's Select and Delete modes ---------------------------------------------------------

    /// The header's Select button (and its Done): rows get a checkbox and a click marks them.
    /// Turning it off drops the marks. It ends Delete mode.
    pub fn panel_select_mode(&mut self, on: bool) {
        if on {
            self.end_delete_mode();
        } else {
            self.model.clear_marks();
        }
        self.select_mode = on;
    }

    /// The header's Delete button (and its Done): each row gets a delete button. It ends Select
    /// mode. Turning it off drops the "Deleted · Undo" rows.
    pub fn panel_delete_mode(&mut self, on: bool) {
        if on {
            self.model.clear_marks();
            self.select_mode = false;
            self.delete_mode = true;
            self.delete_list = self.model.list().map(|l| l.id);
        } else {
            self.end_delete_mode();
        }
    }

    pub(super) fn end_delete_mode(&mut self) {
        self.delete_mode = false;
        self.delete_list = None;
        self.model.clear_ghosts();
    }

    /// Ends Delete mode when the shown list is not the one it began in (another tab, board or
    /// new list, a list deleted): its "Deleted · Undo" rows belong to that list.
    pub(super) fn guard_modes(&mut self) {
        if self.delete_mode && self.model.list().map(|l| l.id) != self.delete_list {
            self.end_delete_mode();
        }
    }

    /// Esc: leaves Select or Delete mode, whichever is on.
    pub fn panel_end_modes(&mut self) {
        self.panel_select_mode(false);
        self.end_delete_mode();
    }

    /// The row shown at `index`, unless it is a "Deleted · Undo" row: every action on a row
    /// goes through this, so the index the window sends is the one the model shows.
    pub(super) fn live_row(&self, index: usize) -> Option<crate::panel::Row> {
        self.model
            .rows()
            .into_iter()
            .nth(index)
            .filter(crate::panel::Row::is_item)
    }

    /// The row delete button in Delete mode: the item goes to the Trash (one history step) and
    /// the row collapses to "Deleted · Undo". The cursor moves to the next row (the previous at
    /// the end).
    pub fn panel_delete_row(&mut self, index: usize) {
        if !self.delete_mode {
            return;
        }
        let Some(row) = self.live_row(index) else {
            return;
        };
        if self.model.searching() {
            // A search shows no collapsed rows: the delete says so with the usual Undo toast.
            self.trash_with_undo(&[row.id]);
            return;
        }
        let now = self.clock.now();
        let done = self
            .store
            .atomically(|b| b.trash(Trashable::Item(row.id), now));
        if done.is_err() {
            self.show_toast(failed());
            return;
        }
        let step = self.record_step(
            crate::history::Action::Delete,
            vec![crate::history::Change::Trash(vec![Trashable::Item(row.id)])],
        );
        let shown = self.model.rows();
        let neighbour = shown
            .iter()
            .skip(index + 1)
            .find(|r| !r.deleted_pending)
            .or_else(|| shown.iter().take(index).rev().find(|r| !r.deleted_pending))
            .map(|r| r.id);
        self.model.add_ghost(index, step);
        if self.refresh_panel().is_err() {
            self.show_toast(failed());
        }
        if let Some(id) = neighbour {
            self.model.select_item(id);
        }
        self.focus_row = neighbour.and_then(|id| self.model.rows().iter().position(|r| r.id == id));
        self.focus_seq += 1;
    }

    /// "Undo" on a "Deleted · Undo" row: that one delete is taken back and the row is whole again.
    /// When the history has forgotten the step the item comes back from the Trash directly.
    pub fn panel_undo_row(&mut self, index: usize) {
        let Some(row) = self
            .model
            .rows()
            .into_iter()
            .nth(index)
            .filter(|r| r.deleted_pending)
        else {
            return;
        };
        let step = self.model.ghost_step(row.id).flatten();
        self.model.remove_ghost(row.id);
        if let Some(step) = step.filter(|s| self.history.can_undo(*s)) {
            self.undo_step(Some(step));
            return;
        }
        match self
            .store
            .atomically(|b| b.restore(Trashable::Item(row.id)))
        {
            Ok(()) => {
                self.show_toast(Toast {
                    kind: ToastKind::Success,
                    message: ToastMessage::Restored { count: 1 },
                });
                self.library_changed();
            }
            Err(error) => {
                self.show_toast(super::main_ctl::restore_failed(&error));
                self.refresh_logged("a failed undo of a deleted row");
            }
        }
    }

    // ---- Clear list… -----------------------------------------------------------------------------------

    /// The strip's "Clear list…" (Delete mode): asks first (nothing to ask about in an empty list or a search).
    pub fn panel_open_clear_list(&mut self) {
        if self.model.searching() {
            return;
        }
        if let Some(list) = self.model.list()
            && !self.model.list_item_ids().is_empty()
        {
            self.menu = PanelMenu::ClearList(list.id);
        }
    }

    /// The "Clear list…" confirmation's view: the list's name and how many items go.
    pub(super) fn clear_list_view(&self) -> Option<ClearListView> {
        let PanelMenu::ClearList(id) = self.menu else {
            return None;
        };
        let list = self.store.list(id).ok()?;
        Some(ClearListView {
            name: list.name.as_str().to_owned(),
            count: self.store.items(id).map_or(0, |items| items.len()),
        })
    }

    /// "Clear list" in the confirmation: every item of the list goes to the Trash as one history
    /// step, with an Undo toast.
    pub fn panel_clear_list(&mut self) {
        let PanelMenu::ClearList(id) = self.menu else {
            return;
        };
        self.menu = PanelMenu::None;
        let ids: Vec<ItemId> = self
            .store
            .items(id)
            .map(|items| items.iter().map(|i| i.id).collect())
            .unwrap_or_default();
        self.end_delete_mode();
        self.model.clear_marks();
        self.trash_with_undo(&ids);
    }

    /// Cancel, Esc or a click on the scrim: the confirmation closes and nothing is deleted.
    pub fn cancel_clear_list(&mut self) {
        if matches!(self.menu, PanelMenu::ClearList(_)) {
            self.menu = PanelMenu::None;
        }
    }

    // ---- The bulk bar ------------------------------------------------------------------------------

    /// The bulk bar's "Move to…": the lists of the board, for the marked rows.
    pub fn bulk_open_move(&mut self) {
        self.open_choose(PanelMenu::Move);
    }

    /// The bulk bar's "Colour": the labels, for the marked rows.
    pub fn bulk_open_colour(&mut self) {
        self.open_choose(PanelMenu::Colour);
    }

    fn open_choose(&mut self, menu: PanelMenu) {
        let ids = self.model.marked_ids();
        if ids.is_empty() {
            return;
        }
        self.menu_targets = ids;
        self.menu = menu;
    }

    /// The bulk bar's Delete, and the Delete key while rows are marked: the marked items go to
    /// the Trash with an Undo toast, no confirmation.
    pub fn bulk_delete(&mut self) {
        let ids = self.model.marked_ids();
        self.model.clear_marks();
        self.trash_with_undo(&ids);
    }

    /// The bulk bar's "Copy all": see `copy_items` for the marked items.
    pub fn bulk_copy(&mut self) {
        let ids = self.model.marked_ids();
        let _ = self.copy_items(&ids);
    }

    /// Ctrl+C on the Panel's list: the marked items, else the selected row.
    /// Returns the use of the one item copied, for [`after_use`](Self::after_use).
    pub fn panel_copy_selection(&mut self) -> Option<UseResult> {
        let mut ids = self.model.marked_ids();
        if ids.is_empty() {
            ids.extend(self.model.selected());
        }
        self.copy_items(&ids)
    }

    /// Copies `ids`. One item is copied the way its Copy button does (every format, and the
    /// Sensitive rules for clearing). Several are joined with a line between them, in order,
    /// as text; Sensitive items are left out (and said so by the count).
    pub(super) fn copy_items(&mut self, ids: &[ItemId]) -> Option<UseResult> {
        if let [id] = ids {
            return Some(self.use_item(*id, snipdeck_core::UseMode::Copy));
        }
        let mut parts = Vec::new();
        for id in ids.iter().copied() {
            let Ok(item) = self.store.item(id) else {
                continue;
            };
            if item.sensitive {
                continue;
            }
            if let Ok(Some(text)) = usecase::item_text(&self.store, id) {
                parts.push(text);
            }
        }
        if parts.is_empty() {
            self.show_toast(neutral(ToastMessage::NothingToSave));
            return None;
        }
        let copied = Payload::new(vec![Data::Text(parts.join("\n"))])
            .is_some_and(|payload| self.services.clipboard.write_persistent(&payload).is_ok());
        self.show_toast(if copied {
            Toast {
                kind: ToastKind::Success,
                message: ToastMessage::Copied,
            }
        } else {
            Toast {
                kind: ToastKind::Danger,
                message: ToastMessage::ClipboardBusy,
            }
        });
        None
    }

    /// Moves the items `ids` to the end of list `list` (named `name`).
    fn move_to_list(&mut self, ids: &[ItemId], list: snipdeck_core::ListId, name: String) {
        let movable: Vec<ItemId> = ids
            .iter()
            .copied()
            .filter(|id| self.store.item(*id).is_ok_and(|i| i.list_id != list))
            .collect();
        if movable.is_empty() {
            return;
        }
        let done = self.store.atomically(|b| {
            for id in &movable {
                b.move_item(*id, list, Placement::Last)?;
            }
            Ok(())
        });
        match done {
            Ok(()) => {
                self.show_toast(neutral(ToastMessage::ItemsMoved {
                    count: movable.len(),
                    list: name,
                }));
                self.library_changed();
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    /// Gives the items `ids` colour `label` (`None` clears it).
    pub(super) fn label_items(&mut self, ids: &[ItemId], label: Option<ItemLabel>) {
        let now = self.clock.now();
        let done = self.store.atomically(|b| {
            for id in ids {
                let mut item = b.item(*id)?;
                item.label = label;
                item.updated_at = now;
                b.update_item(&item, None)?;
            }
            Ok(())
        });
        match done {
            Ok(()) => {
                self.library_changed();
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    // ---- The second level of the item menu -----------------------------------------------------------

    /// The Move or Colour choice, while it is open.
    pub(super) fn choose_menu_view(&self) -> Option<ChooseMenu> {
        let items: Vec<snipdeck_core::Item> = self
            .menu_targets
            .iter()
            .filter_map(|id| self.store.item(*id).ok())
            .collect();
        match self.menu {
            PanelMenu::Move => Some(ChooseMenu {
                entries: self
                    .model
                    .tabs()
                    .into_iter()
                    .map(|tab| ChooseEntry {
                        checked: !items.is_empty() && items.iter().all(|i| i.list_id == tab.id),
                        label: ChooseLabel::List(tab.name),
                    })
                    .collect(),
            }),
            PanelMenu::Colour => {
                let all = |label: Option<ItemLabel>| {
                    !items.is_empty() && items.iter().all(|i| i.label == label)
                };
                let mut entries = vec![ChooseEntry {
                    label: ChooseLabel::Colour(None),
                    checked: all(None),
                }];
                entries.extend(LABELS.iter().map(|l| ChooseEntry {
                    label: ChooseLabel::Colour(Some(*l)),
                    checked: all(Some(*l)),
                }));
                Some(ChooseMenu { entries })
            }
            PanelMenu::PasteAs(id) if let Some(c) = self.colour_of_item(id) => {
                Some(ChooseMenu {
                    entries: snipdeck_colour::Format::ALL
                        .iter()
                        .map(|f| ChooseEntry {
                            label: ChooseLabel::Verbatim(format!(
                                "{}  {}",
                                f.label(),
                                snipdeck_colour::format(c, *f)
                            )),
                            checked: false,
                        })
                        .collect(),
                })
            }
            PanelMenu::PasteAs(_) => Some(ChooseMenu {
                entries: Transform::ALL
                    .iter()
                    .map(|t| ChooseEntry {
                        label: ChooseLabel::Transform(*t),
                        checked: false,
                    })
                    .collect(),
            }),
            _ => None,
        }
    }

    /// The user chose row `index` of the Move or Colour choice. The menu closes.
    pub(super) fn choose_second_level(&mut self, index: usize) {
        let menu = self.menu;
        self.menu = PanelMenu::None;
        let ids = std::mem::take(&mut self.menu_targets);
        match menu {
            PanelMenu::Move => {
                let Some(tab) = self.model.tabs().into_iter().nth(index) else {
                    return;
                };
                self.move_to_list(&ids, tab.id, tab.name);
            }
            PanelMenu::Colour => {
                let label = match index {
                    0 => None,
                    n => LABELS.get(n - 1).copied(),
                };
                if index == 0 || label.is_some() {
                    self.label_items(&ids, label);
                }
            }
            _ => {}
        }
    }

    /// Moves `ids` to the Trash and offers Undo for 5 s (the same Trash and Undo as the Main
    /// window; [`main_undo`](Self::main_undo) brings them back).
    pub(super) fn trash_with_undo(&mut self, ids: &[ItemId]) {
        if ids.is_empty() {
            return;
        }
        let now = self.clock.now();
        let done = self.store.atomically(|b| {
            for id in ids {
                b.trash(Trashable::Item(*id), now)?;
            }
            Ok(())
        });
        if done.is_err() {
            self.show_toast(failed());
            return;
        }
        let step = self.record_step(
            crate::history::Action::Delete,
            vec![crate::history::Change::Trash(
                ids.iter().map(|id| Trashable::Item(*id)).collect(),
            )],
        );
        if self.refresh_panel().is_err() {
            self.show_toast(failed());
            return;
        }
        self.show_undo_toast(
            Toast {
                kind: ToastKind::Neutral,
                message: ToastMessage::ItemsTrashed { count: ids.len() },
            },
            step,
        );
    }
}
