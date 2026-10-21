//! User settings: what is stored, the defaults, and the rules that keep a hand-edited or
//! half-written file from breaking the app. Loading never fails: unreadable parts fall back to
//! their defaults (`docs/format.md` has no say here; this file is not part of a `.snipdeck`
//! export).

use serde::{Deserialize, Serialize};
use snipdeck_platform::chord::{Chord, Conflict, conflicts};
use snipdeck_platform::keys::{BUILTIN_PASTE_KEYS, PasteKey};

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
/// How long a copied sensitive item may stay on the clipboard, in seconds (Settings > Privacy);
/// 0 never clears it (SPEC 5.7).
pub const CLIPBOARD_CLEAR_SECS: std::ops::RangeInclusive<u32> = 0..=600;
/// The default time before a copied sensitive item is cleared from the clipboard.
pub const CLIPBOARD_CLEAR_SECS_DEFAULT: u32 = 30;

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
    /// The width and height the Panel had before it was docked, so that undocking (or dragging it
    /// off its edge) gives them back. Only set while docked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_dock: Option<(u32, u32)>,
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

    /// The default chord (SPEC §5). Empty means off: Capture saves the clipboard text without
    /// asking, so it only works once the user has chosen a chord (SPEC 5.5).
    #[must_use]
    pub const fn default_chord(self) -> &'static str {
        match self {
            Self::TogglePanel => "Ctrl+Alt+Space",
            Self::Picker => "Ctrl+Shift+Space",
            Self::Capture => "",
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

/// The text size Snipdeck uses (Settings > Appearance), WCAG 1.4.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TextSize {
    /// Follow Windows' "Text size" (Settings > Accessibility).
    #[default]
    #[serde(rename = "system")]
    System,
    /// 100 %.
    #[serde(rename = "100")]
    P100,
    /// 125 %.
    #[serde(rename = "125")]
    P125,
    /// 150 %.
    #[serde(rename = "150")]
    P150,
    /// 175 %.
    #[serde(rename = "175")]
    P175,
}

/// The smallest and largest text scale the UI is laid out for (Windows allows 100 % to 225 %).
pub const TEXT_SCALE: std::ops::RangeInclusive<f32> = 1.0..=2.25;

impl TextSize {
    /// Every choice, in the order Settings lists them.
    pub const ALL: [Self; 5] = [Self::System, Self::P100, Self::P125, Self::P150, Self::P175];

    /// The fixed percentage, or `None` for "follow Windows".
    #[must_use]
    pub const fn percent(self) -> Option<u32> {
        match self {
            Self::System => None,
            Self::P100 => Some(100),
            Self::P125 => Some(125),
            Self::P150 => Some(150),
            Self::P175 => Some(175),
        }
    }

    /// The scale factor the UI uses: the fixed percentage, or `system` (Windows' text scale)
    /// for "System". Always inside [`TEXT_SCALE`], and 1.0 for a value that is not a number.
    #[must_use]
    pub fn scale(self, system: f32) -> f32 {
        #[allow(
            clippy::cast_precision_loss,
            reason = "a percentage up to 175 is exact in an f32"
        )]
        let wanted = self.percent().map_or(system, |p| p as f32 / 100.0);
        if wanted.is_nan() {
            1.0
        } else {
            wanted.clamp(*TEXT_SCALE.start(), *TEXT_SCALE.end())
        }
    }
}

/// What Snipdeck presses to paste into one program, or "never paste there".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProgramPaste {
    /// `Ctrl+V`: nearly every program.
    #[default]
    CtrlV,
    /// `Ctrl+Shift+V`: terminals that keep `Ctrl+V` for themselves.
    CtrlShiftV,
    /// `Shift+Insert`: terminals that do not accept `Ctrl+V`.
    ShiftInsert,
    /// Never paste: the item is copied instead.
    Never,
}

