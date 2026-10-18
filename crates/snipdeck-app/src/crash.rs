//! The crash log: a panic hook that writes `logs/crash-<yyyy-mm-dd-hhmmss>.txt` (UTC) in the data
//! folder, and the check at the next start that tells the user.
//!
//! A report holds the panic message only when it is a string literal (a `String` payload is
//! formatted at run time and can quote library text, so only its length is written), where it
//! happened, the thread, a backtrace, the app version and the operating system. It never holds
//! library content: the hook reads nothing from the store. File names carry UTC, so they sort by
//! real time across daylight-saving changes and travel; the header line has local time. Only the newest [`MAX_FILES`] reports are kept.
//!
//! The hook is installed first thing in `main` with [`install`]; the data folder is not known
//! then, so [`set_data_dir`] gives it later (a panic before that writes nothing). Everything that
//! decides something (the file name, the pruning, whether a report is new) is a plain function
//! of a folder and a time, so tests run without a real panic or the real clock.

use std::{
    fmt::Write as _,
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
};

use jiff::{Timestamp, Zoned, tz::TimeZone};

/// The folder inside the data folder that holds the reports.
pub const LOG_DIR: &str = "logs";
/// How many reports are kept; the oldest go first.
pub const MAX_FILES: usize = 10;
/// The longest panic message written, in characters.
const MAX_MESSAGE: usize = 300;

const PREFIX: &str = "crash-";
const EXTENSION: &str = ".txt";

/// Where the hook writes, once known.
static DATA_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// What a report says about one panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The panic message when it was a string literal, cut to a few hundred characters; else a
    /// note with the payload's length.
    pub message: String,
    /// `file:line:column` of the panic, if known.
    pub location: Option<String>,
    /// The panicking thread's name, if it has one.
    pub thread: Option<String>,
    /// The backtrace as text.
    pub backtrace: String,
}

impl Report {
    /// The text of the file.
    #[must_use]
    pub fn render(&self, now: &Zoned) -> String {
        let mut text = String::new();
        let _ = writeln!(text, "Snipdeck crash report");
        let _ = writeln!(text, "time: {}", now.strftime("%Y-%m-%d %H:%M:%S %:z"));
        let _ = writeln!(text, "version: {}", env!("CARGO_PKG_VERSION"));
        let _ = writeln!(
            text,
            "os: {} {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        let _ = writeln!(text, "thread: {}", self.thread.as_deref().unwrap_or("?"));
        let _ = writeln!(
            text,
            "location: {}",
            self.location.as_deref().unwrap_or("unknown")
        );
        let _ = writeln!(text, "message: {}", self.message);
        let _ = writeln!(text, "\nbacktrace:\n{}", self.backtrace);
        text
    }
}

/// The reports folder of data folder `data_dir`.
#[must_use]
pub fn logs_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(LOG_DIR)
}

/// The file name for a report made at `now`: `crash-2026-10-18-093045.txt`, in UTC so that names
/// sort by real time whatever the local clock does. The header line has the local time.
#[must_use]
pub fn file_name(now: &Zoned) -> String {
    let utc = now.with_time_zone(TimeZone::UTC);
    format!("{PREFIX}{}{EXTENSION}", utc.strftime("%Y-%m-%d-%H%M%S"))
}

/// Whether `name` is exactly a report's file name.
#[must_use]
pub fn is_report_name(name: &str) -> bool {
    let Some(stamp) = name
        .strip_prefix(PREFIX)
        .and_then(|rest| rest.strip_suffix(EXTENSION))
    else {
        return false;
    };
    // yyyy-mm-dd-hhmmss
    stamp.len() == 17
        && stamp.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 | 10 => b == b'-',
            _ => b.is_ascii_digit(),
        })
}

/// The report file names in `data_dir`'s log folder, oldest first. A missing folder is empty.
#[must_use]
pub fn report_names(data_dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(logs_dir(data_dir)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| is_report_name(name))
        .collect();
    names.sort();
    names
}

/// Writes `report` for a panic at `now` and keeps only the newest [`MAX_FILES`] reports. Two
/// panics in the same second go into one file. Returns the file.
///
/// # Errors
///
/// The folder or file cannot be written. Pruning problems are ignored.
pub fn write_report(data_dir: &Path, now: &Zoned, report: &Report) -> io::Result<PathBuf> {
    let dir = logs_dir(data_dir);
    fs::create_dir_all(&dir)?;
    let path = dir.join(file_name(now));
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    io::Write::write_all(&mut file, report.render(now).as_bytes())?;
    file.sync_all()?;
    prune(data_dir);
    Ok(path)
}

/// Deletes all but the newest [`MAX_FILES`] reports.
fn prune(data_dir: &Path) {
    let names = report_names(data_dir);
    let excess = names.len().saturating_sub(MAX_FILES);
    for name in names.into_iter().take(excess) {
        let _ = fs::remove_file(logs_dir(data_dir).join(name));
    }
}

