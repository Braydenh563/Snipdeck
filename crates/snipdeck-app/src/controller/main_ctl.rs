//! The Main window's half of the [`Controller`]: sections, search, selection, the item actions,
//! the Trash with its 5 s Undo, list and board editing and reordering. Slint-free like the rest
//! of the controller: `main_binding` copies a [`MainScreen`] into the window and calls these
//! methods from its callbacks.
//!
//! Every method that changes the library ends in [`Controller::library_changed`], which marks
//! the Main window's model stale (it reads the library again before the next
//! [`main_screen`](Controller::main_screen)) and refreshes the Panel, which shows the same
//! library.

use snipdeck_core::{
    BoardId, Feature, Item, ItemId, ListAccent, ListId, ListSort, Name, Placement, Tag,
};
use snipdeck_platform::{
    pipeline::{Clipboard, Keyboard, Timing},
    system_look::HighContrast,
};
use snipdeck_store::{ItemRepo, TrashRepo, Trashable};

use super::{Controller, Library, Toast, ToastKind, ToastMessage, ToastView, UseResult, failed};
use crate::{
    host::ClipboardText,
    main_window::{
        Counts, DeleteHint, EditorKind, EditorView, Key, NameError, Row, Section, SelectMode,
        SideList, SideTag, ViewMode, reorder,
    },
    panel::UseLabel,
    usecase,
};

/// Why a tag could not be renamed in the Tag manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagRenameError {
    /// The new name is not a valid name (blank, too long, a control character).
    Name(NameError),
    /// Another tag already has that name (compared ignoring case).
    Exists,
    /// The library could not be written; a toast says so.
    Failed,
}

/// What the context menu, the bulk bar and the shortcuts of the Main window ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainAction {
    /// Paste (or copy) the item.
    Use,
    /// Copy the item.
    Copy,
    /// Edit the item in the item editor.
    Edit,
    /// Pin, or unpin when every target is pinned.
    Pin,
    /// Open the *Move to list* chooser for the targets.
    Move,
    /// Open the *Tags* chooser for the targets.
    Tag,
    /// Move the targets to the Trash, with an Undo toast.
    Delete,
    /// Bring the targets back from the Trash.
    Restore,
    /// Delete the targets from the Trash for good.
    DeleteForever,
}

/// An entry of the board menu, before wording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardMenuEntry {
    /// Switch to this board.
    Board {
        /// Its name.
        name: String,
        /// It is the open one.
        current: bool,
    },
    /// A divider.
    Separator,
    /// "New board…".
    New,
    /// "Rename board…" (the board editor for the open board).
    Rename,
    /// "Move board up".
    MoveUp {
        /// There is a board above.
        enabled: bool,
    },
    /// "Move board down".
    MoveDown {
        /// There is a board below.
        enabled: bool,
    },
    /// "Delete board…" (the board editor with its confirmation open).
    Delete,
}

/// A tag in the *Tags* chooser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagChoice {
    /// The tag's name.
    pub name: String,
    /// How many items carry it.
    pub count: usize,
    /// Every item the chooser acts on carries it.
    pub checked: bool,
}

/// Everything the Main window shows, as plain data.
#[derive(Debug, Clone, PartialEq)]
pub struct MainScreen {
    /// Dark theme.
    pub dark: bool,
    /// The high-contrast colours, while a Windows high-contrast theme is on.
    pub high_contrast: Option<HighContrast>,
    /// The open board's name.
    pub board_name: String,
    /// The board menu while it is open.
    pub board_menu: Option<Vec<BoardMenuEntry>>,
    /// The open board's lists.
    pub lists: Vec<SideList>,
    /// The tags.
    pub tags: Vec<SideTag>,
    /// The *Tags* chooser's entries.
    pub tag_choices: Vec<TagChoice>,
    /// The numbers beside the system views.
    pub counts: Counts,
    /// What the sidebar has open.
    pub section: Section,
    /// The index into `lists` or `tags` that `section` names.
    pub section_index: usize,
    /// The row style.
    pub view_mode: ViewMode,
    /// The search text.
    pub query: String,
    /// The keyboard cursor.
    pub cursor: usize,
    /// The rows.
    pub rows: Vec<Row>,
    /// How many rows are selected.
    pub selected_count: usize,
    /// "Use" or "Copy", by what a click can do now.
    pub use_label: UseLabel,
    /// The index of the sensitive row held revealed.
    pub revealed: Option<usize>,
    /// The toast, if one is showing.
    pub toast: Option<ToastView>,
    /// The list or board editor, while one is open.
    pub editor: Option<EditorView>,
}

