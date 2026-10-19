//! The command palette's glue to the Main window: builds the rows the palette shows (the
//! translated words from `CommandTexts`, the shortcuts and the filter from [`crate::commands`]) and
//! fills them into the window.

use slint::{Global, Model};
use snipdeck_ui as ui;

use crate::{
    binding::model,
    commands::{self, Command, Context},
    controller::MainScreen,
    main_window::Section,
    settings::Hotkeys,
};

/// The `(label, synonyms)` of every command in the language selected for the UI now, in the order
/// of [`Command::ALL`].
#[must_use]
pub fn texts(window: &ui::MainWindow) -> Vec<(String, String)> {
    ui::CommandTexts::get(window)
        .get_all()
        .iter()
        .map(|t| (t.label.to_string(), t.words.to_string()))
        .collect()
}

/// What the palette may offer for the screen the Main window shows now.
#[must_use]
pub fn context<'a>(hotkeys: &'a Hotkeys, screen: Option<&MainScreen>) -> Context<'a> {
    let row = screen.and_then(|s| s.rows.get(s.cursor.min(s.rows.len().saturating_sub(1))));
    Context {
        hotkeys,
        has_item: row.is_some(),
        item_pinned: row.is_some_and(|r| r.pinned),
        in_trash: screen.is_some_and(|s| s.section == Section::Trash),
    }
}

/// The rows for `query`, as the window draws them.
#[must_use]
pub fn rows(
    window: &ui::MainWindow,
    hotkeys: &Hotkeys,
    screen: Option<&MainScreen>,
    query: &str,
) -> Vec<ui::PaletteRow> {
    commands::entries(&texts(window), &context(hotkeys, screen), query)
        .into_iter()
        .map(|entry| ui::PaletteRow {
            id: entry.command.id(),
            label: entry.label.as_str().into(),
            keys: model(entry.keys.iter().map(|k| k.as_str().into())),
            spoken: entry.spoken.as_str().into(),
        })
        .collect()
}

/// Fills the palette of `window` with the rows for `query`.
pub fn fill(window: &ui::MainWindow, hotkeys: &Hotkeys, screen: Option<&MainScreen>, query: &str) {
    window.set_palette_rows(model(rows(window, hotkeys, screen, query).into_iter()));
}

/// The command a palette id names.
#[must_use]
pub fn command_of(id: i32) -> Option<Command> {
    Command::from_id(id)
}
