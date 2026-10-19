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
/// How many daily backups may be kept (Settings > Backups).
pub const BACKUPS_KEEP: std::ops::RangeInclusive<u32> = 1..=60;
/// The default number of backups kept (SPEC 5.7a).
pub const BACKUPS_KEEP_DEFAULT: u32 = 7;

/// How many monitors' Panel geometry is remembered.
pub const PANEL_GEOMETRY_LIMIT: usize = 16;
/// The largest Panel side stored, in physical pixels (a hand-edited file cannot ask for more).
const PANEL_SIDE_LIMIT: u32 = 16_384;

/// Where the Panel sat on one monitor the last time the user moved or resized it. Physical
/// pixels in desktop coordinates, as the OS reports them; `monitor_id` names the monitor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelGeometry {
    /// The monitor this is for (its device name and size, see `frame::Monitor::id`).
    pub monitor_id: String,
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width.
    pub w: u32,
    /// Height.
    pub h: u32,
    /// The screen edge the Panel is docked to on this monitor, if the user docked it. A
    /// floating Panel has none. Absent in older files; an unknown name reads as none.
    #[serde(
        default,
        deserialize_with = "lenient_edge",
        skip_serializing_if = "Option::is_none"
    )]
    pub edge: Option<DockEdge>,
}

/// Reads an edge name; anything else (a hand-edited or future value) is "not docked" rather
/// than a reason to drop the whole geometry list.
fn lenient_edge<'de, D>(deserializer: D) -> Result<Option<DockEdge>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

/// A screen edge the Panel can dock to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DockEdge {
    /// The left edge of the work area.
    Left,
    /// The right edge.
    Right,
    /// The top edge.
    Top,
    /// The bottom edge.
    Bottom,
}

impl DockEdge {
    /// Every edge, in the order Settings lists them.
    pub const ALL: [Self; 4] = [Self::Left, Self::Right, Self::Top, Self::Bottom];

    /// The handle strip runs up and down (the Panel is docked to the left or the right).
    #[must_use]
    pub const fn vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }

    /// A name for the log.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}

/// What makes a docked Panel slide out of its handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DockTrigger {
    /// The Panel never collapses to a handle.
    #[default]
    Off,
    /// Resting the pointer on the handle opens it.
    Hover,
    /// Clicking the handle opens it.
    Click,
    /// Either.
    Both,
}

impl DockTrigger {
    /// Every choice, in the order Settings lists them.
    pub const ALL: [Self; 4] = [Self::Off, Self::Hover, Self::Click, Self::Both];

    /// Hovering the handle opens the Panel.
    #[must_use]
    pub const fn hover(self) -> bool {
        matches!(self, Self::Hover | Self::Both)
    }

    /// Clicking the handle opens the Panel.
    #[must_use]
    pub const fn click(self) -> bool {
        matches!(self, Self::Click | Self::Both)
    }
}

/// How long the pointer rests on the handle before the Panel opens, in milliseconds.
pub const HOVER_DELAY_MS: std::ops::RangeInclusive<u32> = 0..=2000;
/// How long the pointer is off the open Panel before it collapses, in milliseconds.
pub const LEAVE_DELAY_MS: std::ops::RangeInclusive<u32> = 0..=5000;

/// The edge dock (SPEC 5.1): a docked Panel collapses to a thin handle and slides out again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DockSettings {
    /// What opens the collapsed Panel; `off` keeps a docked Panel open.
    pub trigger: DockTrigger,
    /// Hover delay, 0 to 2000 ms (DESIGN.md: 150 ms).
    pub hover_delay_ms: u32,
    /// Leave delay, 0 to 5000 ms.
    pub leave_delay_ms: u32,
}

impl Default for DockSettings {
    fn default() -> Self {
        Self {
            trigger: DockTrigger::Off,
            hover_delay_ms: 150,
            leave_delay_ms: 400,
        }
    }
}

impl DockSettings {
    /// Puts both delays in range.
    pub fn sanitize(&mut self) {
        self.hover_delay_ms = self
            .hover_delay_ms
            .clamp(*HOVER_DELAY_MS.start(), *HOVER_DELAY_MS.end());
        self.leave_delay_ms = self
            .leave_delay_ms
            .clamp(*LEAVE_DELAY_MS.start(), *LEAVE_DELAY_MS.end());
    }
}

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
    pub(crate) fn slot(&mut self, action: HotkeyAction) -> &mut String {
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

/// The names in the `no_paste_apps` list of a settings file that is not valid JSON, read up to
/// the point where the text stops making sense (a file cut short keeps the names before the cut).
fn salvage_no_paste_apps(text: &str) -> Vec<String> {
    let Some(key) = text.find("\"no_paste_apps\"") else {
        return Vec::new();
    };
    let rest = text[key + "\"no_paste_apps\"".len()..].trim_start();
    let Some(rest) = rest.strip_prefix(':') else {
        return Vec::new();
    };
    let Some(mut rest) = rest.trim_start().strip_prefix('[') else {
        return Vec::new();
    };
    let mut apps = Vec::new();
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
        if rest.is_empty() || rest.starts_with(']') {
            break;
        }
        let mut stream = serde_json::Deserializer::from_str(rest).into_iter::<serde_json::Value>();
        let Some(Ok(value)) = stream.next() else {
            break;
        };
        let used = stream.byte_offset();
        if let serde_json::Value::String(name) = value {
            apps.push(name);
        }
        rest = &rest[used..];
    }
    apps
}

