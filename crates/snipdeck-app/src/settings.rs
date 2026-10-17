//! User settings: what is stored, the defaults, and the rules that keep a hand-edited or
//! half-written file from breaking the app. Loading never fails: unreadable parts fall back to
//! their defaults (`docs/format.md` has no say here; this file is not part of a `.snipdeck`
//! export).

use serde::{Deserialize, Serialize};
use snipdeck_platform::chord::{Chord, Conflict, conflicts};

/// The settings file format version.
pub const VERSION: u32 = 1;
/// Narrowest and widest Panel, in logical pixels (DESIGN.md §5).
pub const PANEL_WIDTH: std::ops::RangeInclusive<u32> = 260..=480;
/// The Panel's default width.
pub const PANEL_WIDTH_DEFAULT: u32 = 320;

/// What a global hotkey does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HotkeyAction {
    /// Show or hide the Panel.
    TogglePanel,
    /// Open the Picker at the caret.
    Picker,
    /// Start a screen capture.
    Capture,
    /// Paste the next item of the paste stack.
    PasteNext,
}

impl HotkeyAction {
    /// Every action, in the order Settings lists them.
    pub const ALL: [Self; 4] = [
        Self::TogglePanel,
        Self::Picker,
        Self::Capture,
        Self::PasteNext,
    ];

    /// The default chord (SPEC §5).
    #[must_use]
    pub const fn default_chord(self) -> &'static str {
        match self {
            Self::TogglePanel => "Ctrl+Alt+Space",
            Self::Picker => "Ctrl+Shift+Space",
            Self::Capture => "Ctrl+Shift+2",
            Self::PasteNext => "Ctrl+Alt+V",
        }
    }
}

/// The colour theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    /// Follow Windows.
    #[default]
    System,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

/// Hotkeys as typed text (`Ctrl+Shift+Space`); empty means off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hotkeys {
    /// [`HotkeyAction::TogglePanel`].
    pub toggle_panel: String,
    /// [`HotkeyAction::Picker`].
    pub picker: String,
    /// [`HotkeyAction::Capture`].
    pub capture: String,
    /// [`HotkeyAction::PasteNext`].
    pub paste_next: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            toggle_panel: HotkeyAction::TogglePanel.default_chord().into(),
            picker: HotkeyAction::Picker.default_chord().into(),
            capture: HotkeyAction::Capture.default_chord().into(),
            paste_next: HotkeyAction::PasteNext.default_chord().into(),
        }
    }
}

impl Hotkeys {
    fn slot(&mut self, action: HotkeyAction) -> &mut String {
        match action {
            HotkeyAction::TogglePanel => &mut self.toggle_panel,
            HotkeyAction::Picker => &mut self.picker,
            HotkeyAction::Capture => &mut self.capture,
            HotkeyAction::PasteNext => &mut self.paste_next,
        }
    }

    /// The configured chord for `action`: `None` when it is off or not a valid chord.
    #[must_use]
    pub fn chord(&self, action: HotkeyAction) -> Option<Chord> {
        let text = match action {
            HotkeyAction::TogglePanel => &self.toggle_panel,
            HotkeyAction::Picker => &self.picker,
            HotkeyAction::Capture => &self.capture,
            HotkeyAction::PasteNext => &self.paste_next,
        };
        text.parse().ok()
    }
}

/// The user's settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// File format version, for forward migration.
    pub version: u32,
    /// Global hotkeys.
    pub hotkeys: Hotkeys,
    /// Colour theme.
    pub theme: Theme,
    /// Interface language as a locale tag (`de`, `pt-BR`); empty follows Windows.
    pub language: String,
    /// Start with Windows.
    pub autostart: bool,
    /// Closing the main window hides it to the tray instead of quitting.
    pub close_to_tray: bool,
    /// Panel width in logical pixels.
    pub panel_width: u32,
    /// Executable names that never receive an auto-paste (lower case).
    pub no_paste_apps: Vec<String>,
    /// Fetch site icons for links. Off by default: Snipdeck makes no network calls unless asked.
    pub site_icons: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: VERSION,
            hotkeys: Hotkeys::default(),
            theme: Theme::System,
            language: String::new(),
            autostart: false,
            close_to_tray: true,
            panel_width: PANEL_WIDTH_DEFAULT,
            no_paste_apps: Vec::new(),
            site_icons: false,
        }
    }
}

impl Settings {
    /// Reads settings from JSON. Never fails: text that is not JSON gives the defaults, and a
    /// field of the wrong type or an out-of-range value gives that field's default.
    #[must_use]
    pub fn from_json(text: &str) -> Self {
        let mut settings = serde_json::from_str::<Self>(text)
            .or_else(|_| {
                // One bad field must not reset the rest: keep every field that parses.
                let value: serde_json::Value = serde_json::from_str(text)?;
                Ok::<_, serde_json::Error>(Self::from_fields(&value))
            })
            .unwrap_or_default();
        settings.sanitize();
        settings
    }

    fn from_fields(value: &serde_json::Value) -> Self {
        let mut settings = Self::default();
        let Some(map) = value.as_object() else {
            return settings;
        };
        macro_rules! field {
            ($name:ident) => {
                if let Some(v) = map
                    .get(stringify!($name))
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                {
                    settings.$name = v;
                }
            };
        }
        field!(hotkeys);
        field!(theme);
        field!(language);
        field!(autostart);
        field!(close_to_tray);
        field!(panel_width);
        field!(no_paste_apps);
        field!(site_icons);
        settings
    }

