//! Housekeeping the app does by itself: the daily backup, the Trash purge, the weekly integrity
//! check, and the one-time "closed unexpectedly" notice after a crash.
//!
//! [`Maintenance`] is a plain scheduler. It reads the injected [`Clock`], decides which jobs are
//! due, runs them through [`Jobs`] and returns what happened as [`MaintenanceOutcome`]s for the
//! controller to show. Nothing here touches a window or a timer: the host starts the first
//! [`tick`](Maintenance::tick) [`FIRST_DELAY`] after the Panel is first shown and then every
//! [`INTERVAL`].
//!
//! # Threading
//!
//! The jobs run on the event loop, one short call each, not on a worker thread. The controller
//! owns the only [`Store`] (the data folder's lock forbids a second connection, and the
//! connection is not shared), so a worker has nothing to run on. The backup is the longest job
//! (it exports the library); it is skipped when today's file exists, which is nearly every tick.
//!
//! When each job last succeeded is kept in `maintenance.json` in the data folder, so the
//! intervals survive a restart. A missing or damaged file just means "never ran".

use std::{
    fs,
    io::{self, Write as _},
    path::{Path, PathBuf},
    time::Duration,
};

use jiff::{Zoned, tz::TimeZone};
use serde::{Deserialize, Serialize};
use snipdeck_core::Timestamp;
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_portable::backup::{self, BackupOutcome};
use snipdeck_store::Store;

use crate::{
    controller::Controller,
    crash,
    host::{ClipboardText, Clock},
    main_window::TRASH_DAYS,
    toast::{Toast, ToastKind, ToastMessage},
};

/// The first run comes this long after the Panel is first shown.
pub const FIRST_DELAY: Duration = Duration::from_secs(2);
/// How often the host runs [`Maintenance::tick`], and how long a backup or purge waits for the
/// next one.
pub const INTERVAL: Duration = Duration::from_hours(6);
/// How often the integrity check runs.
pub const INTEGRITY_INTERVAL: Duration = Duration::from_hours(7 * 24);
/// The file in the data folder that remembers the last runs.
pub const STATE_FILE: &str = "maintenance.json";

/// When each job last succeeded, and the last crash report the user was told about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// Unix seconds of the last backup run (a day's file existing counts).
    pub last_backup: Option<i64>,
    /// Unix seconds of the last Trash purge.
    pub last_purge: Option<i64>,
    /// Unix seconds of the last integrity check.
    pub last_integrity: Option<i64>,
    /// File name of the newest crash report the user has been told about.
    pub crash_acknowledged: Option<String>,
}

impl State {
    /// Reads the state from `data_dir`. A missing, unreadable or damaged file is the default
    /// state: every job is due once.
    #[must_use]
    pub fn load(data_dir: &Path) -> Self {
        fs::read_to_string(data_dir.join(STATE_FILE))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Writes the state next to the library: a temporary file, flushed, then renamed over the
    /// old one, so a crash leaves the old file or the new one, never half of it.
    ///
    /// # Errors
    ///
    /// The file cannot be written.
    pub fn save(&self, data_dir: &Path) -> io::Result<()> {
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        let temp = data_dir.join(format!("{STATE_FILE}.tmp"));
        let written = fs::File::create(&temp).and_then(|mut file| {
            file.write_all(text.as_bytes())?;
            file.sync_all()
        });
        match written.and_then(|()| fs::rename(&temp, data_dir.join(STATE_FILE))) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = fs::remove_file(&temp);
                Err(e)
            }
        }
    }
}

/// Whether a job last run at `last` (Unix seconds) is due at `now` after `interval`: never run,
/// at least `interval` ago, or in the future (the clock was set back).
fn is_due(last: Option<i64>, now: i64, interval: Duration) -> bool {
    let interval = i64::try_from(interval.as_secs()).unwrap_or(i64::MAX);
    last.is_none_or(|last| last > now || now.saturating_sub(last) >= interval)
}

/// What a backup run did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupResult {
    /// Today's file was written; `pruned` older ones were deleted.
    Created {
        /// How many old backups went.
        pruned: usize,
    },
    /// Today's file already existed.
    NotDue,
}