/// The newest report's file name, if any.
#[must_use]
pub fn newest(data_dir: &Path) -> Option<String> {
    report_names(data_dir).pop()
}

/// The newest report's file name when it is newer than `acknowledged` (the last one the user was
/// told about), else `None`.
#[must_use]
pub fn unacknowledged(data_dir: &Path, acknowledged: Option<&str>) -> Option<String> {
    newest(data_dir).filter(|name| acknowledged.is_none_or(|seen| name.as_str() > seen))
}

/// Tells the hook where to write. Call it once the data folder is known.
pub fn set_data_dir(data_dir: &Path) {
    if let Ok(mut slot) = DATA_DIR.lock() {
        *slot = Some(data_dir.to_path_buf());
    }
}

/// The folder set by [`set_data_dir`].
fn data_dir() -> Option<PathBuf> {
    // `try_lock`: a panic inside `set_data_dir` itself must not hang the hook.
    DATA_DIR.try_lock().ok()?.clone()
}

/// Installs the panic hook: it writes a report with the time `now` returns (a function, so a test
/// can pass a fixed one), then calls the hook that was set before.
pub fn install(now: fn() -> Timestamp) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(dir) = data_dir() {
            let _ = write_report(&dir, &now().to_zoned(TimeZone::system()), &report_of(info));
        }
        previous(info);
    }));
}

/// The message to write for a panic payload. A `&'static str` is a literal in the source and
/// safe; a `String` is built at run time (a slice panic quotes the text it sliced), so only its
/// size is written.
fn message_of(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&'static str>() {
        text.chars().take(MAX_MESSAGE).collect()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        format!("(message omitted, {} bytes)", text.len())
    } else {
        "(no message)".to_owned()
    }
}

/// The report for one panic.
fn report_of(info: &std::panic::PanicHookInfo<'_>) -> Report {
    Report {
        message: message_of(info.payload()),
        location: info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())),
        thread: std::thread::current().name().map(str::to_owned),
        backtrace: std::backtrace::Backtrace::force_capture().to_string(),
    }
}