/// What an open editor edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EditorTarget {
    NewList(BoardId),
    List(ListId),
    NewBoard,
    Board(BoardId),
}

/// What to show after an editor saved.
enum Opened {
    List(ListId),
    Board(BoardId),
}

/// An open list or board editor.
#[derive(Debug, Clone)]
pub(super) struct EditorSlot {
    view: EditorView,
    target: EditorTarget,
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    // ---- Reading -------------------------------------------------------------------------------

    /// Everything the Main window shows now. Reads the library again first if it changed.
    pub fn main_screen(&mut self) -> MainScreen {
        self.main_fresh();
        let look = self.look();
        let mut rows = self.main.rows();
        let mut revealed = None;
        if let Some(id) = self.revealed
            && let Some(at) = rows
                .iter()
                .position(|r| r.key == Key::Item(id) && r.sensitive)
        {
            revealed = Some(at);
            if let Ok(Some(text)) = usecase::item_text(&self.store, id) {
                rows[at].preview = text.chars().take(Item::PREVIEW_CHARS).collect();
            }
        }
        MainScreen {
            dark: look.dark,
            high_contrast: look.high_contrast,
            board_name: self.main.board_name().to_owned(),
            board_menu: self
                .main
                .board_menu_open()
                .then(|| self.board_menu_entries()),
            lists: self.main.side_lists(),
            tags: self.main.side_tags(),
            tag_choices: self.tag_choices(),
            counts: self.main.counts(),
            section: self.main.section(),
            section_index: self.main.section_index(),
            view_mode: self.main.view_mode(),
            query: self.main.query().to_owned(),
            cursor: self.main.cursor(),
            selected_count: self.main.selected_count(),
            rows,
            use_label: self.model.use_label(),
            revealed,
            toast: self.toast_view(),
            editor: self.editor.as_ref().map(|e| e.view.clone()),
        }
    }

    pub(super) fn main_fresh(&mut self) {
        if self.main.is_stale() && self.main.reload(&self.store).is_err() {
            self.main.mark_fresh();
            self.show_toast(failed());
        }
    }

    fn tag_choices(&self) -> Vec<TagChoice> {
        let targets: Vec<ItemId> = self
            .main
            .chooser()
            .iter()
            .filter_map(|k| match k {
                Key::Item(id) => Some(*id),
                _ => None,
            })
            .collect();
        self.main
            .tags()
            .iter()
            .zip(self.main.side_tags())
            .map(|(tag, side)| TagChoice {
                name: side.name,
                count: side.count,
                checked: !targets.is_empty()
                    && targets.iter().all(|id| {
                        self.main
                            .item(*id)
                            .is_some_and(|item| item.tags.contains(&tag.id))
                    }),
            })
            .collect()
    }

    /// The library changed: the Main window reads it again and the Panel follows.
    pub(super) fn library_changed(&mut self) {
        self.main.mark_stale();
        if self.model.refresh(&self.store).is_err() {
            self.show_toast(failed());
        }
    }

    // ---- Navigation, search, selection ---------------------------------------------------------

    /// The sidebar opened `section`: the search ends, the selection clears.
    pub fn main_open(&mut self, section: Section) {
        self.main_fresh();
        self.main.open(section);
        self.revealed = None;
    }

    /// The search box changed (every board is searched).
    pub fn main_set_query(&mut self, query: &str) {
        self.main_fresh();
        if self.main.set_query(&self.store, query).is_err() {
            self.show_toast(failed());
        }
        self.revealed = None;
    }

