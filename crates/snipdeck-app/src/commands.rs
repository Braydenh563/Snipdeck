//! The command palette's catalogue: every action the Main window offers, the shortcut each has, and
//! the filter that narrows the list as the user types.
//!
//! Slint-free. The words come from the UI crate's `CommandTexts` global (translated there, in the order
//! of [`Command::ALL`]); the shortcuts come from here, and the three global hotkeys read the chords in
//! the settings, so a rebound hotkey shows as rebound.

use crate::{
    search_mark,
    settings::{HotkeyAction, Hotkeys},
};

/// One thing the palette can run. The order of [`Command::ALL`] is the order of the UI crate's
/// `CommandTexts.all`, and the id the palette reports is the index in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// A new snippet in the open list.
    NewItem,
    /// A new list on the open board.
    NewList,
    /// Move to the search field.
    Search,
    /// Show the Panel.
    ShowPanel,
    /// Show the Panel on its Symbols tab (emoji, kaomoji, icons).
    Symbols,
    /// Show the Panel on its Colours tab.
    Colours,
    /// Show the Panel on the list the Colours tab saves to ("Saved colours").
    SavedColours,
    /// Show the Panel on its Colours tab and start the screen eyedropper.
    PickColour,
    /// Open the Picker.
    OpenPicker,
    /// Quick capture.
    QuickCapture,
    /// Paste the next item of the paste stack (or of the pinned items or the open list).
    PasteNext,
    /// Take a screenshot with the Windows snipping overlay.
    TakeScreenshot,
    /// Open Settings.
    Settings,
    /// Open the user guide.
    UserGuide,
    /// Show the welcome tour.
    Tour,
    /// Show the keyboard shortcuts.
    Shortcuts,
    /// Open the troubleshooting window.
    Troubleshoot,
    /// Open About.
    About,
    /// Open the optional "Support Snipdeck" page in the browser.
    Support,
    /// Comfortable rows (the row density setting).
    DensityComfortable,
    /// Compact rows (the row density setting).
    DensityCompact,
    /// Tiles.
    ViewTile,
    /// Follow the Windows light or dark setting.
    ThemeSystem,
    /// Always light.
    ThemeLight,
    /// Always dark.
    ThemeDark,
    /// Use the item under the cursor.
    UseItem,
    /// Copy the item under the cursor.
    CopyItem,
    /// Copy the item under the cursor into a new item after it.
    DuplicateItem,
    /// Edit the item under the cursor.
    EditItem,
    /// Pin the item under the cursor.
    PinItem,
    /// Unpin the item under the cursor.
    UnpinItem,
    /// Move the item under the cursor to the Trash.
    TrashItem,
    /// Select every item in the open list.
    SelectAll,
    /// Open All items.
    GoAll,
    /// Open Pinned.
    GoPinned,
    /// Open Recent.
    GoRecent,
    /// Open the Trash.
    GoTrash,
    /// Forget the Panel's remembered size and position on its monitor and put it back to the default.
    ResetPanel,
    /// Keep the Panel open (on top) or let it close when the user clicks away: switches the pin.
    KeepPanelOpen,
    /// Fold the Main sidebar to its rail, or open it again (Ctrl+B).
    ToggleSidebar,
    /// Group the selected items (the item under the cursor when none is selected) in the open list (Ctrl+G).
    GroupItems,
    /// Fold the group under the cursor, or unfold it.
    FoldGroup,
    /// Rename the group under the cursor (F2).
    RenameGroup,
    /// Dissolve the group under the cursor and keep its items.
    UngroupGroup,
    /// Move the group under the cursor and its items to the Trash (Del).
    DeleteGroup,
    /// Move the group under the cursor up one place (Alt+Up).
    MoveGroupUp,
    /// Move the group under the cursor down one place (Alt+Down).
    MoveGroupDown,
    /// Hide the Panel (offered while it is shown; "Show Panel" while it is not).
    HidePanel,
    /// Let the Panel close when the user clicks away (offered while "keep open" is on).
    LetPanelClose,
    /// Take back the latest step (Ctrl+Z).
    Undo,
    /// Bring back the step Undo took back (Ctrl+Y).
    Redo,
    /// A new board.
    NewBoard,
    /// Lock the library now (offered with a master password).
    Lock,
    /// Quit Snipdeck.
    Quit,
    /// Paste the item under the cursor as plain text (Alt+Enter).
    UsePlain,
    /// Bring the item under the cursor back from the Trash.
    RestoreItem,
    /// Delete the item under the cursor from the Trash for good.
    DeleteForever,
    /// Open Settings and ask which library to import.
    ImportLibrary,
    /// Open Settings and export the library to a folder.
    ExportLibrary,
    /// Open Settings and make a backup now.
    BackUpNow,
    /// Open the Trash and ask to empty it (offered while the Trash holds something).
    EmptyTrash,
    /// Colour the group under the cursor (opens the colour dialog; the window owns the dialog).
    ColourGroup,
    /// Nest the group under the cursor in another group, or take it out to the top level (opens the
    /// chooser; the window owns it).
    NestGroup,
}

impl Command {
    /// Every command, in the order of `CommandTexts.all`.
    pub const ALL: [Self; 63] = [
        Self::NewItem,
        Self::NewList,
        Self::Search,
        Self::ShowPanel,
        Self::Symbols,
        Self::Colours,
        Self::SavedColours,
        Self::PickColour,
        Self::OpenPicker,
        Self::QuickCapture,
        Self::PasteNext,
        Self::TakeScreenshot,
        Self::Settings,
        Self::UserGuide,
        Self::Tour,
        Self::Shortcuts,
        Self::Troubleshoot,
        Self::About,
        Self::Support,
        Self::DensityComfortable,
        Self::DensityCompact,
        Self::ViewTile,
        Self::ThemeSystem,
        Self::ThemeLight,
        Self::ThemeDark,
        Self::UseItem,
        Self::CopyItem,
        Self::DuplicateItem,
        Self::EditItem,
        Self::PinItem,
        Self::UnpinItem,
        Self::TrashItem,
        Self::SelectAll,
        Self::GoAll,
        Self::GoPinned,
        Self::GoRecent,
        Self::GoTrash,
        Self::ResetPanel,
        Self::KeepPanelOpen,
        Self::ToggleSidebar,
        Self::GroupItems,
        Self::FoldGroup,
        Self::RenameGroup,
        Self::UngroupGroup,
        Self::DeleteGroup,
        Self::MoveGroupUp,
        Self::MoveGroupDown,
        Self::ColourGroup,
        Self::NestGroup,
        Self::HidePanel,
        Self::LetPanelClose,
        Self::Undo,
        Self::Redo,
        Self::NewBoard,
        Self::Lock,
        Self::Quit,
        Self::UsePlain,
        Self::RestoreItem,
        Self::DeleteForever,
        Self::ImportLibrary,
        Self::ExportLibrary,
        Self::BackUpNow,
        Self::EmptyTrash,
    ];

