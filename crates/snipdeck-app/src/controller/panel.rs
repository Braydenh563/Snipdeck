//! The Panel's own menu, its row actions and its window frame: what the kebab offers, what
//! edit / rename / delete do to a row, and how a resize or a move is clamped and remembered per
//! monitor (DESIGN.md §5 Panel). Part of the [`Controller`]; the window is the host's.

use snipdeck_core::ItemId;
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::{TrashRepo, Trashable};

use super::{Controller, Effect, Library, failed};
use crate::{
    frame::{self, Edge, Monitor, Rect},
    host::ClipboardText,
    toast::{Toast, ToastKind, ToastMessage},
};

/// A page the Help entries open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpTopic {
    /// The troubleshooting window ("Troubleshoot…").
    Troubleshoot,
    /// The user guide.
    Guide,
    /// The About box.
    About,
}

/// One row of the Panel's kebab menu, in the order it is drawn. The menu is flat: a row's
/// index in [`APP_MENU`] is the index the window reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMenuEntry {
    /// Open the Main window.
    Open,
    /// Make a new snippet.
    NewSnippet,
    /// Open Settings.
    Settings,
    /// Switch the global hotkeys off and on (checked while they are off).
    PauseHotkeys,
    /// A line between groups.
    Separator,
    /// Help: the troubleshooting window.
    Troubleshoot,
    /// Help: the user guide.
    UserGuide,
    /// Help: About Snipdeck.
    About,
    /// Exit the app.
    Quit,
}

/// The kebab menu's rows. Help is a group of three rows under a line, because the Panel's
/// `Menu` has no sub-menus.
pub const APP_MENU: [AppMenuEntry; 9] = [
    AppMenuEntry::Open,
    AppMenuEntry::NewSnippet,
    AppMenuEntry::Settings,
    AppMenuEntry::PauseHotkeys,
    AppMenuEntry::Separator,
    AppMenuEntry::Troubleshoot,
    AppMenuEntry::UserGuide,
    AppMenuEntry::About,
    AppMenuEntry::Quit,
];

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    // ---- The kebab menu ------------------------------------------------------------------------

    /// The kebab button (or the Menu key on it): opens the Panel menu.
    pub fn open_app_menu(&mut self) {
        self.board_menu_open = false;
        self.app_menu_open = true;
    }

    /// Esc, or a click outside the menu: closes it without a choice.
    pub fn close_app_menu(&mut self) {
        self.app_menu_open = false;
    }

    /// The user chose row `index` of [`APP_MENU`]. The menu closes (a line stays open).
    pub fn choose_app_menu(&mut self, index: usize) -> Vec<Effect> {
        let Some(entry) = APP_MENU.get(index).copied() else {
            self.app_menu_open = false;
            return Vec::new();
        };
        if entry == AppMenuEntry::Separator {
            return Vec::new();
        }
        self.app_menu_open = false;
        match entry {
            AppMenuEntry::Open => vec![Effect::OpenMainWindow],
            AppMenuEntry::NewSnippet => vec![Effect::NewItem],
            AppMenuEntry::Settings => vec![Effect::OpenSettings],
            AppMenuEntry::PauseHotkeys => {
                self.session.hotkeys_paused = !self.session.hotkeys_paused;
                vec![Effect::SetHotkeysPaused(self.session.hotkeys_paused)]
            }
            AppMenuEntry::Troubleshoot => vec![Effect::OpenHelp(HelpTopic::Troubleshoot)],
            AppMenuEntry::UserGuide => vec![Effect::OpenHelp(HelpTopic::Guide)],
            AppMenuEntry::About => vec![Effect::OpenHelp(HelpTopic::About)],
            AppMenuEntry::Quit => vec![Effect::Quit],
            AppMenuEntry::Separator => Vec::new(),
        }
    }

    // ---- Row actions ---------------------------------------------------------------------------

    fn row_id(&self, index: usize) -> Option<ItemId> {
        self.model.rows().get(index).map(|r| r.id)
    }

    /// Ctrl+E or a double-click on row `index`: the quick editor on that item.
    pub fn edit_row(&mut self, index: usize) -> Vec<Effect> {
        self.row_id(index)
            .map(Effect::EditItem)
            .into_iter()
            .collect()
    }

    /// F2 on row `index`. The Panel has no inline title field, so this opens the same editor.
    pub fn rename_row(&mut self, index: usize) -> Vec<Effect> {
        self.edit_row(index)
    }

    /// Delete on row `index`: moves the item to the Trash and offers Undo for 5 s (the same
    /// Trash and Undo as the Main window; [`main_undo`](Self::main_undo) brings it back).
    pub fn delete_row(&mut self, index: usize) {
        let Some(id) = self.row_id(index) else {
            return;
        };
        let now = self.clock.now();
        let done = self.store.atomically(|b| b.trash(Trashable::Item(id), now));
        if done.is_err() {
            self.show_toast(failed());
            return;
        }
        if self.refresh_panel().is_err() {
            self.show_toast(failed());
            return;
        }
        self.show_undo_toast(
            Toast {
                kind: ToastKind::Neutral,
                message: ToastMessage::ItemsTrashed { count: 1 },
            },
            vec![Trashable::Item(id)],
        );
    }

    // ---- The window frame ----------------------------------------------------------------------

    /// Pointer down on a resize grip: the host starts the OS resize from `edge`.
    pub fn resize(&mut self, edge: Edge) -> Vec<Effect> {
        vec![Effect::ResizeWindow(edge)]
    }

    /// The Panel's frame is now `frame` on `monitor` (after a resize or a move; the host
    /// reports each change). The frame is clamped (width 260 to 480, height to the work
    /// area, inside it) and remembered for that monitor; the settings saver writes it after
    /// its quiet time. When the clamp changed the frame the host is told to place the window
    /// there.
    pub fn frame_changed(&mut self, monitor: &Monitor, frame: Rect) -> Vec<Effect> {
        if frame.w == 0 || frame.h == 0 {
            // Minimised or not yet laid out: not a place the user chose.
            return Vec::new();
        }
        let clamped = frame::clamp(frame, monitor);
        self.update_settings(|s| s.remember_panel_geometry(clamped.geometry(monitor)));
        if clamped == frame {
            Vec::new()
        } else {
            vec![Effect::PlaceWindow(clamped)]
        }
    }

    /// Where the Panel was last put on `monitor`, fitted to it (it may have changed size).
    #[must_use]
    pub fn placement(&self, monitor: &Monitor) -> Option<Rect> {
        self.settings
            .panel_geometry
            .iter()
            .find(|g| g.monitor_id == monitor.id)
            .map(|g| frame::clamp(Rect::of(g), monitor))
    }

    /// The Panel is about to be shown on `monitor`: put it where the user left it there.
    pub fn restore_frame(&mut self, monitor: &Monitor) -> Vec<Effect> {
        self.placement(monitor)
            .map(Effect::PlaceWindow)
            .into_iter()
            .collect()
    }
}
