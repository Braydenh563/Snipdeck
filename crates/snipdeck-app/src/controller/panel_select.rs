//! The Panel's multi-selection and what acts on it: marking rows (Ctrl+click, Shift+click,
//! Ctrl+A, Shift+arrows), the bulk bar (Move to, Colour, Delete with Undo, Copy all, Clear) and
//! the item menu's second level (the lists to move to, the colours to give). Part of the
//! [`Controller`]; the window is the host's.

use snipdeck_core::{ItemId, ItemLabel, Placement};
use snipdeck_platform::payload::{Data, Payload};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::{ItemRepo, TrashRepo, Trashable};

use super::{
    Controller, Library, PanelMenu, failed, neutral,
    views::{ChooseEntry, ChooseLabel, ChooseMenu},
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
    pub fn panel_mark(&mut self, index: usize, mode: SelectMode) {
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

    /// The bulk bar's "Copy all": the text of the marked items, in row order, one after another
    /// with a line between them. Sensitive items are left out (and said so by the count).
    pub fn bulk_copy(&mut self) {
        let ids = self.model.marked_ids();
        let mut parts = Vec::new();
        for id in ids {
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
            return;
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
                self.library_changed();
                if self.refresh_panel().is_err() {
                    self.show_toast(failed());
                    return;
                }
                self.show_toast(neutral(ToastMessage::ItemsMoved {
                    count: movable.len(),
                    list: name,
                }));
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    /// Gives the items `ids` colour `label` (`None` clears it).
    fn label_items(&mut self, ids: &[ItemId], label: Option<ItemLabel>) {
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
                if self.refresh_panel().is_err() {
                    self.show_toast(failed());
                }
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