    /// The command with palette id `id`.
    #[must_use]
    pub fn from_id(id: i32) -> Option<Self> {
        usize::try_from(id)
            .ok()
            .and_then(|i| Self::ALL.get(i).copied())
    }

    /// The palette id (the index in [`Command::ALL`]).
    #[must_use]
    pub fn id(self) -> i32 {
        Self::ALL
            .iter()
            .position(|c| *c == self)
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(0)
    }

    /// The shortcut text ("Ctrl+N"), `None` when the command has none (or its hotkey is off).
    /// The three global hotkeys follow the settings.
    #[must_use]
    pub fn shortcut(self, hotkeys: &Hotkeys) -> Option<String> {
        let fixed = match self {
            Self::NewItem => "Ctrl+N",
            Self::NewList => "Ctrl+Shift+N",
            Self::Search => "Ctrl+F",
            Self::Settings => "Ctrl+,",
            Self::UserGuide => "F1",
            Self::Shortcuts => "Ctrl+/",
            Self::UseItem | Self::FoldGroup => "Enter",
            Self::CopyItem => "Shift+Enter",
            Self::DuplicateItem => "Ctrl+D",
            Self::EditItem => "Ctrl+E",
            Self::PinItem | Self::UnpinItem => "Ctrl+P",
            Self::TrashItem | Self::DeleteGroup | Self::DeleteForever => "Del",
            Self::SelectAll => "Ctrl+A",
            Self::ToggleSidebar => "Ctrl+B",
            Self::GroupItems => "Ctrl+G",
            Self::RenameGroup => "F2",
            Self::MoveGroupUp => "Alt+Up",
            Self::MoveGroupDown => "Alt+Down",
            Self::Undo => "Ctrl+Z",
            Self::Redo => "Ctrl+Y",
            Self::UsePlain => "Alt+Enter",
            Self::ShowPanel => return chord_text(hotkeys, HotkeyAction::TogglePanel),
            Self::OpenPicker => return chord_text(hotkeys, HotkeyAction::Picker),
            Self::QuickCapture => return chord_text(hotkeys, HotkeyAction::Capture),
            Self::PasteNext => return chord_text(hotkeys, HotkeyAction::PasteNext),
            _ => return None,
        };
        Some(fixed.to_owned())
    }

    /// Removes something (Trash, delete): never wins a tie in the palette, so Enter on a name that fits
    /// a destructive command and a harmless one runs the harmless one.
    #[must_use]
    pub const fn destructive(self) -> bool {
        matches!(
            self,
            Self::TrashItem
                | Self::DeleteGroup
                | Self::DeleteForever
                | Self::EmptyTrash
                | Self::Quit
                | Self::Lock
        )
    }

    /// Acts on the item under the cursor, so it is offered only with one there.
    #[must_use]
    pub const fn needs_item(self) -> bool {
        matches!(
            self,
            Self::UseItem
                | Self::CopyItem
                | Self::DuplicateItem
                | Self::EditItem
                | Self::PinItem
                | Self::UnpinItem
                | Self::TrashItem
                | Self::UsePlain
                | Self::RestoreItem
                | Self::DeleteForever
        )
    }
}

/// The configured chord of a global hotkey as text ("Ctrl+Alt+Space"); `None` while it is off.
#[must_use]
pub fn chord_text(hotkeys: &Hotkeys, action: HotkeyAction) -> Option<String> {
    hotkeys.chord(action).map(|c| c.label())
}

/// The caps and the spoken form of a global hotkey's configured chord; both empty while it is off.
#[must_use]
pub fn chord_caps(
    hotkeys: &Hotkeys,
    action: HotkeyAction,
    words: &KeyWords,
) -> (Vec<String>, String) {
    chord_text(hotkeys, action).map_or_else(
        || (Vec::new(), String::new()),
        |text| (caps(&text), spoken(&text, words)),
    )
}

/// The caps of a shortcut: `"Ctrl+Shift+Enter"` becomes `["Ctrl", "Shift", "↵"]`. A `+` key is written
/// `"Ctrl++"` and kept as a cap of its own.
#[must_use]
pub fn caps(shortcut: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut rest = shortcut;
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix('+') {
            // A bare "+" at the start of a token is the plus key; the separator is the next one.
            parts.push("+".into());
            rest = tail.strip_prefix('+').unwrap_or(tail);
            continue;
        }
        let (token, tail) = rest.split_once('+').unwrap_or((rest, ""));
        parts.push(if token == "Enter" {
            "↵".into()
        } else {
            token.into()
        });
        rest = tail;
    }
    parts
}

/// How assistive technology reads the keys whose cap is not a word of its own, in the language
/// selected for the UI (the window layer reads them from `NameTexts`; Rust has no catalogues).
/// Other caps ("Shift", "Alt", "K", "F1") are read as they are written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyWords {
    /// `Ctrl`.
    pub control: String,
    /// The Enter key (`↵`).
    pub enter: String,
    /// `Del`.
    pub delete: String,
    /// `Esc`.
    pub escape: String,
    /// The `+` key.
    pub plus: String,
    /// The `,` key.
    pub comma: String,
    /// The `/` key.
    pub slash: String,
    /// [`PHRASES`] in the language selected for the UI, in the same order.
    pub phrases: Vec<String>,
}

