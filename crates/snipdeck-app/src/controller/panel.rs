//! The Panel's own menu, its row actions and its window frame: what the kebab offers, what
//! edit / rename / delete do to a row, and how a resize or a move is clamped and remembered per
//! monitor (DESIGN.md §5 Panel). Part of the [`Controller`]; the window is the host's.

use snipdeck_core::{ItemId, UseMode, template::transform::Transform};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::ItemRepo;

use super::{Controller, Effect, Library, ListDeleteView, PanelMenu, UseResult, failed, success};
use crate::{
    frame::{self, Edge, Monitor, Rect},
    host::ClipboardText,
    settings::PanelGeometry,
    toast::ToastMessage,
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
    /// The Help window on its Tips tab (the tray's "Tips", the tour's last page).
    Tips,
}

/// One row of the Panel's kebab menu, in the order it is drawn. The menu is flat: a row's
/// index in [`APP_MENU`] is the index the window reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMenuEntry {
    /// Take back the latest step of the history (Ctrl+Z); disabled while there is none.
    Undo,
    /// Redo the step taken back (Ctrl+Y); disabled while there is none.
    Redo,
    /// Start the paste stack of the marked items, or end it while it runs (checked while it
    /// runs; disabled while nothing is marked). A Supporter feature, as everywhere.
    PasteStack,
    /// Open the Main window.
    Open,
    /// Open the Main window on its command palette ("Commands…", Ctrl+K).
    Commands,
    /// Make a new snippet.
    NewSnippet,
    /// Take a screenshot: the Panel steps aside while the snipping overlay is up.
    Screenshot,
    /// Open Settings.
    Settings,
    /// Dock the Panel to the edge nearest to it, or undock it when it is docked (the row's
    /// words follow [`View::dock_edge`](super::View::dock_edge)).
    Dock,
    /// Switch the global hotkeys off and on (checked while they are off).
    PauseHotkeys,
    /// Keep the Panel on top of other windows (checked while it is). The header button does the same;
    /// at large text sizes the button leaves the header and this row is the only way (audit 2, item 1).
    KeepOnTop,
    /// Open the header's Select / Delete strip in Select mode ("Clear list…" lives there).
    SelectOrDelete,
    /// Forget the remembered size and position on this monitor and put the Panel back at the default.
    ResetGeometry,
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
    /// Open the "Paste as..." choices (plain text, UPPERCASE, ...) for the item.
    PasteAs,
    /// Show the item's colour in the Colours tab (a colour item only).
    OpenInColours,
    /// Edit the item in the item editor.
    Edit,
    /// Pin the item, or unpin it when `pinned`.
    Pin {
        /// The item is pinned now (the row reads "Unpin").
        pinned: bool,
    },
    /// Add the item to the paste stack, or take it out when `marked`.
    Stack {
        /// The item is marked now (the row reads "Remove from paste stack").
        marked: bool,
    },
    /// Put a copy of the item right after it.
    Duplicate,
    /// Open the lists to move the item (or the marked items) to.
    MoveTo,
    /// Open the colour labels to give the item (or the marked items).
    Colour,
    /// Turn the Sensitive flag of the item on or off.
    Sensitive {
        /// The item is sensitive now (the row is checked).
        on: bool,
        /// Only text, code, link and template items can be sensitive.
        enabled: bool,
    },
    /// Move the item one place up in the shown list (the keyboard way is Alt+Up).
    MoveUp {
        /// The list is in manual order, no search runs and the item is not first.
        enabled: bool,
    },
    /// Move the item one place down in the shown list (Alt+Down).
    MoveDown {
        /// The list is in manual order, no search runs and the item is not last.
        enabled: bool,
    },
    /// Move the item to the Trash, with an Undo toast.
    Trash,
    /// Group the marked items (or this one) under a new header.
    Group,
    /// On a group header: rename it in place.
    RenameGroup,
    /// On a group header: fold the group, or open it when `folded` (the keys are Left, Right, Enter, Space).
    ToggleGroup {
        /// The group is folded now (the row reads "Unfold group").
        folded: bool,
    },
    /// On a group header: move the group one place up (Alt+Up).
    GroupUp {
        /// The list is in manual order, no search runs and the group is not first.
        enabled: bool,
    },
    /// On a group header: move the group one place down (Alt+Down).
    GroupDown {
        /// The list is in manual order, no search runs and the group is not last.
        enabled: bool,
    },
    /// On a group header: remove the header, keep the items.
    Ungroup,
    /// On a group header: move the header and its items to the Trash.
    DeleteGroup,
    /// A line between groups.
    Separator,
}

