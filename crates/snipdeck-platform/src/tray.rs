//! The notification-area icon: what it offers and how raw gestures become app events.
//!
//! Everything decidable without Windows lives here and is tested on Linux:
//!
//! - [`Tray`]: the interface the app uses (show, hide, tooltip, the "Pause hotkeys" check).
//! - [`TrayEvent`]: what the app hears back (toggle the Panel, a menu entry, quit).
//! - [`TrayModel`]: the state behind an icon (wanted visible, tooltip, hotkeys paused) and the
//!   decisions: a left click toggles the Panel, a right click opens the menu, a picked entry
//!   becomes an event, Explorer restarting means "add the icon again".
//! - [`classify`]: which `NOTIFYICON_VERSION_4` callback codes are clicks.
//!
//! The Win32 backend (`windows::tray::WinTray`) feeds raw callbacks into a [`TrayModel`] and
//! draws the menu it returns; `testing::FakeTray` drives the same model, so a controller test
//! exercises the real decisions. Menu labels are English until the app sets translated ones with
//! [`Tray::set_labels`] (a native menu cannot use Slint's `@tr`; the words are the UI crate's `TrayTexts`).

use crate::error::Error;

/// The longest tooltip the shell shows, in UTF-16 units, including the terminating NUL.
pub const TOOLTIP_CAPACITY: usize = 128;

/// An entry of the tray menu, in menu order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrayEntry {
    /// Show the Panel.
    OpenPanel,
    /// Show the main window.
    OpenSnipdeck,
    /// Switch the global hotkeys off or on (a check mark shows when they are off).
    PauseHotkeys,
    /// Exit Snipdeck.
    Quit,
}

impl TrayEntry {
    /// Every entry, in menu order.
    pub const ALL: [Self; 4] = [
        Self::OpenPanel,
        Self::OpenSnipdeck,
        Self::PauseHotkeys,
        Self::Quit,
    ];

    /// The English menu text (what [`TrayLabels::default`] holds).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::OpenPanel => "Open Panel",
            Self::OpenSnipdeck => "Open Snipdeck",
            Self::PauseHotkeys => "Pause hotkeys",
            Self::Quit => "Quit",
        }
    }

    /// The menu command id (never 0, which `TrackPopupMenu` uses for "nothing chosen").
    #[must_use]
    pub const fn command_id(self) -> u32 {
        match self {
            Self::OpenPanel => 1,
            Self::OpenSnipdeck => 2,
            Self::PauseHotkeys => 3,
            Self::Quit => 4,
        }
    }

    /// The entry with command id `id`.
    #[must_use]
    pub fn from_command_id(id: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.command_id() == id)
    }
}

/// The menu texts, in the user's language. [`Default`] is English; the app sets the translated
/// words with [`Tray::set_labels`] (they come from the UI crate's `TrayTexts`, a native menu
/// cannot use Slint's `@tr` itself).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayLabels {
    /// [`TrayEntry::OpenPanel`].
    pub open_panel: String,
    /// [`TrayEntry::OpenSnipdeck`].
    pub open_snipdeck: String,
    /// [`TrayEntry::PauseHotkeys`].
    pub pause_hotkeys: String,
    /// [`TrayEntry::Quit`].
    pub quit: String,
    /// The hover text while the hotkeys are paused (instead of the tooltip).
    pub tooltip_paused: String,
}

impl Default for TrayLabels {
    fn default() -> Self {
        let english = |entry: TrayEntry| entry.label().to_owned();
        Self {
            open_panel: english(TrayEntry::OpenPanel),
            open_snipdeck: english(TrayEntry::OpenSnipdeck),
            pause_hotkeys: english(TrayEntry::PauseHotkeys),
            quit: english(TrayEntry::Quit),
            tooltip_paused: "Snipdeck (hotkeys paused)".to_owned(),
        }
    }
}

impl TrayLabels {
    /// The text of `entry`.
    #[must_use]
    pub fn of(&self, entry: TrayEntry) -> &str {
        match entry {
            TrayEntry::OpenPanel => &self.open_panel,
            TrayEntry::OpenSnipdeck => &self.open_snipdeck,
            TrayEntry::PauseHotkeys => &self.pause_hotkeys,
            TrayEntry::Quit => &self.quit,
        }
    }
}