/// The user's settings.
#[allow(
    clippy::struct_excessive_bools,
    reason = "one field per setting; they are independent switches, not a state"
)]
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
    /// The Panel has a taskbar button (and an Alt+Tab entry). Off by default: the tray icon and
    /// the panel shortcut bring it back.
    pub panel_in_taskbar: bool,
    /// An unpinned Panel hides when another app takes the foreground. A pinned Panel and the
    /// Main window never hide this way.
    pub panel_hide_on_blur: bool,
    /// Panel width in logical pixels.
    pub panel_width: u32,
    /// How many daily backups to keep (1 to 60).
    pub backups_keep: u32,
    /// Where the Panel sat on each monitor it was moved or resized on; wins over `panel_width`
    /// there. Absent in older files.
    pub panel_geometry: Vec<PanelGeometry>,
    /// Executable names that never receive an auto-paste (lower case).
    pub no_paste_apps: Vec<String>,
    /// The edge dock: when a docked Panel collapses to a handle, and the delays.
    pub dock: DockSettings,
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
            panel_in_taskbar: false,
            panel_hide_on_blur: true,
            panel_width: PANEL_WIDTH_DEFAULT,
            backups_keep: BACKUPS_KEEP_DEFAULT,
            panel_geometry: Vec::new(),
            no_paste_apps: Vec::new(),
            dock: DockSettings::default(),
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
            .unwrap_or_else(|_| {
                // Not JSON (a truncated write, say). The exclusion list is a safety setting:
                // keep the names that can still be read rather than pasting into every app.
                Self {
                    no_paste_apps: salvage_no_paste_apps(text),
                    ..Self::default()
                }
            });
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
        field!(panel_in_taskbar);
        field!(panel_hide_on_blur);
        field!(panel_width);
        field!(backups_keep);
        field!(panel_geometry);
        // Element by element: one bad entry must not drop the whole list, or auto-paste would
        // go into the apps the user excluded.
        match map.get("no_paste_apps") {
            Some(serde_json::Value::Array(items)) => {
                settings.no_paste_apps = items
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
            }
            Some(serde_json::Value::String(one)) => settings.no_paste_apps = vec![one.clone()],
            _ => {}
        }
        field!(dock);
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
        self.backups_keep = self
            .backups_keep
            .clamp(*BACKUPS_KEEP.start(), *BACKUPS_KEEP.end());
        self.sanitize_panel_geometry();
        self.dock.sanitize();
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

    /// Keeps one usable entry per monitor, the newest last, and at most
    /// [`PANEL_GEOMETRY_LIMIT`] of them.
    fn sanitize_panel_geometry(&mut self) {
        let mut kept: Vec<PanelGeometry> = Vec::new();
        for mut g in std::mem::take(&mut self.panel_geometry) {
            let id = g.monitor_id.trim();
            if id.is_empty() || g.w == 0 || g.h == 0 {
                continue;
            }
            g.monitor_id = id.into();
            g.w = g.w.min(PANEL_SIDE_LIMIT);
            g.h = g.h.min(PANEL_SIDE_LIMIT);
            kept.retain(|k| k.monitor_id != g.monitor_id);
            kept.push(g);
        }
        let extra = kept.len().saturating_sub(PANEL_GEOMETRY_LIMIT);
        kept.drain(..extra);
        self.panel_geometry = kept;
    }

    /// Remembers `geometry` for its monitor as the newest entry (call [`sanitize`](Self::sanitize)
    /// after, or go through the controller, which does).
    pub fn remember_panel_geometry(&mut self, geometry: PanelGeometry) {
        self.panel_geometry
            .retain(|g| g.monitor_id != geometry.monitor_id);
        self.panel_geometry.push(geometry);
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
        assert_eq!(settings.hotkey_conflicts(), Vec::new());
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
    fn an_old_file_without_panel_geometry_loads_with_none() {
        let settings = Settings::from_json(r#"{"version":1,"panel_width":300}"#);
        assert_eq!(settings.panel_geometry, Vec::new());
        assert_eq!(settings.panel_width, 300);
    }

    #[test]
    fn panel_geometry_round_trips() {
        let mut settings = Settings::default();
        settings.remember_panel_geometry(PanelGeometry {
            monitor_id: "DISPLAY1".into(),
            x: -1920,
            y: 40,
            w: 400,
            h: 700,
            edge: Some(DockEdge::Right),
        });
        let back = Settings::from_json(&settings.to_json());
        assert_eq!(back.panel_geometry, settings.panel_geometry);
    }

    #[test]
    fn the_dock_is_off_by_default_with_the_documented_delays() {
        let dock = Settings::default().dock;
        assert_eq!(dock.trigger, DockTrigger::Off);
        assert_eq!((dock.hover_delay_ms, dock.leave_delay_ms), (150, 400));
    }

    #[test]
    fn dock_settings_round_trip_and_an_old_file_gets_the_defaults() {
        let settings = Settings {
            dock: DockSettings {
                trigger: DockTrigger::Both,
                hover_delay_ms: 300,
                leave_delay_ms: 900,
            },
            ..Settings::default()
        };
        assert_eq!(Settings::from_json(&settings.to_json()), settings);
        assert_eq!(
            Settings::from_json(r#"{"version":1}"#).dock,
            DockSettings::default()
        );
        let partial = Settings::from_json(r#"{"dock":{"trigger":"hover"}}"#).dock;
        assert_eq!(partial.trigger, DockTrigger::Hover);
        assert_eq!(partial.hover_delay_ms, 150);
    }

    #[test]
    fn the_dock_delays_are_clamped() {
        let dock = Settings::from_json(
            r#"{"dock":{"trigger":"click","hover_delay_ms":999999,"leave_delay_ms":999999}}"#,
        )
        .dock;
        assert_eq!((dock.hover_delay_ms, dock.leave_delay_ms), (2000, 5000));
        let mut settings = Settings::default();
        settings.dock.hover_delay_ms = 0;
        settings.dock.leave_delay_ms = 0;
        settings.sanitize();
        assert_eq!(
            (settings.dock.hover_delay_ms, settings.dock.leave_delay_ms),
            (0, 0)
        );
    }

    #[test]
    fn a_bad_dock_trigger_falls_back_without_losing_the_other_fields() {
        let settings = Settings::from_json(r#"{"dock":{"trigger":"sideways"},"theme":"dark"}"#);
        assert_eq!(settings.dock, DockSettings::default());
        assert_eq!(settings.theme, Theme::Dark);
    }

    #[test]
    fn a_geometry_without_an_edge_is_floating_and_an_unknown_edge_is_too() {
        let settings = Settings::from_json(
            r#"{"panel_geometry":[
                {"monitor_id":"A","x":0,"y":0,"w":300,"h":500},
                {"monitor_id":"B","x":0,"y":0,"w":300,"h":500,"edge":"diagonal"},
                {"monitor_id":"C","x":0,"y":0,"w":300,"h":500,"edge":"top"}
            ]}"#,
        );
        let edges: Vec<_> = settings.panel_geometry.iter().map(|g| g.edge).collect();
        assert_eq!(edges, [None, None, Some(DockEdge::Top)]);
        assert!(
            !Settings::default().to_json().contains("edge"),
            "no edge is written for a floating Panel"
        );
    }

    #[test]
    fn panel_geometry_keeps_one_usable_entry_per_monitor_and_a_bounded_list() {
        let entry = |id: &str, w: u32| PanelGeometry {
            monitor_id: id.into(),
            x: 0,
            y: 0,
            w,
            h: 500,
            edge: None,
        };
        let mut settings = Settings {
            panel_geometry: vec![
                entry("A", 300),
                entry(" ", 300),
                entry("B", 0),
                entry("A", 350),
                entry("C", 99_999),
            ],
            ..Settings::default()
        };
        settings.sanitize();
        let ids: Vec<_> = settings
            .panel_geometry
            .iter()
            .map(|g| (g.monitor_id.as_str(), g.w))
            .collect();
        assert_eq!(ids, [("A", 350), ("C", 16_384)]);

        settings.panel_geometry = (0..40).map(|i| entry(&format!("M{i}"), 300)).collect();
        settings.sanitize();
        assert_eq!(settings.panel_geometry.len(), PANEL_GEOMETRY_LIMIT);
        assert_eq!(
            settings.panel_geometry.last().expect("last").monitor_id,
            "M39"
        );
    }

    #[test]
    fn excluded_apps_are_normalised() {
        let settings = Settings::from_json(
            r#"{"no_paste_apps":[" KeePass.EXE ","keepass.exe","","Putty.exe"]}"#,
        );
        assert_eq!(settings.no_paste_apps, ["keepass.exe", "putty.exe"]);
    }

    /// Security review 2, finding 9: one bad element must not empty the exclusion list, or
    /// auto-paste would go into the apps the user excluded.
    #[test]
    fn one_bad_excluded_app_keeps_the_others() {
        let settings = Settings::from_json(
            r#"{"theme":"dark","no_paste_apps":["KeePass.exe",5,null,["x"],"1Password.exe"]}"#,
        );
        assert_eq!(settings.no_paste_apps, ["keepass.exe", "1password.exe"]);
        assert_eq!(settings.theme, Theme::Dark);

        let settings = Settings::from_json(r#"{"no_paste_apps":["keepass.exe",5]}"#);
        assert_eq!(settings.no_paste_apps, ["keepass.exe"]);
    }

    #[test]
    fn a_list_of_the_wrong_shape_is_an_empty_list_and_a_lone_name_is_a_list() {
        let none = Vec::<String>::new();
        assert_eq!(
            Settings::from_json(r#"{"no_paste_apps":5}"#).no_paste_apps,
            none
        );
        assert_eq!(
            Settings::from_json(r#"{"no_paste_apps":{"a":"b.exe"}}"#).no_paste_apps,
            none
        );
        assert_eq!(
            Settings::from_json(r#"{"no_paste_apps":"KeePass.exe"}"#).no_paste_apps,
            ["keepass.exe"]
        );
    }

    #[test]
    fn a_truncated_file_keeps_the_excluded_apps_it_still_has() {
        let full = Settings {
            theme: Theme::Dark,
            no_paste_apps: vec![
                "keepass.exe".into(),
                "1password.exe".into(),
                "putty.exe".into(),
            ],
            ..Settings::default()
        }
        .to_json();
        let start = full.find("\"no_paste_apps\"").unwrap();
        let end = full.find("putty.exe").unwrap();
        // Cut anywhere inside the list: every whole name before the cut survives, a cut name
        // does not.
        let all = ["keepass.exe", "1password.exe", "putty.exe"];
        for cut in start..=end + 4 {
            let settings = Settings::from_json(&full[..cut]);
            let kept: Vec<&str> = settings.no_paste_apps.iter().map(String::as_str).collect();
            let whole = all
                .iter()
                .filter(|name| {
                    let at = full.find(&format!("\"{name}\"")).unwrap();
                    at + name.len() + 2 <= cut
                })
                .count();
            assert_eq!(kept, all[..whole], "cut at {cut}");
        }
        let cut = &full[..full.find("putty.exe").unwrap() - 1];
        assert_eq!(
            Settings::from_json(cut).no_paste_apps,
            ["keepass.exe", "1password.exe"]
        );
        // Cut before the list starts: nothing to keep.
        assert_eq!(
            Settings::from_json(&full[..start]).no_paste_apps,
            Vec::<String>::new()
        );
    }

    #[test]
    fn old_files_without_the_window_fields_get_the_defaults() {
        let settings = Settings::from_json("{}");
        assert!(!settings.panel_in_taskbar);
        assert!(settings.panel_hide_on_blur);
        let settings = Settings::from_json(r#"{"theme":"dark","close_to_tray":false}"#);
        assert!(!settings.panel_in_taskbar && settings.panel_hide_on_blur);
    }

    #[test]
    fn one_bad_window_field_keeps_the_rest() {
        let settings = Settings::from_json(
            r#"{"panel_in_taskbar":"x","panel_hide_on_blur":false,"theme":"dark"}"#,
        );
        assert!(!settings.panel_in_taskbar, "the bad one falls back");
        assert!(!settings.panel_hide_on_blur);
        assert_eq!(settings.theme, Theme::Dark);
    }

    #[test]
    fn backups_to_keep_defaults_to_seven_and_is_held_between_one_and_sixty() {
        assert_eq!(Settings::from_json("{}").backups_keep, 7);
        assert_eq!(Settings::from_json(r#"{"backups_keep":0}"#).backups_keep, 1);
        assert_eq!(
            Settings::from_json(r#"{"backups_keep":900}"#).backups_keep,
            60
        );
        assert_eq!(
            Settings::from_json(r#"{"backups_keep":12}"#).backups_keep,
            12
        );
        let bad = Settings::from_json(r#"{"backups_keep":"many","theme":"dark"}"#);
        assert_eq!((bad.backups_keep, bad.theme), (7, Theme::Dark));
        let kept = Settings {
            backups_keep: 30,
            ..Settings::default()
        };
        assert_eq!(Settings::from_json(&kept.to_json()), kept);
    }

    #[test]
    fn the_window_fields_round_trip() {
        let settings = Settings {
            panel_in_taskbar: true,
            panel_hide_on_blur: false,
            ..Settings::default()
        };
        assert_eq!(Settings::from_json(&settings.to_json()), settings);
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
        assert_eq!(settings.hotkey_conflicts(), Vec::new());
    }
}
