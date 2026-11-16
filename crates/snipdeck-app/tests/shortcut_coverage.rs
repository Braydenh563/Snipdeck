//! UX-5: every key chord the windows handle is listed in Help > Shortcuts. The `.slint` sources are
//! scanned for `Ctrl+letter` handlers (a letter compared to `event.text` under a Ctrl condition) and each
//! one must appear in that window's rows of `shortcuts::CATALOGUE`. A few special keys are checked by
//! name, because their handlers do not look alike.

use snipdeck_app::shortcuts::{CATALOGUE, Keys, Scope};

const MAIN: &str = include_str!("../../snipdeck-ui/ui/main_window.slint");
const PANEL: &str = include_str!("../../snipdeck-ui/ui/panel.slint");
const PICKER: &str = include_str!("../../snipdeck-ui/ui/picker.slint");
const EDITORS: &str = include_str!("../../snipdeck-ui/ui/editors.slint");

/// What the rows of `scope` say, lower-cased: each chord, or the spoken form of a custom row.
fn listed(scope: Scope) -> String {
    CATALOGUE
        .iter()
        .filter(|s| s.scope == scope)
        .map(|s| match s.keys {
            Keys::Chord(c) => c.to_lowercase(),
            Keys::Custom { spoken, .. } | Keys::Plain { spoken, .. } => spoken.to_lowercase(),
            Keys::Global(_) => String::new(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Whether `line` tests for the Control key being down (a `!`-negated test does not count).
fn wants_ctrl(line: &str) -> bool {
    let positive_event = line.replace("!event.modifiers.control", "");
    positive_event.contains("event.modifiers.control")
        || line.contains("(ctrl &&")
        || line.contains("&& ctrl &&")
        || line.contains("(ctrl)")
        || line.contains("if (ctrl")
}

/// The letters a source handles under Ctrl: `event.text == "x"` on a line whose own condition, or an
/// enclosing block's, asks for Control.
fn ctrl_letters(source: &str) -> Vec<(usize, char)> {
    let lines: Vec<&str> = source.lines().collect();
    let mut found = Vec::new();
    for (at, line) in lines.iter().enumerate() {
        let mut rest = *line;
        while let Some(i) = rest.find("event.text == \"") {
            let tail = &rest[i + "event.text == \"".len()..];
            let mut chars = tail.chars();
            if let (Some(c), Some('"')) = (chars.next(), chars.next())
                && c.is_ascii_alphabetic()
                && c.is_ascii_lowercase()
            {
                let mut own = wants_ctrl(line);
                let mut depth = indent(line);
                for above in lines[..at].iter().rev() {
                    if own {
                        break;
                    }
                    if above.trim().is_empty() || indent(above) >= depth {
                        continue;
                    }
                    depth = indent(above);
                    own = wants_ctrl(above);
                }
                if own {
                    found.push((at + 1, c));
                }
            }
            rest = tail;
        }
    }
    found
}

/// Ctrl letters a window handles that are deliberately not listed, each with its reason.
const UNLISTED: &[(&str, char, &str)] = &[(
    "editors",
    'v',
    "the text field's own paste: the editor only routes it through the clipboard rules, so it is the standard editing key",
)];

fn check(name: &str, source: &str, scope: Scope) {
    let text = listed(scope);
    let missing: Vec<String> = ctrl_letters(source)
        .into_iter()
        .filter(|(_, c)| {
            !text.contains(&format!("ctrl+{c}")) && !text.contains(&format!("ctrl+shift+{c}"))
        })
        .filter(|(_, c)| !UNLISTED.iter().any(|(n, l, _)| *n == name && l == c))
        .map(|(line, c)| format!("{name}.slint:{line} Ctrl+{}", c.to_ascii_uppercase()))
        .collect();
    assert_eq!(
        missing,
        Vec::<String>::new(),
        "handled but missing from the shortcut list"
    );
}

#[test]
fn every_ctrl_chord_the_main_window_handles_is_in_the_shortcut_list() {
    check("main_window", MAIN, Scope::Main);
}

#[test]
fn every_ctrl_chord_the_panel_handles_is_in_the_shortcut_list() {
    check("panel", PANEL, Scope::Panel);
}

#[test]
fn every_ctrl_chord_the_picker_handles_is_in_the_shortcut_list() {
    check("picker", PICKER, Scope::Picker);
}

#[test]
fn every_ctrl_chord_an_editor_handles_is_in_the_shortcut_list() {
    check("editors", EDITORS, Scope::Editors);
}

#[test]
fn the_scan_sees_a_ctrl_chord_and_ignores_a_plain_letter() {
    let source = "\
if (event.modifiers.control && !event.modifiers.alt) {
    if (event.text == \"q\") { go(); }
}
if (!event.modifiers.control) {
    if (event.text == \"x\") { typed(); }
}
if (ctrl && event.text == \"j\") { go(); }
";
    let found: Vec<char> = ctrl_letters(source).into_iter().map(|(_, c)| c).collect();
    assert_eq!(found, ['q', 'j']);
}

#[test]
fn the_special_keys_that_work_are_in_the_shortcut_list() {
    // (scope, what the row says): keys whose handlers are not a Ctrl letter.
    let wanted = [
        (Scope::Main, "f6"),
        (Scope::Panel, "f6"),
        (Scope::Main, "slash"),
        (Scope::Main, "shift+f10"),
        (Scope::Main, "ctrl+shift+z"),
        (Scope::Panel, "ctrl+shift+z"),
        (Scope::Editors, "ctrl+shift+enter"),
        (Scope::Editors, "del"),
        (Scope::Picker, "ctrl+o"),
    ];
    for (scope, key) in wanted {
        assert!(listed(scope).contains(key), "{scope:?} should list {key}");
    }
}
