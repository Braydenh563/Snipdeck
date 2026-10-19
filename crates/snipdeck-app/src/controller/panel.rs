//! The Panel's own menu, its row actions and its window frame: what the kebab offers, what
//! edit / rename / delete do to a row, and how a resize or a move is clamped and remembered per
//! monitor (DESIGN.md §5 Panel). Part of the [`Controller`]; the window is the host's.

use snipdeck_core::{ItemId, UseMode};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::{ItemRepo, TrashRepo, Trashable};

use super::{Controller, Effect, Library, PanelMenu, UseResult, failed};
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
    /// The Help window on its Shortcuts tab (Ctrl+/, "Keyboard shortcuts").
    Shortcuts,
}

/// One row of the Panel's kebab menu, in the order it is drawn. The menu is flat: a row's
/// index in [`APP_MENU`] is the index the window reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMenuEntry {
    /// Open the Main window.
    Open,
    /// Open the Main window on its command palette ("All commands…", Ctrl+K).
    Commands,
    /// Make a new snippet.
    NewSnippet,
    /// Open Settings.
    Settings,
    /// Dock the Panel to the edge nearest to it, or undock it when it is docked (the row's
    /// words follow [`View::docked`](super::View::docked)).
    Dock,
    /// Switch the global hotkeys off and on (checked while they are off).
    PauseHotkeys,
    /// A line between groups.
    Separator,
    /// Help: the troubleshooting window.
    Troubleshoot,
    /// Help: the user guide.
    UserGuide,
    /// Help: the keyboard shortcuts (Ctrl+/).
    Shortcuts,
    /// Help: About Snipdeck.
    About,
    /// Exit the app.
    Quit,
}

/// One row of the context menu of an item's card or pinned chip, in the order it is drawn
/// (the Main window's item menu, less its list and tag choosers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemMenuEntry {
    /// Use the item (paste, or copy where it cannot paste).
    Use,
    /// Copy the item.
    Copy,
    /// Edit the item in the item editor.
    Edit,
    /// Pin the item, or unpin it when `pinned`.
    Pin {
        /// The item is pinned now (the row reads "Unpin").
        pinned: bool,
    },
    /// Move the item to the Trash, with an Undo toast.
    Trash,
    /// A line between groups.
    Separator,
}

/// One row of the context menu of a list tab, in the order it is drawn. The Panel draws the
/// words itself (they are `@tr` there), so the order here is the order in `panel.slint`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMenuEntry {
    /// Rename, recolour or re-sort the list in the list editor.
    Edit,
    /// Delete the list: the list editor opens with its confirmation showing.
    Delete,
}

/// The list tab menu's rows.
pub const LIST_MENU: [ListMenuEntry; 2] = [ListMenuEntry::Edit, ListMenuEntry::Delete];