    /// The row style changed.
    pub fn main_set_view(&mut self, view: ViewMode) {
        self.main.set_view_mode(view);
    }

    /// A click or key on row `index`.
    pub fn main_select(&mut self, index: usize, mode: SelectMode) {
        self.main_fresh();
        self.main.select(index, mode);
    }

    /// Ctrl+A.
    pub fn main_select_all(&mut self) {
        self.main_fresh();
        self.main.select_all();
    }

    /// Esc.
    pub fn main_clear_selection(&mut self) {
        self.main.clear_selection();
    }

    /// Holding Space (or the pointer) on sensitive row `index` reveals its text; letting go
    /// hides it again.
    pub fn main_reveal(&mut self, index: usize, on: bool) {
        self.main_fresh();
        self.revealed = if on {
            self.main
                .rows()
                .get(index)
                .filter(|r| r.sensitive)
                .and_then(|r| match r.key {
                    Key::Item(id) => Some(id),
                    _ => None,
                })
        } else {
            None
        };
    }

    // ---- Item actions --------------------------------------------------------------------------

    /// Runs `action` on the selection (when more than one row is selected) or on row `index`.
    pub fn main_action(&mut self, action: MainAction, index: usize) -> Option<UseResult> {
        self.main_fresh();
        let targets = self.main.targets(index);
        if targets.is_empty() {
            return None;
        }
        let items: Vec<ItemId> = targets
            .iter()
            .filter_map(|k| match k {
                Key::Item(id) => Some(*id),
                _ => None,
            })
            .collect();
        match action {
            MainAction::Use | MainAction::Copy => {
                let mode = if action == MainAction::Use {
                    snipdeck_core::UseMode::Paste
                } else {
                    snipdeck_core::UseMode::Copy
                };
                let id = items.first().copied()?;
                return Some(self.use_item(id, mode));
            }
            MainAction::Edit => {
                if let Some(id) = items.first() {
                    self.open_item_editor(*id);
                }
            }
            MainAction::Pin => self.pin(&items),
            MainAction::Move | MainAction::Tag => self.main.set_chooser(targets),
            MainAction::Delete => self.trash_items(&items),
            MainAction::Restore => self.restore(&targets),
            MainAction::DeleteForever => self.delete_forever(&targets),
        }
        None
    }

    fn pin(&mut self, items: &[ItemId]) {
        let now = self.clock.now();
        let pin = !items
            .iter()
            .all(|id| self.main.item(*id).is_some_and(|i| i.pinned));
        let done = self.store.atomically(|b| {
            for id in items {
                let mut item = b.item(*id)?;
                item.pinned = pin;
                item.updated_at = now;
                b.update_item(&item, None)?;
            }
            Ok(())
        });
        self.finish(&done);
    }

    pub(super) fn trash_items(&mut self, items: &[ItemId]) {
        if items.is_empty() {
            return;
        }
        let now = self.clock.now();
        let done = self.store.atomically(|b| {
            for id in items {
                b.trash(Trashable::Item(*id), now)?;
            }
            Ok(())
        });
        if done.is_ok() {
            let targets = items.iter().map(|id| Trashable::Item(*id)).collect();
            self.library_changed();
            self.show_undo_toast(
                Toast {
                    kind: ToastKind::Neutral,
                    message: ToastMessage::ItemsTrashed { count: items.len() },
                },
                targets,
            );
        } else {
            self.show_toast(failed());
        }
    }

    /// Brings `targets` back from the Trash. A board comes back only while the plan allows
    /// another one.
    fn restore(&mut self, targets: &[Key]) {
        let limit = self.board_limit();
        let done = self.store.atomically(|b| {
            for key in targets {
                b.restore_with(Trashable::from(*key), limit)?;
            }
            Ok(())
        });
        self.library_changed();
        match done {
            Ok(()) => self.show_toast(Toast {
                kind: ToastKind::Success,
                message: ToastMessage::Restored {
                    count: targets.len(),
                },
            }),
            Err(error) => self.show_toast(restore_failed(&error)),
        }
    }