/// The three jobs, as the scheduler needs them. The app's implementation is [`ControllerJobs`];
/// tests use a fake that records the calls.
pub trait Jobs {
    /// Writes today's backup (the date of `now`) unless it exists, and keeps the newest 7.
    ///
    /// # Errors
    ///
    /// A message for the log.
    fn backup(&mut self, now: &Zoned) -> Result<BackupResult, String>;

    /// Deletes the Trash entries trashed before `cutoff`; returns how many.
    ///
    /// # Errors
    ///
    /// A message for the log.
    fn purge_trash(&mut self, cutoff: Timestamp) -> Result<usize, String>;

    /// Checks the library; returns one line per problem (empty when healthy).
    ///
    /// # Errors
    ///
    /// A message for the log.
    fn check_integrity(&mut self) -> Result<Vec<String>, String>;
}

/// What a [`Maintenance::tick`] found, for [`Controller::report_maintenance`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaintenanceOutcome {
    /// A backup was written.
    BackupCreated {
        /// How many old backups were deleted.
        pruned: usize,
    },
    /// The backup could not be made; it is tried again at the next run.
    BackupFailed(String),
    /// The Trash purge finished.
    TrashPurged {
        /// How many entries were deleted for good.
        count: usize,
    },
    /// The purge failed.
    TrashPurgeFailed(String),
    /// The integrity check found nothing wrong.
    IntegrityOk,
    /// The integrity check found these problems.
    IntegrityProblems(Vec<String>),
    /// The integrity check could not run.
    IntegrityFailed(String),
    /// The last run ended in a crash the user has not been told about.
    CrashedLastTime,
}

impl MaintenanceOutcome {
    /// The toast to show for this outcome, if it deserves one. Only a damaged library and a
    /// crash do; routine results are only logged.
    #[must_use]
    pub fn into_toast(self) -> Option<Toast> {
        match self {
            Self::IntegrityProblems(_) => Some(Toast {
                kind: ToastKind::Danger,
                message: ToastMessage::LibraryProblem,
            }),
            Self::CrashedLastTime => Some(Toast {
                kind: ToastKind::Neutral,
                message: ToastMessage::CrashedLastTime,
            }),
            _ => None,
        }
    }

    /// One line for the diagnostic log (`SNIPDECK_LOG=1`). Names counts and the checker's own
    /// words, never library content.
    #[must_use]
    pub fn log_line(&self) -> String {
        match self {
            Self::BackupCreated { pruned } => format!("backup written, {pruned} old ones deleted"),
            Self::BackupFailed(e) => format!("backup failed: {e}"),
            Self::TrashPurged { count } => format!("{count} expired Trash entries deleted"),
            Self::TrashPurgeFailed(e) => format!("Trash purge failed: {e}"),
            Self::IntegrityOk => "integrity check: ok".to_owned(),
            Self::IntegrityProblems(problems) => {
                format!(
                    "integrity check: {} problems: {}",
                    problems.len(),
                    problems.join("; ")
                )
            }
            Self::IntegrityFailed(e) => format!("integrity check could not run: {e}"),
            Self::CrashedLastTime => "the last run ended in a crash".to_owned(),
        }
    }
}

/// The scheduler. See the module documentation.
pub struct Maintenance {
    data_dir: PathBuf,
    clock: Box<dyn Clock>,
    state: State,
    crash_checked: bool,
}