/// What the tray tells the app (events arrive on the backend's thread).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    /// Left click on the icon: show the Panel if hidden, hide it if shown.
    TogglePanel,
    /// "Open Panel".
    OpenPanel,
    /// "Open Snipdeck".
    OpenSnipdeck,
    /// "Pause hotkeys" was toggled; `true` = the hotkeys should now be off.
    HotkeysPaused(bool),
    /// "Quit".
    Quit,
    /// The icon is now in the notification area (`true`) or no longer is (`false`): added, taken
    /// away by Explorer restarting, or added again after it. The app closes the Panel to the
    /// tray only while this is `true`. Sent once per change, not per attempt.
    IconPresent(bool),
}

/// One line of the popup menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuLine {
    /// A command.
    Item {
        /// Which command.
        entry: TrayEntry,
        /// Its text.
        label: String,
        /// Whether it shows a check mark.
        checked: bool,
    },
    /// A divider.
    Separator,
}

/// A raw gesture on the icon, after the callback code is decoded (see [`classify`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    /// Left click, or Enter / Space on the focused icon.
    Select,
    /// Right click, or the menu key / Shift+F10 on the focused icon.
    ContextMenu,
}

/// What a gesture asks the backend to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reaction {
    /// Tell the app.
    Event(TrayEvent),
    /// Pop this menu up at the pointer; call [`TrayModel::pick`] with the entry chosen, if any.
    PopupMenu(Vec<MenuLine>),
}

// `NOTIFYICON_VERSION_4` callback codes (the low word of `lParam`). Written out so the decoding
// is testable here; the Windows backend asserts they equal the `windows` crate's constants.
/// `NIN_SELECT`.
pub const NIN_SELECT: u32 = 0x0400;
/// `NIN_KEYSELECT`.
pub const NIN_KEYSELECT: u32 = 0x0401;
/// `WM_CONTEXTMENU`.
pub const WM_CONTEXTMENU: u32 = 0x007B;

/// Decodes the low word of a version-4 tray callback's `lParam`.
///
/// Only the select and context-menu notifications count. The raw mouse messages (`WM_LBUTTONUP`
/// and friends) are ignored on purpose: version 4 sends them *and* `NIN_SELECT` for one click,
/// and acting on both would toggle the Panel twice.
#[must_use]
pub const fn classify(code: u32) -> Option<Gesture> {
    match code {
        NIN_SELECT | NIN_KEYSELECT => Some(Gesture::Select),
        WM_CONTEXTMENU => Some(Gesture::ContextMenu),
        _ => None,
    }
}

/// Splits the `lParam` of a version-4 tray callback into (notification code, icon id): the low
/// and high words.
#[must_use]
pub fn decode_callback(lparam: isize) -> (u32, u32) {
    let bits = lparam.cast_unsigned();
    let word = |shift: u32| u32::try_from((bits >> shift) & 0xFFFF).unwrap_or(0);
    (word(0), word(16))
}

/// The screen position the shell passes in the `wParam` of a version-4 tray callback (where the
/// icon was clicked): the low and high words as signed 16-bit values, so a monitor left of or
/// above the primary one gives negative coordinates.
#[must_use]
pub fn decode_anchor(wparam: usize) -> (i32, i32) {
    let word = |shift: u32| {
        i32::from(
            u16::try_from((wparam >> shift) & 0xFFFF)
                .unwrap_or(0)
                .cast_signed(),
        )
    };
    (word(0), word(16))
}

/// The notification-area icon, as the app sees it.
///
/// Events arrive through the callback given when the backend is created (on the backend's own
/// thread for the Windows one), not through this trait.
pub trait Tray {
    /// Adds the icon. Showing an icon that is already shown succeeds.
    ///
    /// The icon is remembered as *wanted*: if the shell cannot take it yet (Explorer is not up
    /// at logon) the call fails, but the icon is added when the shell announces itself.
    ///
    /// # Errors
    ///
    /// The backend could not add the icon or is not running.
    fn show(&mut self) -> Result<(), Error>;