/// One row of the context menu of a list tab, in the order it is drawn. The Panel draws the
/// words itself (they are `@tr` there), so the order here is the order in `panel.slint`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMenuEntry {
    /// Rename, recolour or re-sort the list in the list editor.
    Edit,
    /// Delete the list: the Panel asks in its own confirmation; no editor opens.
    Delete,
    /// A line between groups.
    Separator,
    /// "Sort by": the list orders itself so. The Panel's `Menu` has no sub-menus, so the three
    /// orders are three rows under a line; the open list's own is checked.
    SortBy(snipdeck_core::ListSort),
    /// "Copy as CSS variables": the list's colours as a `:root` block (disabled while it holds none).
    CopyCss,
}

/// The list tab menu's rows.
pub const LIST_MENU: [ListMenuEntry; 8] = [
    ListMenuEntry::Edit,
    ListMenuEntry::Delete,
    ListMenuEntry::Separator,
    ListMenuEntry::SortBy(snipdeck_core::ListSort::Manual),
    ListMenuEntry::SortBy(snipdeck_core::ListSort::MostUsed),
    ListMenuEntry::SortBy(snipdeck_core::ListSort::RecentlyUsed),
    ListMenuEntry::Separator,
    ListMenuEntry::CopyCss,
];

/// The kebab menu's rows. Help is a group of three rows under a line, because the Panel's
/// `Menu` has no sub-menus.
pub const APP_MENU: [AppMenuEntry; 21] = [
    AppMenuEntry::Undo,
    AppMenuEntry::Redo,
    AppMenuEntry::Separator,
    AppMenuEntry::PasteStack,
    AppMenuEntry::Dock,
    AppMenuEntry::Shortcuts,
    AppMenuEntry::Commands,
    AppMenuEntry::Settings,
    AppMenuEntry::Separator,
    AppMenuEntry::Open,
    AppMenuEntry::NewSnippet,
    AppMenuEntry::Screenshot,
    AppMenuEntry::PauseHotkeys,
    AppMenuEntry::KeepOnTop,
    AppMenuEntry::SelectOrDelete,
    AppMenuEntry::ResetGeometry,
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
        self.menu = PanelMenu::App;
    }

    /// Esc, or a click outside the menu: closes it without a choice.
    pub fn close_app_menu(&mut self) {
        if self.menu == PanelMenu::App {
            self.menu = PanelMenu::None;
        }
    }

    /// The footer's "Open Snipdeck" button: the Main window.
    pub fn open_main_window(&mut self) -> Vec<Effect> {
        self.menu = PanelMenu::None;
        vec![Effect::OpenMainWindow]
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
            AppMenuEntry::Undo => {
                self.undo();
                Vec::new()
            }
            AppMenuEntry::Redo => {
                self.redo();
                Vec::new()
            }
            AppMenuEntry::PasteStack => {
                if self.stack_running {
                    self.end_stack();
                } else {
                    self.start_stack();
                }
                Vec::new()
            }
            AppMenuEntry::Open => vec![Effect::OpenMainWindow],
            AppMenuEntry::Commands => vec![Effect::OpenCommands],
            AppMenuEntry::NewSnippet => vec![Effect::NewItem],
            AppMenuEntry::Screenshot => self.take_screenshot(),
            AppMenuEntry::Settings => vec![Effect::OpenSettings],
            AppMenuEntry::Dock => {
                self.dock_toggle();
                self.take_pending()
            }
            AppMenuEntry::PauseHotkeys => {
                self.set_hotkeys_paused(!self.session.hotkeys_paused);
                vec![Effect::SetHotkeysPaused(self.session.hotkeys_paused)]
            }
            AppMenuEntry::KeepOnTop => {
                self.choose_on_top(!self.on_top);
                Vec::new()
            }
            AppMenuEntry::SelectOrDelete => {
                self.panel_select_mode(true);
                Vec::new()
            }
            AppMenuEntry::ResetGeometry => self.reset_panel_geometry(),
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
        let row = self.menu_row(id);
        if row.as_ref().is_some_and(|r| r.header.is_some()) {
            let at = self.model.rows().iter().position(|r| r.id == id);
            return vec![
                ItemMenuEntry::RenameGroup,
                ItemMenuEntry::ToggleGroup {
                    folded: at
                        .and_then(|at| self.model.group_at(at))
                        .is_some_and(|run| run.group.collapsed),
                },
                ItemMenuEntry::GroupUp {
                    enabled: at.is_some_and(|at| self.can_move_group(at, -1)),
                },
                ItemMenuEntry::GroupDown {
                    enabled: at.is_some_and(|at| self.can_move_group(at, 1)),
                },
                ItemMenuEntry::Ungroup,
                ItemMenuEntry::Separator,
                ItemMenuEntry::DeleteGroup,
            ];
        }
        let pinned = row.as_ref().is_some_and(|r| r.pinned);
        let mut entries = vec![ItemMenuEntry::Use, ItemMenuEntry::Copy];
        // Only text can be transformed.
        if row
            .as_ref()
            .is_some_and(|r| crate::item_editor::is_text_kind(r.kind))
        {
            entries.push(ItemMenuEntry::PasteAs);
        }
        if self.settings.show_colours_tab && self.colour_of_item(id).is_some() {
            entries.push(ItemMenuEntry::OpenInColours);
        }
        entries.extend([
            ItemMenuEntry::Separator,
            ItemMenuEntry::Edit,
            ItemMenuEntry::Duplicate,
            ItemMenuEntry::MoveTo,
            ItemMenuEntry::Colour,
            ItemMenuEntry::Sensitive {
                on: row.as_ref().is_some_and(|r| r.sensitive),
                enabled: row
                    .as_ref()
                    .is_some_and(|r| crate::item_editor::is_text_kind(r.kind)),
            },
            ItemMenuEntry::Pin { pinned },
            ItemMenuEntry::Stack {
                marked: self.stack.contains(&id),
            },
            ItemMenuEntry::MoveUp {
                enabled: self.can_move_item(id, -1),
            },
            ItemMenuEntry::MoveDown {
                enabled: self.can_move_item(id, 1),
            },
            ItemMenuEntry::Group,
            ItemMenuEntry::Separator,
            ItemMenuEntry::Trash,
        ]);
        entries
    }

    /// Right-click, the Menu key or Shift+F10 on card `index` (or on pinned chip `index` when
    /// `in_strip`): opens the item menu.
    pub fn open_item_menu(&mut self, index: usize, in_strip: bool) {
        let rows = if in_strip {
            self.model.pinned()
        } else {
            self.model.rows()
        };
        if let Some(row) = rows.get(index).filter(|r| !r.deleted_pending) {
            self.menu_targets = if in_strip {
                vec![row.id]
            } else {
                self.model.targets(index)
            };
            self.menu = PanelMenu::Item(row.id);
        }
    }

    /// Esc, or a click outside the menu: closes it without a choice.
    pub fn close_item_menu(&mut self) {
        if matches!(
            self.menu,
            PanelMenu::Item(_)
                | PanelMenu::Move
                | PanelMenu::Colour
                | PanelMenu::PasteAs(_)
                | PanelMenu::Symbol
        ) {
            self.menu = PanelMenu::None;
            self.symbol_menu = None;
        }
    }

    /// The user chose row `index` of the item menu. The menu closes (a line stays open).
    #[allow(clippy::too_many_lines, reason = "one arm per row of the item menu")]
    pub fn choose_item_menu(&mut self, index: usize) -> Vec<Effect> {
        self.choose_item_menu_as(index, false)
    }

    /// [`choose_item_menu`](Self::choose_item_menu) with Shift: a colour's "Paste as" copies the
    /// format to the clipboard instead of pasting it.
    pub fn choose_item_menu_as(&mut self, index: usize, copy: bool) -> Vec<Effect> {
        if let PanelMenu::PasteAs(id) = self.menu {
            return self.choose_paste_as(id, index, copy);
        }
        if matches!(self.menu, PanelMenu::Move | PanelMenu::Colour) {
            self.choose_second_level(index);
            return Vec::new();
        }
        let PanelMenu::Item(id) = self.menu else {
            return Vec::new();
        };
        let Some(entry) = self.item_menu_entries(id).get(index).copied() else {
            self.menu = PanelMenu::None;
            return Vec::new();
        };
        if entry == ItemMenuEntry::Separator
            || matches!(entry, ItemMenuEntry::Sensitive { enabled: false, .. })
        {
            return Vec::new();
        }
        // Move to, Colour and Paste as open the second level in the menu's place.
        if matches!(
            entry,
            ItemMenuEntry::MoveTo | ItemMenuEntry::Colour | ItemMenuEntry::PasteAs
        ) {
            self.menu = match entry {
                ItemMenuEntry::MoveTo => PanelMenu::Move,
                ItemMenuEntry::PasteAs => PanelMenu::PasteAs(id),
                _ => PanelMenu::Colour,
            };
            return Vec::new();
        }
        self.menu = PanelMenu::None;
        let targets = std::mem::take(&mut self.menu_targets);
        match entry {
            ItemMenuEntry::Sensitive { .. } => {
                self.toggle_item_sensitive(id);
                Vec::new()
            }
            ItemMenuEntry::Use => {
                let used: UseResult = self.use_item(id, UseMode::Paste);
                self.after_use(&used)
            }
            ItemMenuEntry::Copy => {
                let used = self.use_item(id, UseMode::Copy);
                self.after_use(&used)
            }
            ItemMenuEntry::OpenInColours => self.open_item_in_colours(id),
            ItemMenuEntry::Edit => vec![Effect::EditItem(id)],
            ItemMenuEntry::Duplicate => {
                self.duplicate(&targets);
                Vec::new()
            }
            ItemMenuEntry::Pin { pinned } => {
                self.set_item_pinned(id, !pinned);
                Vec::new()
            }
            ItemMenuEntry::Stack { .. } => {
                self.toggle_stack_mark(id);
                Vec::new()
            }
            ItemMenuEntry::MoveUp { .. } | ItemMenuEntry::MoveDown { .. } => {
                self.choose_move(id, entry);
                Vec::new()
            }
            ItemMenuEntry::Trash => {
                self.trash_with_undo(&targets);
                self.model.clear_marks();
                Vec::new()
            }
            ItemMenuEntry::Group => {
                if let Some(at) = self.model.rows().iter().position(|r| r.id == id) {
                    self.group_marked(at);
                }
                Vec::new()
            }
            ItemMenuEntry::RenameGroup
            | ItemMenuEntry::ToggleGroup { .. }
            | ItemMenuEntry::GroupUp { .. }
            | ItemMenuEntry::GroupDown { .. }
            | ItemMenuEntry::Ungroup
            | ItemMenuEntry::DeleteGroup => {
                self.choose_group_entry(id, entry);
                Vec::new()
            }
            ItemMenuEntry::MoveTo
            | ItemMenuEntry::Colour
            | ItemMenuEntry::PasteAs
            | ItemMenuEntry::Separator => Vec::new(),
        }
    }

    /// Entry `index` of item `id`'s "Paste as" menu: a colour's format (copied with Shift), or a text
    /// transform.
    fn choose_paste_as(&mut self, id: ItemId, index: usize, copy: bool) -> Vec<Effect> {
        self.menu = PanelMenu::None;
        self.menu_targets.clear();
        if let Some(c) = self.colour_of_item(id) {
            let Some(format) = snipdeck_colour::Format::ALL.get(index).copied() else {
                return Vec::new();
            };
            let mode = if copy { UseMode::Copy } else { UseMode::Paste };
            let used = self.use_colour_item(id, c, format, mode);
            return self.after_use(&used);
        }
        let Some(transform) = Transform::ALL.get(index).copied() else {
            return Vec::new();
        };
        let used = self.use_item_as(id, transform);
        self.after_use(&used)
    }

    /// The item menu's Move up / Move down on item `id`.
    fn choose_move(&mut self, id: ItemId, entry: ItemMenuEntry) {
        let (ItemMenuEntry::MoveUp { enabled } | ItemMenuEntry::MoveDown { enabled }) = entry
        else {
            return;
        };
        let delta = if matches!(entry, ItemMenuEntry::MoveUp { .. }) {
            -1
        } else {
            1
        };
        if enabled {
            self.panel_move_item(id, delta);
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
        self.panel_edit_list(id);
        vec![Effect::OpenListEditor]
    }

    /// Esc, or a click outside the menu: closes it without a choice.
    pub fn close_list_menu(&mut self) {
        if matches!(self.menu, PanelMenu::List(_)) {
            self.menu = PanelMenu::None;
        }
    }

    /// The user chose row `index` of [`LIST_MENU`]. A line stays open. Sort by changes the sort of the list
    /// (the field the List editor edits). Edit opens the list editor (its own window,
    /// shared with the Main window). Delete only asks: the Panel shows its confirmation and
    /// nothing else (O52), and [`confirm_list_delete`](Self::confirm_list_delete) moves the list
    /// and its items to the Trash with an Undo toast.
    pub fn choose_list_menu(&mut self, index: usize) -> Vec<Effect> {
        let PanelMenu::List(id) = self.menu else {
            return Vec::new();
        };
        self.menu = PanelMenu::None;
        match LIST_MENU.get(index) {
            Some(ListMenuEntry::Edit) => {
                self.panel_edit_list(id);
                vec![Effect::OpenListEditor]
            }
            Some(ListMenuEntry::Delete) => {
                self.menu = PanelMenu::DeleteList(id);
                Vec::new()
            }
            Some(ListMenuEntry::SortBy(sort)) => {
                self.set_list_sort(id, *sort);
                Vec::new()
            }
            Some(ListMenuEntry::CopyCss) => {
                self.copy_list_as_css(id);
                Vec::new()
            }
            Some(ListMenuEntry::Separator) => {
                self.menu = PanelMenu::List(id);
                Vec::new()
            }
            None => Vec::new(),
        }
    }

    /// The list delete confirmation's view while it asks: the list's name and how many items
    /// go to the Trash with it.
    pub(super) fn list_delete_view(&self) -> Option<ListDeleteView> {
        let PanelMenu::DeleteList(id) = self.menu else {
            return None;
        };
        let list = self.store.list(id).ok()?;
        Some(ListDeleteView {
            name: list.name.as_str().to_owned(),
            count: self.store.items(id).map_or(0, |items| items.len()),
        })
    }

    /// "Delete list" in the Panel's confirmation: the list and its items move to the Trash,
    /// with an Undo toast.
    pub fn confirm_list_delete(&mut self) {
        let PanelMenu::DeleteList(id) = self.menu else {
            return;
        };
        self.menu = PanelMenu::None;
        match self.store.list(id) {
            Ok(list) => self.trash_list(id, list.name.as_str().to_owned()),
            Err(_) => self.show_toast(failed()),
        }
    }

    /// Cancel, Esc or a click on the scrim: the confirmation closes and the list stays.
    pub fn cancel_list_delete(&mut self) {
        if matches!(self.menu, PanelMenu::DeleteList(_)) {
            self.menu = PanelMenu::None;
        }
    }

    fn set_item_pinned(&mut self, id: ItemId, pin: bool) {
        let now = self.clock.now();
        let mut changes = Vec::new();
        let done = self.store.atomically(|b| {
            changes.clear();
            let before = b.item(id)?;
            let mut item = before.clone();
            item.pinned = pin;
            item.updated_at = now;
            super::history_ctl::update_recorded(b, &before, &item, &mut changes)
        });
        match done {
            Ok(()) => {
                self.record_step(crate::history::Action::Pin, changes);
                self.show_toast(success(ToastMessage::Pinned(pin)));
                self.library_changed();
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    // ---- Row actions ---------------------------------------------------------------------------

    fn row_id(&self, index: usize) -> Option<ItemId> {
        self.live_row(index).map(|r| r.id)
    }

    /// Ctrl+E or a double-click on row `index`: the quick editor on that item.
    pub fn edit_row(&mut self, index: usize) -> Vec<Effect> {
        self.row_id(index)
            .map(Effect::EditItem)
            .into_iter()
            .collect()
    }

    /// Ctrl+D on row `index`: a copy of the item right after it.
    pub fn duplicate_row(&mut self, index: usize) {
        if let Some(id) = self.row_id(index) {
            self.duplicate(&[id]);
        }
    }

    /// Ctrl+P on row `index`: pins the item, or unpins it when it is pinned.
    pub fn pin_row(&mut self, index: usize) {
        if let Some(id) = self.row_id(index) {
            let pinned = self.menu_row(id).is_some_and(|r| r.pinned);
            self.set_item_pinned(id, !pinned);
        }
    }

    /// F2 on row `index`. The Panel has no inline title field, so this opens the same editor.
    pub fn rename_row(&mut self, index: usize) -> Vec<Effect> {
        if let Some(run) = self.model.group_at(index) {
            self.start_group_rename(super::group_ctl::Surface::Panel, run.group.id);
            return Vec::new();
        }
        self.edit_row(index)
    }

    /// Delete on row `index`: moves the item to the Trash and offers Undo for 5 s (the same
    /// Trash and Undo as the Main window; [`main_undo`](Self::main_undo) brings it back).
    pub fn delete_row(&mut self, index: usize) {
        if self.model.marked_count() > 0 {
            self.bulk_delete();
        } else if let Some(id) = self.row_id(index) {
            self.trash_with_undo(&[id]);
        }
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

    /// Pointer down on a resize grip: the host starts the OS resize from `edge`. A docked Panel
    /// refuses its outer edge and that edge's corners ([`frame::dock_resize_allowed`], and with the full edge on, its ends too); the grips
    /// are not drawn there, this guards a stale press.
    pub fn resize(&mut self, edge: Edge) -> Vec<Effect> {
        if self
            .dock_edge()
            .is_some_and(|d| !frame::dock_resize_allowed(edge, d, self.settings.dock.full_edge))
        {
            return Vec::new();
        }
        self.resizing = true;
        self.resize_button_seen = false;
        self.resize_loop_seen = false;
        self.resize_at = self.clock.now();
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

    /// The Panel was only moved to `frame` (by the system: the app's own drag reports nothing
    /// here). A floating Panel's place is remembered, but not pulled inside the work area and
    /// the host is not told to place the window: answering every `Moved` could loop on
    /// rounding. The place is fitted when it is restored ([`Self::placement`]).
    pub fn frame_moved(&mut self, monitor: &Monitor, frame: Rect) -> Vec<Effect> {
        self.remember_frame(monitor, frame, false)
    }

    fn remember_frame(&mut self, monitor: &Monitor, frame: Rect, resized: bool) -> Vec<Effect> {
        if frame.w == 0 || frame.h == 0 || frame::is_parked(frame) {
            // Minimised (Windows parks it far off screen), or not yet laid out: not a place
            // the user chose, and storing it would change the size the Panel comes back with.
            return Vec::new();
        }
        if resized && self.resizing {
            // A resize under way says so with each new size.
            self.resize_at = self.clock.now();
        }
        if self.dragging() {
            // A drag remembers its own end.
            return Vec::new();
        }
        if self.dock_collapsed() {
            // The dock's tab is not a frame the user chose. But the system may have moved it:
            // its monitor went away, or that monitor's work area changed. The dock then
            // follows what is remembered for where it is now.
            if self.monitor.as_ref() == Some(monitor) {
                return Vec::new();
            }
            return self.dock_on_monitor(monitor);
        }
        self.monitor = Some(monitor.clone());
        let stored = self.stored_geometry(monitor).cloned();
        let Some((edge, old)) = stored.as_ref().and_then(|g| Some((g.edge?, g))) else {
            let mut clamped = frame::clamp(frame, monitor);
            if !resized {
                clamped = Rect {
                    x: frame.x,
                    y: frame.y,
                    ..clamped
                };
            }
            let geometry = PanelGeometry {
                docked: stored.as_ref().and_then(|g| g.docked),
                docked_flat: stored.as_ref().and_then(|g| g.docked_flat),
                ..clamped.geometry(monitor, None)
            }
            .remembering(stored.as_ref());
            self.update_settings(|s| s.remember_panel_geometry(geometry));
            return if clamped == frame {
                Vec::new()
            } else {
                vec![Effect::PlaceWindow(clamped)]
            };
        };
        // Docked: only a resize changes anything, and only the docked size and where the Panel
        // sits along its edge. The floating frame is left alone.
        let across = if edge.vertical() { frame.w } else { frame.h };
        if !resized || across <= frame::scaled(frame::HANDLE_THICKNESS, monitor.scale) * 2 {
            // A move, or a late report of the tucked tab.
            return Vec::new();
        }
        let full = self.settings.dock.full_edge && edge.vertical();
        let dock = |full_edge| {
            frame::docked_rect(
                edge,
                monitor.work_area,
                (frame.w, frame.h),
                monitor.scale,
                Some(frame.along(edge).start),
                full_edge,
            )
        };
        let place = dock(self.settings.dock.full_edge);
        let mut geometry = self.geometry_on(monitor);
        if full {
            // A full-edge dock reports the work area's height back; the Panel's own height and
            // place along the edge stay remembered, so turning the full edge off gives them back.
            let (_, h) = self.dock_size(old, edge, monitor, old.along);
            geometry.set_docked_size(edge, (place.w, h));
        } else {
            geometry.set_docked_size(edge, (place.w, place.h));
            geometry.along = Some(place.along(edge).start);
        }
        self.update_settings(|s| s.remember_panel_geometry(geometry));
        if place == frame {
            Vec::new()
        } else {
            vec![Effect::PlaceWindow(place)]
        }
    }

    /// Where the Panel was last put on `monitor`, fitted to it (it may have changed size). A
    /// docked Panel is placed flush with its edge. A monitor the Panel was never left on (the
    /// one it was on is gone, or changed resolution) gets the newest place stored anywhere,
    /// at its floating size and brought inside this monitor's work area.
    #[must_use]
    pub fn placement(&self, monitor: &Monitor) -> Option<Rect> {
        let Some(g) = self.stored_geometry(monitor) else {
            let mut last = self.settings.panel_geometry.last()?.clone();
            // An older version's full-height dock is not as tall as this screen.
            super::dock_ctl::mend_full_height(&mut last, monitor);
            return Some(frame::clamp(Rect::of(&last), monitor));
        };
        Some(match g.edge {
            Some(edge) => self.dock_open(g, edge, monitor),
            None => frame::clamp(Rect::of(g), monitor),
        })
    }

    /// The Panel is about to be shown and its window cannot say what monitor it is on: it does
    /// not exist before the first show (the host makes it once its event loop runs). Places it
    /// at the anchor a [`show`](Self::show) read, once; else, the first time, where it was
    /// left on the monitor it was last left on (`find_monitor`), so that
    /// the window is made there and not wherever the system puts a new one.
    pub fn restore_anchor(&mut self) -> Vec<Effect> {
        let Some(anchor) = self.session.open_anchor.take() else {
            if !self.unplaced {
                return Vec::new();
            }
            // Asked now, not when the app started: a monitor may have gone since. With none
            // the Panel was ever on left, it opens on the one the pointer is on.
            let pointed = |ctl: &Self| {
                let (x, y) = ctl.target.cursor()?;
                ctl.target.monitor_at(x, y)
            };
            return match self.find_monitor().or_else(|| pointed(self)) {
                Some(monitor) => self.restore_on(&monitor),
                None => Vec::new(),
            };
        };
        self.unplaced = false;
        let (x, y) = anchor.point();
        let Some(fallback) = self
            .monitor
            .clone()
            .or_else(|| self.target.monitor_at(x, y))
        else {
            return Vec::new();
        };
        let (on, frame) = self.frame_at(anchor, &fallback);
        if self.stored_geometry(&on).is_some_and(|g| g.edge.is_some()) {
            // The Panel is docked on the monitor under the anchor: it opens at its edge there,
            // not floating at the anchor with that edge's dock still in force.
            return self.restore_on(&on);
        }
        self.monitor = Some(on);
        vec![Effect::PlaceWindow(frame)]
    }

    /// The Panel is about to be shown on `monitor`: put it where the user left it there.
    ///
    /// A [`show`](Self::show) that read an anchor ("Panel opens" at the mouse or the text
    /// cursor) places it there instead, once.
    pub fn restore_frame(&mut self, monitor: &Monitor) -> Vec<Effect> {
        if self.session.open_anchor.is_some() {
            return self.restore_anchor();
        }
        self.restore_on(monitor)
    }

    /// Puts the Panel where the user left it on `monitor`.
    fn restore_on(&mut self, monitor: &Monitor) -> Vec<Effect> {
        self.monitor = Some(monitor.clone());
        self.unplaced = false;
        self.dock_resync();
        self.placement(monitor)
            .map(Effect::PlaceWindow)
            .into_iter()
            .collect()
    }
}