/// The spoken forms of the shortcut rows that are not one chord, and the two words shown instead
/// of keys, in English; the English text is what the shortcut catalogue names a phrase by.
pub const PHRASES: [&str; 22] = [
    "The item’s trigger",
    "Tab while the suggestion hint shows",
    "Alt+Up or Alt+Down",
    "Hold Space",
    "Left, Right or Enter on a group header",
    "Type",
    "Up or Down",
    "PageUp or PageDown",
    "Alt+1 to 9",
    "Shift+Up or Shift+Down",
    "Shift+F10 or the Menu key",
    "Left, Right, Enter or Space on a group header",
    "Alt+Up or Alt+Down on a group header",
    "Tab or Shift+Tab",
    "Unlock with Windows Hello",
    "Trigger",
    "Hello",
    "Ctrl+Y or Ctrl+Shift+Z",
    "Slash",
    "Ctrl+C in the Colours tab",
    "Ctrl+Enter or Ctrl+S",
    "Esc or Ctrl+W",
];

impl KeyWords {
    /// The English words: what tests that do not run a window compare against.
    #[must_use]
    pub fn english() -> Self {
        let word = |w: &str| w.to_owned();
        Self {
            control: word("Control"),
            enter: word("Enter"),
            delete: word("Delete"),
            escape: word("Escape"),
            plus: word("Plus"),
            comma: word("Comma"),
            slash: word("Slash"),
            phrases: PHRASES.iter().map(|p| (*p).to_owned()).collect(),
        }
    }

    /// `english` (one of [`PHRASES`]) in the selected language; anything else is returned as is.
    #[must_use]
    pub fn phrase(&self, english: &str) -> String {
        PHRASES
            .iter()
            .position(|p| *p == english)
            .and_then(|i| self.phrases.get(i))
            .map_or_else(|| english.to_owned(), Clone::clone)
    }
}

/// The spoken form of a shortcut ("Control Shift N") for assistive technology.
#[must_use]
pub fn spoken(shortcut: &str, words: &KeyWords) -> String {
    caps(shortcut)
        .iter()
        .map(|cap| match cap.as_str() {
            "Ctrl" => words.control.as_str(),
            "↵" => words.enter.as_str(),
            "Del" => words.delete.as_str(),
            "Esc" => words.escape.as_str(),
            "+" => words.plus.as_str(),
            "," => words.comma.as_str(),
            "/" => words.slash.as_str(),
            other => other,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// What the palette may offer right now.
#[expect(
    clippy::struct_excessive_bools,
    reason = "one independent availability flag per command family; the palette reads them as a plain checklist"
)]
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    /// The configured hotkeys, for the shortcuts of the global ones.
    pub hotkeys: &'a Hotkeys,
    /// There is an item under the cursor.
    pub has_item: bool,
    /// That item is pinned (offers Unpin instead of Pin).
    pub item_pinned: bool,
    /// The Trash is open (its items are restored or deleted there, not edited or pinned).
    pub in_trash: bool,
    /// The Panel's Symbols tab is on (the `show_symbols_tab` setting).
    pub symbols: bool,
    /// The Panel's Colours tab is on (the `show_colours_tab` setting).
    pub colours: bool,
    /// This machine has a screen eyedropper (Windows).
    pub eyedropper: bool,
    /// The "Colours" list exists (something was saved), so "Saved colours" has somewhere to go.
    pub saved_colours: bool,
    /// A list is open (groups live in lists).
    pub in_list: bool,
    /// The cursor is on a group's header; `None` on an item.
    pub group: Option<CursorGroup>,
    /// The Panel is on screen (offers "Hide Panel" instead of "Show Panel").
    pub panel_shown: bool,
    /// "Keep Panel open" is on (offers "Let Panel close…" instead of "Keep Panel open").
    pub keep_open: bool,
    /// There is a step to take back.
    pub can_undo: bool,
    /// There is a step to bring back.
    pub can_redo: bool,
    /// The library has a master password to lock with.
    pub lock_set: bool,
    /// The Trash holds something to empty.
    pub trash_full: bool,
}

/// Why a command the user may look for is listed but cannot run now. The palette shows the
/// sentence on the row, dimmed, and Enter on it does nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    /// The Symbols tab is off in Settings.
    SymbolsTabOff,
    /// The Colours tab is off in Settings.
    ColoursTabOff,
}

/// The group whose header is under the cursor, and where it can go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorGroup {
    /// There is a group or loose item above to move past.
    pub up: bool,
    /// There is a group or loose item below to move past.
    pub down: bool,
}

/// One row the palette shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// What runs.
    pub command: Command,
    /// The translated name.
    pub label: String,
    /// Where the query matched in the label: the text before it and the match (none when only a
    /// synonym matched).
    pub mark: Option<(String, String)>,
    /// The shortcut as caps (empty = none).
    pub keys: Vec<String>,
    /// The shortcut spoken.
    pub spoken: String,
    /// Why the command cannot run now (the row is dimmed and Enter does nothing); `None` when it can.
    pub hint: Option<Hint>,
}

/// What stops `command` from running when it is one the user looks for by name, so the palette lists
/// it with the reason instead of hiding it.
#[must_use]
pub fn hint(command: Command, ctx: &Context<'_>) -> Option<Hint> {
    match command {
        Command::Symbols if !ctx.symbols => Some(Hint::SymbolsTabOff),
        Command::Colours | Command::SavedColours | Command::PickColour if !ctx.colours => {
            Some(Hint::ColoursTabOff)
        }
        _ => None,
    }
}

/// Whether `command` is offered in `ctx`.
#[must_use]
pub fn available(command: Command, ctx: &Context<'_>) -> bool {
    match command {
        Command::UseItem
        | Command::CopyItem
        | Command::DuplicateItem
        | Command::EditItem
        | Command::UsePlain
        | Command::TrashItem => ctx.has_item && !ctx.in_trash,
        Command::PinItem => ctx.has_item && !ctx.in_trash && !ctx.item_pinned,
        Command::UnpinItem => ctx.has_item && !ctx.in_trash && ctx.item_pinned,
        Command::ShowPanel => !ctx.panel_shown,
        Command::HidePanel => ctx.panel_shown,
        Command::KeepPanelOpen => !ctx.keep_open,
        Command::LetPanelClose => ctx.keep_open,
        Command::Undo => ctx.can_undo,
        Command::Redo => ctx.can_redo,
        Command::Lock => ctx.lock_set,
        Command::EmptyTrash => ctx.trash_full,
        Command::RestoreItem | Command::DeleteForever => ctx.has_item && ctx.in_trash,
        Command::Symbols => ctx.symbols,
        Command::Colours => ctx.colours,
        Command::SavedColours => ctx.colours && ctx.saved_colours,
        Command::PickColour => ctx.colours && ctx.eyedropper,
        Command::GroupItems => ctx.has_item && ctx.in_list,
        Command::FoldGroup
        | Command::RenameGroup
        | Command::ColourGroup
        | Command::NestGroup
        | Command::UngroupGroup
        | Command::DeleteGroup => ctx.group.is_some(),
        Command::MoveGroupUp => ctx.group.is_some_and(|g| g.up),
        Command::MoveGroupDown => ctx.group.is_some_and(|g| g.down),
        _ => true,
    }
}

