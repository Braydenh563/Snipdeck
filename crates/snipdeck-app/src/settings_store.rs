//! Writing the settings file: atomically, and not on every keystroke.
//!
//! - [`write_atomic`] never leaves a half-written `settings.json`: the new text goes to
//!   `settings.json.tmp`, is flushed to disk (`fsync`), and only then renamed over the old file
//!   (on Windows `std::fs::rename` replaces an existing file in one `MoveFileExW` call). A crash
//!   before the rename leaves the old file untouched and a stale `.tmp`, which the next write
//!   simply overwrites. Reading is [`crate::paths::load_settings`], which also sets a damaged
//!   file aside as `settings.json.bad`.
//! - [`SettingsSaver`] decides *when* to write. A change waits for a quiet moment
//!   ([`Debounce::quiet`]) so that dragging a slider writes once; continuous changes still write
//!   every [`Debounce::max_wait`]; a failed write is retried after [`Debounce::retry`]. It never
//!   reads a clock: callers pass `now` (the controller's injected clock), so tests are exact.

use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use snipdeck_core::Timestamp;

use crate::{paths::SETTINGS_FILE, settings::Settings};

/// The file a write goes to before it replaces `settings.json`.
pub const TEMP_FILE: &str = "settings.json.tmp";

/// Writes `settings` to `folder/settings.json` so that a reader (or a crash) sees the old file
/// or the new one, never a mixture. Creates `folder` if needed.
///
/// # Errors
///
/// Any I/O error from creating, writing, syncing or renaming. On error the old file is intact.
pub fn write_atomic(folder: &Path, settings: &Settings) -> io::Result<()> {
    let temp = write_temp(folder, &settings.to_json())?;
    commit(&temp, &folder.join(SETTINGS_FILE))
}

/// Step one of [`write_atomic`]: the complete new text, on disk, under the temporary name.
pub(crate) fn write_temp(folder: &Path, text: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(folder)?;
    let temp = folder.join(TEMP_FILE);
    let mut file = File::create(&temp)?;
    let written = file
        .write_all(text.as_bytes())
        .and_then(|()| file.write_all(b"\n"))
        // Data and metadata on disk before the rename can make it visible.
        .and_then(|()| file.sync_all());
    if let Err(e) = written {
        drop(file);
        let _ = fs::remove_file(&temp);
        return Err(e);
    }
    Ok(temp)
}

/// Step two: the rename that makes the new file the settings file.
pub(crate) fn commit(temp: &Path, dest: &Path) -> io::Result<()> {
    if let Err(e) = fs::rename(temp, dest) {
        let _ = fs::remove_file(temp);
        return Err(e);
    }
    // Make the rename itself durable. Unix can sync a directory; Windows cannot open one this
    // way, and NTFS journals the rename. Best effort either way: the file content is already safe.
    #[cfg(unix)]
    if let Some(parent) = dest.parent()
        && let Ok(dir) = File::open(parent)
    {
        let _ = dir.sync_all();
    }
    Ok(())
}

/// Where a [`SettingsSaver`] puts the settings.
pub trait SettingsSink {
    /// Stores `settings` durably.
    ///
    /// # Errors
    ///
    /// Whatever stopped the write; the saver keeps the change and retries.
    fn save(&mut self, settings: &Settings) -> io::Result<()>;
}

/// [`SettingsSink`] on `folder/settings.json` ([`write_atomic`]).
#[derive(Debug, Clone)]
pub struct FileSink {
    folder: PathBuf,
}

impl FileSink {
    /// A sink writing into `folder`.
    #[must_use]
    pub fn new(folder: impl Into<PathBuf>) -> Self {
        Self {
            folder: folder.into(),
        }
    }
}

impl SettingsSink for FileSink {
    fn save(&mut self, settings: &Settings) -> io::Result<()> {
        write_atomic(&self.folder, settings)
    }
}

/// How long the saver waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Debounce {
    /// A change is written once nothing changed for this long.
    pub quiet: Duration,
    /// ... but never later than this after the first unsaved change.
    pub max_wait: Duration,
    /// After a failed write, try again after this long.
    pub retry: Duration,
}

impl Default for Debounce {
    fn default() -> Self {
        Self {
            quiet: Duration::from_millis(500),
            max_wait: Duration::from_secs(5),
            retry: Duration::from_secs(5),
        }
    }
}