    /// Removes the icon. Hiding a hidden icon succeeds.
    ///
    /// # Errors
    ///
    /// The backend could not remove the icon or is not running.
    fn hide(&mut self) -> Result<(), Error>;

    /// Sets the hover text (cut to what the shell can show).
    ///
    /// # Errors
    ///
    /// The backend could not update the icon or is not running.
    fn set_tooltip(&mut self, text: &str) -> Result<(), Error>;

    /// Sets the check mark of "Pause hotkeys" (the app changed the state itself).
    ///
    /// # Errors
    ///
    /// The backend is not running.
    fn set_hotkeys_paused(&mut self, paused: bool) -> Result<(), Error>;

    /// Sets the menu texts; the next menu shown uses them.
    ///
    /// # Errors
    ///
    /// The backend is not running.
    fn set_labels(&mut self, labels: TrayLabels) -> Result<(), Error>;
}

/// The state behind a tray icon and the decisions about it. Pure; see the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayModel {
    wanted: bool,
    tooltip: String,
    hotkeys_paused: bool,
    labels: TrayLabels,
}

impl TrayModel {
    /// A hidden icon with this tooltip.
    #[must_use]
    pub fn new(tooltip: &str) -> Self {
        Self {
            wanted: false,
            tooltip: tooltip.to_owned(),
            hotkeys_paused: false,
            labels: TrayLabels::default(),
        }
    }

    /// The icon should be in the notification area.
    pub fn show(&mut self) {
        self.wanted = true;
    }

    /// The icon should not be in the notification area.
    pub fn hide(&mut self) {
        self.wanted = false;
    }

    /// Whether the icon should be in the notification area.
    #[must_use]
    pub fn is_wanted(&self) -> bool {
        self.wanted
    }

    /// Replaces the hover text.
    pub fn set_tooltip(&mut self, text: &str) {
        text.clone_into(&mut self.tooltip);
    }

    /// The hover text as given, or the paused text while the hotkeys are paused.
    #[must_use]
    pub fn tooltip(&self) -> &str {
        if self.hotkeys_paused {
            &self.labels.tooltip_paused
        } else {
            &self.tooltip
        }
    }

    /// The hover text as UTF-16 for the shell: at most [`TOOLTIP_CAPACITY`] - 1 units (no
    /// surrogate pair is split), without the NUL.
    #[must_use]
    pub fn tooltip_utf16(&self) -> Vec<u16> {
        let mut units = Vec::new();
        for c in self.tooltip().chars() {
            if units.len() + c.len_utf16() >= TOOLTIP_CAPACITY {
                break;
            }
            let mut buf = [0u16; 2];
            units.extend_from_slice(c.encode_utf16(&mut buf));
        }
        units
    }

    /// Whether "Pause hotkeys" is checked.
    #[must_use]
    pub fn hotkeys_paused(&self) -> bool {
        self.hotkeys_paused
    }

    /// The app changed the paused state itself.
    pub fn set_hotkeys_paused(&mut self, paused: bool) {
        self.hotkeys_paused = paused;
    }

    /// Replaces the menu texts.
    pub fn set_labels(&mut self, labels: TrayLabels) {
        self.labels = labels;
    }

    /// The popup menu as it is now.
    #[must_use]
    pub fn menu(&self) -> Vec<MenuLine> {
        let mut lines = Vec::new();
        for entry in TrayEntry::ALL {
            if entry == TrayEntry::Quit {
                lines.push(MenuLine::Separator);
            }
            lines.push(MenuLine::Item {
                entry,
                label: self.labels.of(entry).to_owned(),
                checked: entry == TrayEntry::PauseHotkeys && self.hotkeys_paused,
            });
        }
        lines
    }

