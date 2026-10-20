//! The Panel's settings, gestures (tabs, drops, moves, the board menu) and row activation.

use std::time::Duration;

use snipdeck_core::{BoardId, ItemId, Placement, UseMode};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};

use super::{Controller, Library, PanelMenu, UseResult, failed, neutral, policy_of};
use crate::{
    host::ClipboardText,
    main_window::reorder,
    settings::{Settings, Theme},
    settings_store::{Poll, SettingsSaver},
    toast::{Toast, ToastKind, ToastMessage},
};

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

    /// The theme setting (`system`, `light`, `dark`).
    pub fn set_theme(&mut self, theme: Theme) {
        self.update_settings(|s| s.theme = theme);
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

    /// The pin button: keep the Panel on top or not.
    pub fn set_on_top(&mut self, on_top: bool) {
        self.on_top = on_top;
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
        match self.store.move_item(item, target.id, Placement::Last) {
            Ok(()) => {
                self.library_changed();
                if self.refresh_panel().is_err() {
                    self.show_toast(failed());
                    return;
                }
                self.show_toast(neutral(ToastMessage::ItemsMoved {
                    count: 1,
                    list: target.name,
                }));
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
        self.panel_place(moved, delta, false);
    }

    /// Alt+Up / Alt+Down in the Panel: moves the selected row one place.
    pub fn panel_move_selected(&mut self, delta: isize) {
        if let Some(id) = self.model.selected() {
            self.panel_move_item(id, delta);
        }
    }

    /// Moves item `id` by `delta` places in the shown list and says where it went.
    pub(crate) fn panel_move_item(&mut self, id: ItemId, delta: isize) {
        self.panel_place(id, delta, true);
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

    /// Moves `moved` by `delta` places, keeps it selected and, when `announce`, raises a toast
    /// with its new place.
    pub(super) fn panel_place(&mut self, moved: ItemId, delta: isize, announce: bool) {
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
        match self.store.move_item(moved, list, placement) {
            Ok(()) => {
                self.library_changed();
                if self.refresh_panel().is_err() {
                    self.show_toast(failed());
                    return;
                }
                self.model.select_item(moved);
                if announce {
                    self.announce_placed(to + 1, order.len());
                }
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

    /// The user chose board `index` in the menu.
    pub fn choose_board(&mut self, index: usize) {
        self.menu = PanelMenu::None;
        let Some(id) = self.model.boards().get(index).map(|b| b.id) else {
            return;
        };
        self.switch_board(id);
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
        if shown.is_err() {
            self.show_toast(failed());
        }
    }

    /// Reads the library again into the Panel (and tells the Main window to).
    pub(super) fn refresh_panel(&mut self) -> snipdeck_store::Result<()> {
        self.main.mark_stale();
        self.model.refresh(&self.store)
    }

    pub(super) fn switch_board(&mut self, board: BoardId) {
        if self.model.select_board(&self.store, board).is_err() {
            self.show_toast(failed());
        }
    }
}
