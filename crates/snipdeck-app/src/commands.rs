//! The command palette's catalogue: every action the Main window offers, the shortcut each has, and
//! the filter that narrows the list as the user types.
//!
//! Slint-free. The words come from the UI crate's `CommandTexts` global (translated there, in the order
//! of [`Command::ALL`]); the shortcuts come from here, and the three global hotkeys read the chords in
//! the settings, so a rebound hotkey shows as rebound.

use crate::settings::{HotkeyAction, Hotkeys};

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
}

impl Command {
    /// Every command, in the order of `CommandTexts.all`.
    pub const ALL: [Self; 34] = [
        Self::NewItem,
        Self::NewList,
        Self::Search,
        Self::ShowPanel,
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
            Self::UseItem => "Enter",
            Self::CopyItem => "Shift+Enter",
            Self::DuplicateItem => "Ctrl+D",
            Self::EditItem => "Ctrl+E",
            Self::PinItem | Self::UnpinItem => "Ctrl+P",
            Self::TrashItem => "Del",
            Self::SelectAll => "Ctrl+A",
            Self::ShowPanel => return chord_text(hotkeys, HotkeyAction::TogglePanel),
            Self::OpenPicker => return chord_text(hotkeys, HotkeyAction::Picker),
            Self::QuickCapture => return chord_text(hotkeys, HotkeyAction::Capture),
            Self::PasteNext => return chord_text(hotkeys, HotkeyAction::PasteNext),
            _ => return None,
        };
        Some(fixed.to_owned())
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
        )
    }
}

/// The configured chord of a global hotkey as text ("Ctrl+Alt+Space"); `None` while it is off.
#[must_use]
pub fn chord_text(hotkeys: &Hotkeys, action: HotkeyAction) -> Option<String> {
    hotkeys.chord(action).map(|c| c.to_string())
}