/// Opens the reports folder in the file manager (what "Open the log?" does).
///
/// # Errors
///
/// The data folder is not known yet, or the file manager cannot be started.
pub fn open_log_folder() -> io::Result<()> {
    let dir = data_dir().ok_or_else(|| io::Error::other("the data folder is not known"))?;
    let dir = logs_dir(&dir);
    fs::create_dir_all(&dir)?;
    let opener = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(dir)
        .spawn()
        .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{Clock, FixedClock};

    fn start() -> Timestamp {
        "2026-10-18T09:30:45Z".parse().expect("timestamp")
    }

    fn zoned(at: Timestamp) -> Zoned {
        at.to_zoned(TimeZone::UTC)
    }

    fn report(message: &str) -> Report {
        Report {
            message: message.to_owned(),
            location: Some("src/x.rs:1:2".to_owned()),
            thread: Some("main".to_owned()),
            backtrace: "frame".to_owned(),
        }
    }

    #[test]
    fn the_file_name_is_the_time_and_sorts_by_it() {
        let clock = FixedClock::new(start());
        let a = file_name(&zoned(clock.now()));
        assert_eq!(a, "crash-2026-10-18-093045.txt");
        clock.set("2026-10-18T09:30:46Z".parse().unwrap());
        assert!(file_name(&zoned(clock.now())) > a);
        clock.set("2027-01-02T00:00:00Z".parse().unwrap());
        assert!(file_name(&zoned(clock.now())) > a);
    }

    #[test]
    fn the_file_name_uses_utc_whatever_the_local_zone() {
        let at = start();
        let plus_two = at.to_zoned(TimeZone::fixed(jiff::tz::offset(2)));
        assert_eq!(file_name(&plus_two), "crash-2026-10-18-093045.txt");
        assert_eq!(file_name(&plus_two), file_name(&zoned(at)));
    }

    #[test]
    fn a_later_crash_stays_newest_when_the_local_clock_goes_back() {
        // Local time falls back an hour between two crashes (DST end): the later crash has the
        // earlier local wall time but the later real time.
        let dir = tempfile::tempdir().unwrap();
        let first: Timestamp = "2026-10-25T00:30:00Z".parse().unwrap(); // 02:30 local (+02:00)
        let second: Timestamp = "2026-10-25T01:10:00Z".parse().unwrap(); // 02:10 local (+01:00)
        let before = first.to_zoned(TimeZone::fixed(jiff::tz::offset(2)));
        let after = second.to_zoned(TimeZone::fixed(jiff::tz::offset(1)));
        write_report(dir.path(), &before, &report("a")).unwrap();
        let seen = newest(dir.path()).unwrap();
        write_report(dir.path(), &after, &report("b")).unwrap();
        let latest = newest(dir.path()).unwrap();
        assert!(latest > seen, "{latest} vs {seen}");
        assert_eq!(unacknowledged(dir.path(), Some(&seen)), Some(latest));
    }

    #[test]
    fn a_runtime_built_panic_message_is_not_written() {
        let literal: Box<dyn std::any::Any + Send> = Box::new("plain words");
        assert_eq!(message_of(&*literal), "plain words");
        let built: Box<dyn std::any::Any + Send> = Box::new(String::from("quotes `héllo`"));
        let text = message_of(&*built);
        assert_eq!(text, "(message omitted, 15 bytes)");
        assert!(!text.contains("llo"));
        let other: Box<dyn std::any::Any + Send> = Box::new(7_u8);
        assert_eq!(message_of(&*other), "(no message)");
    }

    #[test]
    fn only_exact_report_names_count() {
        assert!(is_report_name("crash-2026-10-18-093045.txt"));
        for bad in [
            "crash-2026-10-18-093045.txt.bak",
            "crash-2026-10-18-09304.txt",
            "crash-2026-10-18-0930455.txt",
            "crash-2026_10-18-093045.txt",
            "crash-abcd-10-18-093045.txt",
            "crash.txt",
            "notes.txt",
        ] {
            assert!(!is_report_name(bad), "{bad}");
        }
    }

    #[test]
    fn a_report_names_the_panic_and_the_build_but_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_report(dir.path(), &zoned(start()), &report("boom")).unwrap();
        assert_eq!(path.parent().unwrap(), dir.path().join("logs"));
        let text = fs::read_to_string(path).unwrap();
        assert!(text.contains("message: boom"), "{text}");
        assert!(text.contains("location: src/x.rs:1:2"));
        assert!(text.contains("thread: main"));
        assert!(text.contains(&format!("version: {}", env!("CARGO_PKG_VERSION"))));
        assert!(text.contains("os: "));
        assert!(text.contains("backtrace:\nframe"));
        assert!(text.contains("time: 2026-10-18 09:30:45 +00:00"));
    }

    #[test]
    fn two_panics_in_one_second_share_a_file() {
        let dir = tempfile::tempdir().unwrap();
        write_report(dir.path(), &zoned(start()), &report("one")).unwrap();
        write_report(dir.path(), &zoned(start()), &report("two")).unwrap();
        let names = report_names(dir.path());
        assert_eq!(names.len(), 1);
        let text = fs::read_to_string(logs_dir(dir.path()).join(&names[0])).unwrap();
        assert!(text.contains("message: one") && text.contains("message: two"));
    }

    #[test]
    fn the_newest_ten_reports_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let clock = FixedClock::new(start());
        for i in 0..13 {
            write_report(dir.path(), &zoned(clock.now()), &report(&format!("n{i}"))).unwrap();
            clock.set(clock.now() + std::time::Duration::from_mins(1));
        }
        let names = report_names(dir.path());
        assert_eq!(names.len(), MAX_FILES);
        // The first three (09:30, 09:31, 09:32) went; 09:33 is the oldest left.
        assert_eq!(names[0], "crash-2026-10-18-093345.txt");
        assert_eq!(names[9], "crash-2026-10-18-094245.txt");
    }

    #[test]
    fn files_that_are_not_reports_are_left_alone_by_the_pruning() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(logs_dir(dir.path())).unwrap();
        fs::write(logs_dir(dir.path()).join("notes.txt"), "mine").unwrap();
        let clock = FixedClock::new(start());
        for _ in 0..12 {
            write_report(dir.path(), &zoned(clock.now()), &report("x")).unwrap();
            clock.set(clock.now() + std::time::Duration::from_secs(1));
        }
        assert_eq!(report_names(dir.path()).len(), MAX_FILES);
        assert!(logs_dir(dir.path()).join("notes.txt").exists());
    }

    #[test]
    fn a_report_is_new_only_when_newer_than_the_acknowledged_one() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            unacknowledged(dir.path(), None),
            None,
            "no folder, no crash"
        );

        let clock = FixedClock::new(start());
        write_report(dir.path(), &zoned(clock.now()), &report("a")).unwrap();
        let first = "crash-2026-10-18-093045.txt";
        assert_eq!(unacknowledged(dir.path(), None).as_deref(), Some(first));
        assert_eq!(unacknowledged(dir.path(), Some(first)), None);

        clock.set(clock.now() + std::time::Duration::from_secs(5));
        write_report(dir.path(), &zoned(clock.now()), &report("b")).unwrap();
        assert_eq!(
            unacknowledged(dir.path(), Some(first)).as_deref(),
            Some("crash-2026-10-18-093050.txt")
        );
        assert_eq!(
            unacknowledged(dir.path(), Some("crash-2026-10-18-093050.txt")),
            None
        );
    }
}
