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
        wanted(std::env::var("SNIPDECK_LOG").ok().as_deref())
    })
}

/// One log line: `snipdeck-log +<ms> <what>`.
#[must_use]
pub fn line(elapsed: Duration, what: fmt::Arguments<'_>) -> String {
    format!("snipdeck-log +{}ms {what}", elapsed.as_millis())
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
}