/// The caps and the spoken form of a global hotkey's configured chord; both empty while it is off.
#[must_use]
pub fn chord_caps(hotkeys: &Hotkeys, action: HotkeyAction) -> (Vec<String>, String) {
    chord_text(hotkeys, action).map_or_else(
        || (Vec::new(), String::new()),
        |text| (caps(&text), spoken(&text)),
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

/// The spoken form of a shortcut ("Control Shift N") for assistive technology.
#[must_use]
pub fn spoken(shortcut: &str) -> String {
    caps(shortcut)
        .iter()
        .map(|cap| match cap.as_str() {
            "Ctrl" => "Control",
            "↵" => "Enter",
            "Del" => "Delete",
            "Esc" => "Escape",
            "+" => "Plus",
            "," => "Comma",
            "/" => "Slash",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// What the palette may offer right now.
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
}

/// One row the palette shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// What runs.
    pub command: Command,
    /// The translated name.
    pub label: String,
    /// The shortcut as caps (empty = none).
    pub keys: Vec<String>,
    /// The shortcut spoken.
    pub spoken: String,
}

/// Whether `command` is offered in `ctx`.
#[must_use]
pub fn available(command: Command, ctx: &Context<'_>) -> bool {
    match command {
        Command::UseItem
        | Command::CopyItem
        | Command::DuplicateItem
        | Command::EditItem
        | Command::TrashItem => ctx.has_item && !ctx.in_trash,
        Command::PinItem => ctx.has_item && !ctx.in_trash && !ctx.item_pinned,
        Command::UnpinItem => ctx.has_item && !ctx.in_trash && ctx.item_pinned,
        _ => true,
    }
}

/// How well `word` matches a command: 0 at the start of the label, 1 at the start of a label word, 2
/// inside the label, 3 in the synonyms, `None` not at all. Both texts are lowercase.
fn rank(label: &str, synonyms: &str, word: &str) -> Option<usize> {
    if label.starts_with(word) {
        Some(0)
    } else if label.split_whitespace().any(|w| w.starts_with(word)) {
        Some(1)
    } else if label.contains(word) {
        Some(2)
    } else if synonyms.contains(word) {
        Some(3)
    } else {
        None
    }
}

/// The entries for `query`, best match first. `texts` holds `(label, synonyms)` per command in the order
/// of [`Command::ALL`]. Each whitespace-separated word of the query must appear (ignoring case) in the
/// label or the synonyms; a word at the start of the label ranks above one inside it, above one in the
/// synonyms. An empty query keeps the catalogue order.
#[must_use]
pub fn entries(texts: &[(String, String)], ctx: &Context<'_>, query: &str) -> Vec<Entry> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut scored: Vec<(usize, usize, Entry)> = Vec::new();
    for (i, command) in Command::ALL.iter().copied().enumerate() {
        let Some((label, synonyms)) = texts.get(i) else {
            continue;
        };
        if !available(command, ctx) {
            continue;
        }
        let (label_lc, synonyms_lc) = (label.to_lowercase(), synonyms.to_lowercase());
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
            i,
            Entry {
                command,
                label: label.clone(),
                keys: shortcut.as_deref().map(caps).unwrap_or_default(),
                spoken: shortcut.as_deref().map(spoken).unwrap_or_default(),
            },
        ));
    }
    scored.sort_by_key(|(score, i, _)| (*score, *i));
    scored.into_iter().map(|(_, _, entry)| entry).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts() -> Vec<(String, String)> {
        let labels = [
            ("New item", "add create snippet"),
            ("New list", "add create folder"),
            ("Search", "find filter"),
            ("Show Panel", "open toggle"),
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
        }
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
        let hotkeys = Hotkeys::default();
        let found = entries(&texts(), &ctx(&hotkeys), "");
        assert_eq!(found.first().map(|e| e.label.as_str()), Some("New item"));
        assert!(labels(&found).contains(&"Pin selected item"));
        assert!(!labels(&found).contains(&"Unpin selected item"));
    }

    #[test]
    fn item_commands_follow_the_cursor_and_the_trash() {
        let hotkeys = Hotkeys::default();
        let mut c = ctx(&hotkeys);
        c.item_pinned = true;
        let pinned = entries(&texts(), &c, "pin");
        assert!(labels(&pinned).contains(&"Unpin selected item"));
        assert!(!labels(&pinned).contains(&"Pin selected item"));
        c.has_item = false;
        assert!(!labels(&entries(&texts(), &c, "")).contains(&"Edit selected item"));
        c.has_item = true;
        c.in_trash = true;
        assert!(!labels(&entries(&texts(), &c, "")).contains(&"Edit selected item"));
    }

    #[test]
    fn duplicate_is_offered_for_an_item_outside_the_trash_and_has_ctrl_d() {
        let hotkeys = Hotkeys::default();
        let mut c = ctx(&hotkeys);
        assert!(labels(&entries(&texts(), &c, "")).contains(&"Duplicate selected item"));
        assert_eq!(
            Command::DuplicateItem.shortcut(&hotkeys).as_deref(),
            Some("Ctrl+D")
        );
        assert!(Command::DuplicateItem.needs_item());
        c.in_trash = true;
        assert!(!labels(&entries(&texts(), &c, "")).contains(&"Duplicate selected item"));
        c.in_trash = false;
        c.has_item = false;
        assert!(!labels(&entries(&texts(), &c, "")).contains(&"Duplicate selected item"));
    }

    #[test]
    fn the_filter_matches_names_and_synonyms_ignoring_case() {
        let hotkeys = Hotkeys::default();
        let c = ctx(&hotkeys);
        assert_eq!(
            labels(&entries(&texts(), &c, "ADD"))[..2],
            ["New item", "New list"]
        );
        let by_name = entries(&texts(), &c, "short");
        assert_eq!(labels(&by_name), ["Keyboard shortcuts"]);
        assert_eq!(entries(&texts(), &c, "zzzz"), Vec::new());
    }

    #[test]
    fn a_name_match_outranks_a_synonym_match() {
        let hotkeys = Hotkeys::default();
        let found = entries(&texts(), &ctx(&hotkeys), "hotkeys");
        // Both reach it through synonyms: catalogue order breaks the tie.
        assert_eq!(labels(&found), ["Settings", "Keyboard shortcuts"]);
        let found = entries(&texts(), &ctx(&hotkeys), "view");
        assert_eq!(
            found.first().map(|e| e.label.as_str()),
            Some("Tile view"),
            "the name match leads; the density commands reach \"view\" through their synonyms"
        );
        let found = entries(&texts(), &ctx(&hotkeys), "trash");
        assert_eq!(
            labels(&found),
            ["Move selected item to Trash", "Go to Trash"]
        );
    }

    #[test]
    fn take_screenshot_is_found_by_its_name_and_a_synonym() {
        let hotkeys = Hotkeys::default();
        let by_name = entries(&texts(), &ctx(&hotkeys), "screenshot");
        assert_eq!(labels(&by_name), ["Take screenshot"]);
        let by_synonym = entries(&texts(), &ctx(&hotkeys), "picture");
        assert_eq!(labels(&by_synonym), ["Take screenshot"]);
        assert_eq!(by_name[0].command, Command::TakeScreenshot);
        assert!(by_name[0].keys.is_empty(), "it has no shortcut");
    }

    #[test]
    fn every_word_of_the_query_must_match() {
        let hotkeys = Hotkeys::default();
        let c = ctx(&hotkeys);
        assert_eq!(labels(&entries(&texts(), &c, "new list")), ["New list"]);
        assert_eq!(entries(&texts(), &c, "new zzz"), Vec::new());
    }

    #[test]
    fn global_hotkeys_show_the_configured_chords() {
        let hotkeys = Hotkeys {
            picker: "Ctrl+Alt+K".into(),
            capture: String::new(),
            ..Hotkeys::default()
        };
        let found = entries(&texts(), &ctx(&hotkeys), "");
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

    #[test]
    fn new_list_and_new_item_have_their_own_chords() {
        let hotkeys = Hotkeys::default();
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
        let hotkeys = Hotkeys::default();
        let c = ctx(&hotkeys);
        assert_eq!(
            labels(&entries(&texts(), &c, "theme")),
            ["Theme: System", "Theme: Light", "Theme: Dark"]
        );
        let dark = entries(&texts(), &c, "dark");
        assert_eq!(dark.first().map(|e| e.command), Some(Command::ThemeDark));
        assert_eq!(dark[0].keys, Vec::<String>::new());
        assert!(!Command::ThemeLight.needs_item());
        // Offered with no item under the cursor and in the Trash.
        let mut c = ctx(&hotkeys);
        c.has_item = false;
        c.in_trash = true;
        assert!(labels(&entries(&texts(), &c, "theme")).contains(&"Theme: Dark"));
    }

    #[test]
    fn paste_next_and_select_all_are_in_the_palette_with_their_keys() {
        let hotkeys = Hotkeys::default();
        let c = ctx(&hotkeys);
        let next = entries(&texts(), &c, "paste next");
        assert_eq!(next.first().map(|e| e.command), Some(Command::PasteNext));
        assert_eq!(
            Command::PasteNext.shortcut(&hotkeys).as_deref(),
            chord_text(&hotkeys, HotkeyAction::PasteNext).as_deref(),
            "the chord follows Settings"
        );
        let all = entries(&texts(), &c, "select all");
        assert_eq!(all.first().map(|e| e.command), Some(Command::SelectAll));
        assert_eq!(all[0].keys, ["Ctrl", "A"]);
        assert!(
            !Command::SelectAll.needs_item(),
            "it works on an empty cursor too"
        );
    }

    #[test]
    fn caps_and_speech_of_a_shortcut() {
        assert_eq!(caps("Ctrl+Shift+Enter"), ["Ctrl", "Shift", "↵"]);
        assert_eq!(caps("Ctrl+,"), ["Ctrl", ","]);
        assert_eq!(caps("Ctrl++"), ["Ctrl", "+"]);
        assert_eq!(caps("F1"), ["F1"]);
        assert_eq!(spoken("Ctrl+/"), "Control Slash");
        assert_eq!(spoken("Del"), "Delete");
        assert_eq!(spoken("Esc"), "Escape");
    }
}
