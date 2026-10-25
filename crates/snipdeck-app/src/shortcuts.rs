//! The keyboard shortcuts list (Help > Shortcuts): every key Snipdeck answers, grouped by where it
//! works, and the filter that narrows the list as the user types in its search box.
//!
//! Slint-free. The words come from the UI crate's `ShortcutTexts` global (translated there, in the
//! order of [`CATALOGUE`]); the chords come from here, and the four global hotkeys read the chords in
//! the settings, so a rebound hotkey shows as rebound. The key names are not translated (they are the
//! names on the keyboard).

use crate::{
    commands::{caps, spoken},
    settings::{HotkeyAction, Hotkeys},
};

/// Where a shortcut works. The order of [`Scope::ALL`] is the order of `ShortcutTexts.scopes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Global hotkeys: they work in any program.
    Anywhere,
    /// The Main window.
    Main,
    /// The Panel.
    Panel,
    /// The Picker.
    Picker,
    /// The item, list and board editors.
    Editors,
}

impl Scope {
    /// Every scope, in the order the list shows them.
    pub const ALL: [Self; 5] = [
        Self::Anywhere,
        Self::Main,
        Self::Panel,
        Self::Picker,
        Self::Editors,
    ];
}

/// The keys of one shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keys {
    /// A chord written as text ("Ctrl+Shift+N"); the caps and the spoken form follow from it.
    Chord(&'static str),
    /// Keys that are not one chord ("Up or Down", "Alt+1 to 9"): the caps and the spoken form given.
    Custom {
        /// The key caps.
        caps: &'static [&'static str],
        /// What assistive technology reads.
        spoken: &'static str,
    },
    /// The chord of a global hotkey as set in Settings; empty while the hotkey is off.
    Global(HotkeyAction),
}

/// One row of the list. The words of the row are `ShortcutTexts.titles[index in the catalogue]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    /// Where it works.
    pub scope: Scope,
    /// Its keys.
    pub keys: Keys,
}

const fn row(scope: Scope, chord: &'static str) -> Shortcut {
    Shortcut {
        scope,
        keys: Keys::Chord(chord),
    }
}

const fn custom(scope: Scope, caps: &'static [&'static str], spoken: &'static str) -> Shortcut {
    Shortcut {
        scope,
        keys: Keys::Custom { caps, spoken },
    }
}

const fn global(action: HotkeyAction) -> Shortcut {
    Shortcut {
        scope: Scope::Anywhere,
        keys: Keys::Global(action),
    }
}

use Scope::{Editors, Main, Panel, Picker};

