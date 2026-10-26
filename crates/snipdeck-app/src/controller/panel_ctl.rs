//! The Panel's settings, gestures (tabs, drops, moves, the board menu) and row activation.

use std::time::Duration;

use snipdeck_core::{BoardId, ItemId, Placement, UseMode};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};

use super::{
    BoardDeleteView, BoardMenuEntry, Controller, Effect, Library, PanelMenu, UseResult, failed,
    neutral, policy_of,
};
use crate::{
    host::ClipboardText,
    main_window::reorder,
    settings::{Settings, Theme},
    settings_store::{Poll, SettingsSaver},
    toast::{Toast, ToastKind, ToastMessage},
};

#[cfg(test)]
thread_local! {
    /// Makes [`refresh_panel`](Controller::refresh_panel) fail, to test what each caller does then.
    pub(super) static FAIL_REFRESH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    // ---- Settings ----------------------------------------------------------------------------

    /// Changes the settings with `change` (then sanitised). A real change is applied now and
    /// scheduled to be written (see [`settings_wait`](Self::settings_wait)). Hotkey changes are
    /// not re-registered here: the host does that when a Settings window exists.
    pub fn update_settings(&mut self, change: impl FnOnce(&mut Settings)) {
        let mut next = self.settings.clone();
        change(&mut next);
        next.sanitize();
        if next == self.settings {
            return;
        }
        self.settings = next;
        self.model.set_policy(policy_of(&self.settings));
        self.model.set_paste_rules(self.settings.paste_rules());
        let now = self.clock.now();
        if let Some(saver) = &mut self.saver {
            saver.changed(&self.settings, now);
        }
        self.dock_resync();
        self.sync_recent_gate();
    }

    /// The Main window was moved, resized, maximised or restored: remembers where it is, to
    /// be written after the quiet time and restored the next time it opens.
    pub fn remember_main_window(&mut self, place: crate::settings::WindowPlace) {
        self.update_settings(|s| s.main_window = Some(place));
    }

    /// The item editor's "More options" was opened or closed: remembered across editors and runs.
    pub fn set_editor_more_open(&mut self, open: bool) {
        self.update_settings(|s| s.editor_more_open = open);
    }

    /// The theme setting (`system`, `light`, `dark`).
    pub fn set_theme(&mut self, theme: Theme) {
        self.update_settings(|s| s.theme = theme);
    }

    /// The user picked a theme (the Main window's View menu, the command palette, Settings): the
    /// setting changes, is saved after the quiet time, and a toast says which theme is on.
    pub fn choose_theme(&mut self, theme: Theme) {
        self.set_theme(theme);
        self.show_toast(neutral(ToastMessage::ThemeSet(theme)));
    }

    /// How long until a changed setting is due to be written, if one waits. The host arms a
    /// timer for this and then calls [`save_settings_if_due`](Self::save_settings_if_due).
    #[must_use]
    pub fn settings_wait(&self) -> Option<Duration> {
        self.saver.as_ref()?.wait(self.clock.now())
    }

    /// Writes the changed settings if they are due. A failed write shows a toast (once until a
    /// write succeeds) and is retried by the saver.
    pub fn save_settings_if_due(&mut self) {
        let now = self.clock.now();
        let Some(saver) = &mut self.saver else {
            return;
        };
        match saver.poll(now) {
            Poll::Saved => self.session.save_failed = false,
            Poll::Failed(_) if !self.session.save_failed => {
                self.session.save_failed = true;
                self.show_toast(Toast {
                    kind: ToastKind::Danger,
                    message: ToastMessage::SettingsNotSaved,
                });
            }
            Poll::Idle | Poll::Waiting(_) | Poll::Failed(_) => {}
        }
    }

    /// Writes any changed settings now (at exit).
    ///
    /// # Errors
    ///
    /// The write error; the settings stay in memory only.
    pub fn flush_settings(&mut self) -> std::io::Result<()> {
        self.saver.as_mut().map_or(Ok(()), SettingsSaver::flush)
    }

    /// The pin button: keep the Panel on top or not. Remembered in the settings (U8).
    pub fn set_on_top(&mut self, on_top: bool) {
        self.on_top = on_top;
        self.update_settings(|s| s.panel_pinned = on_top);
    }

    /// The user switched the pin (the button, Ctrl+Shift+P, the palette): [`set_on_top`](Self::set_on_top)
    /// and a toast saying what the Panel does now. Nothing happens when it is already so.
    pub fn choose_on_top(&mut self, on_top: bool) {
        if self.on_top == on_top {
            return;
        }
        self.set_on_top(on_top);
        self.show_toast(neutral(ToastMessage::PanelOnTop(on_top)));
    }

    /// Sets the words a new list's and a new board's editor name starts from ("New list" gives
    /// "New list 1"), translated by the window layer: the controller has no `@tr`.
    pub fn set_new_name_bases(&mut self, list: &str, board: &str) {
        list.clone_into(&mut self.new_list_base);
        board.clone_into(&mut self.new_board_base);
    }

    /// A regular Snipdeck window (Main, Settings, an editor, Help) gained or lost the focus
    /// (`active`: one of them has it now). While one has it a pinned Panel is not topmost, so
    /// that window can cover it; once the focus is in another app the Panel is topmost again.
    pub fn set_own_window_active(&mut self, active: bool) {
        self.own_window_active = active;
        if active {
            // The Panel lost the focus to a Snipdeck window: no key release will reach it.
            self.panel_revealed = None;
        }
    }

    // ---- Gestures ----------------------------------------------------------------------------

    /// The search text changed.
    pub fn set_query(&mut self, query: &str) {
        self.model.set_query(query);
    }

    /// The keyboard selection moved by `delta` rows.
    pub fn move_selection(&mut self, delta: isize) {
        self.model.move_selection(delta);
    }

    /// The user picked tab `index`: shows that list and ends the search.
    pub fn select_tab(&mut self, index: usize) {
        let Some(tab) = self.model.tabs().into_iter().nth(index) else {
            return;
        };
        if self.model.select_list(&self.store, tab.id).is_err() {
            self.show_toast(failed());
        }
    }

    /// Row `row` of the Panel was dropped on tab `tab`: the item goes to the end of that list.
    pub fn panel_drop_on_tab(&mut self, row: usize, tab: usize) {
        let Some(item) = self.model.rows().get(row).map(|r| r.id) else {
            return;
        };
        let Some(target) = self.model.tabs().into_iter().nth(tab) else {
            return;
        };
        if self.store.item(item).is_ok_and(|i| i.list_id == target.id) {
            return;
        }
        let mut changes = Vec::new();
        match super::history_ctl::move_recorded(
            &mut self.store,
            item,
            target.id,
            Placement::Last,
            &mut changes,
        ) {
            Ok(()) => {
                self.record_step(crate::history::Action::Move, changes);
                self.show_toast(neutral(ToastMessage::ItemsMoved {
                    count: 1,
                    list: target.name,
                }));
                self.library_changed();
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    /// A drag of Panel row `from` ended on row `onto`: the item takes that row's place. Like the
    /// Main window, only a list in manual order, with no search running, can be arranged.
    pub fn panel_drop_row(&mut self, from: usize, onto: usize) {
        let rows = self.model.rows();
        let (Some(moved), Some(target)) =
            (rows.get(from).map(|r| r.id), rows.get(onto).map(|r| r.id))
        else {
            return;
        };
        let order = self.model.manual_order();
        let (Some(a), Some(b)) = (
            order.iter().position(|i| *i == moved),
            order.iter().position(|i| *i == target),
        ) else {
            return;
        };
        let Some(delta) = b.checked_signed_diff(a) else {
            return;
        };
        self.panel_move_item(moved, delta);
    }

    /// Alt+Up / Alt+Down in the Panel: moves the selected row one place.
    pub fn panel_move_selected(&mut self, delta: isize) {
        if let Some(id) = self.model.selected() {
            self.panel_move_item(id, delta);
        }
    }

    /// Whether item `id` can move by `delta` places in the shown list: it is in manual order,
    /// no search runs and the place exists.
    pub(crate) fn can_move_item(&self, id: ItemId, delta: isize) -> bool {
        if !self.model.reorderable() {
            return false;
        }
        let order = self.model.manual_order();
        order
            .iter()
            .position(|i| *i == id)
            .is_some_and(|at| reorder(&order, at, delta).is_some())
    }

    /// Moves item `moved` by `delta` places in the shown list, keeps it selected and says where
    /// it went (the keys and a drag alike).
    pub(crate) fn panel_move_item(&mut self, moved: ItemId, delta: isize) {
        if !self.model.reorderable() {
            return;
        }
        let Some(list) = self.model.list().map(|l| l.id) else {
            return;
        };
        let order = self.model.manual_order();
        let Some(a) = order.iter().position(|i| *i == moved) else {
            return;
        };
        let Some((to, placement)) = reorder(&order, a, delta) else {
            return;
        };
        let mut changes = Vec::new();
        match super::history_ctl::move_recorded(
            &mut self.store,
            moved,
            list,
            placement,
            &mut changes,
        ) {
            Ok(()) => {
                self.record_step(crate::history::Action::Reorder, changes);
                if !self.library_changed() {
                    return;
                }
                self.model.select_item(moved);
                self.announce_placed(to + 1, order.len());
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    /// Enter or a click on row `index`: uses it with `mode`.
    pub fn activate_row(&mut self, index: usize, mode: UseMode) -> UseResult {
        let Some(id) = self.model.rows().get(index).map(|r| r.id) else {
            return UseResult::Failed;
        };
        self.use_item(id, mode)
    }

    /// A digit 1 to 9 on an empty search.
    pub fn quick_use(&mut self, n: u8) -> UseResult {
        match self.model.quick_use(n) {
            Some(id) => self.use_item(id, UseMode::Paste),
            None => UseResult::Failed,
        }
    }

    /// A click on pinned chip `index`: uses it with `mode`.
    pub fn activate_pinned(&mut self, index: usize, mode: UseMode) -> UseResult {
        let Some(id) = self.model.pinned().get(index).map(|r| r.id) else {
            return UseResult::Failed;
        };
        self.use_item(id, mode)
    }

    /// Opens the board menu.
    pub fn open_board_menu(&mut self) {
        self.menu = PanelMenu::Board;
    }

    /// Closes the board menu without a choice.
    pub fn close_board_menu(&mut self) {
        if self.menu == PanelMenu::Board {
            self.menu = PanelMenu::None;
        }
    }

    /// The board menu's rows: the boards, a divider, then New, Rename and Delete (off for the last board).
    pub(super) fn panel_board_menu_entries(&self) -> Vec<BoardMenuEntry> {
        let current = self.model.board();
        let boards = self.model.boards();
        let mut entries: Vec<BoardMenuEntry> = boards
            .iter()
            .map(|b| BoardMenuEntry::Board {
                name: b.name.as_str().to_owned(),
                current: Some(b.id) == current,
            })
            .collect();
        entries.push(BoardMenuEntry::Separator);
        entries.push(BoardMenuEntry::New);
        entries.push(BoardMenuEntry::Rename);
        entries.push(BoardMenuEntry::Delete {
            enabled: boards.len() > 1,
        });
        entries
    }

    /// The user chose row `index` of the board menu: a board to switch to, or an action. New and
    /// Rename open the board editor (its own window, shared with the Main window); Delete only
    /// asks, and [`confirm_board_delete`](Self::confirm_board_delete) moves the board to the Trash.
    pub fn choose_board(&mut self, index: usize) -> Vec<Effect> {
        self.menu = PanelMenu::None;
        let Some(entry) = self.panel_board_menu_entries().into_iter().nth(index) else {
            return Vec::new();
        };
        match entry {
            BoardMenuEntry::Board { .. } => {
                if let Some(id) = self.model.boards().get(index).map(|b| b.id) {
                    self.switch_board(id);
                }
                Vec::new()
            }
            BoardMenuEntry::New => {
                self.panel_new_board();
                vec![Effect::OpenListEditor]
            }
            BoardMenuEntry::Rename => {
                if let Some(id) = self.model.board() {
                    self.panel_edit_board(id);
                    vec![Effect::OpenListEditor]
                } else {
                    Vec::new()
                }
            }
            BoardMenuEntry::Delete { enabled: true } => {
                if let Some(id) = self.model.board() {
                    self.menu = PanelMenu::DeleteBoard(id);
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// The board delete confirmation's view while it asks.
    pub(super) fn board_delete_view(&self) -> Option<BoardDeleteView> {
        let PanelMenu::DeleteBoard(id) = self.menu else {
            return None;
        };
        let board = self.store.board(id).ok()?;
        let count = self.store.lists(id).ok()?.iter().fold(0, |n, l| {
            n + self.store.items(l.id).map_or(0, |items| items.len())
        });
        Some(BoardDeleteView {
            name: board.name.as_str().to_owned(),
            count,
        })
    }

    /// "Delete board" in the Panel's confirmation: the board, its lists and their items move to
    /// the Trash, with an Undo toast.
    pub fn confirm_board_delete(&mut self) {
        let PanelMenu::DeleteBoard(id) = self.menu else {
            return;
        };
        self.menu = PanelMenu::None;
        match self.store.board(id) {
            Ok(board) if self.model.boards().len() > 1 => {
                self.trash_board(id, board.name.as_str().to_owned());
            }
            Ok(_) => {}
            Err(_) => self.show_toast(failed()),
        }
    }

    /// Cancel, Esc or a click on the scrim: the confirmation closes and the board stays.
    pub fn cancel_board_delete(&mut self) {
        if matches!(self.menu, PanelMenu::DeleteBoard(_)) {
            self.menu = PanelMenu::None;
        }
    }

    /// The `+` tab and the "New list" button of an empty board: adds a list to the shown board
    /// (or, in a library without boards, makes the starter board) and shows it.
    pub fn add_list(&mut self) {
        let now = self.clock.now();
        self.main.mark_stale();
        let added = crate::starter::add_list(&mut self.store, self.model.board(), now);
        let shown = added.and_then(|(board, list)| {
            self.model.select_board(&self.store, board)?;
            self.model.select_list(&self.store, list)
        });
        match shown {
            Ok(()) => self.show_toast(super::success(ToastMessage::ListAdded)),
            Err(_) => self.show_toast(failed()),
        }
    }

    /// Reads the library again into the Panel (and tells the Main window to).
    pub(super) fn refresh_panel(&mut self) -> snipdeck_store::Result<()> {
        self.main.mark_stale();
        #[cfg(test)]
        if FAIL_REFRESH.get() {
            return Err(snipdeck_store::Error::NotFound {
                entity: "list",
                id: "a test".into(),
            });
        }
        self.model.refresh(&self.store)
    }

    /// [`refresh_panel`](Self::refresh_panel) for a change that has already worked (a lock, an
    /// import, a setting): a failure is logged and the user is told that the list on screen may
    /// be out of date, instead of the failure going unseen. Returns whether the refresh worked.
    /// `what` names the caller in the log.
    pub(super) fn refresh_or_notice(&mut self, what: &str) -> bool {
        match self.refresh_panel() {
            Ok(()) => true,
            Err(error) => {
                crate::trace!("refresh after {what} failed: {error}");
                self.show_toast(Toast {
                    kind: ToastKind::Danger,
                    message: ToastMessage::NotRefreshed,
                });
                false
            }
        }
    }

    /// Like [`refresh_or_notice`](Self::refresh_or_notice) but only logs: the caller has a
    /// success of its own to show, and the next refresh puts the rows right.
    pub(super) fn refresh_logged(&mut self, what: &str) {
        if let Err(error) = self.refresh_panel() {
            crate::trace!("refresh after {what} failed: {error}");
        }
    }

    pub(super) fn switch_board(&mut self, board: BoardId) {
        if self.model.select_board(&self.store, board).is_err() {
            self.show_toast(failed());
        }
    }
}