/// Whether `word` is a subsequence of the label's letters whose first letter starts a label word
/// ("stg" in "Settings", "ni" in "New item"). Both texts are folded.
fn fuzzy(label: &str, word: &str) -> bool {
    let mut letters = word.chars();
    let mut want = letters.next();
    let mut at_word_start = true;
    let mut started = false;
    for c in label.chars() {
        if let Some(w) = want
            && c == w
            && (started || at_word_start)
        {
            started = true;
            want = letters.next();
        }
        at_word_start = !c.is_alphanumeric();
    }
    want.is_none() && !word.is_empty()
}

/// How well `word` matches a command: 0 at the start of the label, 1 at the start of a label word, 2
/// inside the label, 3 in the synonyms, 4 as letters spread over the label (a subsequence from a word
/// start, so "stg" finds Settings), `None` not at all. Both texts are folded (lowercase, accents
/// removed, so "é" matches "e").
fn rank(label: &str, synonyms: &str, word: &str) -> Option<usize> {
    if label.starts_with(word) {
        Some(0)
    } else if label.split_whitespace().any(|w| w.starts_with(word)) {
        Some(1)
    } else if label.contains(word) {
        Some(2)
    } else if synonyms.contains(word) {
        Some(3)
    } else if fuzzy(label, word) {
        Some(4)
    } else {
        None
    }
}

/// Somewhere the palette can go by name: a board, a list or a tag. Rows for these come from the library,
/// not from [`Command::ALL`], and appear once the user has typed something.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// A board.
    Board {
        /// Which.
        id: snipdeck_core::BoardId,
        /// Its name.
        name: String,
    },
    /// A list, on its board.
    List {
        /// The board it is on.
        board: snipdeck_core::BoardId,
        /// Which.
        id: snipdeck_core::ListId,
        /// Its name.
        name: String,
        /// The name of its board.
        board_name: String,
    },
    /// A tag.
    Tag {
        /// Which.
        id: snipdeck_core::TagId,
        /// Its name.
        name: String,
    },
}

/// The palette id of the first [`Place`] row; the ids below it are [`Command::ALL`] indexes.
pub const PLACE_BASE: i32 = 1000;

/// The most place rows the palette shows at once.
pub const PLACE_LIMIT: usize = 8;

/// The place a palette id names, as an index into the places the palette was filled from.
#[must_use]
pub fn place_index(id: i32) -> Option<usize> {
    id.checked_sub(PLACE_BASE)
        .and_then(|i| usize::try_from(i).ok())
}

/// The places that match `query`, best first, as indexes into `labels` (the translated row text of each
/// place). Nothing matches an empty query: the places would drown the commands.
#[must_use]
pub fn matching_places(labels: &[String], query: &str) -> Vec<usize> {
    let words: Vec<String> = query
        .split_whitespace()
        .map(snipdeck_symbols::fold)
        .collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<(usize, usize)> = labels
        .iter()
        .enumerate()
        .filter_map(|(i, label)| {
            // The quotes around a name are not part of it: “Work” starts with a w.
            let label = snipdeck_symbols::fold(label).replace(['“', '”'], " ");
            let ranks: Option<Vec<usize>> =
                words.iter().map(|word| rank(&label, "", word)).collect();
            ranks.map(|r| (r.iter().sum::<usize>(), i))
        })
        .collect();
    hits.sort_unstable();
    hits.into_iter().take(PLACE_LIMIT).map(|(_, i)| i).collect()
}