impl ProgramPaste {
    /// Every choice, in the order Settings lists them.
    pub const ALL: [Self; 4] = [
        Self::CtrlV,
        Self::CtrlShiftV,
        Self::ShiftInsert,
        Self::Never,
    ];

    /// The key to press, or `None` for [`Never`](Self::Never).
    #[must_use]
    pub const fn key(self) -> Option<PasteKey> {
        match self {
            Self::CtrlV => Some(PasteKey::CtrlV),
            Self::CtrlShiftV => Some(PasteKey::CtrlShiftV),
            Self::ShiftInsert => Some(PasteKey::ShiftInsert),
            Self::Never => None,
        }
    }

    const fn of_key(key: PasteKey) -> Self {
        match key {
            PasteKey::CtrlV => Self::CtrlV,
            PasteKey::CtrlShiftV => Self::CtrlShiftV,
            PasteKey::ShiftInsert => Self::ShiftInsert,
        }
    }
}

/// How Snipdeck pastes into one program (Settings > Programs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramRule {
    /// The executable file name, lower case (`windowsterminal.exe`).
    pub exe: String,
    /// What to press there.
    pub paste: ProgramPaste,
}

/// The rules a new settings file starts with: the programs known to need another paste key.
fn builtin_program_rules() -> Vec<ProgramRule> {
    BUILTIN_PASTE_KEYS
        .iter()
        .map(|&(exe, key)| ProgramRule {
            exe: exe.to_owned(),
            paste: ProgramPaste::of_key(key),
        })
        .collect()
}

/// Reads the `programs` list element by element: one bad entry must not drop the rest, or a
/// "never paste" rule could be lost and auto-paste would go into a program the user excluded.
fn programs_from_value(value: &serde_json::Value) -> Option<Vec<ProgramRule>> {
    let items = value.as_array()?;
    Some(
        items
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect(),
    )
}

fn lenient_programs<'de, D>(deserializer: D) -> Result<Vec<ProgramRule>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(programs_from_value(&value).unwrap_or_else(builtin_program_rules))
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

    /// The chords to register with Windows. Paste next works with Supporter, so without it its
    /// chord stays in the settings but is not registered: `Ctrl+Alt+V` is Paste Special in Word
    /// and Excel and a Free user would only get an upsell.
    #[must_use]
    pub fn registrable(&self, supporter: bool) -> Self {
        let mut chords = self.clone();
        if !supporter {
            chords.paste_next.clear();
        }
        chords
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

/// The values of the array stored under `key` in a settings file that is not valid JSON, read
/// up to the point where the text stops making sense (a file cut short keeps the values before
/// the cut).
fn salvage_array(text: &str, key: &str) -> Vec<serde_json::Value> {
    let quoted = format!("\"{key}\"");
    let Some(at) = text.find(&quoted) else {
        return Vec::new();
    };
    let rest = text[at + quoted.len()..].trim_start();
    let Some(rest) = rest.strip_prefix(':') else {
        return Vec::new();
    };
    let Some(mut rest) = rest.trim_start().strip_prefix('[') else {
        return Vec::new();
    };
    let mut values = Vec::new();
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
        values.push(value);
        rest = &rest[used..];
    }
    values
}

/// The "never paste" programs of a settings file that is not valid JSON: both the current
/// `programs` rules and the `no_paste_apps` names an older version wrote.
fn salvage_no_paste(text: &str) -> (Vec<ProgramRule>, Vec<String>) {
    let rules = programs_from_value(&serde_json::Value::Array(salvage_array(text, "programs")))
        .unwrap_or_default();
    let names = salvage_array(text, "no_paste_apps")
        .into_iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    (rules, names)
}

/// Where the Main window sat when it was last moved or resized, in physical pixels in desktop
/// coordinates (the outer frame, as the OS reports it). While maximised the rectangle is the
/// restored (normal) place and `maximized` is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowPlace {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width.
    pub w: u32,
    /// Height.
    pub h: u32,
    /// The window was maximised.
    pub maximized: bool,
}

