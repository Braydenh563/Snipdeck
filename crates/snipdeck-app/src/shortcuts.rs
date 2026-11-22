//! The keyboard shortcuts list (Help > Shortcuts): every key Snipdeck answers, grouped by where it
//! works, and the filter that narrows the list as the user types in its search box.
//!
//! Slint-free. The words come from the UI crate's `ShortcutTexts` global (translated there, in the
//! order of [`CATALOGUE`]); the chords come from here, and the four global hotkeys read the chords in
//! the settings, so a rebound hotkey shows as rebound. The key names are not translated (they are the
//! names on the keyboard).

use crate::{
    commands::{KeyWords, caps, spoken},
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
    /// The window that asks for the master password.
    Unlock,
}

impl Scope {
    /// Every scope, in the order the list shows them.
    pub const ALL: [Self; 6] = [
        Self::Anywhere,
        Self::Main,
        Self::Panel,
        Self::Picker,
        Self::Editors,
        Self::Unlock,
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
    /// Not keys at all but a word ("Trigger", "Hello"): shown as plain text, not as a cap.
    Plain {
        /// The word.
        text: &'static str,
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

/// Between two alternatives in a `Custom` list of caps ("Up" or "Down"); the list draws it as the word "or".
pub const ALT: &str = "|";

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

const fn plain(scope: Scope, text: &'static str, spoken: &'static str) -> Shortcut {
    Shortcut {
        scope,
        keys: Keys::Plain { text, spoken },
    }
}

const fn global(action: HotkeyAction) -> Shortcut {
    Shortcut {
        scope: Scope::Anywhere,
        keys: Keys::Global(action),
    }
}

use Scope::{Anywhere, Editors, Main, Panel, Picker, Unlock};

/// Every shortcut, in the order of `ShortcutTexts.titles` (a test in `tests/help_window.rs` checks the
/// length) and grouped by scope in the order of [`Scope::ALL`].
pub const CATALOGUE: [Shortcut; 99] = [
    // Anywhere in Windows.
    global(HotkeyAction::TogglePanel),
    global(HotkeyAction::Picker),
    global(HotkeyAction::Capture),
    global(HotkeyAction::PasteNext),
    // Typed in any program (Windows only): the Trigger field of an item, and the suggestion hint.
    plain(Anywhere, "Trigger", "The item’s trigger"),
    custom(Anywhere, &["Tab"], "Tab while the suggestion hint shows"),
    // Main window.
    row(Main, "Ctrl+K"),
    row(Main, "Ctrl+Shift+P"),
    row(Main, "Ctrl+N"),
    row(Main, "Ctrl+Shift+N"),
    row(Main, "Ctrl+F"),
    row(Main, "Ctrl+,"),
    row(Main, "Ctrl+/"),
    row(Main, "F1"),
    row(Main, "Enter"),
    row(Main, "Shift+Enter"),
    row(Main, "Alt+Enter"),
    row(Main, "Ctrl+E"),
    row(Main, "F2"),
    row(Main, "Ctrl+D"),
    row(Main, "Ctrl+P"),
    row(Main, "Del"),
    row(Main, "Ctrl+A"),
    row(Main, "Ctrl+Space"),
    row(Main, "Ctrl+Z"),
    custom(
        Main,
        &["Ctrl", "Y", ALT, "Ctrl", "Shift", "Z"],
        "Ctrl+Y or Ctrl+Shift+Z",
    ),
    row(Main, "Ctrl+W"),
    row(Main, "Ctrl+B"),
    custom(
        Main,
        &["Alt", "Up", ALT, "Alt", "Down"],
        "Alt+Up or Alt+Down",
    ),
    custom(Main, &["Space"], "Hold Space"),
    row(Main, "Ctrl+C"),
    row(Main, "Ctrl+V"),
    custom(Main, &["/"], "Slash"),
    custom(Main, &["Shift", "F10"], "Shift+F10 or the Menu key"),
    row(Main, "F6"),
    // Groups in the Main window.
    row(Main, "Ctrl+G"),
    custom(
        Main,
        &["Left", ALT, "Right", ALT, "Enter"],
        "Left, Right or Enter on a group header",
    ),
    row(Main, "F2"),
    row(Main, "Alt+Up"),
    row(Main, "Alt+Down"),
    row(Main, "Del"),
    // Panel.
    custom(Panel, &["A-Z"], "Type"),
    custom(Panel, &["Up", ALT, "Down"], "Up or Down"),
    custom(Panel, &["PgUp", ALT, "PgDn"], "PageUp or PageDown"),
    row(Panel, "Enter"),
    row(Panel, "Shift+Enter"),
    row(Panel, "Alt+Enter"),
    custom(
        Panel,
        &["Alt", "Up", ALT, "Alt", "Down"],
        "Alt+Up or Alt+Down",
    ),
    custom(Panel, &["Alt", "1-9"], "Alt+1 to 9"),
    custom(Panel, &["Space"], "Hold Space"),
    custom(
        Panel,
        &["Shift", "Up", ALT, "Shift", "Down"],
        "Shift+Up or Shift+Down",
    ),
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
    custom(
        Panel,
        &["Ctrl", "Y", ALT, "Ctrl", "Shift", "Z"],
        "Ctrl+Y or Ctrl+Shift+Z",
    ),
    row(Panel, "Ctrl+W"),
    row(Panel, "Ctrl+/"),
    row(Panel, "F1"),
    row(Panel, "Esc"),
    row(Panel, "Ctrl+C"),
    row(Panel, "Ctrl+V"),
    row(Panel, "F6"),
    // The Symbols and Colours tabs.
    row(Panel, "Ctrl+S"),
    row(Panel, "Ctrl+Shift+S"),
    row(Panel, "Ctrl+I"),
    custom(Panel, &["Ctrl", "C"], "Ctrl+C in the Colours tab"),
    // Groups.
    row(Panel, "Ctrl+G"),
    custom(
        Panel,
        &["Left", ALT, "Right", ALT, "Enter", ALT, "Space"],
        "Left, Right, Enter or Space on a group header",
    ),
    row(Panel, "F2"),
    custom(
        Panel,
        &["Alt", "Up", ALT, "Alt", "Down"],
        "Alt+Up or Alt+Down on a group header",
    ),
    custom(Panel, &["Shift", "F10"], "Shift+F10 or the Menu key"),
    // Picker.
    custom(Picker, &["Up", ALT, "Down"], "Up or Down"),
    row(Picker, "Enter"),
    row(Picker, "Shift+Enter"),
    row(Picker, "Alt+Enter"),
    custom(Picker, &["Alt", "1-9"], "Alt+1 to 9"),
    custom(Picker, &["Tab", ALT, "Shift", "Tab"], "Tab or Shift+Tab"),
    row(Picker, "Ctrl+E"),
    row(Picker, "Ctrl+O"),
    row(Picker, "Esc"),
    // Editors.
    custom(
        Editors,
        &["Ctrl", "Enter", ALT, "Ctrl", "S"],
        "Ctrl+Enter or Ctrl+S",
    ),
    row(Editors, "Alt+P"),
    custom(Editors, &["Esc", ALT, "Ctrl", "W"], "Esc or Ctrl+W"),
    row(Editors, "Ctrl+Shift+Enter"),
    row(Editors, "Del"),
    // The unlock window.
    plain(Unlock, "Hello", "Unlock with Windows Hello"),
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
    /// A word rather than keys: drawn as plain text.
    pub plain: bool,
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
    fn resolve(self, hotkeys: &Hotkeys, words: &KeyWords) -> (Vec<String>, String) {
        match self {
            Self::Chord(text) => (caps(text), spoken(text, words)),
            Self::Custom { caps, spoken } => (
                caps.iter().map(|c| (*c).to_owned()).collect(),
                words.phrase(spoken),
            ),
            Self::Plain { text, spoken } => (vec![words.phrase(text)], words.phrase(spoken)),
            Self::Global(action) => crate::commands::chord_caps(hotkeys, action, words),
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
        .filter(|k| *k != ALT)
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
    key_words: &KeyWords,
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
                let (keys, spoken) = shortcut.keys.resolve(hotkeys, key_words);
                Line {
                    title: title.clone(),
                    keys,
                    spoken,
                    plain: matches!(shortcut.keys, Keys::Plain { .. }),
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
pub fn groups(
    titles: &[String],
    key_words: &KeyWords,
    hotkeys: &Hotkeys,
    query: &str,
) -> Vec<Group> {
    groups_of(&CATALOGUE, titles, key_words, hotkeys, query)
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
        groups_of(&catalogue, &titles, &KeyWords::english(), &off, query)
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
            ..Hotkeys::new_install()
        };
        let all = groups_of(&catalogue, &titles, &KeyWords::english(), &hotkeys, "");
        let anywhere = &all[0];
        assert_eq!(anywhere.scope, Scope::Anywhere);
        assert_eq!(anywhere.lines[0].keys, ["Ctrl", "Alt", "Space"]);
        assert!(anywhere.lines[1].keys.is_empty() && anywhere.lines[1].spoken.is_empty());
        // The rebound chord is searchable; an off hotkey is found by its name only.
        let by_chord = groups_of(&catalogue, &titles, &KeyWords::english(), &hotkeys, "space");
        assert_eq!(by_chord[0].lines[0].title, "Show or hide the Panel");
        assert_eq!(by_chord.len(), 1);
    }

    #[test]
    fn a_group_with_no_matching_line_is_left_out() {
        let (catalogue, titles) = fixture();
        let groups = groups_of(
            &catalogue,
            &titles,
            &KeyWords::english(),
            &Hotkeys::new_install(),
            "trash",
        );
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].scope, Scope::Main);
    }

    #[test]
    fn a_query_of_only_a_plus_looks_for_the_plus_key() {
        assert_eq!(words("+"), ["+"]);
        assert_eq!(words("ctrl +"), ["ctrl"]);
    }

    #[test]
    fn the_real_catalogue_is_grouped_by_scope_in_order_and_lists_the_panels_commands_chord() {
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
            "Ctrl+Shift+P (Commands, as in the Main window) is listed under the Panel"
        );
    }

    #[test]
    fn the_main_window_keys_match_the_commands_that_have_a_shortcut() {
        let hotkeys = Hotkeys::new_install();
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
        ("Alt+Enter", Listed::Chord("Alt+Enter")),
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

    /// The user guide's "Item editor" rows list every Editors shortcut (the guide is how a user
    /// finds Preview without hunting for the button).
    #[test]
    fn the_user_guide_lists_every_editor_shortcut() {
        let guide = include_str!("../../../docs/user-guide.md");
        for s in CATALOGUE.iter().filter(|s| s.scope == Editors) {
            let Keys::Chord(chord) = s.keys else { continue };
            if chord == "Esc" {
                continue;
            }
            assert!(
                guide
                    .lines()
                    .any(|l| l.starts_with("| Item editor |") && l.contains(&format!("`{chord}`"))),
                "docs/user-guide.md has no `| Item editor |` row for {chord}"
            );
        }
    }

    /// The lower-case words of a key text: "Alt+1" and "Alt+1 to 9" both give `alt` and `1`.
    fn atoms(text: &str) -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(|w| match w.to_lowercase().as_str() {
                "pageup" => "pgup".to_owned(),
                "pagedown" => "pgdn".to_owned(),
                "delete" => "del".to_owned(),
                "escape" => "esc".to_owned(),
                other => other.to_owned(),
            })
            .collect()
    }

    /// The scope a "Where" cell of the guide's keyboard table names.
    fn guide_scopes(cell: &str) -> Vec<Scope> {
        let mut scopes = Vec::new();
        for part in cell.split(", ") {
            let scope = match part {
                "Anywhere" | "Any program (Typing on)" | "Any program (Suggest on)" => {
                    Scope::Anywhere
                }
                "Main window" => Scope::Main,
                "Panel" => Scope::Panel,
                "Picker" => Scope::Picker,
                "Item editor" | "Fill form" => Scope::Editors,
                "Unlock window" => Scope::Unlock,
                other => panic!(
                    "the guide's keyboard table names a place this test does not know: {other}"
                ),
            };
            scopes.push(scope);
        }
        scopes
    }

    /// The keys and words of every row of `scope` as the list shows them with the install's hotkeys.
    fn listed(scope: Scope) -> Vec<Vec<String>> {
        let hotkeys = Hotkeys::new_install();
        CATALOGUE
            .iter()
            .filter(|s| s.scope == scope)
            .map(|s| {
                let (caps, spoken) = s.keys.resolve(&hotkeys, &KeyWords::english());
                atoms(&format!("{} {spoken}", caps.join(" ")))
            })
            .collect()
    }

    /// Every spoken form and every plain word of the list is one of `commands::PHRASES`, which is
    /// what gets translated: a new row with a new phrase must be added there too.
    #[test]
    fn every_phrase_the_list_speaks_is_one_that_gets_translated() {
        for s in &CATALOGUE {
            let named: Vec<&str> = match s.keys {
                Keys::Custom { spoken, .. } => vec![spoken],
                Keys::Plain { text, spoken } => vec![text, spoken],
                Keys::Chord(_) | Keys::Global(_) => continue,
            };
            for phrase in named {
                assert!(
                    crate::commands::PHRASES.contains(&phrase),
                    "{phrase:?} is not in commands::PHRASES"
                );
            }
        }
        for phrase in crate::commands::PHRASES {
            assert!(
                CATALOGUE.iter().any(|s| match s.keys {
                    Keys::Custom { spoken, .. } => spoken == phrase,
                    Keys::Plain { text, spoken } => text == phrase || spoken == phrase,
                    Keys::Chord(_) | Keys::Global(_) => false,
                }),
                "{phrase:?} in commands::PHRASES is spoken by no row"
            );
        }
    }

    #[test]
    fn every_shortcut_in_the_guides_keyboard_table_is_in_the_list() {
        let guide = include_str!("../../../docs/user-guide.md");
        let table: Vec<&str> = guide
            .lines()
            .skip_while(|l| !l.starts_with("## Keyboard reference"))
            .skip(1)
            .take_while(|l| !l.starts_with("## "))
            .filter(|l| l.starts_with('|'))
            .skip(2)
            .collect();
        assert!(
            table.len() > 40,
            "the table was found: {} rows",
            table.len()
        );
        let mut missing = Vec::new();
        for line in table {
            let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
            let (where_, keys) = (cells[0], cells[1]);
            // A cell with keys in code spans: each span is a key or chord. Without spans (a trigger,
            // a button, "none until you set one") the words of the cell must be in the row.
            let spans: Vec<&str> = keys.split('`').skip(1).step_by(2).collect();
            let wanted: Vec<Vec<String>> = if spans.is_empty() {
                if keys.starts_with("none") {
                    continue;
                }
                vec![atoms(keys.trim_matches('*').trim_start_matches("the "))]
            } else {
                spans.iter().map(|k| atoms(k)).collect()
            };
            for scope in guide_scopes(where_) {
                let rows = listed(scope);
                for key in &wanted {
                    if !rows.iter().any(|row| key.iter().all(|a| row.contains(a))) {
                        missing.push(format!("{keys} ({where_})"));
                    }
                }
            }
        }
        assert!(
            missing.is_empty(),
            "the guide's keyboard table lists keys the shortcuts list lacks: {missing:#?}"
        );
    }

    /// The other direction: every row of the list is in the guide's keyboard table, so the guide and the Help list are
    /// two views of the one catalogue and neither can gain a key alone.
    #[test]
    fn every_shortcut_in_the_list_is_in_the_guides_keyboard_table() {
        let guide = include_str!("../../../docs/user-guide.md");
        let table: Vec<(Vec<Scope>, Vec<Vec<String>>, String)> = guide
            .lines()
            .skip_while(|l| !l.starts_with("## Keyboard reference"))
            .skip(1)
            .take_while(|l| !l.starts_with("## "))
            .filter(|l| l.starts_with('|'))
            .skip(2)
            .map(|line| {
                let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
                let spans: Vec<Vec<String>> =
                    cells[1].split('`').skip(1).step_by(2).map(atoms).collect();
                let text = if spans.is_empty() {
                    vec![atoms(cells[1])]
                } else {
                    spans
                };
                (guide_scopes(cells[0]), text, line.to_owned())
            })
            .collect();
        let hotkeys = Hotkeys::new_install();
        let mut missing = Vec::new();
        for s in CATALOGUE {
            let (caps, _) = s.keys.resolve(&hotkeys, &KeyWords::english());
            if caps.is_empty() {
                continue;
            }
            // Each alternative ("Up" or "Down") is looked for on its own, among the keys of one guide row.
            for alt in caps.split(|c| c == ALT) {
                let want = atoms(&alt.join(" "));
                let found = table.iter().any(|(scopes, rows, _)| {
                    scopes.contains(&s.scope) && {
                        let have: Vec<&String> = rows.iter().flatten().collect();
                        want.iter().all(|a| have.contains(&a))
                    }
                });
                if !found {
                    missing.push(format!("{:?}: {}", s.scope, want.join(" ")));
                }
            }
        }
        assert_eq!(
            missing.len(),
            0,
            "the list has keys the guide's table lacks: {missing:#?}"
        );
    }
}