/// Every shortcut, in the order of `ShortcutTexts.titles` (a test in `tests/help_window.rs` checks the
/// length) and grouped by scope in the order of [`Scope::ALL`].
pub const CATALOGUE: [Shortcut; 61] = [
    // Anywhere in Windows.
    global(HotkeyAction::TogglePanel),
    global(HotkeyAction::Picker),
    global(HotkeyAction::Capture),
    global(HotkeyAction::PasteNext),
    // Main window.
    row(Main, "Ctrl+K"),
    row(Main, "Ctrl+N"),
    row(Main, "Ctrl+Shift+N"),
    row(Main, "Ctrl+F"),
    row(Main, "Ctrl+,"),
    row(Main, "Ctrl+/"),
    row(Main, "F1"),
    row(Main, "Enter"),
    row(Main, "Shift+Enter"),
    row(Main, "Ctrl+E"),
    row(Main, "Ctrl+D"),
    row(Main, "Ctrl+P"),
    row(Main, "Del"),
    row(Main, "Ctrl+A"),
    row(Main, "Ctrl+Z"),
    row(Main, "Ctrl+Y"),
    row(Main, "Ctrl+W"),
    custom(Main, &["Alt", "Up", "Down"], "Alt+Up or Alt+Down"),
    // Panel.
    custom(Panel, &["A-Z"], "Type"),
    custom(Panel, &["Up", "Down"], "Up or Down"),
    custom(Panel, &["PgUp", "PgDn"], "PageUp or PageDown"),
    row(Panel, "Enter"),
    row(Panel, "Shift+Enter"),
    custom(Panel, &["Alt", "Up", "Down"], "Alt+Up or Alt+Down"),
    custom(Panel, &["Alt", "1-9"], "Alt+1 to 9"),
    custom(Panel, &["Space"], "Hold Space"),
    custom(Panel, &["Shift", "Up", "Down"], "Shift+Up or Shift+Down"),
    row(Panel, "Ctrl+A"),
    row(Panel, "Ctrl+Tab"),
    row(Panel, "Ctrl+Shift+Tab"),
    row(Panel, "Ctrl+F"),
    custom(Panel, &["Shift", "F10"], "Shift+F10 or the Menu key"),
    row(Panel, "Ctrl+E"),
    row(Panel, "F2"),
    row(Panel, "Ctrl+D"),
    row(Panel, "Ctrl+P"),
    row(Panel, "Ctrl+Shift+P"),
    row(Panel, "Del"),
    row(Panel, "Ctrl+K"),
    row(Panel, "Ctrl+N"),
    row(Panel, "Ctrl+Shift+N"),
    row(Panel, "Ctrl+,"),
    row(Panel, "Ctrl+Z"),
    row(Panel, "Ctrl+Y"),
    row(Panel, "Ctrl+W"),
    row(Panel, "Ctrl+/"),
    row(Panel, "F1"),
    row(Panel, "Esc"),
    // Picker.
    custom(Picker, &["Up", "Down"], "Up or Down"),
    row(Picker, "Enter"),
    row(Picker, "Shift+Enter"),
    custom(Picker, &["Alt", "1-9"], "Alt+1 to 9"),
    row(Picker, "Tab"),
    row(Picker, "Ctrl+E"),
    row(Picker, "Esc"),
    // Editors.
    row(Editors, "Ctrl+Enter"),
    row(Editors, "Esc"),
];

/// One line the list shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// What the keys do (translated).
    pub title: String,
    /// The key caps; empty = the hotkey is off.
    pub keys: Vec<String>,
    /// The keys spoken; empty = off.
    pub spoken: String,
}

/// The lines of one scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// The scope.
    pub scope: Scope,
    /// Its lines that match the query.
    pub lines: Vec<Line>,
}

impl Keys {
    fn resolve(self, hotkeys: &Hotkeys) -> (Vec<String>, String) {
        match self {
            Self::Chord(text) => (caps(text), spoken(text)),
            Self::Custom { caps, spoken } => (
                caps.iter().map(|c| (*c).to_owned()).collect(),
                spoken.to_owned(),
            ),
            Self::Global(action) => crate::commands::chord_caps(hotkeys, action),
        }
    }
}

/// The words of a query: split at spaces and at `+` so "ctrl+shift+n" and "ctrl shift n" are the
/// same. A query of only `+` signs is the plus key.
fn words(query: &str) -> Vec<String> {
    let found: Vec<String> = query
        .split(|c: char| c.is_whitespace() || c == '+')
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    if found.is_empty() && query.contains('+') {
        vec!["+".to_owned()]
    } else {
        found
    }
}

/// Whether `line` answers `words`: each word starts a word of the title, starts a key name (the caps
/// or what they are called aloud: "esc" and "escape", "del" and "delete"), or, from three letters on,
/// appears inside the title.
fn matches(line: &Line, words: &[String]) -> bool {
    let title = line.title.to_lowercase();
    let key_words: Vec<String> = line
        .keys
        .iter()
        .map(|k| k.to_lowercase())
        .chain(
            line.spoken
                .split(|c: char| c.is_whitespace() || c == '+')
                .map(str::to_lowercase),
        )
        .collect();
    words.iter().all(|word| {
        title
            .split_whitespace()
            .any(|t| t.starts_with(word.as_str()))
            || (word.chars().count() >= 3 && title.contains(word.as_str()))
            || key_words.iter().any(|k| k.starts_with(word.as_str()))
    })
}