    /// A gesture on the icon. Gestures while the icon is not wanted are stale messages and give
    /// `None`.
    pub fn gesture(&mut self, gesture: Gesture) -> Option<Reaction> {
        if !self.wanted {
            return None;
        }
        Some(match gesture {
            Gesture::Select => Reaction::Event(TrayEvent::TogglePanel),
            Gesture::ContextMenu => Reaction::PopupMenu(self.menu()),
        })
    }

    /// The user chose `entry` in the menu.
    pub fn pick(&mut self, entry: TrayEntry) -> TrayEvent {
        match entry {
            TrayEntry::OpenPanel => TrayEvent::OpenPanel,
            TrayEntry::OpenSnipdeck => TrayEvent::OpenSnipdeck,
            TrayEntry::PauseHotkeys => {
                self.hotkeys_paused = !self.hotkeys_paused;
                TrayEvent::HotkeysPaused(self.hotkeys_paused)
            }
            TrayEntry::Quit => TrayEvent::Quit,
        }
    }

    /// Explorer (re)started and announced `TaskbarCreated`: the icon must be added again if it
    /// is wanted. Every icon was dropped with the old taskbar.
    #[must_use]
    pub fn on_taskbar_created(&self) -> bool {
        self.wanted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> TrayModel {
        let mut model = TrayModel::new("Snipdeck");
        model.show();
        model
    }

    #[test]
    fn only_select_and_context_menu_codes_are_gestures() {
        assert_eq!(classify(NIN_SELECT), Some(Gesture::Select));
        assert_eq!(classify(NIN_KEYSELECT), Some(Gesture::Select));
        assert_eq!(classify(WM_CONTEXTMENU), Some(Gesture::ContextMenu));
        // WM_LBUTTONUP, WM_RBUTTONUP, WM_MOUSEMOVE, NIN_POPUPOPEN: not acted on.
        for code in [0x0202, 0x0205, 0x0200, 0x0406, 0] {
            assert_eq!(classify(code), None, "{code:#x}");
        }
    }

    #[test]
    fn the_callback_words_are_split_and_the_anchor_is_signed() {
        assert_eq!(decode_callback(0x0001_0400), (NIN_SELECT, 1));
        assert_eq!(decode_callback(0x007B), (WM_CONTEXTMENU, 0));
        assert_eq!(decode_anchor(0x0123_0456), (0x0456, 0x0123));
        // A monitor to the left of and above the primary: -1, -2.
        assert_eq!(decode_anchor(0xFFFE_FFFF), (-1, -2));
    }

    #[test]
    fn left_click_toggles_the_panel() {
        let mut m = model();
        assert_eq!(
            m.gesture(Gesture::Select),
            Some(Reaction::Event(TrayEvent::TogglePanel))
        );
    }

    #[test]
    fn right_click_offers_the_menu_with_quit_after_a_separator() {
        let mut m = model();
        let Some(Reaction::PopupMenu(lines)) = m.gesture(Gesture::ContextMenu) else {
            panic!("a menu")
        };
        let labels: Vec<_> = lines
            .iter()
            .map(|l| match l {
                MenuLine::Item { label, .. } => label.as_str(),
                MenuLine::Separator => "-",
            })
            .collect();
        assert_eq!(
            labels,
            ["Open Panel", "Open Snipdeck", "Pause hotkeys", "-", "Quit"]
        );
        assert!(
            lines
                .iter()
                .all(|l| !matches!(l, MenuLine::Item { checked: true, .. }))
        );
    }

    #[test]
    fn the_tooltip_says_the_hotkeys_are_paused_while_they_are() {
        let mut m = model();
        let usual = m.tooltip().to_owned();
        m.set_hotkeys_paused(true);
        assert_eq!(m.tooltip(), "Snipdeck (hotkeys paused)");
        assert_eq!(
            String::from_utf16(&m.tooltip_utf16()).ok().as_deref(),
            Some("Snipdeck (hotkeys paused)")
        );
        m.set_hotkeys_paused(false);
        assert_eq!(m.tooltip(), usual);
    }

    #[test]
    fn translated_labels_replace_the_english_ones_in_the_menu() {
        let mut m = model();
        m.set_labels(TrayLabels {
            open_panel: "Panel öffnen".into(),
            open_snipdeck: "Snipdeck öffnen".into(),
            pause_hotkeys: "Tastenkürzel pausieren".into(),
            quit: "Beenden".into(),
            tooltip_paused: "Snipdeck (Tastenkürzel pausiert)".into(),
        });
        let Some(Reaction::PopupMenu(lines)) = m.gesture(Gesture::ContextMenu) else {
            panic!("a menu")
        };
        let labels: Vec<_> = lines
            .iter()
            .filter_map(|l| match l {
                MenuLine::Item { label, .. } => Some(label.as_str()),
                MenuLine::Separator => None,
            })
            .collect();
        assert_eq!(
            labels,
            [
                "Panel öffnen",
                "Snipdeck öffnen",
                "Tastenkürzel pausieren",
                "Beenden"
            ]
        );
    }

    #[test]
    fn the_default_labels_are_the_english_entry_names() {
        let labels = TrayLabels::default();
        for entry in TrayEntry::ALL {
            assert_eq!(labels.of(entry), entry.label());
        }
    }

    #[test]
    fn pause_hotkeys_toggles_and_shows_a_check() {
        let mut m = model();
        assert_eq!(
            m.pick(TrayEntry::PauseHotkeys),
            TrayEvent::HotkeysPaused(true)
        );
        let checked = m.menu().into_iter().any(|l| {
            matches!(
                l,
                MenuLine::Item {
                    entry: TrayEntry::PauseHotkeys,
                    checked: true,
                    ..
                }
            )
        });
        assert!(checked);
        assert_eq!(
            m.pick(TrayEntry::PauseHotkeys),
            TrayEvent::HotkeysPaused(false)
        );
        m.set_hotkeys_paused(true);
        assert!(m.hotkeys_paused());
    }

    #[test]
    fn the_other_entries_map_to_their_events() {
        let mut m = model();
        assert_eq!(m.pick(TrayEntry::OpenPanel), TrayEvent::OpenPanel);
        assert_eq!(m.pick(TrayEntry::OpenSnipdeck), TrayEvent::OpenSnipdeck);
        assert_eq!(m.pick(TrayEntry::Quit), TrayEvent::Quit);
    }

    #[test]
    fn command_ids_round_trip_and_are_never_zero() {
        for entry in TrayEntry::ALL {
            assert_ne!(entry.command_id(), 0);
            assert_eq!(TrayEntry::from_command_id(entry.command_id()), Some(entry));
        }
        assert_eq!(TrayEntry::from_command_id(0), None);
        assert_eq!(TrayEntry::from_command_id(99), None);
    }

    #[test]
    fn a_hidden_icon_ignores_stale_gestures() {
        let mut m = TrayModel::new("Snipdeck");
        assert_eq!(m.gesture(Gesture::Select), None);
        m.show();
        m.hide();
        assert_eq!(m.gesture(Gesture::ContextMenu), None);
    }

    #[test]
    fn explorer_restarting_re_adds_a_wanted_icon_only() {
        let mut m = TrayModel::new("Snipdeck");
        assert!(!m.on_taskbar_created());
        m.show();
        assert!(m.on_taskbar_created());
        m.hide();
        assert!(!m.on_taskbar_created());
    }

    #[test]
    fn the_tooltip_is_cut_to_what_the_shell_shows_without_splitting_a_pair() {
        let mut m = model();
        assert_eq!(
            String::from_utf16(&m.tooltip_utf16()).ok().as_deref(),
            Some("Snipdeck")
        );
        m.set_tooltip(&"x".repeat(300));
        assert_eq!(m.tooltip_utf16().len(), TOOLTIP_CAPACITY - 1);
        // 126 units of ASCII, then a pair (2 units) would reach 128: it must be dropped whole.
        m.set_tooltip(&format!("{}\u{1F4CB}", "y".repeat(126)));
        let units = m.tooltip_utf16();
        assert_eq!(units.len(), 126);
        assert!(String::from_utf16(&units).is_ok());
        assert_eq!(m.tooltip().chars().count(), 127, "the text itself is kept");
    }
}
