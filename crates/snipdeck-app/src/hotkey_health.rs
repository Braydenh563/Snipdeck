//! Hotkey health: what a global shortcut is doing right now, for the "press it now" test in the
//! welcome tour and Settings, and the one-click alternatives offered when a chord is taken.
//!
//! Slint-free. Whether Windows really delivers the chord is only known on Windows: this module
//! holds the decisions, the controller feeds it what the registration and the key press said.

use crate::{
    host::HotkeyFailure,
    settings::{HotkeyAction, Hotkeys},
};

/// What one shortcut is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyHealth {
    /// No chord is set for it.
    Off,
    /// "Pause hotkeys" is on: nothing is registered.
    Paused,
    /// Registering the chord failed (taken by another program, unusable, or the service is down).
    Failed(HotkeyFailure),
    /// Registered. Windows said yes; nobody has pressed it yet to prove the press arrives.
    Ready,
    /// The user pressed it during the test and the app heard it.
    Heard,
}

/// The chords offered when one fails, in the order tried. None uses Ctrl+Alt (`AltGr` on many
/// layouts); see `docs/reviews/default-chords.md`. Not verified against other programs on Windows.
pub const ALTERNATIVE_POOL: [&str; 6] = [
    "Ctrl+Shift+Y",
    "Ctrl+Shift+U",
    "Ctrl+Shift+Space",
    "Ctrl+Shift+G",
    "Ctrl+Shift+H",
    "Ctrl+Shift+F9",
];

/// At most this many alternatives are shown for one shortcut.
pub const MAX_ALTERNATIVES: usize = 3;

/// Chords to offer instead of `action`'s current one: from [`ALTERNATIVE_POOL`], leaving out the
/// chords the other actions hold and the one `action` has now (it is the one that failed).
#[must_use]
pub fn alternatives(action: HotkeyAction, hotkeys: &Hotkeys) -> Vec<String> {
    let taken: Vec<String> = HotkeyAction::ALL
        .into_iter()
        .map(|a| chord_text(hotkeys, a).to_ascii_lowercase())
        .filter(|c| !c.is_empty())
        .collect();
    let _ = action;
    ALTERNATIVE_POOL
        .into_iter()
        .filter(|c| !taken.contains(&c.to_ascii_lowercase()))
        .take(MAX_ALTERNATIVES)
        .map(str::to_owned)
        .collect()
}

/// The chord text stored for `action` (empty = off).
#[must_use]
pub fn chord_text(hotkeys: &Hotkeys, action: HotkeyAction) -> &str {
    match action {
        HotkeyAction::TogglePanel => &hotkeys.toggle_panel,
        HotkeyAction::Picker => &hotkeys.picker,
        HotkeyAction::Capture => &hotkeys.capture,
        HotkeyAction::PasteNext => &hotkeys.paste_next,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alternatives_skip_every_chord_in_use_and_stop_at_three() {
        let hotkeys = Hotkeys::new_install();
        let offered = alternatives(HotkeyAction::TogglePanel, &hotkeys);
        assert_eq!(offered.len(), MAX_ALTERNATIVES);
        for chord in &offered {
            assert!(!chord.contains("Alt"), "{chord}");
            for action in HotkeyAction::ALL {
                assert!(
                    !chord.eq_ignore_ascii_case(chord_text(&hotkeys, action)),
                    "{chord} is already held by {action:?}"
                );
            }
        }
        // Y (Panel), Space (Picker) and U (Paste next) are held on a new install.
        assert_eq!(offered, ["Ctrl+Shift+G", "Ctrl+Shift+H", "Ctrl+Shift+F9"]);
    }

    #[test]
    fn a_cleared_shortcut_holds_nothing_back() {
        let mut hotkeys = Hotkeys::new_install();
        hotkeys.toggle_panel = String::new();
        let offered = alternatives(HotkeyAction::Picker, &hotkeys);
        assert_eq!(offered[0], "Ctrl+Shift+Y");
    }
}
