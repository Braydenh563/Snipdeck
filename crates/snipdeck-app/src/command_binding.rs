//! The command palette's glue to the Main window: builds the rows the palette shows (the
//! translated words from `CommandTexts`, the shortcuts and the filter from [`crate::commands`]) and
//! fills them into the window.

use slint::{Global, Model};
use snipdeck_ui as ui;

use crate::{
    binding::model,
    commands::{self, Command, Context, Place},
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

/// Which optional command families this install offers.
#[derive(Debug, Clone, Copy)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one independent availability flag per command family, read as a checklist"
)]
pub struct Offers {
    /// The Symbols tab is on.
    pub symbols: bool,
    /// The Colours tab is on.
    pub colours: bool,
    /// This machine has a screen eyedropper.
    pub eyedropper: bool,
    /// The "Colours" list exists.
    pub saved_colours: bool,
    /// The Panel is on screen.
    pub panel_shown: bool,
    /// "Keep Panel open" is on.
    pub keep_open: bool,
    /// There is a step to take back.
    pub can_undo: bool,
    /// There is a step to bring back.
    pub can_redo: bool,
    /// The library has a master password.
    pub lock_set: bool,
    /// The Panel asked for the palette with a row selected: whether that item is pinned.
    pub panel_item: Option<bool>,
}

/// What the palette may offer for the screen the Main window shows now.
#[must_use]
pub fn context<'a>(
    hotkeys: &'a Hotkeys,
    screen: Option<&MainScreen>,
    offers: Offers,
) -> Context<'a> {
    let row = screen.and_then(|s| s.rows.get(s.cursor.min(s.rows.len().saturating_sub(1))));
    Context {
        hotkeys,
        // A group's header is a row but not an item.
        has_item: offers.panel_item.is_some() || row.is_some_and(|r| r.header.is_none()),
        item_pinned: offers
            .panel_item
            .unwrap_or_else(|| row.is_some_and(|r| r.pinned)),
        in_trash: offers.panel_item.is_none()
            && screen.is_some_and(|s| s.section == Section::Trash),
        symbols: offers.symbols,
        colours: offers.colours,
        eyedropper: offers.eyedropper,
        saved_colours: offers.saved_colours,
        in_list: screen.is_some_and(|s| matches!(s.section, Section::List(_))),
        group: screen.and_then(|s| s.cursor_group),
        panel_shown: offers.panel_shown,
        keep_open: offers.keep_open,
        can_undo: offers.can_undo,
        can_redo: offers.can_redo,
        lock_set: offers.lock_set,
        trash_full: screen.is_some_and(|s| s.counts.trash > 0),
    }
}

/// The row text of each place: "Go to board “Work”", in the language shown now.
#[must_use]
pub fn place_labels(window: &ui::MainWindow, places: &[Place]) -> Vec<String> {
    let words = ui::CommandTexts::get(window);
    places
        .iter()
        .map(|place| match place {
            Place::Board { name, .. } => words.invoke_go_to_board(name.as_str().into()),
            Place::List {
                name, board_name, ..
            } => words.invoke_go_to_list(name.as_str().into(), board_name.as_str().into()),
            Place::Tag { name, .. } => words.invoke_go_to_tag(name.as_str().into()),
        })
        .map(|label| label.to_string())
        .collect()
}

/// The rows for `query`, as the window draws them: the commands, then the boards, lists and tags
/// of `places` that match.
#[must_use]
pub fn rows(
    window: &ui::MainWindow,
    hotkeys: &Hotkeys,
    screen: Option<&MainScreen>,
    offers: Offers,
    places: &[Place],
    query: &str,
) -> Vec<ui::PaletteRow> {
    let words = ui::CommandTexts::get(window);
    let labels = place_labels(window, places);
    let place_rows = commands::matching_places(&labels, query)
        .into_iter()
        .filter_map(|i| {
            let id = commands::PLACE_BASE.checked_add(i32::try_from(i).ok()?)?;
            Some(ui::PaletteRow {
                id,
                label: labels.get(i)?.as_str().into(),
                keys: model(std::iter::empty()),
                ..Default::default()
            })
        });
    commands::entries(
        &texts(window),
        &crate::wording::key_words(window),
        &context(hotkeys, screen, offers),
        query,
    )
    .into_iter()
    .map(|entry| {
        let (mark_before, mark_text) = entry.mark.clone().unwrap_or_default();
        let hint = match entry.hint {
            Some(commands::Hint::SymbolsTabOff) => words.get_hint_symbols_off(),
            Some(commands::Hint::ColoursTabOff) => words.get_hint_colours_off(),
            None => "".into(),
        };
        ui::PaletteRow {
            id: entry.command.id(),
            label: entry.label.as_str().into(),
            keys: model(entry.keys.iter().map(|k| k.as_str().into())),
            spoken: entry.spoken.as_str().into(),
            mark_before: mark_before.into(),
            mark_text: mark_text.into(),
            hint,
        }
    })
    .chain(place_rows)
    .collect()
}

/// Fills the palette of `window` with the rows for `query`.
pub fn fill(
    window: &ui::MainWindow,
    hotkeys: &Hotkeys,
    screen: Option<&MainScreen>,
    offers: Offers,
    places: &[Place],
    query: &str,
) {
    window.set_palette_rows(model(
        rows(window, hotkeys, screen, offers, places, query).into_iter(),
    ));
}

/// The command a palette id names.
#[must_use]
pub fn command_of(id: i32) -> Option<Command> {
    Command::from_id(id)
}
