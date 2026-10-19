//! The Recent inbox policy (SPEC §5.6): which clipboard changes are worth keeping, and
//! promoting an entry into a list.
//!
//! Slint-free and clock-free: the caller passes `now`. The clipboard listener (Windows) reads
//! the clipboard and builds a [`ClipboardObservation`]; [`RecentCapture::consider`] decides.
//! Nothing here reads the clipboard itself.

use std::time::Duration;

use snipdeck_core::Timestamp;
use snipdeck_store::{RecentEntry, RecentRepo};

use crate::capture::{self, Captured, Destination};
use crate::controller::Library;

/// The longest text kept, in bytes. Longer copies are skipped, not cut (v1 keeps text only;
/// images and files are not captured).
pub const MAX_ENTRY_BYTES: usize = 64 * 1024;
/// Default number of entries kept.
pub const DEFAULT_MAX_ENTRIES: usize = 200;
/// Default retention in days.
pub const DEFAULT_MAX_AGE_DAYS: u64 = 7;

/// The settings the policy reads. `Default` is the shipped default: off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentSettings {
    /// The user opted in.
    pub enabled: bool,
    /// Paused from the tray ("Pause history").
    pub paused: bool,
    /// Keep at most this many entries.
    pub max_entries: usize,
    /// Keep entries at most this long.
    pub max_age: Duration,
    /// Executable names (lower case) whose copies are never kept.
    pub excluded_apps: Vec<String>,
}

impl Default for RecentSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            paused: false,
            max_entries: DEFAULT_MAX_ENTRIES,
            max_age: Duration::from_hours(24 * DEFAULT_MAX_AGE_DAYS),
            excluded_apps: Vec::new(),
        }
    }
}

impl RecentSettings {
    /// The policy's view of the app settings; `paused` is the tray's session-only switch.
    #[must_use]
    pub fn from_settings(settings: &crate::settings::Settings, paused: bool) -> Self {
        Self {
            enabled: settings.recent_history,
            paused,
            excluded_apps: settings.no_history_apps.clone(),
            ..Self::default()
        }
    }

    /// Whether the inbox is collecting right now.
    #[must_use]
    pub const fn collecting(&self) -> bool {
        self.enabled && !self.paused
    }
}

/// One clipboard change, as the listener saw it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClipboardObservation {
    /// The text on the clipboard, if it has any.
    pub text: Option<String>,
    /// The executable that owns the clipboard, lower case, when known.
    pub source_app: Option<String>,
    /// The clipboard carries `ExcludeClipboardContentFromMonitorProcessing`, or
    /// `CanIncludeInClipboardHistory` = 0 (password managers set these).
    pub excluded_format: bool,
    /// Snipdeck wrote it (a paste or copy from the deck).
    pub own_copy: bool,
    /// The text came from a Sensitive item.
    pub sensitive: bool,
}

impl From<snipdeck_platform::clipboard_watch::ClipboardChange> for ClipboardObservation {
    fn from(change: snipdeck_platform::clipboard_watch::ClipboardChange) -> Self {
        Self {
            text: change.text,
            source_app: change.source_app,
            excluded_format: change.excluded_format,
            own_copy: change.own,
            sensitive: false,
        }
    }
}

/// Why a change was not kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skipped {
    /// The inbox is off.
    Disabled,
    /// Paused from the tray.
    Paused,
    /// Snipdeck's own copy.
    OwnCopy,
    /// A Sensitive item's text.
    Sensitive,
    /// The source asked not to be recorded.
    ExcludedFormat,
    /// The source app is on the exclusion list.
    ExcludedApp,
    /// No text, or only whitespace.
    Empty,
    /// Over [`MAX_ENTRY_BYTES`].
    TooLarge,
}

/// What [`RecentCapture::consider`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Considered {
    /// Kept (a re-copy of an earlier text moves it to the top).
    Kept {
        /// The entry's id.
        id: i64,
        /// Entries removed by retention afterwards.
        pruned: usize,
    },
    /// Not kept, and why.
    Skipped(Skipped),
}

/// The policy entry points.
#[derive(Debug, Clone, Copy)]
pub struct RecentCapture;

impl RecentCapture {
    /// Applies the policy to one clipboard change and stores it when it passes. Checks run in
    /// a fixed order so the reason is stable: off, paused, own copy, sensitive, excluded
    /// format, excluded app, empty, too large.
    ///
    /// # Errors
    ///
    /// Any store error.
    pub fn consider<S: RecentRepo>(
        store: &mut S,
        obs: &ClipboardObservation,
        settings: &RecentSettings,
        now: Timestamp,
    ) -> snipdeck_store::Result<Considered> {
        let skip = |why| Ok(Considered::Skipped(why));
        if !settings.enabled {
            return skip(Skipped::Disabled);
        }
        if settings.paused {
            return skip(Skipped::Paused);
        }
        if obs.own_copy {
            return skip(Skipped::OwnCopy);
        }
        if obs.sensitive {
            return skip(Skipped::Sensitive);
        }
        if obs.excluded_format {
            return skip(Skipped::ExcludedFormat);
        }
        let app = obs.source_app.as_deref().map(str::to_ascii_lowercase);
        if app
            .as_ref()
            .is_some_and(|a| settings.excluded_apps.iter().any(|x| x == a))
        {
            return skip(Skipped::ExcludedApp);
        }
        let Some(text) = obs.text.as_deref().filter(|t| !t.trim().is_empty()) else {
            return skip(Skipped::Empty);
        };
        if text.len() > MAX_ENTRY_BYTES {
            return skip(Skipped::TooLarge);
        }
        let id = store.add_recent(text, app.as_deref(), now)?;
        let pruned = store.prune_recent(now, settings.max_entries, settings.max_age)?;
        Ok(Considered::Kept { id, pruned })
    }

