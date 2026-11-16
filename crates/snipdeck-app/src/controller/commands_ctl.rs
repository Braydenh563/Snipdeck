//! The command palette's half of the [`Controller`]: running a [`Command`] by the same paths the
//! menus, buttons and keys use. Slint-free; `main_binding` calls [`Controller::run_command`] when
//! the palette reports a choice. Commands that need a window the Main window does not own (the Panel,
//! Settings, Help, the tour) queue their effect for the host, like [`Controller::main_open_help`].

use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};

use super::{Controller, Effect, HelpTopic, Library, MainAction};
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

    /// Runs `command`. `cursor` is the row under the keyboard cursor: the item commands act on it
    /// (the selection, when several rows are selected, as the menus do). Commands that only
    /// move focus inside the Main window (Search) are the window's and do nothing here.
    pub fn run_command(&mut self, command: Command, cursor: usize) {
        if let Some(action) = item_action(command) {
            self.main_action(action, cursor);
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
            // (Search moves the window's focus, Tour opens the tour window). The arm stays only so
            // the match is exhaustive.
            // The item commands never get here (`item_action` took them).
            Command::Search
            | Command::Tour
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
            Command::GoTrash => self.main_open(Section::Trash),
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