    /// The settings as pretty JSON.
    ///
    /// # Panics
    ///
    /// Never: the type serialises to JSON without failing.
    #[must_use]
    #[allow(clippy::expect_used, reason = "plain data always serialises")]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("settings serialise")
    }

    /// Puts every field in range: width clamped, apps lower-cased and de-duplicated, invalid
    /// chords replaced by their defaults, and duplicates between chords turned off (the
    /// later action loses).
    pub fn sanitize(&mut self) {
        self.version = VERSION;
        self.panel_width = self
            .panel_width
            .clamp(*PANEL_WIDTH.start(), *PANEL_WIDTH.end());
        let mut seen = std::collections::BTreeSet::new();
        self.no_paste_apps = self
            .no_paste_apps
            .iter()
            .map(|a| a.trim().to_ascii_lowercase())
            .filter(|a| !a.is_empty() && seen.insert(a.clone()))
            .collect();
        for action in HotkeyAction::ALL {
            let slot = self.hotkeys.slot(action);
            if slot.trim().is_empty() {
                slot.clear();
            } else if let Ok(chord) = slot.parse::<Chord>() {
                *slot = chord.to_string();
            } else {
                *slot = action.default_chord().into();
            }
        }
        for conflict in self.hotkey_conflicts() {
            for action in conflict.ids.into_iter().skip(1) {
                self.hotkeys.slot(action).clear();
            }
        }
    }

    /// Actions that share a chord.
    #[must_use]
    pub fn hotkey_conflicts(&self) -> Vec<Conflict<HotkeyAction>> {
        let set: Vec<(HotkeyAction, Chord)> = HotkeyAction::ALL
            .into_iter()
            .filter_map(|a| self.hotkeys.chord(a).map(|c| (a, c)))
            .collect();
        conflicts(&set)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_conflict_free() {
        let settings = Settings::default();
        for action in HotkeyAction::ALL {
            assert!(settings.hotkeys.chord(action).is_some(), "{action:?}");
        }
        assert!(settings.hotkey_conflicts().is_empty());
        let mut sanitised = settings.clone();
        sanitised.sanitize();
        assert_eq!(sanitised, settings);
    }

    #[test]
    fn it_round_trips_through_json() {
        let settings = Settings {
            theme: Theme::Dark,
            language: "pt-BR".into(),
            no_paste_apps: vec!["keepass.exe".into()],
            ..Settings::default()
        };
        assert_eq!(Settings::from_json(&settings.to_json()), settings);
    }

    #[test]
    fn garbage_gives_the_defaults() {
        for text in ["", "not json", "[]", "null", "42"] {
            assert_eq!(Settings::from_json(text), Settings::default(), "{text:?}");
        }
    }

    #[test]
    fn one_bad_field_keeps_the_rest() {
        let settings =
            Settings::from_json(r#"{"theme":"dark","panel_width":"wide","autostart":true}"#);
        assert_eq!(settings.theme, Theme::Dark);
        assert!(settings.autostart);
        assert_eq!(settings.panel_width, PANEL_WIDTH_DEFAULT);
    }

    #[test]
    fn unknown_fields_and_old_versions_are_tolerated() {
        let settings =
            Settings::from_json(r#"{"version":0,"from_the_future":{"a":1},"theme":"light"}"#);
        assert_eq!(settings.theme, Theme::Light);
        assert_eq!(settings.version, VERSION);
    }

    #[test]
    fn the_panel_width_is_clamped() {
        assert_eq!(
            Settings::from_json(r#"{"panel_width":10}"#).panel_width,
            260
        );
        assert_eq!(
            Settings::from_json(r#"{"panel_width":9999}"#).panel_width,
            480
        );
    }

    #[test]
    fn excluded_apps_are_normalised() {
        let settings = Settings::from_json(
            r#"{"no_paste_apps":[" KeePass.EXE ","keepass.exe","","Putty.exe"]}"#,
        );
        assert_eq!(settings.no_paste_apps, ["keepass.exe", "putty.exe"]);
    }

    #[test]
    fn an_invalid_chord_falls_back_and_a_blank_one_stays_off() {
        let settings = Settings::from_json(r#"{"hotkeys":{"picker":"Hyper+Q","capture":"  "}}"#);
        assert_eq!(
            settings.hotkeys.picker,
            HotkeyAction::Picker.default_chord()
        );
        assert_eq!(settings.hotkeys.capture, "");
        assert!(settings.hotkeys.chord(HotkeyAction::Capture).is_none());
    }

    #[test]
    fn chords_are_stored_in_canonical_form() {
        let settings = Settings::from_json(r#"{"hotkeys":{"picker":"shift+CTRL+space"}}"#);
        assert_eq!(settings.hotkeys.picker, "Ctrl+Shift+Space");
    }

    #[test]
    fn a_shared_chord_is_kept_by_the_first_action_only() {
        let settings = Settings::from_json(
            r#"{"hotkeys":{"toggle_panel":"Ctrl+Shift+Space","picker":"Ctrl+Shift+Space"}}"#,
        );
        assert_eq!(settings.hotkeys.toggle_panel, "Ctrl+Shift+Space");
        assert_eq!(settings.hotkeys.picker, "");
        assert!(settings.hotkey_conflicts().is_empty());
    }
}
