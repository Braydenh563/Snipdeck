//! Lists and boards: creating, moving, renaming, reordering and deleting.

use super::super::history_ctl::move_recorded;
use snipdeck_core::{BoardId, ItemId, ListId, Placement};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::Trashable;

use super::super::{
    Controller, Effect, HelpTopic, Library, Toast, ToastKind, ToastMessage, failed,
};
use super::{BoardMenuEntry, neutral_toast};
use crate::{
    host::ClipboardText,
    list_text,
    main_window::{Key, Section, reorder},
    usecase,
};

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
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

    /// "Export as text…" on list `index` of the sidebar: its text items go to a new file in
    /// `folder` (see [`crate::list_text`]). Returns the file.
    pub fn main_export_list(
        &mut self,
        index: usize,
        folder: &std::path::Path,
    ) -> Option<std::path::PathBuf> {
        self.main_fresh();
        let list = self.main.lists().get(index)?;
        let (id, name) = (list.id, list.name.as_str().to_owned());
        let written = self.store.items(id).ok().and_then(|items| {
            let entries: Vec<_> = items
                .iter()
                .filter(|item| !item.sensitive)
                .filter_map(|item| {
                    let text = usecase::item_text(&self.store, item.id).ok()??;
                    Some(list_text::Entry {
                        title: item.title.clone(),
                        text,
                    })
                })
                .collect();
            let date = self
                .clock
                .now()
                .to_zoned(jiff::tz::TimeZone::system())
                .date();
            let path = list_text::path(folder, &name, date);
            std::fs::create_dir_all(folder).ok()?;
            std::fs::write(&path, list_text::render(&name, &entries)).ok()?;
            Some(path)
        });
        if let Some(path) = &written {
            let file = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.show_toast(Toast {
                kind: ToastKind::Success,
                message: ToastMessage::Exported { file },
            });
        } else {
            self.show_toast(Toast {
                kind: ToastKind::Danger,
                message: ToastMessage::ExportFailed,
            });
        }
        written
    }

    pub(in crate::controller) fn trash_list(&mut self, id: ListId, name: String) {
        let now = self.clock.now();
        match self.store.trash(Trashable::List(id), now) {
            Ok(()) => {
                self.library_changed();
                let step = self.record_step(
                    crate::history::Action::Delete,
                    vec![crate::history::Change::Trash(vec![Trashable::List(id)])],
                );
                self.show_undo_toast(neutral_toast(ToastMessage::ListTrashed { name }), step);
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

    /// Alt+Up / Alt+Down or the item menu's Move up / Move down: moves the row at the cursor one
    /// row among the rows shown (a label filter can hide some of the list), like dropping it on
    /// that row, and says where it went. Only a list in manual order can be arranged by hand, and
    /// a multi-selection is not moved.
    pub fn main_reorder_items(&mut self, delta: isize) {
        self.main_fresh();
        if self.main.selected_count() > 1 {
            return;
        }
        let from = self.main.cursor();
        let Some(onto) = from.checked_add_signed(delta) else {
            return;
        };
        self.main_drop_row(from, onto);
        if self.main.reorderable() && onto < self.main.keys().len() && self.main.cursor() == onto {
            self.announce_placed(onto + 1, self.main.keys().len());
        }
    }

    /// Puts the cursor on item `id` among the rows now shown (a label filter can hide some of the
    /// list, so its place in the list is not its row).
    pub(super) fn follow_moved_row(&mut self, id: ItemId) {
        if let Some(at) = self.main.keys().iter().position(|k| *k == Key::Item(id)) {
            self.main.set_cursor(at);
        }
    }

    /// A drag of row `from` ended on row `onto`: the item takes that row's place. Like
    /// Alt+Up / Alt+Down, only a list in manual order can be arranged by hand.
    pub fn main_drop_row(&mut self, from: usize, onto: usize) {
        self.main_fresh();
        if !self.main.reorderable() {
            return;
        }
        let Some(list) = self.main.open_list().map(|l| l.id) else {
            return;
        };
        let keys = self.main.keys();
        let (Some(Key::Item(moved)), Some(Key::Item(target))) =
            (keys.get(from).copied(), keys.get(onto).copied())
        else {
            return;
        };
        let order = self.main.manual_order(list);
        let (Some(a), Some(b)) = (
            order.iter().position(|i| *i == moved),
            order.iter().position(|i| *i == target),
        ) else {
            return;
        };
        let Some(delta) = b.checked_signed_diff(a) else {
            return;
        };
        let Some((_, placement)) = reorder(&order, a, delta) else {
            return;
        };
        let mut changes = Vec::new();
        let done = move_recorded(&mut self.store, moved, list, placement, &mut changes);
        let moved_ok = done.is_ok();
        if moved_ok {
            self.record_step(crate::history::Action::Reorder, changes);
        }
        self.finish(&done);
        if moved_ok {
            self.main_fresh();
            self.follow_moved_row(moved);
        }
    }

    /// A drag of row `row` ended on list `list` of the sidebar: the item goes to the end of it.
    pub fn main_drop_on_list(&mut self, row: usize, list: usize) {
        self.main_fresh();
        if self.main.section() == Section::Trash {
            return;
        }
        let Some(Key::Item(id)) = self.main.keys().get(row).copied() else {
            return;
        };
        let Some(target) = self.main.lists().get(list) else {
            return;
        };
        let (target, name) = (target.id, target.name.as_str().to_owned());
        if self.main.item(id).is_none_or(|i| i.list_id == target) {
            return;
        }
        let mut changes = Vec::new();
        match move_recorded(&mut self.store, id, target, Placement::Last, &mut changes) {
            Ok(()) => {
                self.record_step(crate::history::Action::Move, changes);
                self.library_changed();
                self.show_toast(neutral_toast(ToastMessage::ItemsMoved {
                    count: 1,
                    list: name,
                }));
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    /// A drag of list `from` of the sidebar ended on list `onto`: the list takes its place.
    pub fn main_drop_list(&mut self, from: usize, onto: usize) {
        let Some(delta) = onto.checked_signed_diff(from) else {
            return;
        };
        self.main_reorder_list(from, delta);
    }

    // ---- Boards --------------------------------------------------------------------------------

    /// The Main window's Help menu or F1: queues `OpenHelp(topic)` for the Panel's binding to
    /// hand to the host on its next redraw ([`Controller::take_pending`]).
    pub fn main_open_help(&mut self, topic: HelpTopic) {
        self.pending.push(Effect::OpenHelp(topic));
    }

    /// "Show Panel" in the Main window: shows the Panel (or brings an open one forward) the way
    /// the tray and the hotkey do. The Panel's window acts on the effect when it redraws.
    pub fn main_show_panel(&mut self) {
        let effects = self.show();
        self.pending.extend(effects);
    }

    /// Opens the board menu.
    pub fn main_board_menu_open(&mut self) {
        self.main.set_board_menu(true);
    }

    /// Closes the board menu without a choice.
    pub fn main_board_menu_close(&mut self) {
        self.main.set_board_menu(false);
    }

    pub(super) fn board_menu_entries(&self) -> Vec<BoardMenuEntry> {
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
        entries.push(BoardMenuEntry::Delete { enabled: count > 1 });
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
            BoardMenuEntry::Rename => self.edit_board(),
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
            BoardMenuEntry::Delete { enabled } => {
                if enabled {
                    self.board_delete = self.main.board();
                }
            }
        }
    }

    /// How many items the open board holds while its delete confirmation asks.
    pub(super) fn board_delete_count(&self) -> Option<usize> {
        let id = self.board_delete?;
        (self.main.board() == Some(id))
            .then(|| self.main.side_lists().iter().map(|l| l.count).sum())
    }

    /// "Delete board" in the Main window's confirmation: the open board, its lists and their
    /// items move to the Trash, with an Undo toast.
    pub fn main_board_delete_confirm(&mut self) {
        self.main_fresh();
        let Some(id) = self.board_delete.take() else {
            return;
        };
        if self.main.board() == Some(id) && self.main.boards().len() > 1 {
            let name = self.main.board_name().to_owned();
            self.trash_board(id, name);
        }
    }

    /// Cancel, Esc or a click on the scrim: the board stays.
    pub fn main_board_delete_cancel(&mut self) {
        self.board_delete = None;
    }

    /// Opens board `id` in the sidebar.
    pub(super) fn open_board(&mut self, id: BoardId) {
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
}
