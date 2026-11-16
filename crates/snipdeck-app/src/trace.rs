//! `SNIPDECK_LOG=1`: a line on stderr for every Panel callback, controller effect, tray and hotkey
//! event and pointer press, so a report from a machine we cannot run (Windows) shows which clicks
//! reached the app and what the app did with them. Off by default.
//!
//! Lines never carry what items contain: rows and chips are logged by index, the search by its
//! length, board and list names not at all.

use std::{
    fmt,
    sync::OnceLock,
    time::{Duration, Instant},
};

/// When the log was first asked about, for the timestamps.
static START: OnceLock<Instant> = OnceLock::new();
/// Whether `SNIPDECK_LOG` asks for the log, read once.
static ON: OnceLock<bool> = OnceLock::new();

/// Whether `value` (the `SNIPDECK_LOG` variable) switches the log on: `1`, `true`, `yes` or `on`.
#[must_use]
pub fn wanted(value: Option<&str>) -> bool {
    value.is_some_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

/// Whether the log is on for this run.
#[must_use]
pub fn enabled() -> bool {
    *ON.get_or_init(|| {
        let _ = START.get_or_init(Instant::now);
        // `SNIPDECK_DIAG` writes its lines here, so it switches the log on too.
        wanted(std::env::var("SNIPDECK_LOG").ok().as_deref()) || crate::diag::get().any()
    })
}

/// `text` with every file path replaced by `<path>`, so an error line such as
/// `backup failed: cannot write C:\Users\Ann\x` never carries a user name or folder (MAIN-61).
/// A word is a path when it starts with a drive letter and a slash, has a backslash, or is a Unix
/// path with a second slash.
#[must_use]
pub fn scrub_paths(text: &str) -> String {
    let is_path = |word: &str| {
        let w = word.trim_matches(|c: char| matches!(c, '"' | '\'' | '(' | ')' | ',' | ';'));
        let bytes = w.as_bytes();
        let drive = bytes.len() > 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/');
        drive || w.contains('\\') || (w.starts_with('/') && w[1..].contains('/'))
    };
    let mut out = String::with_capacity(text.len());
    for (i, word) in text.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if is_path(word) {
            out.push_str("<path>");
        } else {
            out.push_str(word);
        }
    }
    out
}

/// One log line: `snipdeck-log +<ms> <what>`, with file paths scrubbed.
#[must_use]
pub fn line(elapsed: Duration, what: fmt::Arguments<'_>) -> String {
    format!(
        "snipdeck-log +{}ms {}",
        elapsed.as_millis(),
        scrub_paths(&what.to_string())
    )
}

/// Writes one line to stderr when the log is on. Use [`trace!`](crate::trace!).
pub fn write(what: fmt::Arguments<'_>) {
    if !enabled() {
        return;
    }
    let elapsed = START.get_or_init(Instant::now).elapsed();
    #[allow(
        clippy::print_stderr,
        reason = "the opt-in diagnostic log goes to stderr, which the launcher saves to a file"
    )]
    {
        eprintln!("{}", line(elapsed, what));
    }
}

/// `trace!("callback close-requested")`: one diagnostic line when `SNIPDECK_LOG=1`. The
/// arguments are not formatted when the log is off.
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        if $crate::trace::enabled() {
            $crate::trace::write(format_args!($($arg)*));
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_yes_switches_the_log_on() {
        for on in ["1", "true", "YES", " on "] {
            assert!(wanted(Some(on)), "{on:?}");
        }
        for off in ["", "0", "false", "no", "verbose"] {
            assert!(!wanted(Some(off)), "{off:?}");
        }
        assert!(!wanted(None));
    }

    #[test]
    fn a_line_has_a_prefix_and_the_elapsed_milliseconds() {
        assert_eq!(
            line(Duration::from_millis(1250), format_args!("effect {:?}", 3)),
            "snipdeck-log +1250ms effect 3"
        );
    }

    #[test]
    fn a_file_path_in_a_line_is_never_written() {
        assert_eq!(
            scrub_paths(r"backup failed: cannot write C:\Users\Ann\Snipdeck\a.db now"),
            "backup failed: cannot write <path> now"
        );
        assert_eq!(
            scrub_paths("draft not written: \"/home/ann/.local/x.json\" (denied)"),
            "draft not written: <path> (denied)"
        );
        assert_eq!(
            scrub_paths("main window close requested: quit true"),
            "main window close requested: quit true"
        );
        assert_eq!(scrub_paths("callback a/b ok"), "callback a/b ok");
        assert_eq!(
            line(
                Duration::from_millis(5),
                format_args!("open folder: {}", r"D:\data")
            ),
            "snipdeck-log +5ms open folder: <path>"
        );
    }
}