/// The groups for `query` over `catalogue`: `titles` is one title per shortcut. An empty query keeps
/// every line; a group with no matching line is left out. Order is the catalogue's.
#[must_use]
pub fn groups_of(
    catalogue: &[Shortcut],
    titles: &[String],
    hotkeys: &Hotkeys,
    query: &str,
) -> Vec<Group> {
    let words = words(query);
    let mut out: Vec<Group> = Vec::new();
    for scope in Scope::ALL {
        let lines: Vec<Line> = catalogue
            .iter()
            .zip(titles)
            .filter(|(shortcut, _)| shortcut.scope == scope)
            .map(|(shortcut, title)| {
                let (keys, spoken) = shortcut.keys.resolve(hotkeys);
                Line {
                    title: title.clone(),
                    keys,
                    spoken,
                }
            })
            .filter(|line| matches(line, &words))
            .collect();
        if !lines.is_empty() {
            out.push(Group { scope, lines });
        }
    }
    out
}

/// The groups of the real list for `query`.
#[must_use]
pub fn groups(titles: &[String], hotkeys: &Hotkeys, query: &str) -> Vec<Group> {
    groups_of(&CATALOGUE, titles, hotkeys, query)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small catalogue with titles, as the UI crate would hand them over.
    fn fixture() -> (Vec<Shortcut>, Vec<String>) {
        let rows = [
            (global(HotkeyAction::TogglePanel), "Show or hide the Panel"),
            (global(HotkeyAction::Capture), "Quick capture the clipboard"),
            (row(Main, "Ctrl+N"), "New item"),
            (row(Main, "Ctrl+Shift+N"), "New list"),
            (row(Main, "Del"), "Move the item to the Trash"),
            (
                custom(Panel, &["Alt", "1-9"], "Alt+1 to 9"),
                "Use the Nth item",
            ),
            (row(Panel, "Esc"), "Clear the search, then close"),
            (row(Panel, "Ctrl+Shift+P"), "Keep the Panel open"),
        ];
        (
            rows.iter().map(|(s, _)| *s).collect(),
            rows.iter().map(|(_, t)| (*t).to_owned()).collect(),
        )
    }

    /// The titles that answer `query` with every global hotkey off, so only the fixed keys count.
    fn found(query: &str) -> Vec<String> {
        let (catalogue, titles) = fixture();
        let off = Hotkeys {
            toggle_panel: String::new(),
            picker: String::new(),
            capture: String::new(),
            paste_next: String::new(),
        };
        groups_of(&catalogue, &titles, &off, query)
            .into_iter()
            .flat_map(|g| g.lines)
            .map(|l| l.title)
            .collect()
    }

    #[test]
    fn an_empty_query_lists_every_line_in_catalogue_order() {
        assert_eq!(found("").len(), 8);
        assert_eq!(found("   "), found(""));
        assert_eq!(found("")[0], "Show or hide the Panel");
    }

    #[test]
    fn the_filter_matches_the_action_name_ignoring_case() {
        assert_eq!(found("TRASH"), ["Move the item to the Trash"]);
        assert_eq!(found("new"), ["New item", "New list"]);
        assert_eq!(found("clip"), ["Quick capture the clipboard"]);
        assert_eq!(found("zzzz"), Vec::<String>::new());
    }

    #[test]
    fn every_word_of_the_query_must_match() {
        assert_eq!(found("new list"), ["New list"]);
        assert_eq!(found("new zzz"), Vec::<String>::new());
    }

    #[test]
    fn the_filter_matches_the_chord_however_it_is_typed() {
        assert_eq!(found("ctrl+n"), ["New item", "New list"]);
        assert_eq!(found("ctrl n"), ["New item", "New list"]);
        assert_eq!(found("Ctrl+Shift+N"), ["New list"]);
        assert_eq!(found("shift n"), ["New list"]);
        assert_eq!(found("ctrl shift p"), ["Keep the Panel open"]);
        assert_eq!(found("alt"), ["Use the Nth item"]);
    }

    #[test]
    fn a_key_is_found_by_its_cap_and_by_what_it_is_called_aloud() {
        assert_eq!(found("del"), ["Move the item to the Trash"]);
        assert_eq!(found("delete"), ["Move the item to the Trash"]);
        assert_eq!(found("esc"), ["Clear the search, then close"]);
        assert_eq!(found("escape"), ["Clear the search, then close"]);
        assert_eq!(
            found("control"),
            ["New item", "New list", "Keep the Panel open"]
        );
    }

    #[test]
    fn a_single_letter_matches_a_key_or_a_word_start_not_the_middle_of_words() {
        // "n" is the N key and the start of "New", "Nth"; it is not the n inside "Panel" or "clipboard".
        let n = found("n");
        assert!(n.contains(&"New item".to_owned()));
        assert!(!n.contains(&"Quick capture the clipboard".to_owned()));
    }

    #[test]
    fn the_global_hotkeys_follow_the_settings_and_a_hotkey_that_is_off_shows_no_keys() {
        let (catalogue, titles) = fixture();
        let hotkeys = Hotkeys {
            toggle_panel: "Ctrl+Alt+Space".into(),
            capture: String::new(),
            ..Hotkeys::default()
        };
        let all = groups_of(&catalogue, &titles, &hotkeys, "");
        let anywhere = &all[0];
        assert_eq!(anywhere.scope, Scope::Anywhere);
        assert_eq!(anywhere.lines[0].keys, ["Ctrl", "Alt", "Space"]);
        assert!(anywhere.lines[1].keys.is_empty() && anywhere.lines[1].spoken.is_empty());
        // The rebound chord is searchable; an off hotkey is found by its name only.
        let by_chord = groups_of(&catalogue, &titles, &hotkeys, "space");
        assert_eq!(by_chord[0].lines[0].title, "Show or hide the Panel");
        assert_eq!(by_chord.len(), 1);
    }

    #[test]
    fn a_group_with_no_matching_line_is_left_out() {
        let (catalogue, titles) = fixture();
        let groups = groups_of(&catalogue, &titles, &Hotkeys::default(), "trash");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].scope, Scope::Main);
    }

    #[test]
    fn a_query_of_only_a_plus_looks_for_the_plus_key() {
        assert_eq!(words("+"), ["+"]);
        assert_eq!(words("ctrl +"), ["ctrl"]);
    }

    #[test]
    fn the_real_catalogue_is_grouped_by_scope_in_order_and_lists_the_panels_keep_open_key() {
        let scopes: Vec<Scope> = CATALOGUE.iter().map(|s| s.scope).collect();
        let mut sorted = scopes.clone();
        sorted.sort_by_key(|s| Scope::ALL.iter().position(|a| a == s));
        assert_eq!(
            scopes, sorted,
            "rows of one scope sit together, in Scope::ALL order"
        );
        assert!(
            CATALOGUE
                .iter()
                .any(|s| s.scope == Scope::Panel && s.keys == Keys::Chord("Ctrl+Shift+P")),
            "Ctrl+Shift+P (keep the Panel open) is listed under the Panel"
        );
    }

    #[test]
    fn the_main_window_keys_match_the_commands_that_have_a_shortcut() {
        let hotkeys = Hotkeys::default();
        for command in crate::commands::Command::ALL {
            let Some(chord) = command.shortcut(&hotkeys) else {
                continue;
            };
            let global = matches!(
                command,
                crate::commands::Command::ShowPanel
                    | crate::commands::Command::OpenPicker
                    | crate::commands::Command::QuickCapture
                    | crate::commands::Command::PasteNext
            );
            if global {
                continue;
            }
            assert!(
                CATALOGUE
                    .iter()
                    .any(|s| s.scope == Scope::Main
                        && matches!(s.keys, Keys::Chord(c) if c == chord)),
                "{command:?} ({chord}) is in the shortcuts list"
            );
        }
    }

    /// How a key named in the Panel's header comment is listed in the catalogue.
    enum Listed {
        /// A row with this chord.
        Chord(&'static str),
        /// A custom row with this spoken form.
        Spoken(&'static str),
    }

    /// Every key the header comment of `panel.slint` names, and the Panel row that lists it. The
    /// header is read from the source: a key added there without a row here fails the test.
    const PANEL_KEYS: &[(&str, Listed)] = &[
        ("Up", Listed::Spoken("Up or Down")),
        ("Down", Listed::Spoken("Up or Down")),
        ("PageUp", Listed::Spoken("PageUp or PageDown")),
        ("PageDown", Listed::Spoken("PageUp or PageDown")),
        ("Enter", Listed::Chord("Enter")),
        ("Shift+Enter", Listed::Chord("Shift+Enter")),
        ("Alt+1-9", Listed::Spoken("Alt+1 to 9")),
        ("Alt+Up", Listed::Spoken("Alt+Up or Alt+Down")),
        ("Alt+Down", Listed::Spoken("Alt+Up or Alt+Down")),
        ("Ctrl+Tab", Listed::Chord("Ctrl+Tab")),
        ("Ctrl+Shift+Tab", Listed::Chord("Ctrl+Shift+Tab")),
        ("Ctrl+F", Listed::Chord("Ctrl+F")),
        ("Menu", Listed::Spoken("Shift+F10 or the Menu key")),
        ("Shift+F10", Listed::Spoken("Shift+F10 or the Menu key")),
        ("Ctrl+K", Listed::Chord("Ctrl+K")),
        ("Ctrl+/", Listed::Chord("Ctrl+/")),
        ("F1", Listed::Chord("F1")),
        ("Ctrl+D", Listed::Chord("Ctrl+D")),
        ("Ctrl+P", Listed::Chord("Ctrl+P")),
        ("Ctrl+Shift+P", Listed::Chord("Ctrl+Shift+P")),
        ("Ctrl+E", Listed::Chord("Ctrl+E")),
        ("F2", Listed::Chord("F2")),
        ("Delete", Listed::Chord("Del")),
        ("Esc", Listed::Chord("Esc")),
        ("Ctrl+A", Listed::Chord("Ctrl+A")),
        ("Shift+Up", Listed::Spoken("Shift+Up or Shift+Down")),
        ("Shift+Down", Listed::Spoken("Shift+Up or Shift+Down")),
    ];

    /// The keys named in the "Keyboard" block of the Panel's header comment: the words that are
    /// chords (contain a `+`) or key names.
    fn header_keys() -> Vec<String> {
        const NAMES: &[&str] = &[
            "Up", "Down", "PageUp", "PageDown", "Enter", "Esc", "Delete", "Menu", "F1", "F2",
        ];
        let source = include_str!("../../snipdeck-ui/ui/panel.slint");
        let block: Vec<&str> = source
            .lines()
            .take_while(|l| l.starts_with("//"))
            .skip_while(|l| !l.starts_with("// Keyboard"))
            .take_while(|l| !l.starts_with("// Tooltips"))
            .collect();
        assert!(block.len() > 10, "the header's keyboard block was found");
        let mut keys = Vec::new();
        for line in block {
            for word in line.trim_start_matches('/').split_whitespace() {
                let word = word.trim_matches(|c| matches!(c, ',' | ';' | '(' | ')' | ':'));
                if (word.contains('+') && word.len() > 1 || NAMES.contains(&word))
                    && !keys.iter().any(|k| k == word)
                {
                    keys.push(word.to_owned());
                }
            }
        }
        keys
    }

    #[test]
    fn every_key_in_the_panels_header_comment_has_a_row_in_the_list() {
        let keys = header_keys();
        for key in &keys {
            assert!(
                PANEL_KEYS.iter().any(|(k, _)| k == key),
                "the header names {key}, which this test does not know: add it to PANEL_KEYS \
                 and to the shortcuts list"
            );
        }
        for (key, listed) in PANEL_KEYS {
            assert!(
                keys.iter().any(|k| k == key),
                "{key} is no longer in the Panel's header comment"
            );
            let found = CATALOGUE.iter().any(|s| {
                s.scope == Scope::Panel
                    && match (listed, s.keys) {
                        (Listed::Chord(c), Keys::Chord(row)) => *c == row,
                        (Listed::Spoken(w), Keys::Custom { spoken, .. }) => *w == spoken,
                        _ => false,
                    }
            });
            assert!(found, "{key} has no row under Panel in the shortcuts list");
        }
    }
}