/// What [`SettingsSaver::poll`] did.
#[derive(Debug)]
pub enum Poll {
    /// Nothing is waiting to be written.
    Idle,
    /// A change is waiting; poll again after this long.
    Waiting(Duration),
    /// The waiting change was written.
    Saved,
    /// The write failed; the change is kept and is retried after [`Debounce::retry`].
    Failed(io::Error),
}

struct Pending {
    settings: Settings,
    due: Timestamp,
    deadline: Timestamp,
}

/// Debounced writing of the settings. See the module documentation.
pub struct SettingsSaver {
    sink: Box<dyn SettingsSink>,
    timing: Debounce,
    /// What is on disk, as far as this saver knows.
    saved: Settings,
    pending: Option<Pending>,
}

impl std::fmt::Debug for SettingsSaver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingsSaver")
            .field("pending", &self.pending.is_some())
            .finish_non_exhaustive()
    }
}

fn after(at: Timestamp, delay: Duration) -> Timestamp {
    at.checked_add(delay).unwrap_or(at)
}

impl SettingsSaver {
    /// A saver for `sink`, where `saved` is what the file holds now (what was loaded).
    #[must_use]
    pub fn new(sink: Box<dyn SettingsSink>, saved: Settings) -> Self {
        Self::with_timing(sink, saved, Debounce::default())
    }

    /// As [`new`](Self::new), with explicit timing.
    #[must_use]
    pub fn with_timing(sink: Box<dyn SettingsSink>, saved: Settings, timing: Debounce) -> Self {
        Self {
            sink,
            timing,
            saved,
            pending: None,
        }
    }

    /// The settings are now `settings`, at `now`. Cheap; call on every change.
    pub fn changed(&mut self, settings: &Settings, now: Timestamp) {
        if *settings == self.saved {
            // Changed back to what is on disk: nothing to write.
            self.pending = None;
            return;
        }
        let quiet_until = after(now, self.timing.quiet);
        match &mut self.pending {
            Some(pending) => {
                pending.settings.clone_from(settings);
                pending.due = quiet_until.min(pending.deadline);
            }
            None => {
                self.pending = Some(Pending {
                    settings: settings.clone(),
                    due: quiet_until,
                    deadline: after(now, self.timing.max_wait),
                });
            }
        }
    }

    /// Whether a change is waiting to be written.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// How long until the waiting change is due (zero if it already is), or `None` if nothing
    /// waits. Use it to arm a timer.
    #[must_use]
    pub fn wait(&self, now: Timestamp) -> Option<Duration> {
        self.pending.as_ref().map(|p| until(p.due, now))
    }

    /// Writes the waiting change if it is due at `now`.
    pub fn poll(&mut self, now: Timestamp) -> Poll {
        let Some(pending) = &mut self.pending else {
            return Poll::Idle;
        };
        if now < pending.due {
            return Poll::Waiting(until(pending.due, now));
        }
        match self.sink.save(&pending.settings) {
            Ok(()) => {
                if let Some(done) = self.pending.take() {
                    self.saved = done.settings;
                }
                Poll::Saved
            }
            Err(e) => {
                pending.due = after(now, self.timing.retry);
                pending.deadline = pending.due;
                Poll::Failed(e)
            }
        }
    }

    /// Writes the waiting change now, due or not (at exit).
    ///
    /// # Errors
    ///
    /// The sink's error; the change is kept.
    pub fn flush(&mut self) -> io::Result<()> {
        let Some(pending) = &self.pending else {
            return Ok(());
        };
        self.sink.save(&pending.settings)?;
        if let Some(done) = self.pending.take() {
            self.saved = done.settings;
        }
        Ok(())
    }
}