    fn delete_forever(&mut self, targets: &[Key]) {
        let done = self.store.atomically(|b| {
            for key in targets {
                b.purge_entry(Trashable::from(*key))?;
            }
            Ok(())
        });
        self.library_changed();
        match done {
            Ok(()) => self.show_toast(neutral_toast(ToastMessage::DeletedForever {
                count: targets.len(),
            })),
            Err(_) => self.show_toast(failed()),
        }
    }

    /// The Free plan's live-board limit, for restoring a board; `None` on Pro.
    fn board_limit(&self) -> Option<usize> {
        (!self.entitlements.allows(Feature::UnlimitedBoards))
            .then_some(snipdeck_core::Entitlements::FREE_BOARD_LIMIT)
    }

    /// Ends an action: a failure is a toast, a success is a changed library.
    fn finish(&mut self, done: &snipdeck_store::Result<()>) {
        match done {
            Ok(()) => self.library_changed(),
            Err(_) => self.show_toast(failed()),
        }
    }

    /// The *Move to list* chooser picked list `index`: the rows it was opened for go to its end.
    pub fn main_move_to(&mut self, index: usize) {
        self.main_fresh();
        let Some(list) = self.main.lists().get(index) else {
            return;
        };
        let (list, name) = (list.id, list.name.as_str().to_owned());
        let items: Vec<ItemId> = self
            .main
            .chooser()
            .iter()
            .filter_map(|k| match k {
                Key::Item(id) => Some(*id),
                _ => None,
            })
            .collect();
        if items.is_empty() {
            return;
        }
        let done = self.store.atomically(|b| {
            for id in &items {
                b.move_item(*id, list, Placement::Last)?;
            }
            Ok(())
        });
        self.main.set_chooser(Vec::new());
        match done {
            Ok(()) => {
                self.library_changed();
                self.show_toast(neutral_toast(ToastMessage::ItemsMoved {
                    count: items.len(),
                    list: name,
                }));
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    /// The *Tags* chooser ticked (`on`) or unticked tag `index` for the rows it was opened for.
    pub fn main_toggle_tag(&mut self, index: usize, on: bool) {
        self.main_fresh();
        let Some(tag) = self.main.tags().get(index).map(|t| t.id) else {
            return;
        };
        let now = self.clock.now();
        let items: Vec<ItemId> = self
            .main
            .chooser()
            .iter()
            .filter_map(|k| match k {
                Key::Item(id) => Some(*id),
                _ => None,
            })
            .collect();
        let done = self.store.atomically(|b| {
            for id in &items {
                let mut item = b.item(*id)?;
                let changed = if on {
                    item.tags.insert(tag)
                } else {
                    item.tags.remove(&tag)
                };
                if changed {
                    item.updated_at = now;
                    b.update_item(&item, None)?;
                }
            }
            Ok(())
        });
        self.finish(&done);
    }

    /// The Main window's close button. Returns whether Snipdeck should quit with it: when
    /// "Keep running in the tray" is off, or when nothing could bring a window back (no tray
    /// icon and the Panel hidden). Otherwise the window just hides.
    #[must_use]
    pub fn main_window_closed(&self) -> bool {
        !self.settings.close_to_tray || (!self.session.tray_available && !self.visible)
    }

    /// Renames tag `index` (in the order of [`MainScreen::tags`]) to `name`. A name another tag
    /// has is refused with [`TagRenameError::Exists`] and no toast: the Tag manager shows the
    /// reason beside the field. A tag that is no longer there is left alone.
    ///
    /// # Errors
    ///
    /// Why the name was refused, or [`TagRenameError::Failed`] when the library could not be
    /// written.
    pub fn main_rename_tag(&mut self, index: usize, name: &str) -> Result<(), TagRenameError> {
        self.main_fresh();
        let name = Name::new(name).map_err(|error| {
            NameError::of(&error).map_or(TagRenameError::Failed, TagRenameError::Name)
        })?;
        let Some(tag) = self.main.tags().get(index).map(|t| t.id) else {
            return Ok(());
        };
        match self.store.update_tag(&Tag { id: tag, name }) {
            Ok(()) => {
                self.library_changed();
                Ok(())
            }
            Err(snipdeck_store::Error::TagExists(_)) => Err(TagRenameError::Exists),
            Err(_) => {
                self.show_toast(failed());
                Err(TagRenameError::Failed)
            }
        }
    }

    /// Deletes tag `index` after the window's confirmation (its items stay).
    pub fn main_delete_tag(&mut self, index: usize) {
        self.main_fresh();
        let Some(tag) = self.main.tags().get(index).map(|t| t.id) else {
            return;
        };
        let done = self.store.delete_tag(tag);
        self.finish(&done);
    }

    // ---- The Trash and Undo --------------------------------------------------------------------

    /// The toast's Undo: brings back what the delete moved to the Trash.
    pub fn main_undo(&mut self) {
        let Some(undo) = self.undo.take() else {
            return;
        };
        if self.toast.as_ref().is_none_or(|t| t.seq != undo.seq) {
            return;
        }
        let keys: Vec<Key> = undo.targets.iter().map(|t| Key::from(*t)).collect();
        // The delete is undone: the old toast goes, a "Restored" toast replaces it.
        self.restore(&keys);
    }

    /// "Empty Trash" after the window's confirmation.
    pub fn main_empty_trash(&mut self) {
        let Some(cutoff) = self.trash_cutoff(0) else {
            return;
        };
        // Everything trashed up to now, including this very second.
        let cutoff = snipdeck_core::Timestamp::from_second(cutoff.as_second().saturating_add(1))
            .unwrap_or(cutoff);
        match self.store.purge(cutoff) {
            Ok(_) => {
                self.library_changed();
                self.show_toast(neutral_toast(ToastMessage::TrashEmptied));
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    /// Deletes what has been in the Trash longer than [`TRASH_DAYS`](crate::main_window::TRASH_DAYS)
    /// (the host calls this at start-up). Returns how many entries went.
    pub fn purge_expired_trash(&mut self) -> usize {
        let Some(cutoff) = self.trash_cutoff(i64::from(crate::main_window::TRASH_DAYS)) else {
            return 0;
        };
        let purged = self.store.purge(cutoff).unwrap_or(0);
        if purged > 0 {
            self.library_changed();
        }
        purged
    }

    fn trash_cutoff(&self, days: i64) -> Option<snipdeck_core::Timestamp> {
        let now = self.clock.now().as_second();
        snipdeck_core::Timestamp::from_second(now.checked_sub(days.checked_mul(86_400)?)?).ok()
    }

    // ---- Lists ---------------------------------------------------------------------------------

    /// Deletes list `index` of the sidebar after the window's confirmation: it and its items
    /// move to the Trash, with an Undo toast.
    pub fn main_delete_list(&mut self, index: usize) {
        self.main_fresh();
        let Some(list) = self.main.lists().get(index) else {
            return;
        };
        self.trash_list(list.id, list.name.as_str().to_owned());
    }

    fn trash_list(&mut self, id: ListId, name: String) {
        let now = self.clock.now();
        match self.store.trash(Trashable::List(id), now) {
            Ok(()) => {
                self.library_changed();
                self.show_undo_toast(
                    neutral_toast(ToastMessage::ListTrashed { name }),
                    vec![Trashable::List(id)],
                );
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    /// Alt+Up / Alt+Down on list `index` of the sidebar.
    pub fn main_reorder_list(&mut self, index: usize, delta: isize) {
        self.main_fresh();
        let Some(board) = self.main.board() else {
            return;
        };
        let order: Vec<ListId> = self.main.lists().iter().map(|l| l.id).collect();
        let Some((_, placement)) = reorder(&order, index, delta) else {
            return;
        };
        let done = self.store.move_list(order[index], board, placement);
        self.finish(&done);
    }

    /// Alt+Up / Alt+Down in the item area: moves the row at the cursor within its list. Only
    /// a list in manual order can be arranged by hand.
    pub fn main_reorder_items(&mut self, delta: isize) {
        self.main_fresh();
        if !self.main.reorderable() {
            return;
        }
        let Some(list) = self.main.open_list().map(|l| l.id) else {
            return;
        };
        let Some(Key::Item(id)) = self.main.keys().get(self.main.cursor()).copied() else {
            return;
        };
        let order = self.main.manual_order(list);
        let Some(from) = order.iter().position(|i| *i == id) else {
            return;
        };
        let Some((to, placement)) = reorder(&order, from, delta) else {
            return;
        };
        let done = self.store.move_item(id, list, placement);
        let moved = done.is_ok();
        self.finish(&done);
        if moved {
            self.main_fresh();
            self.main.set_cursor(to);
        }
    }

    // ---- Boards --------------------------------------------------------------------------------

    /// Opens the board menu.
    pub fn main_board_menu_open(&mut self) {
        self.main.set_board_menu(true);
    }

    /// Closes the board menu without a choice.
    pub fn main_board_menu_close(&mut self) {
        self.main.set_board_menu(false);
    }

    fn board_menu_entries(&self) -> Vec<BoardMenuEntry> {
        let current = self.main.board_position();
        let count = self.main.boards().len();
        let mut entries: Vec<BoardMenuEntry> = self
            .main
            .boards()
            .iter()
            .enumerate()
            .map(|(i, b)| BoardMenuEntry::Board {
                name: b.name.as_str().to_owned(),
                current: Some(i) == current,
            })
            .collect();
        entries.push(BoardMenuEntry::Separator);
        entries.push(BoardMenuEntry::New);
        entries.push(BoardMenuEntry::Rename);
        entries.push(BoardMenuEntry::MoveUp {
            enabled: current.is_some_and(|i| i > 0),
        });
        entries.push(BoardMenuEntry::MoveDown {
            enabled: current.is_some_and(|i| i + 1 < count),
        });
        entries.push(BoardMenuEntry::Delete);
        entries
    }

    /// The board menu's entry `index` was chosen.
    pub fn main_board_menu_activate(&mut self, index: usize) {
        self.main_fresh();
        self.main.set_board_menu(false);
        let Some(entry) = self.board_menu_entries().into_iter().nth(index) else {
            return;
        };
        match entry {
            BoardMenuEntry::Board { .. } => {
                if let Some(id) = self.main.boards().get(index).map(|b| b.id) {
                    self.open_board(id);
                    self.revealed = None;
                }
            }
            BoardMenuEntry::Separator => {}
            BoardMenuEntry::New => self.new_board(),
            BoardMenuEntry::Rename => self.edit_board(false),
            BoardMenuEntry::MoveUp { enabled } => {
                if enabled {
                    self.main_reorder_board(-1);
                }
            }
            BoardMenuEntry::MoveDown { enabled } => {
                if enabled {
                    self.main_reorder_board(1);
                }
            }
            BoardMenuEntry::Delete => self.edit_board(true),
        }
    }

    /// Opens board `id` in the sidebar.
    fn open_board(&mut self, id: BoardId) {
        if self.main.open_board(&self.store, id).is_err() {
            self.show_toast(failed());
        }
    }

    /// Moves the open board up (-1) or down (+1) among the boards.
    pub fn main_reorder_board(&mut self, delta: isize) {
        self.main_fresh();
        let Some(from) = self.main.board_position() else {
            return;
        };
        let order: Vec<BoardId> = self.main.boards().iter().map(|b| b.id).collect();
        let Some((_, placement)) = reorder(&order, from, delta) else {
            return;
        };
        let done = self.store.move_board(order[from], placement);
        self.finish(&done);
    }

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
                name: String::new(),
                accent: None,
                sort: ListSort::Manual,
                item_count: 0,
                can_delete: false,
                delete_hint: None,
                name_error: None,
                open_delete: false,
            },
        );
    }

    /// "Edit list…": opens the list editor for list `index` of the sidebar.
    pub fn main_edit_list(&mut self, index: usize) {
        self.main_fresh();
        let Some(list) = self.main.lists().get(index) else {
            return;
        };
        let count = self
            .main
            .side_lists()
            .get(index)
            .map_or(0, |side| side.count);
        let view = EditorView {
            seq: 0,
            kind: EditorKind::List,
            is_new: false,
            name: list.name.as_str().to_owned(),
            accent: list.accent,
            sort: list.sort,
            item_count: count,
            can_delete: true,
            delete_hint: None,
            name_error: None,
            open_delete: false,
        };
        self.open_editor(EditorTarget::List(list.id), view);
    }

    fn new_board(&mut self) {
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
                name: String::new(),
                accent: None,
                sort: ListSort::Manual,
                item_count: 0,
                can_delete: false,
                delete_hint: None,
                name_error: None,
                open_delete: false,
            },
        );
    }

    /// The board editor for the open board; `open_delete` starts with the confirmation open.
    fn edit_board(&mut self, open_delete: bool) {
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
            sort: ListSort::Manual,
            item_count: self.main.side_lists().iter().map(|l| l.count).sum(),
            can_delete,
            delete_hint: (!can_delete).then_some(DeleteHint::LastBoard),
            name_error: None,
            open_delete: open_delete && can_delete,
        };
        self.open_editor(EditorTarget::Board(board), view);
    }

    fn open_editor(&mut self, target: EditorTarget, mut view: EditorView) {
        self.editor_seq += 1;
        view.seq = self.editor_seq;
        self.editor = Some(EditorSlot { view, target });
    }

    /// The editor's Save: checks the name, writes the list or board and closes the editor. A
    /// refused name keeps it open with the reason (see [`EditorView::name_error`]).
    pub fn main_editor_save(&mut self, name: &str, accent: Option<ListAccent>, sort: ListSort) {
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
        match self.write_editor(target, name, accent, sort, now) {
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
    fn write_editor(
        &mut self,
        target: EditorTarget,
        name: Name,
        accent: Option<ListAccent>,
        sort: ListSort,
        now: snipdeck_core::Timestamp,
    ) -> snipdeck_store::Result<Option<Opened>> {
        match target {
            EditorTarget::NewList(board) => {
                let mut list = snipdeck_core::List::new(board, name, now);
                list.accent = accent;
                list.sort = sort;
                self.store.insert_list(&list, Placement::Last)?;
                Ok(Some(Opened::List(list.id)))
            }
            EditorTarget::List(id) => {
                let mut list = self.store.list(id)?;
                list.name = name;
                list.accent = accent;
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
                let now = self.clock.now();
                match self.store.trash(Trashable::Board(id), now) {
                    Ok(()) => {
                        self.library_changed();
                        self.show_undo_toast(
                            neutral_toast(ToastMessage::BoardTrashed {
                                name: slot.view.name,
                            }),
                            vec![Trashable::Board(id)],
                        );
                    }
                    Err(_) => self.show_toast(failed()),
                }
            }
            _ => {}
        }
    }

    /// The editor was cancelled or closed.
    pub fn main_editor_close(&mut self) {
        self.editor = None;
    }

    /// The user edited a field of the editor: a refusal no longer applies.
    pub fn main_editor_edited(&mut self) {
        if let Some(slot) = self.editor.as_mut() {
            slot.view.name_error = None;
        }
    }
}

fn neutral_toast(message: ToastMessage) -> Toast {
    Toast {
        kind: ToastKind::Neutral,
        message,
    }
}

/// The toast for a restore that failed: a board past the Free limit says so.
fn restore_failed(error: &snipdeck_store::Error) -> Toast {
    if matches!(
        error,
        snipdeck_store::Error::Core(snipdeck_core::Error::ProRequired(_))
    ) {
        Toast {
            kind: ToastKind::Neutral,
            message: ToastMessage::BoardLimit,
        }
    } else {
        failed()
    }
}