impl WindowPlace {
    /// Smallest width or height worth restoring; anything less is a minimised or broken frame.
    pub const MIN_SIDE: u32 = 200;
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
    /// Accent colour.
    pub accent: crate::accent::AccentChoice,
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
    /// The Panel's pin is on (always on top): remembered between runs.
    pub panel_pinned: bool,
    /// "Take screenshot" leaves the Panel on screen while the snipping overlay is up. Off
    /// (the default): the Panel hides until the picture is taken, so it is not in front of
    /// the overlay.
    pub screenshot_keep_panel: bool,
    /// The one-time "Snipdeck is still running" toast has been shown (the first time the Panel
    /// hid itself by the close button or by losing focus).
    pub hide_hint_shown: bool,
    /// Panel width in logical pixels.
    pub panel_width: u32,
    /// How many daily backups to keep (1 to 60).
    pub backups_keep: u32,
    /// Animations are off even when Windows' "Animation effects" is on.
    pub reduce_motion: bool,
    /// The text size: follow Windows, or a fixed percentage (WCAG 1.4.4).
    pub text_size: TextSize,
    /// Seconds after copying a sensitive item before the clipboard is cleared (0 = never).
    pub clipboard_clear_secs: u32,
    /// Where the Panel sat on each monitor it was moved or resized on; wins over `panel_width`
    /// there. Absent in older files.
    pub panel_geometry: Vec<PanelGeometry>,
    /// Where the Main window was left; absent until it has been moved or resized.
    pub main_window: Option<WindowPlace>,
    /// How Snipdeck pastes into each program, lower-case executable name first: the paste key,
    /// or "never paste" (SPEC §5.8). A program with no rule gets `Ctrl+V`. A new file starts
    /// with the programs known to need another key.
    #[serde(deserialize_with = "lenient_programs")]
    pub programs: Vec<ProgramRule>,
    /// The programs whose rule is [`ProgramPaste::Never`], derived by [`Settings::sanitize`]
    /// and never written. Older files wrote this list: it is read once, folded into
    /// `programs` on load, and then only mirrors them for the code that routes a paste.
    #[serde(skip_serializing)]
    pub no_paste_apps: Vec<String>,
    /// The Recent inbox is on: Snipdeck keeps the text you copy (SPEC 5.6). Off by default.
    pub recent_history: bool,
    /// Executable names that are never kept in the Recent inbox (lower case).
    pub no_history_apps: Vec<String>,
    /// The edge dock: when a docked Panel collapses to a handle, and the delays.
    pub dock: DockSettings,
    /// The welcome tour has been finished or skipped. Absent (false) in older files; the tour
    /// is shown on a first run only, see [`Settings::tour_due`].
    pub onboarded: bool,
    /// The item editor's "More options" (Kind, Title, Colour, Tags, Privacy) is open.
    pub editor_more_open: bool,
    /// The Panel's rows show their 1 to 9 quick-use numbers (the keys work either way).
    pub show_numbers: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: VERSION,
            hotkeys: Hotkeys::default(),
            theme: Theme::System,
            accent: crate::accent::AccentChoice::default(),
            language: String::new(),
            autostart: false,
            close_to_tray: true,
            panel_in_taskbar: false,
            panel_hide_on_blur: true,
            panel_pinned: true,
            screenshot_keep_panel: false,
            hide_hint_shown: false,
            panel_width: PANEL_WIDTH_DEFAULT,
            backups_keep: BACKUPS_KEEP_DEFAULT,
            reduce_motion: false,
            text_size: TextSize::System,
            clipboard_clear_secs: CLIPBOARD_CLEAR_SECS_DEFAULT,
            panel_geometry: Vec::new(),
            main_window: None,
            programs: builtin_program_rules(),
            no_paste_apps: Vec::new(),
            recent_history: false,
            no_history_apps: Vec::new(),
            dock: DockSettings::default(),
            onboarded: false,
            editor_more_open: false,
            show_numbers: true,
        }
    }
}