    /// Applies retention (start-up, or when the settings shrink it).
    ///
    /// # Errors
    ///
    /// Any store error.
    pub fn prune<S: RecentRepo>(
        store: &mut S,
        settings: &RecentSettings,
        now: Timestamp,
    ) -> snipdeck_store::Result<usize> {
        store.prune_recent(now, settings.max_entries, settings.max_age)
    }
}

/// One row of the Recent list in the Panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentRowView {
    /// The entry.
    pub id: i64,
    /// The start of the text, as the card shows it.
    pub preview: String,
    /// A readable name of the app it was copied from; empty when unknown.
    pub app: String,
    /// Whole minutes since the copy (the window words them: "3 min ago").
    pub age_minutes: u64,
}

/// A list a Recent entry can be saved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveTarget {
    /// The list.
    pub id: snipdeck_core::ListId,
    /// What the chooser shows: the list's name, with its board's when there are several boards.
    pub label: String,
    /// The list's colour, if it has one.
    pub accent: Option<snipdeck_core::ListAccent>,
}

/// What the Panel shows for the Recent inbox (only while it is on).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentView {
    /// The clock tab is selected and the list shows the entries.
    pub open: bool,
    /// Collecting is paused from the tray.
    pub paused: bool,
    /// The entries, newest first.
    pub rows: Vec<RecentRowView>,
    /// The lists "Save to…" offers.
    pub targets: Vec<SaveTarget>,
}

/// The row for `entry` as of `now`.
#[must_use]
pub fn row_view(entry: &RecentEntry, now: Timestamp) -> RecentRowView {
    let seconds = now.as_second().saturating_sub(entry.copied_at.as_second());
    RecentRowView {
        id: entry.id,
        preview: snipdeck_core::Item::preview_of(&entry.text, false),
        app: entry
            .source_app
            .as_deref()
            .map(crate::panel::friendly_app_name)
            .unwrap_or_default(),
        age_minutes: u64::try_from(seconds / 60).unwrap_or(0),
    }
}