/// The kebab menu's rows. Help is a group of three rows under a line, because the Panel's
/// `Menu` has no sub-menus.
pub const APP_MENU: [AppMenuEntry; 12] = [
    AppMenuEntry::Open,
    AppMenuEntry::Commands,
    AppMenuEntry::NewSnippet,
    AppMenuEntry::Settings,
    AppMenuEntry::Dock,
    AppMenuEntry::PauseHotkeys,
    AppMenuEntry::Separator,
    AppMenuEntry::Troubleshoot,
    AppMenuEntry::UserGuide,
    AppMenuEntry::Shortcuts,
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
        self.menu = PanelMenu::App;
    }

    /// Esc, or a click outside the menu: closes it without a choice.
    pub fn close_app_menu(&mut self) {
        if self.menu == PanelMenu::App {
            self.menu = PanelMenu::None;
        }
    }

    /// The user chose row `index` of [`APP_MENU`]. The menu closes (a line stays open).
    pub fn choose_app_menu(&mut self, index: usize) -> Vec<Effect> {
        let Some(entry) = APP_MENU.get(index).copied() else {
            self.menu = PanelMenu::None;
            return Vec::new();
        };
        if entry == AppMenuEntry::Separator {
            return Vec::new();
        }
        self.menu = PanelMenu::None;
        match entry {
            AppMenuEntry::Open => vec![Effect::OpenMainWindow],
            AppMenuEntry::Commands => vec![Effect::OpenCommands],
            AppMenuEntry::NewSnippet => vec![Effect::NewItem],
            AppMenuEntry::Settings => vec![Effect::OpenSettings],
            AppMenuEntry::Dock => {
                self.dock_toggle();
                self.take_pending()
            }
            AppMenuEntry::PauseHotkeys => {
                self.session.hotkeys_paused = !self.session.hotkeys_paused;
                vec![Effect::SetHotkeysPaused(self.session.hotkeys_paused)]
            }
            AppMenuEntry::Troubleshoot => vec![Effect::OpenHelp(HelpTopic::Troubleshoot)],
            AppMenuEntry::UserGuide => vec![Effect::OpenHelp(HelpTopic::Guide)],
            AppMenuEntry::Shortcuts => vec![Effect::OpenHelp(HelpTopic::Shortcuts)],
            AppMenuEntry::About => vec![Effect::OpenHelp(HelpTopic::About)],
            AppMenuEntry::Quit => vec![Effect::Quit],
            AppMenuEntry::Separator => Vec::new(),
        }
    }

    /// F1 in the Panel: asks the host for `topic` (the same effect as the kebab's Help rows).
    pub fn open_help(&mut self, topic: HelpTopic) -> Vec<Effect> {
        vec![Effect::OpenHelp(topic)]
    }

    // ---- The item menu -------------------------------------------------------------------------

    /// The row of item `id` in the list or the pinned strip.
    fn menu_row(&self, id: ItemId) -> Option<crate::panel::Row> {
        self.model
            .rows()
            .into_iter()
            .chain(self.model.pinned())
            .find(|r| r.id == id)
    }

    /// The rows of the item menu for `id`.
    pub(super) fn item_menu_entries(&self, id: ItemId) -> Vec<ItemMenuEntry> {
        let pinned = self.menu_row(id).is_some_and(|r| r.pinned);
        vec![
            ItemMenuEntry::Use,
            ItemMenuEntry::Copy,
            ItemMenuEntry::Separator,
            ItemMenuEntry::Edit,
            ItemMenuEntry::Pin { pinned },
            ItemMenuEntry::Separator,
            ItemMenuEntry::Trash,
        ]
    }

    /// Right-click, the Menu key or Shift+F10 on card `index` (or on pinned chip `index` when
    /// `in_strip`): opens the item menu.
    pub fn open_item_menu(&mut self, index: usize, in_strip: bool) {
        let rows = if in_strip {
            self.model.pinned()
        } else {
            self.model.rows()
        };
        if let Some(row) = rows.get(index) {
            self.menu = PanelMenu::Item(row.id);
        }
    }

    /// Esc, or a click outside the menu: closes it without a choice.
    pub fn close_item_menu(&mut self) {
        if matches!(self.menu, PanelMenu::Item(_)) {
            self.menu = PanelMenu::None;
        }
    }

    /// The user chose row `index` of the item menu. The menu closes (a line stays open).
    pub fn choose_item_menu(&mut self, index: usize) -> Vec<Effect> {
        let PanelMenu::Item(id) = self.menu else {
            return Vec::new();
        };
        let Some(entry) = self.item_menu_entries(id).get(index).copied() else {
            self.menu = PanelMenu::None;
            return Vec::new();
        };
        if entry == ItemMenuEntry::Separator {
            return Vec::new();
        }
        self.menu = PanelMenu::None;
        match entry {
            ItemMenuEntry::Use => {
                let used: UseResult = self.use_item(id, UseMode::Paste);
                self.dock_after_use(&used)
            }
            ItemMenuEntry::Copy => {
                self.use_item(id, UseMode::Copy);
                Vec::new()
            }
            ItemMenuEntry::Edit => vec![Effect::EditItem(id)],
            ItemMenuEntry::Pin { pinned } => {
                self.set_item_pinned(id, !pinned);
                Vec::new()
            }
            ItemMenuEntry::Trash => {
                self.trash_with_undo(id);
                Vec::new()
            }
            ItemMenuEntry::Separator => Vec::new(),
        }
    }

    // ---- The list tab menu ---------------------------------------------------------------------

    /// Right-click, the Menu key or Shift+F10 on list tab `index`: opens the list menu.
    pub fn open_list_menu(&mut self, index: usize) {
        if let Some(tab) = self.model.tabs().get(index) {
            self.menu = PanelMenu::List(tab.id);
        }
    }

    /// A double-click on list tab `index`: the list editor, without the menu.
    pub fn edit_list_tab(&mut self, index: usize) -> Vec<Effect> {
        let Some(id) = self.model.tabs().get(index).map(|t| t.id) else {
            return Vec::new();
        };
        self.menu = PanelMenu::None;
        self.panel_edit_list(id, false);
        vec![Effect::OpenListEditor]
    }

    /// Esc, or a click outside the menu: closes it without a choice.
    pub fn close_list_menu(&mut self) {
        if matches!(self.menu, PanelMenu::List(_)) {
            self.menu = PanelMenu::None;
        }
    }

    /// The user chose row `index` of [`LIST_MENU`]. The list editor (its own window, drawn
    /// from the Main window's screen) is how the list is renamed and deleted, so the Main
    /// window and the Panel share one path: a delete asks there, then moves the list and its
    /// items to the Trash with an Undo toast.
    pub fn choose_list_menu(&mut self, index: usize) -> Vec<Effect> {
        let PanelMenu::List(id) = self.menu else {
            return Vec::new();
        };
        self.menu = PanelMenu::None;
        match LIST_MENU.get(index) {
            Some(ListMenuEntry::Edit) => self.panel_edit_list(id, false),
            Some(ListMenuEntry::Delete) => self.panel_edit_list(id, true),
            None => return Vec::new(),
        }
        vec![Effect::OpenListEditor]
    }

    fn set_item_pinned(&mut self, id: ItemId, pin: bool) {
        let now = self.clock.now();
        let done = self.store.atomically(|b| {
            let mut item = b.item(id)?;
            item.pinned = pin;
            item.updated_at = now;
            b.update_item(&item, None)?;
            Ok(())
        });
        match done {
            Ok(()) => self.library_changed(),
            Err(_) => self.show_toast(failed()),
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
        if let Some(id) = self.row_id(index) {
            self.trash_with_undo(id);
        }
    }

    /// Moves item `id` to the Trash and offers Undo for 5 s.
    fn trash_with_undo(&mut self, id: ItemId) {
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

    /// Holding Space (or the pointer) on sensitive row `index` reveals its text in the Panel;
    /// letting go hides it again. The same rule as [`main_reveal`](Self::main_reveal): only a
    /// sensitive row reveals.
    pub fn reveal_row(&mut self, index: usize, on: bool) {
        let id = on
            .then(|| {
                self.model
                    .rows()
                    .get(index)
                    .filter(|r| r.sensitive)
                    .map(|r| r.id)
            })
            .flatten();
        self.panel_revealed = id.filter(|_| self.reveal_allowed());
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
        self.remember_frame(monitor, frame, true)
    }

    /// The Panel was only moved to `frame` (a drag in progress or ended). The size is clamped
    /// and the place remembered, but the position is not pulled inside the work area and the
    /// host is not told to place the window: a Panel straddling two monitors must be able to
    /// cross, and answering every `Moved` could loop on rounding. The place is fitted when it
    /// is restored ([`Self::placement`]) and when the Panel is next resized.
    pub fn frame_moved(&mut self, monitor: &Monitor, frame: Rect) -> Vec<Effect> {
        self.remember_frame(monitor, frame, false)
    }

    fn remember_frame(
        &mut self,
        monitor: &Monitor,
        frame: Rect,
        clamp_position: bool,
    ) -> Vec<Effect> {
        if frame.w == 0 || frame.h == 0 {
            // Minimised or not yet laid out: not a place the user chose.
            return Vec::new();
        }
        if self.dock_collapsed() {
            // The dock's handle strip is not a frame the user chose, and clamping it to the
            // Panel's limits would undo the collapse (resizing is off while it shows).
            return Vec::new();
        }
        self.monitor = Some(monitor.clone());
        let mut clamped = frame::clamp(frame, monitor);
        if !clamp_position {
            clamped = Rect {
                x: frame.x,
                y: frame.y,
                ..clamped
            };
        }
        let mut edge = self.stored_geometry(monitor).and_then(|g| g.edge);
        // The first move of a drag that started on a docked Panel takes it off its edge.
        let dragged = std::mem::take(&mut self.drag_started);
        if dragged && edge.is_some_and(|e| !frame::is_flush(e, monitor.work_area, clamped)) {
            edge = None;
        }
        self.update_settings(|s| s.remember_panel_geometry(clamped.geometry(monitor, edge)));
        if clamped == frame || !clamp_position {
            Vec::new()
        } else {
            vec![Effect::PlaceWindow(clamped)]
        }
    }

    /// Where the Panel was last put on `monitor`, fitted to it (it may have changed size). A
    /// docked Panel is placed flush with its edge.
    #[must_use]
    pub fn placement(&self, monitor: &Monitor) -> Option<Rect> {
        let g = self.stored_geometry(monitor)?;
        Some(match g.edge {
            Some(edge) => frame::docked_rect(edge, monitor.work_area, (g.w, g.h), monitor.scale),
            None => frame::clamp(Rect::of(g), monitor),
        })
    }

    /// The Panel is about to be shown on `monitor`: put it where the user left it there.
    pub fn restore_frame(&mut self, monitor: &Monitor) -> Vec<Effect> {
        self.monitor = Some(monitor.clone());
        self.dock_resync();
        self.placement(monitor)
            .map(Effect::PlaceWindow)
            .into_iter()
            .collect()
    }
}