impl Settings {
    /// Whether the welcome tour opens by itself: it has not been seen and the library is empty.
    /// A settings file from before the tour has no `onboarded` field, so a library that already
    /// holds items counts as onboarded.
    #[must_use]
    pub const fn tour_due(&self, library_is_empty: bool) -> bool {
        !self.onboarded && library_is_empty
    }

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
                // keep the rules and names that can still be read rather than pasting into
                // every app.
                let (rules, names) = salvage_no_paste(text);
                Self {
                    programs: if rules.is_empty() {
                        builtin_program_rules()
                    } else {
                        rules
                    },
                    no_paste_apps: names,
                    ..Self::default()
                }
            });
        settings.fold_legacy_no_paste();
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
        field!(accent);
        field!(language);
        field!(autostart);
        field!(close_to_tray);
        field!(panel_in_taskbar);
        field!(panel_hide_on_blur);
        field!(panel_pinned);
        field!(screenshot_keep_panel);
        field!(hide_hint_shown);
        field!(panel_width);
        field!(backups_keep);
        field!(reduce_motion);
        field!(text_size);
        field!(clipboard_clear_secs);
        field!(panel_geometry);
        field!(main_window);
        if let Some(rules) = map.get("programs").and_then(programs_from_value) {
            settings.programs = rules;
        }
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
        field!(recent_history);
        field!(no_history_apps);
        field!(dock);
        field!(onboarded);
        field!(editor_more_open);
        field!(show_numbers);
        settings
    }

    /// Turns the names an older file kept in `no_paste_apps` into "never paste" rules. Only on
    /// load: afterwards the field is a mirror of `programs`.
    fn fold_legacy_no_paste(&mut self) {
        for name in std::mem::take(&mut self.no_paste_apps) {
            let exe = name.trim().to_ascii_lowercase();
            if exe.is_empty() {
                continue;
            }
            match self
                .programs
                .iter_mut()
                .find(|r| r.exe.trim().eq_ignore_ascii_case(&exe))
            {
                Some(rule) => rule.paste = ProgramPaste::Never,
                None => self.programs.push(ProgramRule {
                    exe,
                    paste: ProgramPaste::Never,
                }),
            }
        }
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
        self.clipboard_clear_secs = self
            .clipboard_clear_secs
            .clamp(*CLIPBOARD_CLEAR_SECS.start(), *CLIPBOARD_CLEAR_SECS.end());
        self.sanitize_panel_geometry();
        self.main_window = self.main_window.filter(|p| {
            (WindowPlace::MIN_SIDE..=PANEL_SIDE_LIMIT).contains(&p.w)
                && (WindowPlace::MIN_SIDE..=PANEL_SIDE_LIMIT).contains(&p.h)
        });
        self.dock.sanitize();
        self.sanitize_programs();
        let mut seen = std::collections::BTreeSet::new();
        self.no_history_apps = self
            .no_history_apps
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

    /// One rule per program: names trimmed and lower-cased, blanks dropped, and a program named
    /// twice keeps its first place, with "never paste" winning (it is the safe choice).
    fn sanitize_programs(&mut self) {
        let mut kept: Vec<ProgramRule> = Vec::new();
        for rule in std::mem::take(&mut self.programs) {
            let exe = rule.exe.trim().to_ascii_lowercase();
            if exe.is_empty() {
                continue;
            }
            if let Some(first) = kept.iter_mut().find(|k| k.exe == exe) {
                if rule.paste == ProgramPaste::Never {
                    first.paste = ProgramPaste::Never;
                }
            } else {
                kept.push(ProgramRule {
                    exe,
                    paste: rule.paste,
                });
            }
        }
        self.no_paste_apps = kept
            .iter()
            .filter(|r| r.paste == ProgramPaste::Never)
            .map(|r| r.exe.clone())
            .collect();
        self.programs = kept;
    }

    /// The paste keys for the programs that have one (not the "never paste" ones), for the
    /// code that decides how to paste.
    #[must_use]
    pub fn paste_rules(&self) -> Vec<snipdeck_platform::keys::PasteRule> {
        self.programs
            .iter()
            .filter_map(|r| {
                r.paste.key().map(|key| snipdeck_platform::keys::PasteRule {
                    exe: r.exe.clone(),
                    key,
                })
            })
            .collect()
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

    fn rule(exe: &str, paste: ProgramPaste) -> ProgramRule {
        ProgramRule {
            exe: exe.into(),
            paste,
        }
    }

    #[test]
    fn defaults_are_valid_and_conflict_free() {
        let settings = Settings::default();
        for action in HotkeyAction::ALL {
            // Capture is off until the user picks a chord (SPEC 5.5).
            assert_eq!(
                settings.hotkeys.chord(action).is_some(),
                action != HotkeyAction::Capture,
                "{action:?}"
            );
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
            text_size: TextSize::P150,
            programs: vec![
                rule("keepass.exe", ProgramPaste::Never),
                rule("wt.exe", ProgramPaste::CtrlShiftV),
            ],
            no_paste_apps: vec!["keepass.exe".into()],
            ..Settings::default()
        };
        assert_eq!(Settings::from_json(&settings.to_json()), settings);
    }

    #[test]
    fn the_welcome_tour_flag_round_trips() {
        let settings = Settings {
            onboarded: true,
            ..Settings::default()
        };
        assert!(Settings::from_json(&settings.to_json()).onboarded);
        assert!(!Settings::from_json(&Settings::default().to_json()).onboarded);
    }

    #[test]
    fn an_old_file_without_the_flag_is_not_onboarded_and_keeps_its_fields() {
        let old = Settings::from_json(r#"{"theme":"dark","autostart":true}"#);
        assert!(!old.onboarded);
        assert_eq!(old.theme, Theme::Dark);
        // One bad field elsewhere does not lose the flag.
        let mixed = Settings::from_json(r#"{"onboarded":true,"panel_width":"wide"}"#);
        assert!(mixed.onboarded);
    }

    #[test]
    fn the_tour_opens_only_for_an_unseen_empty_library() {
        let new = Settings::default();
        assert!(new.tour_due(true));
        assert!(
            !new.tour_due(false),
            "an old library with no flag in its file"
        );
        let seen = Settings {
            onboarded: true,
            ..Settings::default()
        };
        assert!(!seen.tour_due(true));
        assert!(!seen.tour_due(false));
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
    fn the_main_window_place_round_trips_and_a_degenerate_one_is_dropped() {
        let mut settings = Settings::default();
        assert_eq!(settings.main_window, None);
        let place = WindowPlace {
            x: -1200,
            y: 30,
            w: 1100,
            h: 700,
            maximized: true,
        };
        settings.main_window = Some(place);
        let back = Settings::from_json(&settings.to_json());
        assert_eq!(back.main_window, Some(place));
        settings.main_window = Some(WindowPlace { w: 0, ..place });
        settings.sanitize();
        assert_eq!(settings.main_window, None);
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
            pre_dock: Some((320, 480)),
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
            pre_dock: None,
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
    fn the_recent_inbox_is_off_by_default_and_its_exclusions_are_normalised() {
        assert!(!Settings::default().recent_history);
        assert!(!Settings::from_json("{}").recent_history);
        let s = Settings::from_json(
            r#"{"recent_history":true,"no_history_apps":[" KeePass.EXE ","keepass.exe",""]}"#,
        );
        assert!(s.recent_history);
        assert_eq!(s.no_history_apps, ["keepass.exe"]);
        // A bad field elsewhere keeps the opt-in and the exclusions.
        let s =
            Settings::from_json(r#"{"theme":5,"recent_history":true,"no_history_apps":["a.exe"]}"#);
        assert!(s.recent_history);
        assert_eq!(s.no_history_apps, ["a.exe"]);
    }

    #[test]
    fn excluded_apps_are_normalised() {
        let settings = Settings::from_json(
            r#"{"no_paste_apps":[" KeePass.EXE ","keepass.exe","","Putty.exe"]}"#,
        );
        // putty.exe is in the built-in table, so its rule turns to Never in place; keepass.exe is added after it.
        assert_eq!(settings.no_paste_apps, ["putty.exe", "keepass.exe"]);
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

    /// Cut `full` anywhere inside the array stored under `key`: every whole element before the
    /// cut survives, a cut one does not.
    fn assert_cuts_keep_whole_elements(
        full: &str,
        key: &str,
        names: [&str; 3],
        kept: impl Fn(&Settings) -> Vec<String>,
    ) {
        let start = full.find(&format!("\"{key}\"")).unwrap();
        let end = full.rfind(names[2]).unwrap();
        for cut in start..=end + 4 {
            let settings = Settings::from_json(&full[..cut]);
            let whole = names
                .iter()
                .filter(|name| {
                    let at = full.find(&format!("\"{name}\"")).unwrap();
                    at + name.len() + 2 <= cut
                })
                .count();
            // The element may be followed by more fields (a rule is an object); it is whole
            // once its closing brace is inside the cut.
            let got = kept(&settings);
            assert!(
                got.len() >= whole.saturating_sub(1) && got.len() <= whole,
                "cut at {cut}: {got:?} with {whole} whole"
            );
            assert_eq!(got, names[..got.len()], "cut at {cut}");
        }
    }

    #[test]
    fn a_truncated_file_keeps_the_excluded_apps_it_still_has() {
        let names = ["keepass.exe", "1password.exe", "putty.exe"];
        // The current format: rules in `programs`.
        let full = Settings {
            theme: Theme::Dark,
            programs: names.iter().map(|n| rule(n, ProgramPaste::Never)).collect(),
            ..Settings::default()
        }
        .to_json();
        assert_cuts_keep_whole_elements(&full, "programs", names, |s| s.no_paste_apps.clone());
        let cut = &full[..full.find("putty.exe").unwrap() - 1];
        assert_eq!(
            Settings::from_json(cut).no_paste_apps,
            ["keepass.exe", "1password.exe"]
        );
        // Cut before the list starts: nothing to keep.
        assert_eq!(
            Settings::from_json(&full[..full.find("\"programs\"").unwrap()]).no_paste_apps,
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_truncated_old_file_keeps_the_names_of_its_no_paste_list() {
        let full = r#"{"theme":"dark","no_paste_apps":["keepass.exe","1password.exe","putty.exe"],"autostart":true}"#;
        let names = ["keepass.exe", "1password.exe", "putty.exe"];
        let start = full.find("\"no_paste_apps\"").unwrap();
        for cut in start..=full.find("putty.exe").unwrap() + 4 {
            let settings = Settings::from_json(&full[..cut]);
            let whole = names
                .iter()
                .filter(|name| full.find(&format!("\"{name}\"")).unwrap() + name.len() + 2 <= cut)
                .count();
            assert_eq!(settings.no_paste_apps, names[..whole], "cut at {cut}");
            assert_eq!(
                settings
                    .programs
                    .iter()
                    .filter(|r| r.paste == ProgramPaste::Never)
                    .count(),
                whole,
                "cut at {cut}"
            );
        }
    }

    #[test]
    fn old_files_without_the_window_fields_get_the_defaults() {
        let settings = Settings::from_json("{}");
        assert!(!settings.panel_in_taskbar);
        assert!(settings.panel_hide_on_blur);
        assert!(!settings.screenshot_keep_panel, "off unless asked for");
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
    fn reduced_motion_is_off_by_default_and_round_trips() {
        assert!(!Settings::from_json("{}").reduce_motion);
        assert!(Settings::from_json(r#"{"reduce_motion":true}"#).reduce_motion);
        let bad = Settings::from_json(r#"{"reduce_motion":"yes","theme":"dark"}"#);
        assert_eq!((bad.reduce_motion, bad.theme), (false, Theme::Dark));
        let on = Settings {
            reduce_motion: true,
            ..Settings::default()
        };
        assert_eq!(Settings::from_json(&on.to_json()), on);
    }

    #[test]
    fn the_clipboard_clear_time_defaults_to_thirty_seconds_and_zero_means_never() {
        assert_eq!(Settings::from_json("{}").clipboard_clear_secs, 30);
        assert_eq!(
            Settings::from_json(r#"{"clipboard_clear_secs":0}"#).clipboard_clear_secs,
            0
        );
        assert_eq!(
            Settings::from_json(r#"{"clipboard_clear_secs":99999}"#).clipboard_clear_secs,
            600
        );
        let bad = Settings::from_json(r#"{"clipboard_clear_secs":"soon","theme":"dark"}"#);
        assert_eq!((bad.clipboard_clear_secs, bad.theme), (30, Theme::Dark));
        let kept = Settings {
            clipboard_clear_secs: 90,
            ..Settings::default()
        };
        assert_eq!(Settings::from_json(&kept.to_json()), kept);
    }

    #[test]
    fn the_window_fields_round_trip() {
        let settings = Settings {
            panel_in_taskbar: true,
            panel_hide_on_blur: false,
            panel_pinned: false,
            screenshot_keep_panel: true,
            hide_hint_shown: true,
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

    #[test]
    fn paste_next_is_registered_only_with_supporter() {
        let chords = Settings::default().hotkeys;
        assert!(
            chords.chord(HotkeyAction::PasteNext).is_some(),
            "it stays in the settings"
        );
        let free = chords.registrable(false);
        assert!(free.chord(HotkeyAction::PasteNext).is_none());
        assert_eq!(free.toggle_panel, chords.toggle_panel);
        assert_eq!(free.picker, chords.picker);
        assert_eq!(chords.registrable(true), chords);
    }

    #[test]
    fn capture_is_off_until_the_user_picks_a_chord() {
        assert_eq!(HotkeyAction::Capture.default_chord(), "");
        assert_eq!(Settings::default().hotkeys.capture, "");
        // A chord chosen earlier stays.
        let old = Settings::from_json(r#"{"hotkeys":{"capture":"Ctrl+Shift+2"}}"#);
        assert_eq!(old.hotkeys.capture, "Ctrl+Shift+2");
        // A missing one is off, not the old default.
        assert_eq!(Settings::from_json("{}").hotkeys.capture, "");
    }

    #[test]
    fn the_text_size_defaults_to_the_system_one_and_round_trips() {
        assert_eq!(Settings::default().text_size, TextSize::System);
        assert_eq!(Settings::from_json("{}").text_size, TextSize::System);
        for size in TextSize::ALL {
            let settings = Settings {
                text_size: size,
                ..Settings::default()
            };
            assert_eq!(Settings::from_json(&settings.to_json()).text_size, size);
        }
        assert!(
            Settings::default()
                .to_json()
                .contains("\"text_size\": \"system\"")
        );
        // An unknown value keeps the other fields and gives the default.
        let odd = Settings::from_json(r#"{"text_size":"huge","theme":"dark"}"#);
        assert_eq!(odd.text_size, TextSize::System);
        assert_eq!(odd.theme, Theme::Dark);
    }

    #[test]
    fn the_text_scale_is_the_fixed_percentage_or_windows_and_always_in_range() {
        let near = |a: f32, b: f32| (a - b).abs() < 1e-6;
        assert!(near(TextSize::P100.scale(2.0), 1.0));
        assert!(near(TextSize::P125.scale(1.0), 1.25));
        assert!(near(TextSize::P150.scale(1.0), 1.5));
        assert!(near(TextSize::P175.scale(1.0), 1.75));
        assert!(near(TextSize::System.scale(1.0), 1.0));
        assert!(near(TextSize::System.scale(1.5), 1.5));
        // Windows' value is held to 1.0 ..= 2.25, whatever it says.
        assert!(near(TextSize::System.scale(0.2), 1.0));
        assert!(near(TextSize::System.scale(9.0), 2.25));
        assert!(near(TextSize::System.scale(f32::NAN), 1.0));
        assert!(near(TextSize::System.scale(f32::INFINITY), 2.25));
    }

    #[test]
    fn a_new_file_starts_with_the_terminals_that_need_another_paste_key() {
        let settings = Settings::default();
        let find = |exe: &str| {
            settings
                .programs
                .iter()
                .find(|r| r.exe == exe)
                .map(|r| r.paste)
        };
        assert_eq!(find("mintty.exe"), Some(ProgramPaste::ShiftInsert));
        assert_eq!(find("alacritty.exe"), Some(ProgramPaste::CtrlShiftV));
        assert_eq!(find("notepad.exe"), None);
        assert_eq!(settings.no_paste_apps.len(), 0);
        // Those rules are what pastes: ShiftInsert for mintty.
        let rules = settings.paste_rules();
        assert_eq!(
            snipdeck_platform::keys::paste_key_for("MinTTY.exe", &rules),
            PasteKey::ShiftInsert
        );
    }

    #[test]
    fn an_old_file_with_a_no_paste_list_loads_into_never_paste_rules() {
        let old = Settings::from_json(
            r#"{"theme":"dark","no_paste_apps":[" KeePass.EXE ","mintty.exe","keepass.exe"]}"#,
        );
        let never: Vec<&str> = old
            .programs
            .iter()
            .filter(|r| r.paste == ProgramPaste::Never)
            .map(|r| r.exe.as_str())
            .collect();
        // An excluded program that already had a key keeps its place and becomes "never".
        assert_eq!(never, ["mintty.exe", "keepass.exe"]);
        assert_eq!(old.no_paste_apps, ["mintty.exe", "keepass.exe"]);
        assert_eq!(old.theme, Theme::Dark);
        // The rest of the built-in table is still there.
        assert!(
            old.programs
                .iter()
                .any(|r| r.exe == "putty.exe" && r.paste == ProgramPaste::ShiftInsert)
        );
        // And saving writes rules only: the old key is gone for good.
        let saved = old.to_json();
        assert!(!saved.contains("no_paste_apps"));
        assert_eq!(Settings::from_json(&saved), old);
    }

    #[test]
    fn rules_are_normalised_and_a_bad_one_keeps_the_rest() {
        let settings = Settings::from_json(
            r#"{"programs":[
                {"exe":" WT.EXE ","paste":"ctrl-shift-v"},
                {"exe":"wt.exe","paste":"never"},
                {"exe":"","paste":"ctrl-v"},
                {"exe":"zed.exe","paste":"flip"},
                5,
                {"exe":"nvim.exe","paste":"shift-insert"}
            ]}"#,
        );
        // A program named twice keeps its first place; "never" wins.
        assert_eq!(
            settings.programs,
            [
                rule("wt.exe", ProgramPaste::Never),
                rule("nvim.exe", ProgramPaste::ShiftInsert)
            ]
        );
        assert_eq!(settings.no_paste_apps, ["wt.exe"]);
        // Removing every row is a choice: no built-in rules come back.
        assert_eq!(Settings::from_json(r#"{"programs":[]}"#).programs, []);
        // A list of the wrong shape gives the starting rules.
        assert_eq!(
            Settings::from_json(r#"{"programs":7}"#).programs,
            Settings::default().programs
        );
    }
}