impl std::fmt::Debug for Maintenance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Maintenance")
            .field("data_dir", &self.data_dir)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl Maintenance {
    /// A scheduler for `data_dir`, with the saved state loaded.
    #[must_use]
    pub fn new(data_dir: impl Into<PathBuf>, clock: Box<dyn Clock>) -> Self {
        let data_dir = data_dir.into();
        let state = State::load(&data_dir);
        Self {
            data_dir,
            clock,
            state,
            crash_checked: false,
        }
    }

    /// What is remembered about the last runs.
    #[must_use]
    pub fn state(&self) -> &State {
        &self.state
    }

    /// Runs what is due, saves when something succeeded, and returns what happened. The first
    /// call also looks for a crash report the user has not been told about. A failed job leaves
    /// its last-run time alone, so the next tick tries it again.
    pub fn tick(&mut self, jobs: &mut dyn Jobs) -> Vec<MaintenanceOutcome> {
        let now = self.clock.now();
        let seconds = now.as_second();
        let mut outcomes = Vec::new();
        let mut changed = false;

        if !self.crash_checked {
            self.crash_checked = true;
            if let Some(newest) =
                crash::unacknowledged(&self.data_dir, self.state.crash_acknowledged.as_deref())
            {
                self.state.crash_acknowledged = Some(newest);
                outcomes.push(MaintenanceOutcome::CrashedLastTime);
                changed = true;
            }
        }

        if is_due(self.state.last_backup, seconds, INTERVAL) {
            match jobs.backup(&now.to_zoned(TimeZone::system())) {
                Ok(result) => {
                    self.state.last_backup = Some(seconds);
                    changed = true;
                    if let BackupResult::Created { pruned } = result {
                        outcomes.push(MaintenanceOutcome::BackupCreated { pruned });
                    }
                }
                Err(e) => outcomes.push(MaintenanceOutcome::BackupFailed(e)),
            }
        }

        if is_due(self.state.last_purge, seconds, INTERVAL) {
            match jobs.purge_trash(trash_cutoff(now)) {
                Ok(count) => {
                    self.state.last_purge = Some(seconds);
                    changed = true;
                    outcomes.push(MaintenanceOutcome::TrashPurged { count });
                }
                Err(e) => outcomes.push(MaintenanceOutcome::TrashPurgeFailed(e)),
            }
        }

        if is_due(self.state.last_integrity, seconds, INTEGRITY_INTERVAL) {
            match jobs.check_integrity() {
                Ok(problems) => {
                    self.state.last_integrity = Some(seconds);
                    changed = true;
                    outcomes.push(if problems.is_empty() {
                        MaintenanceOutcome::IntegrityOk
                    } else {
                        MaintenanceOutcome::IntegrityProblems(problems)
                    });
                }
                Err(e) => outcomes.push(MaintenanceOutcome::IntegrityFailed(e)),
            }
        }

        if changed && let Err(e) = self.state.save(&self.data_dir) {
            crate::trace!("maintenance: cannot save {STATE_FILE}: {e}");
        }
        outcomes
    }
}

/// Trash entries older than [`TRASH_DAYS`] days at `now` are expired.
fn trash_cutoff(now: Timestamp) -> Timestamp {
    let seconds = now
        .as_second()
        .saturating_sub(i64::from(TRASH_DAYS) * 86_400);
    Timestamp::from_second(seconds).unwrap_or(Timestamp::MIN)
}

/// The jobs on the app's own library, run through the controller that owns it.
pub struct ControllerJobs<'a, C, K, T> {
    controller: &'a mut Controller<Store, C, K, T>,
    data_dir: &'a Path,
}

impl<'a, C, K, T> ControllerJobs<'a, C, K, T> {
    /// Jobs on `controller`'s library, with backups in `data_dir`.
    pub fn new(controller: &'a mut Controller<Store, C, K, T>, data_dir: &'a Path) -> Self {
        Self {
            controller,
            data_dir,
        }
    }
}

impl<C, K, T> Jobs for ControllerJobs<'_, C, K, T>
where
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    fn backup(&mut self, now: &Zoned) -> Result<BackupResult, String> {
        let store = self.controller.store();
        // Encrypted with the library's key when it has one and it is unlocked; with no key the
        // export leaves sensitive items out, so a backup never holds one in plain text.
        let key = snipdeck_portable::Library::data_key(store);
        match backup::run_if_due(store, self.data_dir, now, backup::DEFAULT_KEEP, key) {
            Ok(BackupOutcome::Created { pruned, .. }) => Ok(BackupResult::Created {
                pruned: pruned.len(),
            }),
            Ok(BackupOutcome::NotDue { .. }) => Ok(BackupResult::NotDue),
            Err(e) => Err(e.to_string()),
        }
    }

    /// The controller cuts at its own clock's `now - TRASH_DAYS`, the same moment as `_cutoff`.
    fn purge_trash(&mut self, _cutoff: Timestamp) -> Result<usize, String> {
        Ok(self.controller.purge_expired_trash())
    }

    fn check_integrity(&mut self) -> Result<Vec<String>, String> {
        self.controller
            .store()
            .check_integrity()
            .map_err(|e| e.to_string())
    }
}

