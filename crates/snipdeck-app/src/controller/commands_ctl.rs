//! The command palette's half of the [`Controller`]: running a [`Command`] by the same paths the
//! menus, buttons and keys use. Slint-free; `main_binding` calls [`Controller::run_command`] when
//! the palette reports a choice. Commands that need a window the Main window does not own (the Panel,
//! Settings, Help, the tour) queue their effect for the host, like [`Controller::main_open_help`].

use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};

use super::{Controller, Effect, HelpTopic, Library, MainAction, SettingsAction};
use crate::{
    commands::Command,
    host::ClipboardText,
    item_editor::Origin,
    main_window::{Section, ViewMode},
    settings::{RowDensity, Theme},
};

/// The item action a command stands for (the commands that act on the row under the cursor).
const fn item_action(command: Command) -> Option<MainAction> {
    Some(match command {
        Command::UseItem => MainAction::Use,
        Command::UsePlain => MainAction::UsePlain,
        Command::CopyItem => MainAction::Copy,
        Command::DuplicateItem => MainAction::Duplicate,
        Command::EditItem => MainAction::Edit,
        Command::PinItem | Command::UnpinItem => MainAction::Pin,
        Command::TrashItem => MainAction::Delete,
        Command::RestoreItem => MainAction::Restore,
        Command::DeleteForever => MainAction::DeleteForever,
        _ => return None,
    })
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// "Try it" on a card of Help > Tips: runs the card's command, and brings the Main window
    /// forward first when the command shows its result there.
    pub fn try_tip(&mut self, tip: crate::tips::Tip) {
        let action = tip.action;
        if action.opens_main() {
            self.pending.push(Effect::OpenMainWindow);
        }
        self.run_command(action.command(), 0);
    }

    /// The Panel's Ctrl+K and "Commands…": the palette opens in the Main window, and its item
    /// commands act on the row the Panel had selected.
    pub fn ask_commands(&mut self) -> Vec<Effect> {
        self.menu = super::PanelMenu::None;
        self.palette_armed = self.model.selected();
        vec![Effect::OpenCommands]
    }

    /// The Main window's palette opened: it acts on the Panel's row when the Panel asked for it,
    /// on Main's own cursor otherwise.
    pub fn palette_opened(&mut self) {
        self.palette_item = self.palette_armed.take();
    }

    /// Whether the open palette acts on a Panel row, and whether that item is pinned.
    #[must_use]
    pub fn palette_item_pinned(&self) -> Option<bool> {
        let id = self.palette_item?;
        self.store.item(id).ok().map(|i| i.pinned)
    }

    /// `action` on the one item `id` (the Panel's row), by the paths the menus use.
    fn item_action_on(&mut self, id: snipdeck_core::ItemId, action: MainAction) {
        match action {
            MainAction::Use => {
                self.use_item(id, snipdeck_core::UseMode::Paste);
            }
            MainAction::UsePlain => {
                self.use_item(id, snipdeck_core::UseMode::PastePlain);
            }
            MainAction::Copy => {
                self.copy_items(&[id]);
            }
            MainAction::Edit => self.open_item_editor(id),
            MainAction::Pin => self.pin(&[id]),
            MainAction::Duplicate => self.duplicate(&[id]),
            MainAction::Delete => self.trash_items(&[id]),
            // The Panel has no Trash and no choosers.
            MainAction::Restore
            | MainAction::DeleteForever
            | MainAction::Move
            | MainAction::Tag
            | MainAction::PasteAs
            | MainAction::OpenLink => {}
        }
    }

    /// Every board, list and tag by name, in library order: what the palette can go to.
    #[must_use]
    pub fn palette_places(&self) -> Vec<crate::commands::Place> {
        use crate::commands::Place;
        let mut places = Vec::new();
        let boards = self.store.boards().unwrap_or_default();
        for board in &boards {
            places.push(Place::Board {
                id: board.id,
                name: board.name.to_string(),
            });
        }
        for board in &boards {
            for list in self.store.lists(board.id).unwrap_or_default() {
                places.push(Place::List {
                    board: board.id,
                    id: list.id,
                    name: list.name.to_string(),
                    board_name: board.name.to_string(),
                });
            }
        }
        for tag in self.store.tags().unwrap_or_default() {
            places.push(Place::Tag {
                id: tag.id,
                name: tag.name.to_string(),
            });
        }
        places
    }

    /// The palette chose `place`: the Main window shows that board, list or tag.
    pub fn main_go(&mut self, place: &crate::commands::Place) {
        use crate::commands::Place;
        self.main_fresh();
        match place {
            Place::Board { id, .. } => {
                self.open_board(*id);
                self.revealed = None;
            }
            Place::List { board, id, .. } => {
                if self.main.board() != Some(*board) {
                    self.open_board(*board);
                }
                self.main_open(Section::List(*id));
            }
            Place::Tag { id, .. } => self.main_open(Section::Tag(*id)),
        }
    }

    /// Opens Settings and starts `action` there.
    fn settings_action(&mut self, action: SettingsAction) {
        self.pending.push(Effect::SettingsAction(action));
    }

    /// An item command acts on the Panel's row when the palette was asked for by the Panel (for one
    /// command), else on the Main window's selection. False when `command` is no item command.
    fn run_item_command(&mut self, command: Command, cursor: usize) -> bool {
        let panel_item = self.palette_item.take();
        let Some(action) = item_action(command) else {
            return false;
        };
        match panel_item {
            Some(id) => self.item_action_on(id, action),
            None => {
                self.main_action(action, cursor);
            }
        }
        true
    }

    /// Runs `command`. `cursor` is the row under the keyboard cursor: the item commands act on it
    /// (the selection, when several rows are selected, as the menus do). Commands that only
    /// move focus inside the Main window (Search) are the window's and do nothing here.
    pub fn run_command(&mut self, command: Command, cursor: usize) {
        if self.run_item_command(command, cursor) {
            return;
        }
        match command {
            Command::NewItem => self.open_new_item(Origin::Main),
            Command::NewList => self.main_new_list(),
            Command::ToggleSidebar => {
                let collapsed = self.settings.main_sidebar_collapsed;
                self.set_main_sidebar_collapsed(!collapsed);
            }
            // Not the controller's: `MainBinding::run_command` handles both before it gets here
            // (Search moves the window's focus, Tour opens the tour window, the group commands open a dialog). The arm stays only so
            // the match is exhaustive.
            // The item commands never get here (`item_action` took them).
            Command::Search
            | Command::Tour
            | Command::ColourGroup
            | Command::NestGroup
            | Command::UseItem
            | Command::CopyItem
            | Command::DuplicateItem
            | Command::EditItem
            | Command::PinItem
            | Command::UnpinItem
            | Command::TrashItem
            | Command::UsePlain
            | Command::RestoreItem
            | Command::DeleteForever => {}
            Command::ImportLibrary => self.settings_action(SettingsAction::Import),
            Command::ExportLibrary => self.settings_action(SettingsAction::Export),
            Command::BackUpNow => self.settings_action(SettingsAction::BackUp),
            Command::ShowPanel | Command::HidePanel => self.main_toggle_panel(),
            Command::Undo => self.undo(),
            Command::Redo => self.redo(),
            Command::NewBoard => self.main_new_board(),
            Command::Lock => self.lock_now(),
            Command::Quit => self.pending.push(Effect::Quit),
            Command::Symbols => {
                if self.settings.show_symbols_tab {
                    let mut effects = self.show();
                    effects.push(Effect::ShowSymbols);
                    self.pending.extend(effects);
                }
            }
            Command::Colours | Command::PickColour => {
                if self.settings.show_colours_tab {
                    let mut effects = self.show();
                    effects.push(Effect::ShowColours);
                    if command == Command::PickColour {
                        effects.extend(self.start_eyedropper());
                    }
                    self.pending.extend(effects);
                }
            }
            Command::SavedColours => {
                if self.settings.show_colours_tab {
                    let effects = self.open_saved_colours();
                    self.pending.extend(effects);
                }
            }
            Command::OpenPicker => self.open_picker(),
            Command::QuickCapture => self.quick_capture(),
            Command::PasteNext => {
                self.paste_next();
            }
            Command::GroupItems => self.main_group_marked(cursor),
            Command::FoldGroup => self.main_group_collapse(cursor, None),
            Command::RenameGroup => self.main_group_rename_start(cursor),
            Command::UngroupGroup => self.main_group_ungroup(cursor),
            Command::DeleteGroup => self.main_group_delete(cursor),
            Command::MoveGroupUp => self.main_move_group(cursor, -1),
            Command::MoveGroupDown => self.main_move_group(cursor, 1),
            Command::SelectAll => self.main_select_all(),
            Command::TakeScreenshot => {
                let effects = self.take_screenshot();
                self.pending.extend(effects);
            }
            Command::Settings => self.pending.push(Effect::OpenSettings),
            Command::UserGuide => self.main_open_help(HelpTopic::Guide),
            Command::Shortcuts => self.main_open_help(HelpTopic::Shortcuts),
            Command::Troubleshoot => self.main_open_help(HelpTopic::Troubleshoot),
            Command::About => self.main_open_help(HelpTopic::About),
            Command::Support => self.pending.push(Effect::OpenSupport),
            Command::DensityComfortable => self.set_row_density(RowDensity::Comfortable),
            Command::DensityCompact => self.set_row_density(RowDensity::Compact),
            Command::ViewTile => self.main_set_view(ViewMode::Tile),
            Command::ThemeSystem => self.choose_theme(Theme::System),
            Command::ThemeLight => self.choose_theme(Theme::Light),
            Command::ThemeDark => self.choose_theme(Theme::Dark),
            Command::GoAll => self.main_open(Section::All),
            Command::GoPinned => self.main_open(Section::Pinned),
            Command::GoRecent => self.main_open(Section::Recent),
            Command::GoTrash | Command::EmptyTrash => self.main_open(Section::Trash),
            Command::KeepPanelOpen | Command::LetPanelClose => {
                let on_top = !self.on_top;
                self.choose_on_top(on_top);
            }
            Command::ResetPanel => {
                let placed = self.reset_panel_geometry();
                self.pending.extend(placed);
            }
        }
    }
}