/// The entries for `query`, best match first. `texts` holds `(label, synonyms)` per command in the order
/// of [`Command::ALL`]. Each whitespace-separated word of the query must appear (ignoring case) in the
/// label or the synonyms; a word at the start of the label ranks above one inside it, above one in the
/// synonyms. An empty query keeps the catalogue order. `keys` words the spoken shortcuts.
#[must_use]
pub fn entries(
    texts: &[(String, String)],
    keys: &KeyWords,
    ctx: &Context<'_>,
    query: &str,
) -> Vec<Entry> {
    let words: Vec<String> = query
        .split_whitespace()
        .map(snipdeck_symbols::fold)
        .collect();
    let mut scored: Vec<(usize, bool, bool, usize, Entry)> = Vec::new();
    for (i, command) in Command::ALL.iter().copied().enumerate() {
        let Some((label, synonyms)) = texts.get(i) else {
            continue;
        };
        let hint = hint(command, ctx);
        if hint.is_none() && !available(command, ctx) {
            continue;
        }
        let (label_lc, synonyms_lc) = (
            snipdeck_symbols::fold(label),
            snipdeck_symbols::fold(synonyms),
        );
        let ranks: Option<Vec<usize>> = words
            .iter()
            .map(|word| rank(&label_lc, &synonyms_lc, word))
            .collect();
        let Some(ranks) = ranks else {
            continue;
        };
        let score: usize = ranks.iter().sum();
        let shortcut = command.shortcut(ctx.hotkeys);
        scored.push((
            score,
            hint.is_some(),
            command.destructive() && !words.is_empty(),
            i,
            Entry {
                command,
                label: label.clone(),
                mark: search_mark::word_mark(label, query),
                keys: shortcut.as_deref().map(caps).unwrap_or_default(),
                spoken: shortcut
                    .as_deref()
                    .map(|s| spoken(s, keys))
                    .unwrap_or_default(),
                hint,
            },
        ));
    }
    // A command that cannot run now goes after every one that can.
    scored.sort_by_key(|(score, unavailable, destructive, i, _)| {
        (*unavailable, *score, *destructive, *i)
    });
    scored.into_iter().map(|(.., entry)| entry).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_show_only_for_a_query_and_the_best_match_comes_first() {
        let labels: Vec<String> = [
            "Go to board “Work”",
            "Go to list “Links” on Work",
            "Go to list “Work notes” on Home",
            "Go to tag “urgent”",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        assert!(
            matching_places(&labels, "").is_empty(),
            "nothing without a query"
        );
        assert_eq!(matching_places(&labels, "   "), Vec::<usize>::new());
        assert_eq!(matching_places(&labels, "tag urg"), [3]);
        assert_eq!(matching_places(&labels, "go work"), [0, 1, 2]);
        assert_eq!(matching_places(&labels, "wörk"), [0, 1, 2], "accents fold");
        assert_eq!(matching_places(&labels, "zzz"), Vec::<usize>::new());
    }

    #[test]
    fn at_most_eight_places_are_shown_and_ids_map_back_to_an_index() {
        let labels: Vec<String> = (0..20).map(|i| format!("Go to tag “t{i}”")).collect();
        assert_eq!(matching_places(&labels, "tag").len(), PLACE_LIMIT);
        assert_eq!(place_index(PLACE_BASE + 3), Some(3));
        assert_eq!(place_index(5), None, "a command id is not a place");
        assert_eq!(place_index(-1), None);
    }

    fn texts() -> Vec<(String, String)> {
        let labels = [
            ("New item", "add create snippet"),
            ("New list", "add create folder"),
            ("Search", "find filter"),
            ("Show Panel", "open toggle"),
            ("Symbols", "emoji kaomoji icons smiley"),
            ("Colours", "color picker palette hex swatch"),
            ("Saved colours", "colors palette swatches"),
            ("Pick a colour from the screen", "eyedropper color pick"),
            ("Open Picker", "quick find paste"),
            ("Quick capture", "clipboard save"),
            ("Paste next item", "stack queue"),
            ("Take screenshot", "snip capture screen picture"),
            ("Settings", "preferences hotkeys"),
            ("User guide", "help docs"),
            ("Welcome tour", "tutorial intro"),
            ("Keyboard shortcuts", "hotkeys keys cheat sheet"),
            ("Troubleshoot…", "diagnose"),
            ("About Snipdeck", "version"),
            ("Support Snipdeck", "donate coffee"),
            ("Main list density: Comfortable", "view mode"),
            ("Main list density: Compact", "view mode"),
            ("Tile view", "view mode"),
            ("Theme: System", "appearance colour dark light"),
            ("Theme: Light", "appearance colour bright"),
            ("Theme: Dark", "appearance colour night"),
            ("Use selected item", "paste"),
            ("Copy selected item", "clipboard"),
            ("Duplicate selected item", "copy clone"),
            ("Edit selected item", "change"),
            ("Pin selected item", "favourite"),
            ("Unpin selected item", "favourite"),
            ("Move selected item to Trash", "delete"),
            ("Select all items", "every mark"),
            ("Go to All items", "everything"),
            ("Go to Pinned", "favourites"),
            ("Go to Recent", "history"),
            ("Go to Trash", "deleted"),
        ];
        labels
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect()
    }

    fn ctx(hotkeys: &Hotkeys) -> Context<'_> {
        Context {
            hotkeys,
            has_item: true,
            item_pinned: false,
            in_trash: false,
            symbols: true,
            colours: true,
            eyedropper: true,
            saved_colours: true,
            in_list: true,
            group: None,
            panel_shown: false,
            keep_open: false,
            can_undo: true,
            can_redo: true,
            lock_set: true,
            trash_full: true,
        }
    }

    /// The test words padded to the whole catalogue, with the ten newest commands' words.
    fn all_texts() -> Vec<(String, String)> {
        let mut all = texts();
        all.resize(Command::ALL.len(), (String::new(), String::new()));
        let newest = [
            (Command::HidePanel, "Hide Panel", "close"),
            (
                Command::LetPanelClose,
                "Close Panel when I click away",
                "keep open",
            ),
            (Command::Undo, "Undo", "back"),
            (Command::Redo, "Redo", "again"),
            (Command::NewBoard, "New board", "add create"),
            (Command::Lock, "Lock Snipdeck", "password secure"),
            (Command::Quit, "Quit Snipdeck", "exit close"),
            (
                Command::UsePlain,
                "Paste selected item as plain text",
                "unformatted",
            ),
            (Command::RestoreItem, "Restore selected item", "bring back"),
            (
                Command::DeleteForever,
                "Delete selected item for good",
                "remove",
            ),
            (
                Command::ImportLibrary,
                "Import snippets",
                "ditto copyq espanso restore",
            ),
            (Command::ExportLibrary, "Export library", "save file"),
            (Command::BackUpNow, "Back up now", "backup copy"),
            (Command::EmptyTrash, "Empty Trash", "delete everything bin"),
        ];
        for (command, label, words) in newest {
            if let Some(slot) = all.get_mut(usize::try_from(command.id()).unwrap_or(0)) {
                *slot = (label.to_owned(), words.to_owned());
            }
        }
        all
    }

    fn labels(found: &[Entry]) -> Vec<&str> {
        found.iter().map(|e| e.label.as_str()).collect()
    }

    #[test]
    fn the_catalogue_ids_round_trip() {
        for (i, command) in Command::ALL.iter().enumerate() {
            assert_eq!(usize::try_from(command.id()), Ok(i));
            assert_eq!(Command::from_id(command.id()), Some(*command));
        }
        assert_eq!(Command::from_id(-1), None);
        assert_eq!(Command::from_id(99), None);
    }

    #[test]
    fn an_empty_query_lists_what_applies_in_catalogue_order() {
        let hotkeys = Hotkeys::new_install();
        let found = entries(&texts(), &KeyWords::english(), &ctx(&hotkeys), "");
        assert_eq!(found.first().map(|e| e.label.as_str()), Some("New item"));
        assert!(labels(&found).contains(&"Pin selected item"));
        assert!(!labels(&found).contains(&"Unpin selected item"));
    }

    #[test]
    fn palette_marks_label_prefix() {
        let hotkeys = Hotkeys::new_install();
        let found = entries(&texts(), &KeyWords::english(), &ctx(&hotkeys), "new");
        assert_eq!(found[0].label, "New item");
        assert_eq!(found[0].mark, Some((String::new(), "New".into())));
        let by_synonym = entries(&texts(), &KeyWords::english(), &ctx(&hotkeys), "emoji");
        assert_eq!(by_synonym[0].label, "Symbols");
        assert_eq!(by_synonym[0].mark, None, "a synonym hit marks nothing");
        let all = entries(&texts(), &KeyWords::english(), &ctx(&hotkeys), "");
        assert!(all.iter().all(|e| e.mark.is_none()));
    }

    #[test]
    fn item_commands_follow_the_cursor_and_the_trash() {
        let hotkeys = Hotkeys::new_install();
        let mut c = ctx(&hotkeys);
        c.item_pinned = true;
        let pinned = entries(&texts(), &KeyWords::english(), &c, "pin");
        assert!(labels(&pinned).contains(&"Unpin selected item"));
        assert!(!labels(&pinned).contains(&"Pin selected item"));
        c.has_item = false;
        assert!(
            !labels(&entries(&texts(), &KeyWords::english(), &c, ""))
                .contains(&"Edit selected item")
        );
        c.has_item = true;
        c.in_trash = true;
        assert!(
            !labels(&entries(&texts(), &KeyWords::english(), &c, ""))
                .contains(&"Edit selected item")
        );
    }

    #[test]
    fn duplicate_is_offered_for_an_item_outside_the_trash_and_has_ctrl_d() {
        let hotkeys = Hotkeys::new_install();
        let mut c = ctx(&hotkeys);
        assert!(
            labels(&entries(&texts(), &KeyWords::english(), &c, ""))
                .contains(&"Duplicate selected item")
        );
        assert_eq!(
            Command::DuplicateItem.shortcut(&hotkeys).as_deref(),
            Some("Ctrl+D")
        );
        assert!(Command::DuplicateItem.needs_item());
        c.in_trash = true;
        assert!(
            !labels(&entries(&texts(), &KeyWords::english(), &c, ""))
                .contains(&"Duplicate selected item")
        );
        c.in_trash = false;
        c.has_item = false;
        assert!(
            !labels(&entries(&texts(), &KeyWords::english(), &c, ""))
                .contains(&"Duplicate selected item")
        );
    }

    #[test]
    fn the_filter_matches_names_and_synonyms_ignoring_case() {
        let hotkeys = Hotkeys::new_install();
        let c = ctx(&hotkeys);
        assert_eq!(
            labels(&entries(&texts(), &KeyWords::english(), &c, "ADD"))[..2],
            ["New item", "New list"]
        );
        let by_name = entries(&texts(), &KeyWords::english(), &c, "short");
        assert_eq!(labels(&by_name), ["Keyboard shortcuts"]);
        assert_eq!(
            entries(&texts(), &KeyWords::english(), &c, "zzzz"),
            Vec::new()
        );
    }

    #[test]
    fn a_name_match_outranks_a_synonym_match() {
        let hotkeys = Hotkeys::new_install();
        let found = entries(&texts(), &KeyWords::english(), &ctx(&hotkeys), "hotkeys");
        // Both reach it through synonyms: catalogue order breaks the tie.
        assert_eq!(labels(&found), ["Settings", "Keyboard shortcuts"]);
        let found = entries(&texts(), &KeyWords::english(), &ctx(&hotkeys), "view");
        assert_eq!(
            found.first().map(|e| e.label.as_str()),
            Some("Tile view"),
            "the name match leads; the density commands reach \"view\" through their synonyms"
        );
        let found = entries(&texts(), &KeyWords::english(), &ctx(&hotkeys), "trash");
        assert_eq!(
            labels(&found),
            ["Go to Trash", "Move selected item to Trash"],
            "a destructive command never wins a tie: Enter on \"trash\" opens the Trash"
        );
    }

    #[test]
    fn the_top_hit_of_every_word_a_user_would_type_is_pinned_and_never_destructive_on_a_tie() {
        let hotkeys = Hotkeys::new_install();
        for (word, top) in [
            ("trash", "Go to Trash"),
            ("delete", "Go to Trash"),
            ("settings", "Settings"),
            ("pin", "Pin selected item"),
            ("paste", "Paste next item"),
            ("recent", "Go to Recent"),
            ("copy", "Copy selected item"),
        ] {
            let found = entries(&texts(), &KeyWords::english(), &ctx(&hotkeys), word);
            assert_eq!(
                found.first().map(|e| e.label.as_str()),
                Some(top),
                "top hit of {word:?}"
            );
        }
    }

    #[test]
    fn letters_spread_over_a_name_and_accents_still_find_it() {
        let hotkeys = Hotkeys::new_install();
        let found = entries(&texts(), &KeyWords::english(), &ctx(&hotkeys), "stg");
        assert_eq!(
            labels(&found),
            ["Settings"],
            "a subsequence from a word start"
        );
        let found = entries(&texts(), &KeyWords::english(), &ctx(&hotkeys), "SETTÍNGS");
        assert_eq!(labels(&found), ["Settings"], "accents fold away");
        let mut accented = texts();
        accented[12].0 = "Réglages".to_owned();
        let found = entries(&accented, &KeyWords::english(), &ctx(&hotkeys), "reglages");
        assert_eq!(
            labels(&found),
            ["Réglages"],
            "an accented name is found without the accent"
        );
        assert_eq!(
            entries(&texts(), &KeyWords::english(), &ctx(&hotkeys), "zzq").len(),
            0
        );
    }

    #[test]
    fn take_screenshot_is_found_by_its_name_and_a_synonym() {
        let hotkeys = Hotkeys::new_install();
        let by_name = entries(&texts(), &KeyWords::english(), &ctx(&hotkeys), "screenshot");
        assert_eq!(labels(&by_name), ["Take screenshot"]);
        let by_synonym = entries(&texts(), &KeyWords::english(), &ctx(&hotkeys), "picture");
        assert_eq!(labels(&by_synonym), ["Take screenshot"]);
        assert_eq!(by_name[0].command, Command::TakeScreenshot);
        assert!(by_name[0].keys.is_empty(), "it has no shortcut");
    }

    #[test]
    fn every_word_of_the_query_must_match() {
        let hotkeys = Hotkeys::new_install();
        let c = ctx(&hotkeys);
        assert_eq!(
            labels(&entries(&texts(), &KeyWords::english(), &c, "new list")),
            ["New list"]
        );
        assert_eq!(
            entries(&texts(), &KeyWords::english(), &c, "new zzz"),
            Vec::new()
        );
    }

    #[test]
    fn global_hotkeys_show_the_configured_chords() {
        let hotkeys = Hotkeys {
            picker: "Ctrl+Alt+K".into(),
            capture: String::new(),
            ..Hotkeys::new_install()
        };
        let found = entries(&texts(), &KeyWords::english(), &ctx(&hotkeys), "");
        let find = |label: &str| found.iter().find(|e| e.label == label).cloned();
        assert_eq!(
            find("Open Picker").map(|e| e.keys),
            Some(vec!["Ctrl".to_owned(), "Alt".into(), "K".into()])
        );
        assert_eq!(find("Quick capture").map(|e| e.keys), Some(Vec::new()));
        assert_eq!(
            find("Show Panel").map(|e| e.keys.join("+")),
            Some(HotkeyAction::TogglePanel.default_chord().to_owned())
        );
    }

    /// The catalogue's texts up to the group commands (the three view commands before them filled in).
    fn texts_with_groups() -> Vec<(String, String)> {
        let mut all = texts();
        for filler in ["Reset Panel", "Keep Panel open", "Show or hide sidebar"] {
            all.push((filler.to_owned(), String::new()));
        }
        for (label, words) in [
            ("Group the selected rows", "make new folder"),
            ("Fold or unfold a group", "collapse expand"),
            ("Rename group", "name"),
            ("Ungroup", "dissolve"),
            ("Delete group", "remove trash"),
            ("Move the group up", "reorder earlier"),
            ("Move the group down", "reorder later"),
            ("Group colour…", "colour color dot tint"),
            ("Nest under…", "nest inside child"),
        ] {
            all.push((label.to_owned(), words.to_owned()));
        }
        all
    }

    #[test]
    fn group_commands_are_offered_where_the_code_performs_them_and_carry_their_keys() {
        let hotkeys = Hotkeys::new_install();
        let found = |c: &Context<'_>| -> Vec<Command> {
            entries(&texts_with_groups(), &KeyWords::english(), c, "")
                .into_iter()
                .map(|e| e.command)
                .filter(|c| {
                    matches!(
                        c,
                        Command::GroupItems
                            | Command::FoldGroup
                            | Command::RenameGroup
                            | Command::UngroupGroup
                            | Command::DeleteGroup
                            | Command::MoveGroupUp
                            | Command::MoveGroupDown
                            | Command::ColourGroup
                            | Command::NestGroup
                    )
                })
                .collect()
        };
        // On an item in a list: only "group". In All items, Pinned or the Trash there is nothing to group.
        let mut c = ctx(&hotkeys);
        assert_eq!(found(&c), [Command::GroupItems]);
        c.in_list = false;
        assert_eq!(found(&c), Vec::<Command>::new());
        // On a header (not an item): the header commands, the moves only where there is a place to go.
        let mut h = ctx(&hotkeys);
        h.has_item = false;
        h.group = Some(CursorGroup {
            up: false,
            down: true,
        });
        assert_eq!(
            found(&h),
            [
                Command::FoldGroup,
                Command::RenameGroup,
                Command::UngroupGroup,
                Command::DeleteGroup,
                Command::MoveGroupDown,
                Command::ColourGroup,
                Command::NestGroup
            ]
        );
        h.group = Some(CursorGroup {
            up: true,
            down: false,
        });
        assert!(found(&h).contains(&Command::MoveGroupUp));
        assert!(!found(&h).contains(&Command::MoveGroupDown));
        for (command, keys) in [
            (Command::GroupItems, Some("Ctrl+G")),
            (Command::RenameGroup, Some("F2")),
            (Command::DeleteGroup, Some("Del")),
            (Command::MoveGroupUp, Some("Alt+Up")),
            (Command::MoveGroupDown, Some("Alt+Down")),
            (Command::FoldGroup, Some("Enter")),
            (Command::UngroupGroup, None),
        ] {
            assert_eq!(command.shortcut(&hotkeys).as_deref(), keys, "{command:?}");
        }
    }

    #[test]
    fn new_list_and_new_item_have_their_own_chords() {
        let hotkeys = Hotkeys::new_install();
        assert_eq!(
            Command::NewItem.shortcut(&hotkeys).as_deref(),
            Some("Ctrl+N")
        );
        assert_eq!(
            Command::NewList.shortcut(&hotkeys).as_deref(),
            Some("Ctrl+Shift+N")
        );
    }

    #[test]
    fn the_theme_commands_are_found_by_theme_dark_and_light_and_have_no_shortcut() {
        let hotkeys = Hotkeys::new_install();
        let c = ctx(&hotkeys);
        assert_eq!(
            labels(&entries(&texts(), &KeyWords::english(), &c, "theme")),
            ["Theme: System", "Theme: Light", "Theme: Dark"]
        );
        let dark = entries(&texts(), &KeyWords::english(), &c, "dark");
        assert_eq!(dark.first().map(|e| e.command), Some(Command::ThemeDark));
        assert_eq!(dark[0].keys, Vec::<String>::new());
        assert!(!Command::ThemeLight.needs_item());
        // Offered with no item under the cursor and in the Trash.
        let mut c = ctx(&hotkeys);
        c.has_item = false;
        c.in_trash = true;
        assert!(
            labels(&entries(&texts(), &KeyWords::english(), &c, "theme")).contains(&"Theme: Dark")
        );
    }

    #[test]
    fn paste_next_and_select_all_are_in_the_palette_with_their_keys() {
        let hotkeys = Hotkeys::new_install();
        let c = ctx(&hotkeys);
        let next = entries(&texts(), &KeyWords::english(), &c, "paste next");
        assert_eq!(next.first().map(|e| e.command), Some(Command::PasteNext));
        assert_eq!(
            Command::PasteNext.shortcut(&hotkeys).as_deref(),
            chord_text(&hotkeys, HotkeyAction::PasteNext).as_deref(),
            "the chord follows Settings"
        );
        let all = entries(&texts(), &KeyWords::english(), &c, "select all");
        assert_eq!(all.first().map(|e| e.command), Some(Command::SelectAll));
        assert_eq!(all[0].keys, ["Ctrl", "A"]);
        assert!(
            !Command::SelectAll.needs_item(),
            "it works on an empty cursor too"
        );
    }

    #[test]
    fn the_colour_commands_follow_the_tab_the_eyedropper_and_the_saved_list() {
        let hotkeys = Hotkeys::default();
        let ids = |ctx: &Context<'_>| -> Vec<Command> {
            Command::ALL
                .iter()
                .copied()
                .filter(|c| {
                    matches!(
                        c,
                        Command::Colours | Command::SavedColours | Command::PickColour
                    ) && available(*c, ctx)
                })
                .collect()
        };
        let mut c = ctx(&hotkeys);
        assert_eq!(
            ids(&c),
            [Command::Colours, Command::SavedColours, Command::PickColour]
        );
        c.saved_colours = false;
        assert_eq!(
            ids(&c),
            [Command::Colours, Command::PickColour],
            "no Colours list yet: nothing to view"
        );
        c.eyedropper = false;
        assert_eq!(ids(&c), [Command::Colours]);
        c.colours = false;
        assert!(ids(&c).is_empty(), "the tab off hides them all");
    }

    #[test]
    fn a_tab_that_is_off_stays_in_the_list_dimmed_with_its_reason_after_the_commands_that_run() {
        let hotkeys = Hotkeys::default();
        let mut c = ctx(&hotkeys);
        let found = entries(&all_texts(), &KeyWords::english(), &c, "symbols");
        assert_eq!(found.first().map(|e| e.command), Some(Command::Symbols));
        assert_eq!(found.first().and_then(|e| e.hint), None);
        c.symbols = false;
        c.colours = false;
        let found = entries(&all_texts(), &KeyWords::english(), &c, "symbols");
        assert_eq!(found.first().map(|e| e.command), Some(Command::Symbols));
        assert_eq!(
            found.first().and_then(|e| e.hint),
            Some(Hint::SymbolsTabOff)
        );
        let all = entries(&all_texts(), &KeyWords::english(), &c, "");
        let first_hint = all.iter().position(|e| e.hint.is_some());
        assert!(
            all.iter()
                .skip(first_hint.unwrap_or(0))
                .all(|e| e.hint.is_some()),
            "unavailable rows come last"
        );
        let colours = entries(&all_texts(), &KeyWords::english(), &c, "colours");
        assert_eq!(
            colours.first().and_then(|e| e.hint),
            Some(Hint::ColoursTabOff)
        );
    }

    #[test]
    fn toggles_are_named_by_what_they_do_now() {
        let hotkeys = Hotkeys::default();
        let mut c = ctx(&hotkeys);
        let has = |c: &Context<'_>, cmd: Command| {
            entries(&all_texts(), &KeyWords::english(), c, "")
                .iter()
                .any(|e| e.command == cmd)
        };
        assert!(has(&c, Command::ShowPanel) && !has(&c, Command::HidePanel));
        assert!(has(&c, Command::KeepPanelOpen) && !has(&c, Command::LetPanelClose));
        c.panel_shown = true;
        c.keep_open = true;
        assert!(!has(&c, Command::ShowPanel) && has(&c, Command::HidePanel));
        assert!(!has(&c, Command::KeepPanelOpen) && has(&c, Command::LetPanelClose));
    }

    #[test]
    fn undo_redo_lock_and_the_trash_commands_follow_their_state() {
        let hotkeys = Hotkeys::default();
        let mut c = ctx(&hotkeys);
        let has = |c: &Context<'_>, cmd: Command| {
            entries(&all_texts(), &KeyWords::english(), c, "")
                .iter()
                .any(|e| e.command == cmd)
        };
        assert!(has(&c, Command::Undo) && has(&c, Command::Redo) && has(&c, Command::Lock));
        assert!(has(&c, Command::NewBoard) && has(&c, Command::Quit) && has(&c, Command::UsePlain));
        assert!(!has(&c, Command::RestoreItem) && !has(&c, Command::DeleteForever));
        c.can_undo = false;
        c.can_redo = false;
        c.lock_set = false;
        assert!(!has(&c, Command::Undo) && !has(&c, Command::Redo) && !has(&c, Command::Lock));
        assert!(has(&c, Command::EmptyTrash) && Command::EmptyTrash.destructive());
        c.trash_full = false;
        assert!(
            !has(&c, Command::EmptyTrash),
            "an empty Trash has nothing to empty"
        );
        c.trash_full = true;
        c.in_trash = true;
        assert!(has(&c, Command::RestoreItem) && has(&c, Command::DeleteForever));
        assert!(!has(&c, Command::UsePlain));
        let found = entries(&all_texts(), &KeyWords::english(), &c, "delete");
        let forever = found
            .iter()
            .position(|e| e.command == Command::DeleteForever);
        assert!(forever.is_some(), "found by its words");
        assert!(Command::DeleteForever.destructive() && Command::Quit.destructive());
        assert_eq!(Command::Undo.shortcut(&hotkeys).as_deref(), Some("Ctrl+Z"));
        assert_eq!(
            Command::UsePlain.shortcut(&hotkeys).as_deref(),
            Some("Alt+Enter")
        );
    }

    #[test]
    fn caps_and_speech_of_a_shortcut() {
        assert_eq!(caps("Ctrl+Shift+Enter"), ["Ctrl", "Shift", "↵"]);
        assert_eq!(caps("Ctrl+,"), ["Ctrl", ","]);
        assert_eq!(caps("Ctrl++"), ["Ctrl", "+"]);
        assert_eq!(caps("F1"), ["F1"]);
        assert_eq!(spoken("Ctrl+/", &KeyWords::english()), "Control Slash");
        assert_eq!(spoken("Del", &KeyWords::english()), "Delete");
        assert_eq!(spoken("Esc", &KeyWords::english()), "Escape");
    }
}