/// Runs one [`Maintenance::tick`] on `controller` and hands each outcome to
/// [`Controller::report_maintenance`]. The host calls this from its timers.
pub fn run<C, K, T>(
    maintenance: &mut Maintenance,
    controller: &mut Controller<Store, C, K, T>,
) -> usize
where
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    let data_dir = maintenance.data_dir.clone();
    let outcomes = maintenance.tick(&mut ControllerJobs::new(controller, &data_dir));
    let count = outcomes.len();
    for outcome in outcomes {
        controller.report_maintenance(outcome);
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::FixedClock;

    const HOUR: i64 = 3600;
    const DAY: i64 = 86_400;

    fn at(seconds: i64) -> Timestamp {
        Timestamp::from_second(seconds).unwrap()
    }

    /// 2026-10-18T09:00:00Z.
    fn t0() -> i64 {
        "2026-10-18T09:00:00Z"
            .parse::<Timestamp>()
            .unwrap()
            .as_second()
    }

    #[derive(Default)]
    struct FakeJobs {
        backups: Vec<Timestamp>,
        purges: Vec<Timestamp>,
        checks: usize,
        backup_result: Option<Result<BackupResult, String>>,
        purge_result: Option<Result<usize, String>>,
        integrity_result: Option<Result<Vec<String>, String>>,
    }

    impl Jobs for FakeJobs {
        fn backup(&mut self, now: &Zoned) -> Result<BackupResult, String> {
            self.backups.push(now.timestamp());
            self.backup_result
                .clone()
                .unwrap_or(Ok(BackupResult::Created { pruned: 0 }))
        }
        fn purge_trash(&mut self, cutoff: Timestamp) -> Result<usize, String> {
            self.purges.push(cutoff);
            self.purge_result.clone().unwrap_or(Ok(0))
        }
        fn check_integrity(&mut self) -> Result<Vec<String>, String> {
            self.checks += 1;
            self.integrity_result.clone().unwrap_or(Ok(Vec::new()))
        }
    }

    fn rig(dir: &Path) -> (Maintenance, FixedClock) {
        let clock = FixedClock::new(at(t0()));
        (Maintenance::new(dir, Box::new(clock.clone())), clock)
    }

    #[test]
    fn the_first_tick_runs_every_job_with_the_clock_s_now() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, _clock) = rig(dir.path());
        let mut jobs = FakeJobs::default();
        let out = m.tick(&mut jobs);
        assert_eq!(jobs.backups, [at(t0())]);
        assert_eq!(jobs.checks, 1);
        assert_eq!(
            out,
            [
                MaintenanceOutcome::BackupCreated { pruned: 0 },
                MaintenanceOutcome::TrashPurged { count: 0 },
                MaintenanceOutcome::IntegrityOk,
            ]
        );
    }

    #[test]
    fn the_purge_cuts_at_exactly_thirty_days_before_now() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, _clock) = rig(dir.path());
        let mut jobs = FakeJobs::default();
        m.tick(&mut jobs);
        assert_eq!(jobs.purges, [at(t0() - 30 * DAY)]);
        assert_eq!(
            jobs.purges[0].to_string(),
            "2026-09-18T09:00:00Z",
            "30 days before 2026-10-18"
        );
    }

    #[test]
    fn backup_and_purge_wait_exactly_six_hours() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, clock) = rig(dir.path());
        let mut jobs = FakeJobs::default();
        m.tick(&mut jobs);

        clock.set(at(t0() + 6 * HOUR - 1));
        assert!(m.tick(&mut jobs).is_empty(), "one second early");
        assert_eq!((jobs.backups.len(), jobs.purges.len()), (1, 1));

        clock.set(at(t0() + 6 * HOUR));
        let out = m.tick(&mut jobs);
        assert_eq!((jobs.backups.len(), jobs.purges.len()), (2, 2), "{out:?}");
        assert_eq!(jobs.backups[1], at(t0() + 6 * HOUR));
        assert_eq!(jobs.checks, 1, "the integrity check is weekly");
    }

    #[test]
    fn the_integrity_check_runs_weekly_to_the_second() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, clock) = rig(dir.path());
        let mut jobs = FakeJobs::default();
        m.tick(&mut jobs);
        // Tick every 6 h for a week less one second: still one check.
        let mut t = t0();
        while t + 6 * HOUR < t0() + 7 * DAY {
            t += 6 * HOUR;
            clock.set(at(t));
            m.tick(&mut jobs);
        }
        clock.set(at(t0() + 7 * DAY - 1));
        m.tick(&mut jobs);
        assert_eq!(jobs.checks, 1);
        clock.set(at(t0() + 7 * DAY));
        m.tick(&mut jobs);
        assert_eq!(jobs.checks, 2);
    }

    #[test]
    fn a_backup_that_exists_already_is_not_reported_but_counts_as_run() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, clock) = rig(dir.path());
        let mut jobs = FakeJobs {
            backup_result: Some(Ok(BackupResult::NotDue)),
            ..FakeJobs::default()
        };
        let out = m.tick(&mut jobs);
        assert!(
            !out.iter()
                .any(|o| matches!(o, MaintenanceOutcome::BackupCreated { .. }))
        );
        assert_eq!(m.state().last_backup, Some(t0()));
        clock.set(at(t0() + HOUR));
        m.tick(&mut jobs);
        assert_eq!(jobs.backups.len(), 1);
    }

    #[test]
    fn a_failed_job_is_tried_again_at_the_next_tick() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, clock) = rig(dir.path());
        let mut jobs = FakeJobs {
            backup_result: Some(Err("disk full".into())),
            purge_result: Some(Err("locked".into())),
            integrity_result: Some(Err("busy".into())),
            ..FakeJobs::default()
        };
        let out = m.tick(&mut jobs);
        assert_eq!(
            out,
            [
                MaintenanceOutcome::BackupFailed("disk full".into()),
                MaintenanceOutcome::TrashPurgeFailed("locked".into()),
                MaintenanceOutcome::IntegrityFailed("busy".into()),
            ]
        );
        assert_eq!(*m.state(), State::default(), "nothing is marked as run");
        assert!(!dir.path().join(STATE_FILE).exists());

        clock.set(at(t0() + 1));
        jobs.backup_result = None;
        jobs.purge_result = None;
        jobs.integrity_result = None;
        m.tick(&mut jobs);
        assert_eq!(
            (jobs.backups.len(), jobs.purges.len(), jobs.checks),
            (2, 2, 2)
        );
    }

    #[test]
    fn problems_become_a_danger_toast_and_a_clean_check_does_not() {
        let problems = MaintenanceOutcome::IntegrityProblems(vec!["x".into()]);
        let toast = problems.clone().into_toast().expect("a toast");
        assert_eq!(toast.kind, ToastKind::Danger);
        assert_eq!(toast.message, ToastMessage::LibraryProblem);
        for quiet in [
            MaintenanceOutcome::IntegrityOk,
            MaintenanceOutcome::BackupCreated { pruned: 1 },
            MaintenanceOutcome::BackupFailed("x".into()),
            MaintenanceOutcome::TrashPurged { count: 3 },
        ] {
            assert!(!quiet.log_line().is_empty());
            assert_eq!(quiet.clone().into_toast(), None, "{quiet:?}");
        }
        assert_eq!(
            MaintenanceOutcome::CrashedLastTime
                .into_toast()
                .unwrap()
                .message,
            ToastMessage::CrashedLastTime
        );
        assert!(problems.log_line().contains("1 problems: x"));
    }

    #[test]
    fn the_state_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, clock) = rig(dir.path());
        let mut jobs = FakeJobs::default();
        m.tick(&mut jobs);
        let saved = State::load(dir.path());
        assert_eq!(
            saved,
            State {
                last_backup: Some(t0()),
                last_purge: Some(t0()),
                last_integrity: Some(t0()),
                crash_acknowledged: None,
            }
        );
        assert!(!dir.path().join("maintenance.json.tmp").exists());

        // A new process an hour later: nothing is due.
        clock.set(at(t0() + HOUR));
        let mut again = Maintenance::new(dir.path(), Box::new(clock.clone()));
        assert!(again.tick(&mut jobs).is_empty());
        assert_eq!(jobs.backups.len(), 1);
        // And at six hours it is.
        clock.set(at(t0() + 6 * HOUR));
        again.tick(&mut jobs);
        assert_eq!(jobs.backups.len(), 2);
    }

    #[test]
    fn a_missing_or_damaged_file_means_everything_is_due() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(State::load(dir.path()), State::default());
        for junk in [
            "",
            "not json",
            "{\"last_backup\": \"soon\"}",
            "42",
            "[\"x\"]",
            "\u{0}\u{0}",
        ] {
            fs::write(dir.path().join(STATE_FILE), junk).unwrap();
            assert_eq!(State::load(dir.path()), State::default(), "{junk:?}");
            let (mut m, _clock) = rig(dir.path());
            let mut jobs = FakeJobs::default();
            m.tick(&mut jobs);
            assert_eq!(jobs.backups.len(), 1, "{junk:?}");
            // The damaged file was replaced by a good one.
            assert_eq!(State::load(dir.path()).last_backup, Some(t0()));
        }
    }

    #[test]
    fn unknown_and_missing_fields_are_tolerated() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(STATE_FILE),
            r#"{"last_backup": 5, "from_a_newer_version": true}"#,
        )
        .unwrap();
        let state = State::load(dir.path());
        assert_eq!(state.last_backup, Some(5));
        assert_eq!(state.last_purge, None);
    }

    #[test]
    fn a_clock_set_back_does_not_stall_the_jobs() {
        assert!(is_due(Some(1000), 999, INTERVAL));
        assert!(!is_due(Some(1000), 1000, INTERVAL));
        assert!(is_due(None, 0, INTERVAL));
        let six = i64::try_from(INTERVAL.as_secs()).unwrap();
        assert!(!is_due(Some(0), six - 1, INTERVAL));
        assert!(is_due(Some(0), six, INTERVAL));
        assert!(!is_due(Some(i64::MIN), i64::MIN, INTERVAL));
        assert!(is_due(Some(i64::MIN), i64::MAX, INTERVAL), "no overflow");
    }

    #[test]
    fn a_new_crash_report_is_announced_once() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, clock) = rig(dir.path());
        let mut jobs = FakeJobs::default();
        assert!(
            !m.tick(&mut jobs)
                .contains(&MaintenanceOutcome::CrashedLastTime)
        );

        // The app died; the next start finds the report.
        let report = crash::Report {
            message: "boom".into(),
            location: None,
            thread: None,
            backtrace: String::new(),
        };
        let died = clock.now().to_zoned(TimeZone::UTC);
        crash::write_report(dir.path(), &died, &report).unwrap();
        clock.set(at(t0() + 10 * HOUR));
        let mut next = Maintenance::new(dir.path(), Box::new(clock.clone()));
        assert_eq!(next.tick(&mut jobs)[0], MaintenanceOutcome::CrashedLastTime);
        assert_eq!(
            next.state().crash_acknowledged.as_deref(),
            Some("crash-2026-10-18-090000.txt")
        );

        // Not again, in this process or the next.
        clock.set(at(t0() + 20 * HOUR));
        assert!(
            !next
                .tick(&mut jobs)
                .contains(&MaintenanceOutcome::CrashedLastTime)
        );
        let mut third = Maintenance::new(dir.path(), Box::new(clock.clone()));
        assert!(
            !third
                .tick(&mut jobs)
                .contains(&MaintenanceOutcome::CrashedLastTime)
        );

        // A later crash is a new one.
        let later = at(t0() + 21 * HOUR).to_zoned(TimeZone::UTC);
        crash::write_report(dir.path(), &later, &report).unwrap();
        let mut fourth = Maintenance::new(dir.path(), Box::new(clock.clone()));
        assert!(
            fourth
                .tick(&mut jobs)
                .contains(&MaintenanceOutcome::CrashedLastTime)
        );
    }

    #[test]
    fn a_report_written_while_running_is_not_announced_until_the_next_start() {
        let dir = tempfile::tempdir().unwrap();
        let (mut m, clock) = rig(dir.path());
        let mut jobs = FakeJobs::default();
        m.tick(&mut jobs);
        let report = crash::Report {
            message: "worker".into(),
            location: None,
            thread: None,
            backtrace: String::new(),
        };
        crash::write_report(dir.path(), &clock.now().to_zoned(TimeZone::UTC), &report).unwrap();
        clock.set(at(t0() + 6 * HOUR));
        assert!(
            !m.tick(&mut jobs)
                .contains(&MaintenanceOutcome::CrashedLastTime)
        );
    }
}
