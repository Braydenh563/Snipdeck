//! The Recent inbox policy (SPEC §5.6): which clipboard changes are worth keeping, and
//! promoting an entry into a list.
//!
//! Slint-free and clock-free: the caller passes `now`. The clipboard listener (Windows) reads
//! the clipboard and builds a [`ClipboardObservation`]; [`RecentCapture::consider`] decides.
//! Nothing here reads the clipboard itself.

use std::time::Duration;

use snipdeck_core::Timestamp;
use snipdeck_store::{
    BLOB_BUDGET, RecentContent, RecentEntry, RecentImage, RecentKind, RecentRepo, RecentThumb,
};

use crate::capture::{self, Captured, Destination};
use crate::controller::Library;

/// The longest text kept, in bytes. Longer copies are skipped, not cut.
pub const MAX_ENTRY_BYTES: usize = 64 * 1024;
/// The HTML and RTF flavours of a text copy are kept together up to this many bytes; over it
/// the entry keeps the plain text only.
pub const MAX_RICH_BYTES: usize = 1024 * 1024;
/// The biggest picture kept, in pixels (the preview limit).
pub const MAX_IMAGE_PIXELS: u64 = 16 * 1024 * 1024;
/// The biggest picture kept, as PNG bytes.
pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
/// The most files one entry names.
pub const MAX_FILES: usize = 100;
/// The longest side of a list thumbnail, in pixels.
pub const THUMB_SIDE: u32 = 96;
/// Default number of entries kept.
pub const DEFAULT_MAX_ENTRIES: usize = 200;
/// Default retention in days.
pub const DEFAULT_MAX_AGE_DAYS: u64 = 7;

/// How long entries are kept, in days, as the windows' texts take it.
#[must_use]
pub fn max_age_days() -> i32 {
    i32::try_from(DEFAULT_MAX_AGE_DAYS).unwrap_or(i32::MAX)
}

/// How many entries are kept, as the windows' texts take it.
#[must_use]
pub fn max_entries_count() -> i32 {
    i32::try_from(DEFAULT_MAX_ENTRIES).unwrap_or(i32::MAX)
}

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
    /// Keep the pictures' bytes within this total; the oldest go first.
    pub max_blob_bytes: u64,
}

impl Default for RecentSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            paused: false,
            max_entries: DEFAULT_MAX_ENTRIES,
            max_age: Duration::from_hours(24 * DEFAULT_MAX_AGE_DAYS),
            excluded_apps: Vec::new(),
            max_blob_bytes: BLOB_BUDGET,
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
    /// The HTML flavour next to the text.
    pub html: Option<String>,
    /// The Rich Text Format flavour next to the text.
    pub rtf: Option<Vec<u8>>,
    /// The copied files' paths (only read when there is no text).
    pub files: Vec<String>,
    /// A copied picture as PNG (only read when there is neither text nor files).
    pub image: Option<Vec<u8>>,
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
            html: change.html,
            rtf: change.rtf,
            files: change.files,
            image: change.image,
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
    /// No text, files or picture, or only whitespace.
    Empty,
    /// Over [`MAX_ENTRY_BYTES`], a picture over [`MAX_IMAGE_PIXELS`] or [`MAX_IMAGE_BYTES`], or
    /// more than [`MAX_FILES`] files.
    TooLarge,
    /// A picture or files, and no key to seal them with.
    NoKeyForRich,
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
        let id = if let Some(text) = obs.text.as_deref().filter(|t| !t.trim().is_empty()) {
            if text.len() > MAX_ENTRY_BYTES {
                return skip(Skipped::TooLarge);
            }
            let rich =
                obs.html.as_ref().map_or(0, String::len) + obs.rtf.as_ref().map_or(0, Vec::len);
            let keep = rich <= MAX_RICH_BYTES;
            let content = RecentContent::Text {
                text,
                html: obs.html.as_deref().filter(|_| keep),
                rtf: obs.rtf.as_deref().filter(|_| keep),
            };
            store.add_recent(content, app.as_deref(), now)?
        } else if !obs.files.is_empty() {
            if obs.files.len() > MAX_FILES {
                return skip(Skipped::TooLarge);
            }
            if !store.recent_seals_rich() {
                return skip(Skipped::NoKeyForRich);
            }
            let label = files_label(&obs.files);
            let content = RecentContent::Files {
                paths: &obs.files,
                label: &label,
            };
            store.add_recent(content, app.as_deref(), now)?
        } else if let Some(png) = &obs.image {
            let Some((width, height)) = crate::item_editor::png_size(png) else {
                return skip(Skipped::Empty);
            };
            if png.len() > MAX_IMAGE_BYTES
                || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS
            {
                return skip(Skipped::TooLarge);
            }
            if !store.recent_seals_rich() {
                return skip(Skipped::NoKeyForRich);
            }
            let Some(preview) = crate::item_editor::decode_preview(png) else {
                return skip(Skipped::Empty);
            };
            let thumb = thumbnail(preview.width, preview.height, &preview.rgba);
            let label = format!("Image {width} \u{d7} {height}");
            let content = RecentContent::Image(RecentImage {
                png,
                width,
                height,
                pixel_digest: *blake3::hash(&preview.rgba).as_bytes(),
                thumb: thumb.as_ref(),
                label: &label,
            });
            store.add_recent(content, app.as_deref(), now)?
        } else {
            return skip(Skipped::Empty);
        };
        let pruned = store.prune_recent(now, settings.max_entries, settings.max_age)?
            + store.prune_recent_blobs(settings.max_blob_bytes)?;
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
        Ok(
            store.prune_recent(now, settings.max_entries, settings.max_age)?
                + store.prune_recent_blobs(settings.max_blob_bytes)?,
        )
    }
}

