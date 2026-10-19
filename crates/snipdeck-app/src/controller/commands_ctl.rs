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
};

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// Runs `command`. `cursor` is the row under the keyboard cursor: the item commands act on it
    /// (the selection, when several rows are selected, as the menus do). Commands that only
    /// move focus inside the Main window (Search) are the window's and do nothing here.
    pub fn run_command(&mut self, command: Command, cursor: usize) {
        match command {
            Command::NewItem => self.open_new_item(Origin::Main),
            Command::NewList => self.main_new_list(),
            // The window moves its own focus; the binding opens the tour window
            // (`onboarding_binding::request_open`).
            Command::Search | Command::Tour => {}
            Command::ShowPanel => {
                let effects = self.show();
                self.pending.extend(effects);
            }
            Command::OpenPicker => self.open_picker(),
            Command::QuickCapture => self.quick_capture(),
            Command::Settings => self.pending.push(Effect::OpenSettings),
            Command::UserGuide => self.main_open_help(HelpTopic::Guide),
            Command::Shortcuts => self.main_open_help(HelpTopic::Shortcuts),
            Command::Troubleshoot => self.main_open_help(HelpTopic::Troubleshoot),
            Command::About => self.main_open_help(HelpTopic::About),
            Command::ViewComfortable => self.main_set_view(ViewMode::Comfortable),
            Command::ViewCompact => self.main_set_view(ViewMode::Compact),
            Command::ViewTile => self.main_set_view(ViewMode::Tile),
            Command::UseItem => {
                self.main_action(MainAction::Use, cursor);
            }
            Command::CopyItem => {
                self.main_action(MainAction::Copy, cursor);
            }
            Command::EditItem => {
                self.main_action(MainAction::Edit, cursor);
            }
            Command::PinItem | Command::UnpinItem => {
                self.main_action(MainAction::Pin, cursor);
            }
            Command::TrashItem => {
                self.main_action(MainAction::Delete, cursor);
            }
            Command::GoAll => self.main_open(Section::All),
            Command::GoPinned => self.main_open(Section::Pinned),
            Command::GoRecent => self.main_open(Section::Recent),
            Command::GoTrash => self.main_open(Section::Trash),
        }
    }
}
