//! The list and board editors.

use snipdeck_core::{BoardId, ListAccent, ListIcon, ListId, ListSort, Name, Placement};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::Trashable;

use super::super::{Controller, Library, Toast, ToastKind, ToastMessage, failed};
use super::{EditorSlot, EditorTarget, Opened, neutral_toast};
use crate::{
    host::ClipboardText,
    main_window::{DeleteHint, EditorKind, EditorView, NameError, Section},
};

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    // ---- The list and board editors ------------------------------------------------------------

    /// "Add list": opens the list editor for a new list on the open board. A library without
    /// a board gets the starter board and list instead.
    pub fn main_new_list(&mut self) {
        self.main_fresh();
        let Some(board) = self.main.board() else {
            self.add_list();
            self.main.mark_stale();
            return;
        };
        self.open_editor(
            EditorTarget::NewList(board),
            EditorView {
                seq: 0,
                kind: EditorKind::List,
                is_new: true,
                name: crate::starter::numbered_name(
                    &self.new_list_base,
                    self.main.lists().iter().map(|l| l.name.as_str()),
                ),
                accent: None,
                icon: None,
                sort: ListSort::Manual,
                item_count: 0,
                can_delete: false,
                delete_hint: None,
                name_error: None,
            },
        );
    }

    /// "Edit list…": opens the list editor for list `index` of the sidebar.
    pub fn main_edit_list(&mut self, index: usize) {
        self.main_fresh();
        let Some(list) = self.main.lists().get(index).cloned() else {
            return;
        };
        let count = self
            .main
            .side_lists()
            .get(index)
            .map_or(0, |side| side.count);
        self.open_list_editor(&list, count);
    }

    /// The Panel's "Edit list…" (or a double-click on a tab): opens the list editor on list `id`
    /// (any board).
    pub(in super::super) fn panel_edit_list(&mut self, id: ListId) {
        self.main_fresh();
        let Ok(list) = self.store.list(id) else {
            self.show_toast(failed());
            return;
        };
        let count = self.store.items(id).map_or(0, |items| items.len());
        self.open_list_editor(&list, count);
    }

    pub(super) fn open_list_editor(&mut self, list: &snipdeck_core::List, count: usize) {
        let view = EditorView {
            seq: 0,
            kind: EditorKind::List,
            is_new: false,
            name: list.name.as_str().to_owned(),
            accent: list.accent,
            icon: list.icon,
            sort: list.sort,
            item_count: count,
            can_delete: true,
            delete_hint: None,
            name_error: None,
        };
        self.open_editor(EditorTarget::List(list.id), view);
    }

    /// The board editor for a new board, or the board-limit toast on Free.
    pub(super) fn new_board(&mut self) {
        if !self.entitlements.can_create_board(self.main.boards().len()) {
            self.show_toast(Toast {
                kind: ToastKind::Neutral,
                message: ToastMessage::BoardLimit,
            });
            return;
        }
        self.open_editor(
            EditorTarget::NewBoard,
            EditorView {
                seq: 0,
                kind: EditorKind::Board,
                is_new: true,
                name: crate::starter::numbered_name(
                    &self.new_board_base,
                    self.main.boards().iter().map(|b| b.name.as_str()),
                ),
                accent: None,
                icon: None,
                sort: ListSort::Manual,
                item_count: 0,
                can_delete: false,
                delete_hint: None,
                name_error: None,
            },
        );
    }

    /// The board editor for the open board.
    pub(super) fn edit_board(&mut self) {
        let Some(board) = self.main.board() else {
            return;
        };
        let can_delete = self.main.boards().len() > 1;
        let view = EditorView {
            seq: 0,
            kind: EditorKind::Board,
            is_new: false,
            name: self.main.board_name().to_owned(),
            accent: None,
            icon: None,
            sort: ListSort::Manual,
            item_count: self.main.side_lists().iter().map(|l| l.count).sum(),
            can_delete,
            delete_hint: (!can_delete).then_some(DeleteHint::LastBoard),
            name_error: None,
        };
        self.open_editor(EditorTarget::Board(board), view);
    }

    pub(super) fn open_editor(&mut self, target: EditorTarget, mut view: EditorView) {
        self.editor_seq += 1;
        view.seq = self.editor_seq;
        // Fields typed before the owner window closed come back.
        if let Some(at) = self.parked_editors.iter().position(|(t, _)| *t == target) {
            let (_, typed) = self.parked_editors.remove(at);
            view.name = typed.name;
            view.accent = typed.accent;
            view.icon = typed.icon;
            view.sort = typed.sort;
        }
        self.editor = Some(EditorSlot { view, target });
    }

    /// The editor's Save: checks the name, writes the list or board and closes the editor. A
    /// refused name keeps it open with the reason (see [`EditorView::name_error`]).
    pub fn main_editor_save(
        &mut self,
        name: &str,
        accent: Option<ListAccent>,
        icon: Option<ListIcon>,
        sort: ListSort,
    ) {
        self.main_fresh();
        let Some(slot) = self.editor.as_mut() else {
            return;
        };
        let target = slot.target;
        let name = match Name::new(name) {
            Ok(name) => name,
            Err(error) => {
                slot.view.name_error = NameError::of(&error);
                return;
            }
        };
        if target == EditorTarget::NewBoard
            && !self.entitlements.can_create_board(self.main.boards().len())
        {
            self.editor = None;
            self.show_toast(Toast {
                kind: ToastKind::Neutral,
                message: ToastMessage::BoardLimit,
            });
            return;
        }
        let now = self.clock.now();
        match self.write_editor(target, name, accent, icon, sort, now) {
            Ok(opened) => {
                self.editor = None;
                self.library_changed();
                self.main_fresh();
                match opened {
                    Some(Opened::List(id)) => self.main.open(Section::List(id)),
                    Some(Opened::Board(id)) => self.open_board(id),
                    None => {}
                }
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    /// Writes what an editor made: a new list or board, or a changed one.
    pub(super) fn write_editor(
        &mut self,
        target: EditorTarget,
        name: Name,
        accent: Option<ListAccent>,
        icon: Option<ListIcon>,
        sort: ListSort,
        now: snipdeck_core::Timestamp,
    ) -> snipdeck_store::Result<Option<Opened>> {
        match target {
            EditorTarget::NewList(board) => {
                let mut list = snipdeck_core::List::new(board, name, now);
                list.accent = accent;
                list.icon = icon;
                list.sort = sort;
                self.store.insert_list(&list, Placement::Last)?;
                Ok(Some(Opened::List(list.id)))
            }
            EditorTarget::List(id) => {
                let mut list = self.store.list(id)?;
                list.name = name;
                list.accent = accent;
                list.icon = icon;
                list.sort = sort;
                list.updated_at = now;
                self.store.update_list(&list)?;
                Ok(None)
            }
            EditorTarget::NewBoard => {
                let board = snipdeck_core::Board::new(name, now);
                self.store.insert_board(&board, Placement::Last)?;
                Ok(Some(Opened::Board(board.id)))
            }
            EditorTarget::Board(id) => {
                let mut board = self.store.board(id)?;
                board.name = name;
                board.updated_at = now;
                self.store.update_board(&board)?;
                Ok(None)
            }
        }
    }

    /// The editor's Delete, after its confirmation: the list or board and what is inside it
    /// move to the Trash, with an Undo toast.
    pub fn main_editor_delete(&mut self) {
        self.main_fresh();
        let Some(slot) = self.editor.take() else {
            return;
        };
        match slot.target {
            EditorTarget::List(id) => self.trash_list(id, slot.view.name),
            EditorTarget::Board(id) if self.main.boards().len() > 1 => {
                self.trash_board(id, slot.view.name);
            }
            _ => {}
        }
    }

    /// Moves board `id` (named `name`) with its lists and items to the Trash, with an Undo
    /// toast.
    pub(super) fn trash_board(&mut self, id: BoardId, name: String) {
        let now = self.clock.now();
        match self.store.trash(Trashable::Board(id), now) {
            Ok(()) => {
                self.library_changed();
                self.show_undo_toast(
                    neutral_toast(ToastMessage::BoardTrashed { name }),
                    vec![Trashable::Board(id)],
                );
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    /// The editor was cancelled or closed.
    pub fn main_editor_close(&mut self) {
        self.editor = None;
    }

    /// The window that opened the editor closed: the editor closes without asking and keeps the
    /// fields as typed (for the rest of the app session), so opening the same one again finds
    /// them. Fields equal to what the editor opened with are not kept.
    pub fn main_editor_park(
        &mut self,
        name: &str,
        accent: Option<ListAccent>,
        icon: Option<ListIcon>,
        sort: ListSort,
    ) {
        let Some(slot) = self.editor.take() else {
            return;
        };
        self.parked_editors.retain(|(t, _)| *t != slot.target);
        let v = &slot.view;
        if (v.name.as_str(), v.accent, v.icon, v.sort) == (name, accent, icon, sort) {
            return;
        }
        let mut typed = slot.view;
        name.clone_into(&mut typed.name);
        typed.accent = accent;
        typed.icon = icon;
        typed.sort = sort;
        self.parked_editors.push((slot.target, typed));
    }

    /// The user edited a field of the editor: a refusal no longer applies.
    pub fn main_editor_edited(&mut self) {
        if let Some(slot) = self.editor.as_mut() {
            slot.view.name_error = None;
        }
    }
}