/// The files at `paths` as `(name, bytes)`, read now (Recent keeps only the paths). A file that
/// is gone, a folder, empty, unreadable or over the attachment cap is left out.
#[must_use]
pub fn read_files(paths: &[String]) -> Vec<(String, Vec<u8>)> {
    paths
        .iter()
        .filter_map(|path| {
            crate::item_editor::read_source(snipdeck_core::ItemKind::File, path)
                .ok()
                .map(|source| (source.name, source.bytes))
        })
        .collect()
}

/// The last path component of a Windows path (either separator).
fn file_name_of(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

/// The searchable label of a files entry: "3 files: a.pdf, b.txt, c.txt".
fn files_label(paths: &[String]) -> String {
    let names: Vec<&str> = paths.iter().take(3).map(|p| file_name_of(p)).collect();
    let more = if paths.len() > 3 { ", \u{2026}" } else { "" };
    let noun = if paths.len() == 1 { "file" } else { "files" };
    format!("{} {noun}: {}{more}", paths.len(), names.join(", "))
}

/// `rgba` (`width` x `height`) shrunk to fit [`THUMB_SIDE`], averaging the source pixels that
/// fall in each new pixel. `None` for an empty picture or a buffer of the wrong size.
fn thumbnail(width: u32, height: u32, rgba: &[u8]) -> Option<RecentThumb> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || rgba.len() != w * h * 4 {
        return None;
    }
    let side = THUMB_SIDE as usize;
    let (tw, th) = if w.max(h) <= side {
        (w, h)
    } else if w >= h {
        (side, (h * side / w).max(1))
    } else {
        ((w * side / h).max(1), side)
    };
    let mut out = Vec::with_capacity(tw * th * 4);
    for ty in 0..th {
        let (y0, y1) = (ty * h / th, ((ty + 1) * h / th).max(ty * h / th + 1).min(h));
        for tx in 0..tw {
            let (x0, x1) = (tx * w / tw, ((tx + 1) * w / tw).max(tx * w / tw + 1).min(w));
            let mut sum = [0_u64; 4];
            for y in y0..y1 {
                for x in x0..x1 {
                    let px = &rgba[(y * w + x) * 4..][..4];
                    // Premultiplied, so a transparent pixel adds no colour.
                    let a = u64::from(px[3]);
                    for c in 0..3 {
                        sum[c] += u64::from(px[c]) * a;
                    }
                    sum[3] += a;
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u64;
            if let Some(red) = sum[0].checked_div(sum[3]) {
                for v in [red, sum[1] / sum[3], sum[2] / sum[3], sum[3] / n] {
                    out.push(u8::try_from(v).unwrap_or(255));
                }
            } else {
                out.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    Some(RecentThumb {
        width: u32::try_from(tw).ok()?,
        height: u32::try_from(th).ok()?,
        rgba: out,
    })
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
    /// Text, a picture or files: which card the row shows.
    pub kind: RecentKind,
    /// A picture's thumbnail.
    pub thumb: Option<RecentThumb>,
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
    /// The library is locked behind a master password: no rows until it is unlocked.
    pub locked: bool,
    /// The entries, newest first.
    pub rows: Vec<RecentRowView>,
    /// The lists "Save to…" offers.
    pub targets: Vec<SaveTarget>,
}

/// The row for `entry` as of `now`.
#[must_use]
pub fn row_view(entry: &RecentEntry, now: Timestamp) -> RecentRowView {
    let seconds = now.as_second().saturating_sub(entry.copied_at.as_second());
    let preview = match entry.kind {
        RecentKind::Text => snipdeck_core::Item::preview_of(&entry.text, false),
        RecentKind::Image => entry
            .image_size
            .map_or_else(String::new, |(w, h)| format!("{w} \u{d7} {h}")),
        RecentKind::Files => entry
            .files
            .iter()
            .map(|p| file_name_of(p))
            .collect::<Vec<_>>()
            .join(", "),
    };
    RecentRowView {
        id: entry.id,
        kind: entry.kind,
        thumb: entry.thumb.clone(),
        preview,
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
        let earlier = now() - Duration::from_mins(1);
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

    // ---- Rich captures --------------------------------------------------------------------

    use snipdeck_store::RecentPayload;
    use snipdeck_store::crypto::{KdfParams, KeyRing, MemoryKeyProvider};

    /// A store with a key ring, so images and files can be sealed.
    fn keyed() -> Store {
        let provider = MemoryKeyProvider::new().expect("provider");
        let mut store = Store::open_in_memory().expect("store");
        let ring = KeyRing::new(&provider)
            .with_kdf_params(KdfParams::insecure_for_tests(64, 1, 1).expect("cheap params"));
        store.create_key_ring(&ring, None).expect("ring");
        store
    }

    /// An RGBA picture of pseudo-random pixels (so it does not compress away), as PNG.
    fn picture(width: u32, height: u32, seed: u32, compression: png::Compression) -> Vec<u8> {
        let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
        let rgba: Vec<u8> = (0..width * height * 4)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                state.to_be_bytes()[0]
            })
            .collect();
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(compression);
        let mut writer = encoder.write_header().expect("header");
        writer.write_image_data(&rgba).expect("data");
        writer.finish().expect("finish");
        out
    }

    fn image_obs(png: Vec<u8>) -> ClipboardObservation {
        ClipboardObservation {
            image: Some(png),
            source_app: Some("snippingtool.exe".to_owned()),
            ..ClipboardObservation::default()
        }
    }

    fn files_obs(paths: &[&str]) -> ClipboardObservation {
        ClipboardObservation {
            files: paths.iter().map(|p| (*p).to_owned()).collect(),
            source_app: Some("explorer.exe".to_owned()),
            ..ClipboardObservation::default()
        }
    }

    #[test]
    fn markers_skip_images_and_files_before_read() {
        let mut store = keyed();
        let marked = ClipboardObservation {
            excluded_format: true,
            html: Some("<b>x</b>".to_owned()),
            ..files_obs(&["C:\\a.txt"])
        };
        for observation in [
            marked,
            ClipboardObservation {
                excluded_format: true,
                ..image_obs(picture(8, 8, 1, png::Compression::Fast))
            },
        ] {
            let got =
                RecentCapture::consider(&mut store, &observation, &on(), now()).expect("consider");
            assert_eq!(got, Considered::Skipped(Skipped::ExcludedFormat));
        }
        assert_eq!(store.recent().expect("recent").len(), 0);
    }

    #[test]
    fn an_image_over_16_mpx_is_too_large() {
        let mut store = keyed();
        // 4097 x 4096 is one row over 16 Mpx; zeros compress to almost nothing.
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, 4097, 4096);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder.write_header().expect("header");
        writer
            .write_image_data(&vec![0; 4097 * 4096])
            .expect("data");
        writer.finish().expect("finish");
        let got =
            RecentCapture::consider(&mut store, &image_obs(out), &on(), now()).expect("consider");
        assert_eq!(got, Considered::Skipped(Skipped::TooLarge));
        // 101 files are too many; 100 are fine.
        let many: Vec<String> = (0..=MAX_FILES).map(|i| format!("C:\\f{i}.txt")).collect();
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        let got =
            RecentCapture::consider(&mut store, &files_obs(&refs), &on(), now()).expect("consider");
        assert_eq!(got, Considered::Skipped(Skipped::TooLarge));
        let got = RecentCapture::consider(&mut store, &files_obs(&refs[..MAX_FILES]), &on(), now())
            .expect("consider");
        assert!(matches!(got, Considered::Kept { .. }), "{got:?}");
    }

    #[test]
    fn images_and_files_need_a_key() {
        let mut store = Store::open_in_memory().expect("store");
        let got = RecentCapture::consider(
            &mut store,
            &image_obs(picture(8, 8, 1, png::Compression::Fast)),
            &on(),
            now(),
        )
        .expect("consider");
        assert_eq!(got, Considered::Skipped(Skipped::NoKeyForRich));
        let got = RecentCapture::consider(&mut store, &files_obs(&["C:\\a.txt"]), &on(), now())
            .expect("consider");
        assert_eq!(got, Considered::Skipped(Skipped::NoKeyForRich));
        assert_eq!(store.recent().expect("recent").len(), 0);
    }

    #[test]
    fn rich_over_1_mib_keeps_text_only() {
        let mut store = keyed();
        let small = ClipboardObservation {
            html: Some("<b>small</b>".to_owned()),
            rtf: Some(b"{\\rtf1 small}".to_vec()),
            ..obs("small")
        };
        let big = ClipboardObservation {
            html: Some("x".repeat(MAX_RICH_BYTES)),
            rtf: Some(vec![b'r'; 1]),
            ..obs("big")
        };
        let exactly = ClipboardObservation {
            html: Some("x".repeat(MAX_RICH_BYTES - 1)),
            rtf: Some(vec![b'r'; 1]),
            ..obs("exactly")
        };
        for o in [&small, &big, &exactly] {
            RecentCapture::consider(&mut store, o, &on(), now()).expect("consider");
        }
        let payload = |name: &str| {
            let id = store
                .recent()
                .expect("recent")
                .into_iter()
                .find(|e| e.text == name)
                .expect("entry")
                .id;
            store.recent_payload(id).expect("payload").expect("some")
        };
        assert!(matches!(
            payload("small"),
            RecentPayload::Text {
                html: Some(_),
                rtf: Some(_)
            }
        ));
        assert!(matches!(
            payload("exactly"),
            RecentPayload::Text {
                html: Some(_),
                rtf: Some(_)
            }
        ));
        // One byte over: the text stays, the flavours go.
        assert_eq!(
            payload("big"),
            RecentPayload::Text {
                html: None,
                rtf: None
            }
        );
    }

    #[test]
    fn same_pixels_dedupe() {
        let mut store = keyed();
        let fast = picture(16, 16, 3, png::Compression::Fast);
        let best = picture(16, 16, 3, png::Compression::High);
        assert_ne!(fast, best, "two encoders, two byte strings");
        let later = now() + Duration::from_secs(5);
        RecentCapture::consider(&mut store, &image_obs(fast), &on(), now()).expect("a");
        RecentCapture::consider(&mut store, &image_obs(best), &on(), later).expect("b");
        let all = store.recent().expect("recent");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].copied_at, later);
        // Other pixels are another entry, and the row shows its size and a thumbnail.
        let other = picture(16, 16, 4, png::Compression::Fast);
        RecentCapture::consider(&mut store, &image_obs(other), &on(), later).expect("c");
        let all = store.recent().expect("recent");
        assert_eq!(all.len(), 2);
        let row = row_view(&all[0], later);
        assert_eq!(row.kind, RecentKind::Image);
        assert_eq!(row.preview, "16 \u{d7} 16");
        let thumb = row.thumb.expect("thumbnail");
        assert_eq!((thumb.width, thumb.height), (16, 16));
    }

    #[test]
    fn a_big_picture_gets_a_small_thumbnail() {
        let rgba = vec![200_u8; 400 * 200 * 4];
        let thumb = thumbnail(400, 200, &rgba).expect("thumb");
        assert_eq!((thumb.width, thumb.height), (96, 48));
        assert!(thumb.rgba.len() <= snipdeck_store::THUMB_MAX_BYTES);
        assert!(thumb.rgba.chunks(4).all(|p| p == [200, 200, 200, 200]));
        assert!(thumbnail(0, 0, &[]).is_none());
    }

    #[test]
    fn same_files_any_order_dedupe() {
        let mut store = keyed();
        let later = now() + Duration::from_secs(1);
        RecentCapture::consider(
            &mut store,
            &files_obs(&["C:\\Docs\\b.pdf", "C:\\Docs\\A.txt"]),
            &on(),
            now(),
        )
        .expect("a");
        RecentCapture::consider(
            &mut store,
            &files_obs(&["c:\\docs\\a.TXT", "c:\\docs\\B.pdf"]),
            &on(),
            later,
        )
        .expect("b");
        let all = store.recent().expect("recent");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].kind, RecentKind::Files);
        assert_eq!(all[0].text, "2 files: a.TXT, B.pdf");
        let row = row_view(&all[0], later);
        assert_eq!(row.preview, "a.TXT, B.pdf");
    }

    #[test]
    fn text_from_two_apps_is_one_entry() {
        let mut store = keyed();
        let from_word = ClipboardObservation {
            html: Some("<b>same words</b>".to_owned()),
            source_app: Some("winword.exe".to_owned()),
            ..obs("same words")
        };
        let from_notepad = obs("same words");
        let later = now() + Duration::from_secs(1);
        RecentCapture::consider(&mut store, &from_word, &on(), now()).expect("a");
        RecentCapture::consider(&mut store, &from_notepad, &on(), later).expect("b");
        let all = store.recent().expect("recent");
        assert_eq!(all.len(), 1, "one entry");
        assert_eq!(all[0].source_app.as_deref(), Some("notepad.exe"));
        // The newest formats win: Notepad had no HTML.
        assert_eq!(
            store.recent_payload(all[0].id).expect("payload"),
            Some(RecentPayload::Text {
                html: None,
                rtf: None
            })
        );
    }

    #[test]
    fn budget_prunes_oldest_image_first() {
        let mut store = keyed();
        let pictures: Vec<Vec<u8>> = (1..=3)
            .map(|seed| picture(160, 160, seed, png::Compression::Fast))
            .collect();
        assert!(
            pictures
                .iter()
                .all(|p| p.len() > snipdeck_core::INLINE_LIMIT),
            "big enough to be blobs"
        );
        // Room for two sealed pictures, not three.
        let settings = RecentSettings {
            max_blob_bytes: u64::try_from(pictures[0].len() * 2 + 1024).expect("size"),
            ..on()
        };
        let mut pruned = Vec::new();
        for (i, p) in pictures.iter().enumerate() {
            let at = now() + Duration::from_secs(i as u64);
            let got = RecentCapture::consider(&mut store, &image_obs(p.clone()), &settings, at)
                .expect("consider");
            let Considered::Kept { pruned: n, .. } = got else {
                panic!("{got:?}");
            };
            pruned.push(n);
        }
        assert_eq!(pruned, [0, 0, 1]);
        let left = store.recent().expect("recent");
        assert_eq!(left.len(), 2);
        // The first picture is the one that went.
        let kept: Vec<Vec<u8>> = left
            .iter()
            .map(
                |e| match store.recent_payload(e.id).expect("p").expect("some") {
                    RecentPayload::Image(png) => png,
                    other => panic!("{other:?}"),
                },
            )
            .collect();
        assert!(!kept.contains(&pictures[0]));
        assert!(kept.contains(&pictures[1]) && kept.contains(&pictures[2]));
    }
}
