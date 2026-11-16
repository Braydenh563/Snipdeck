//! The warning log: `logs/snipdeck.log` in the data folder. A release Windows build has no
//! stderr, so every warning the app used to print there goes through [`warn!`](crate::warn!)
//! instead: one line per event, written to this file and to stderr.
//!
//! Lines never carry what items contain (the same rule as [`mod@crate::trace`]): callers pass
//! error texts and counts, not names, titles or snippet text. The file is capped: when it passes
//! [`MAX_BYTES`] it becomes `snipdeck.log.1` (replacing the older one), so at most two files
//! of that size exist.

use std::{
    fmt::Write as _,
    fs::{self, OpenOptions},
    io::{self, Write as _},
    path::{Path, PathBuf},
    sync::Mutex,
};

use jiff::Timestamp;

/// The log's file name inside [`crate::crash::LOG_DIR`].
pub const FILE: &str = "snipdeck.log";
/// The rolled-over file's name.
pub const OLD_FILE: &str = "snipdeck.log.1";
/// The size at which the file rolls over.
pub const MAX_BYTES: u64 = 128 * 1024;
/// Lines the Troubleshoot report carries.
pub const REPORT_LINES: usize = 20;
/// The longest line written, in characters (a longer one is cut).
const MAX_LINE: usize = 400;

/// Where the log is written, once the data folder is known.
static DATA_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Tells the log where to write. Call it once the data folder is known; until then lines go to
/// stderr only.
pub fn set_data_dir(data_dir: &Path) {
    if let Ok(mut slot) = DATA_DIR.lock() {
        *slot = Some(data_dir.to_path_buf());
    }
}

/// The log file's path inside `data_dir`.
#[must_use]
pub fn path(data_dir: &Path) -> PathBuf {
    crate::crash::logs_dir(data_dir).join(FILE)
}

/// One log line: `<UTC time> <message>`, on one line and cut to `MAX_LINE` characters.
#[must_use]
pub fn format_line(at: Timestamp, message: &str) -> String {
    let flat: String = message
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MAX_LINE)
        .collect();
    let mut line = String::new();
    let _ = write!(line, "{at} {flat}");
    line
}

/// Appends `line` to the log in `data_dir`, rolling the file over first when it is full.
///
/// # Errors
/// Any I/O error; the caller has nowhere better to say it, so [`write()`] drops it.
pub fn append(data_dir: &Path, line: &str, max_bytes: u64) -> io::Result<()> {
    let file = path(data_dir);
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    if fs::metadata(&file).is_ok_and(|m| m.len() >= max_bytes) {
        // `rename` replaces the older copy on Windows too (std uses MoveFileExW with replace).
        fs::rename(&file, file.with_file_name(OLD_FILE))?;
    }
    let mut out = OpenOptions::new().create(true).append(true).open(&file)?;
    writeln!(out, "{line}")
}

/// The last `count` lines of the log (the rolled-over file fills in when the log is short), oldest
/// first. An unreadable or missing log is no lines.
#[must_use]
pub fn tail(data_dir: &Path, count: usize) -> Vec<String> {
    let file = path(data_dir);
    let mut lines: Vec<String> = Vec::new();
    for name in [file.with_file_name(OLD_FILE), file] {
        if let Ok(text) = fs::read_to_string(name) {
            lines.extend(text.lines().map(str::to_owned));
        }
    }
    let skip = lines.len().saturating_sub(count);
    lines.split_off(skip)
}

/// Writes one warning with the time `at`: to the file when the data folder is known, and to
/// stderr (the launcher and `cargo run` show it). Use [`warn!`](crate::warn!).
pub fn write(at: Timestamp, message: &str) {
    let line = format_line(at, message);
    if let Ok(slot) = DATA_DIR.lock()
        && let Some(dir) = slot.as_ref()
    {
        let _ = append(dir, &line, MAX_BYTES);
    }
    #[allow(
        clippy::print_stderr,
        reason = "stderr is the log's second sink, for the console and the launcher"
    )]
    {
        eprintln!("snipdeck: {message}");
    }
}

/// `warn!("cannot open the help: {e}")`: one line in `logs/snipdeck.log` and on stderr.
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::log::write(::jiff::Timestamp::now(), &format!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: i64) -> Timestamp {
        Timestamp::from_second(1_760_000_000 + secs).unwrap()
    }

    #[test]
    fn a_line_is_the_utc_time_and_one_line_of_text() {
        assert_eq!(
            format_line(at(0), "tray failed:\nbad\tthing"),
            "2025-10-09T08:53:20Z tray failed: bad thing"
        );
    }

    #[test]
    fn a_long_message_is_cut() {
        let line = format_line(at(0), &"x".repeat(1000));
        assert_eq!(
            line.chars().count(),
            "2025-10-09T08:53:20Z ".len() + MAX_LINE
        );
    }

    #[test]
    fn lines_append_and_the_tail_returns_the_newest_in_order() {
        let dir = tempfile::tempdir().unwrap();
        for n in 0..5 {
            append(dir.path(), &format!("line {n}"), MAX_BYTES).unwrap();
        }
        assert_eq!(tail(dir.path(), 2), ["line 3", "line 4"]);
        assert_eq!(tail(dir.path(), 99).len(), 5);
    }

    #[test]
    fn a_missing_log_is_no_lines() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(tail(dir.path(), 5), Vec::<String>::new());
    }

    #[test]
    fn a_full_file_rolls_over_and_only_two_files_exist() {
        let dir = tempfile::tempdir().unwrap();
        for n in 0..40 {
            append(dir.path(), &format!("line {n:02}"), 50).unwrap();
        }
        let names: Vec<_> = fs::read_dir(crate::crash::logs_dir(dir.path()))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names.len(), 2, "{names:?}");
        let tail = tail(dir.path(), 3);
        assert_eq!(tail.last().map(String::as_str), Some("line 39"));
        assert_eq!(tail.len(), 3, "the tail spans the rolled file");
    }
}