/// Saves `entry` as a new item at the top of `list`. The entry stays in the inbox (a promoted
/// copy can be deleted separately). There is no Free item cap to check: Free limits boards,
/// not items.
///
/// # Errors
///
/// Any store error.
pub fn promote<L: Library>(
    library: &mut L,
    entry: &RecentEntry,
    list: &Destination,
    now: Timestamp,
) -> snipdeck_store::Result<Captured> {
    capture::capture_text(library, list, Some(&entry.text), now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use snipdeck_core::{Board, List, Name, Placement};
    use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, Store};

    fn now() -> Timestamp {
        "2026-10-18T09:00:00Z".parse().expect("timestamp")
    }

    fn on() -> RecentSettings {
        RecentSettings {
            enabled: true,
            excluded_apps: vec!["keepass.exe".to_owned()],
            ..RecentSettings::default()
        }
    }

    fn obs(text: &str) -> ClipboardObservation {
        ClipboardObservation {
            text: Some(text.to_owned()),
            source_app: Some("notepad.exe".to_owned()),
            ..ClipboardObservation::default()
        }
    }

    #[test]
    fn it_is_off_by_default() {
        assert!(!RecentSettings::default().collecting());
        assert_eq!(RecentSettings::default().max_entries, 200);
        assert_eq!(
            RecentSettings::default().max_age,
            Duration::from_hours(7 * 24)
        );
    }

    #[test]
    fn the_policy_table() {
        let big = "x".repeat(MAX_ENTRY_BYTES + 1);
        let edge = "x".repeat(MAX_ENTRY_BYTES);
        let cases: Vec<(&str, RecentSettings, ClipboardObservation, Option<Skipped>)> = vec![
            (
                "off",
                RecentSettings::default(),
                obs("a"),
                Some(Skipped::Disabled),
            ),
            (
                "paused",
                RecentSettings {
                    paused: true,
                    ..on()
                },
                obs("a"),
                Some(Skipped::Paused),
            ),
            (
                "own copy",
                on(),
                ClipboardObservation {
                    own_copy: true,
                    ..obs("a")
                },
                Some(Skipped::OwnCopy),
            ),
            (
                "sensitive",
                on(),
                ClipboardObservation {
                    sensitive: true,
                    ..obs("a")
                },
                Some(Skipped::Sensitive),
            ),
            (
                "exclusion format",
                on(),
                ClipboardObservation {
                    excluded_format: true,
                    ..obs("a")
                },
                Some(Skipped::ExcludedFormat),
            ),
            (
                "excluded app, any case",
                on(),
                ClipboardObservation {
                    source_app: Some("KeePass.EXE".to_owned()),
                    ..obs("a")
                },
                Some(Skipped::ExcludedApp),
            ),
            (
                "no text",
                on(),
                ClipboardObservation {
                    text: None,
                    ..obs("a")
                },
                Some(Skipped::Empty),
            ),
            ("blank text", on(), obs(" \n\t"), Some(Skipped::Empty)),
            ("over 64 KiB", on(), obs(&big), Some(Skipped::TooLarge)),
            ("exactly 64 KiB", on(), obs(&edge), None),
            ("plain text", on(), obs("hello"), None),
            (
                "unknown source app",
                on(),
                ClipboardObservation {
                    source_app: None,
                    ..obs("a")
                },
                None,
            ),
        ];
        for (name, settings, observation, expect) in cases {
            let mut store = Store::open_in_memory().expect("store");
            let got = RecentCapture::consider(&mut store, &observation, &settings, now())
                .expect("consider");
            if let Some(why) = expect {
                assert_eq!(got, Considered::Skipped(why), "{name}");
                assert_eq!(store.recent().expect("recent").len(), 0, "{name}");
            } else {
                assert!(matches!(got, Considered::Kept { .. }), "{name}: {got:?}");
                assert_eq!(store.recent().expect("recent").len(), 1, "{name}");
            }
        }
    }

    #[test]
    fn a_kept_copy_stores_the_source_in_lower_case_and_recopying_bumps_it() {
        let mut store = Store::open_in_memory().expect("store");
        let s = on();
        let later: Timestamp = "2026-10-18T09:05:00Z".parse().expect("timestamp");
        let mut first = obs("one");
        first.source_app = Some("Code.EXE".to_owned());
        RecentCapture::consider(&mut store, &first, &s, now()).expect("a");
        RecentCapture::consider(&mut store, &obs("two"), &s, now()).expect("b");
        RecentCapture::consider(&mut store, &first, &s, later).expect("c");
        let all = store.recent().expect("recent");
        assert_eq!(
            all.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(),
            ["one", "two"]
        );
        assert_eq!(all[0].source_app.as_deref(), Some("code.exe"));
        assert_eq!(all[0].copied_at, later);
    }

    #[test]
    fn keeping_a_copy_applies_retention() {
        let mut store = Store::open_in_memory().expect("store");
        let s = RecentSettings {
            max_entries: 2,
            ..on()
        };
        for (i, t) in ["a", "b", "c"].iter().enumerate() {
            let at = now() + Duration::from_secs(i as u64);
            let got = RecentCapture::consider(&mut store, &obs(t), &s, at).expect("consider");
            let pruned = usize::from(i == 2);
            assert!(
                matches!(got, Considered::Kept { pruned: p, .. } if p == pruned),
                "{i}"
            );
        }
        let texts: Vec<_> = store
            .recent()
            .expect("r")
            .into_iter()
            .map(|e| e.text)
            .collect();
        assert_eq!(texts, ["c", "b"]);
    }

    #[test]
    fn a_row_shows_the_start_of_the_text_the_app_and_whole_minutes() {
        let mut store = Store::open_in_memory().expect("store");
        let mut copy = obs("   padded start\nsecond line");
        copy.source_app = Some("notepad.exe".to_owned());
        RecentCapture::consider(&mut store, &copy, &on(), now()).expect("consider");
        let entry = store.recent().expect("recent").remove(0);
        let later = now() + Duration::from_secs(5 * 60 + 59);
        let row = row_view(&entry, later);
        assert_eq!(row.preview, "padded start\nsecond line");
        assert_eq!(row.app, "Notepad");
        assert_eq!(row.age_minutes, 5);
        // A clock that went backwards never shows a negative age.
        let earlier = now() - Duration::from_secs(60);
        assert_eq!(row_view(&entry, earlier).age_minutes, 0);
    }

    #[test]
    fn promoting_creates_an_item_and_leaves_the_entry() {
        let mut store = Store::open_in_memory().expect("store");
        let board = Board::new(Name::new("B").expect("name"), now());
        store.insert_board(&board, Placement::Last).expect("board");
        let list = List::new(board.id, Name::new("Notes").expect("name"), now());
        store.insert_list(&list, Placement::Last).expect("list");
        RecentCapture::consider(&mut store, &obs("keep me"), &on(), now()).expect("consider");
        let entry = store.recent().expect("recent").remove(0);
        let to = Destination {
            id: list.id,
            name: "Notes".to_owned(),
        };
        let done = promote(&mut store, &entry, &to, now()).expect("promote");
        assert!(matches!(done, Captured::Saved { .. }), "{done:?}");
        assert_eq!(store.items(list.id).expect("items").len(), 1);
        assert_eq!(store.recent().expect("recent").len(), 1);
    }
}