fn until(due: Timestamp, now: Timestamp) -> Duration {
    Duration::try_from(due.duration_since(now)).unwrap_or(Duration::ZERO)
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::*;
    use crate::{paths::load_settings, settings::Theme};

    fn at(s: &str) -> Timestamp {
        s.parse().expect("timestamp")
    }

    fn dark() -> Settings {
        Settings {
            theme: Theme::Dark,
            ..Settings::default()
        }
    }

    // ---- write_atomic ------------------------------------------------------------------------

    #[test]
    fn a_write_round_trips_and_leaves_no_temporary_file() {
        let temp = tempfile::tempdir().expect("dir");
        let folder = temp.path().join("not").join("yet");
        write_atomic(&folder, &dark()).expect("write");
        assert_eq!(load_settings(&folder), dark());
        assert!(!folder.join(TEMP_FILE).exists());
        let names: Vec<_> = fs::read_dir(&folder)
            .expect("dir")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(names, [SETTINGS_FILE]);
    }

    #[test]
    fn a_crash_between_the_temporary_write_and_the_rename_keeps_the_old_file() {
        let temp = tempfile::tempdir().expect("dir");
        let dir = temp.path();
        write_atomic(dir, &Settings::default()).expect("first");

        // The process dies after the temporary file is complete but before the rename.
        let staged = write_temp(dir, &dark().to_json()).expect("temp");
        assert_eq!(
            load_settings(dir),
            Settings::default(),
            "readers still see the old settings"
        );
        assert_eq!(
            fs::read_to_string(&staged).expect("temp text").trim_end(),
            dark().to_json()
        );

        // The next run writes again over the stale temporary file.
        write_atomic(dir, &dark()).expect("second");
        assert_eq!(load_settings(dir), dark());
        assert!(!staged.exists());
    }

    #[test]
    fn a_failed_rename_keeps_the_old_file_and_cleans_up() {
        let temp = tempfile::tempdir().expect("dir");
        let dir = temp.path();
        write_atomic(dir, &Settings::default()).expect("first");
        let staged = write_temp(dir, &dark().to_json()).expect("temp");
        // Pull the temporary file out from under the rename.
        fs::remove_file(&staged).expect("remove");
        assert!(commit(&staged, &dir.join(SETTINGS_FILE)).is_err());
        assert_eq!(load_settings(dir), Settings::default());
    }

    // ---- the debounce ------------------------------------------------------------------------

    #[derive(Clone, Default)]
    struct Recorder {
        writes: Rc<RefCell<Vec<Settings>>>,
        fail: Rc<RefCell<bool>>,
    }

    impl SettingsSink for Recorder {
        fn save(&mut self, settings: &Settings) -> io::Result<()> {
            if *self.fail.borrow() {
                return Err(io::Error::other("disk full"));
            }
            self.writes.borrow_mut().push(settings.clone());
            Ok(())
        }
    }

    fn saver(rec: &Recorder) -> SettingsSaver {
        SettingsSaver::new(Box::new(rec.clone()), Settings::default())
    }

    #[test]
    fn nothing_is_written_when_nothing_changed() {
        let rec = Recorder::default();
        let mut s = saver(&rec);
        s.changed(&Settings::default(), at("2026-10-17T09:00:00Z"));
        assert!(!s.has_pending());
        assert!(matches!(s.poll(at("2026-10-17T10:00:00Z")), Poll::Idle));
        assert_eq!(s.wait(at("2026-10-17T10:00:00Z")), None);
        assert!(rec.writes.borrow().is_empty());
    }

    #[test]
    fn a_change_is_written_after_the_quiet_time_and_not_before() {
        let rec = Recorder::default();
        let mut s = saver(&rec);
        s.changed(&dark(), at("2026-10-17T09:00:00Z"));
        assert_eq!(
            s.wait(at("2026-10-17T09:00:00Z")),
            Some(Duration::from_millis(500))
        );
        assert!(matches!(
            s.poll(at("2026-10-17T09:00:00.4Z")),
            Poll::Waiting(d) if d == Duration::from_millis(100)
        ));
        assert!(rec.writes.borrow().is_empty());
        assert!(matches!(s.poll(at("2026-10-17T09:00:00.5Z")), Poll::Saved));
        assert_eq!(*rec.writes.borrow(), [dark()]);
        assert!(!s.has_pending());
        assert!(matches!(s.poll(at("2026-10-17T09:00:01Z")), Poll::Idle));
    }

    #[test]
    fn changes_inside_the_quiet_time_become_one_write_of_the_latest() {
        let rec = Recorder::default();
        let mut s = saver(&rec);
        let mut width = Settings::default();
        for (i, w) in [300, 310, 320 + 1, 340].into_iter().enumerate() {
            width.panel_width = w;
            let now = at("2026-10-17T09:00:00Z")
                .checked_add(Duration::from_millis(200 * i as u64))
                .expect("time");
            s.changed(&width, now);
            assert!(matches!(s.poll(now), Poll::Waiting(_)));
        }
        // Last change at +600 ms; due at +1100 ms.
        assert!(matches!(
            s.poll(at("2026-10-17T09:00:01Z")),
            Poll::Waiting(_)
        ));
        assert!(matches!(s.poll(at("2026-10-17T09:00:01.1Z")), Poll::Saved));
        assert_eq!(rec.writes.borrow().len(), 1);
        assert_eq!(rec.writes.borrow()[0].panel_width, 340);
    }

    #[test]
    fn continuous_changes_still_write_at_the_maximum_wait() {
        let rec = Recorder::default();
        let mut s = saver(&rec);
        let start = at("2026-10-17T09:00:00Z");
        let mut width = Settings::default();
        // A change every 300 ms (always inside the 500 ms quiet time).
        for i in 0..40_u64 {
            width.panel_width = 260 + u32::try_from(i).expect("small");
            let now = start
                .checked_add(Duration::from_millis(300 * i))
                .expect("time");
            s.changed(&width, now);
            if let Poll::Saved = s.poll(now) {
                // The first change was at +0; the cap is 5 s.
                assert!(now >= start.checked_add(Duration::from_secs(5)).expect("time"));
                assert!(
                    now < start
                        .checked_add(Duration::from_millis(5400))
                        .expect("time")
                );
                assert_eq!(rec.writes.borrow().len(), 1);
                return;
            }
        }
        panic!("never written");
    }

    #[test]
    fn changing_back_to_what_is_saved_cancels_the_write() {
        let rec = Recorder::default();
        let mut s = saver(&rec);
        s.changed(&dark(), at("2026-10-17T09:00:00Z"));
        s.changed(&Settings::default(), at("2026-10-17T09:00:00.1Z"));
        assert!(!s.has_pending());
        assert!(matches!(s.poll(at("2026-10-17T09:01:00Z")), Poll::Idle));
        assert!(rec.writes.borrow().is_empty());
    }

    #[test]
    fn a_failed_write_keeps_the_change_and_retries_later() {
        let rec = Recorder::default();
        let mut s = saver(&rec);
        *rec.fail.borrow_mut() = true;
        s.changed(&dark(), at("2026-10-17T09:00:00Z"));
        assert!(matches!(
            s.poll(at("2026-10-17T09:00:01Z")),
            Poll::Failed(_)
        ));
        assert!(s.has_pending());
        assert_eq!(
            s.wait(at("2026-10-17T09:00:01Z")),
            Some(Duration::from_secs(5))
        );
        assert!(matches!(
            s.poll(at("2026-10-17T09:00:05Z")),
            Poll::Waiting(_)
        ));
        *rec.fail.borrow_mut() = false;
        assert!(matches!(s.poll(at("2026-10-17T09:00:06Z")), Poll::Saved));
        assert_eq!(*rec.writes.borrow(), [dark()]);
    }

    #[test]
    fn flush_writes_at_once_and_reports_failure() {
        let rec = Recorder::default();
        let mut s = saver(&rec);
        assert!(s.flush().is_ok(), "nothing to write");
        s.changed(&dark(), at("2026-10-17T09:00:00Z"));
        *rec.fail.borrow_mut() = true;
        assert!(s.flush().is_err());
        assert!(s.has_pending(), "kept for the next try");
        *rec.fail.borrow_mut() = false;
        assert!(s.flush().is_ok());
        assert_eq!(*rec.writes.borrow(), [dark()]);
        assert!(!s.has_pending());
    }

    #[test]
    fn the_file_sink_writes_the_settings_file_after_the_quiet_time() {
        let temp = tempfile::tempdir().expect("dir");
        let mut s = SettingsSaver::new(Box::new(FileSink::new(temp.path())), Settings::default());
        s.changed(&dark(), at("2026-10-17T09:00:00Z"));
        assert!(!temp.path().join(SETTINGS_FILE).exists(), "not yet");
        assert!(matches!(s.poll(at("2026-10-17T09:00:00.5Z")), Poll::Saved));
        assert_eq!(load_settings(temp.path()), dark());
    }
}
