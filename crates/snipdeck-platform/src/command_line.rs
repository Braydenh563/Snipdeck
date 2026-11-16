//! The command line stored in the `Run` registry key for "Start with Windows".
//!
//! Portable so the quoting rules are tested on any platform; `windows::autostart` writes it.

/// Why a command line cannot be stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CommandLineError {
    /// The executable path is empty.
    #[error("the executable path is empty")]
    EmptyExe,
    /// The path or arguments contain a character that cannot be stored (NUL, or a `"` in a path:
    /// Windows paths cannot contain one, so quoting the path would be ambiguous).
    #[error("the command line contains a character that cannot be stored")]
    BadCharacter,
    /// Longer than the 260 characters the `Run` key supports.
    #[error("the command line is longer than the {MAX_CHARS} characters the Run key allows")]
    TooLong,
}

/// `Run` key command lines are limited to 260 characters (Microsoft Learn, "Run and `RunOnce`
/// Registry Keys"); longer ones are silently ignored at logon.
pub const MAX_CHARS: usize = 260;

/// `"<exe>"` followed by a space and `args` when `args` is not blank.
///
/// The path is always quoted, so spaces in it are safe. `args` is used as given (the caller owns
/// its quoting) after trimming surrounding whitespace.
///
/// # Errors
///
/// See [`CommandLineError`].
pub fn run_value(exe: &str, args: &str) -> Result<String, CommandLineError> {
    if exe.is_empty() {
        return Err(CommandLineError::EmptyExe);
    }
    if exe.contains(['"', '\0']) || args.contains('\0') {
        return Err(CommandLineError::BadCharacter);
    }
    let args = args.trim();
    let mut value = String::with_capacity(exe.len() + args.len() + 3);
    value.push('"');
    value.push_str(exe);
    value.push('"');
    if !args.is_empty() {
        value.push(' ');
        value.push_str(args);
    }
    // The limit counts UTF-16 units (what is stored), not bytes or scalar values.
    if value.encode_utf16().count() > MAX_CHARS {
        return Err(CommandLineError::TooLong);
    }
    Ok(value)
}

/// What "Start with Windows" amounts to, from the `Run` value and Windows' own switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutostartStatus {
    /// No `Run` value: nothing starts at sign-in.
    Off,
    /// A `Run` value exists and Windows has not switched it off.
    On,
    /// A `Run` value exists but the user switched it off in Task Manager or Settings > Apps >
    /// Startup (`StartupApproved\Run`): it will not run at the next sign-in.
    SwitchedOffInWindows,
}

/// Whether the `StartupApproved\Run` data for our value says "disabled".
///
/// The value is a `REG_BINARY` of 12 bytes: a state word (the first byte: even, `02` / `06`, is
/// enabled; odd, `03`, is disabled) and the time of the change. Anything unreadable (missing or
/// empty) is "not disabled": Windows treats a value without a record as enabled. Format from
/// community documentation, **unverified on Windows**.
#[must_use]
pub fn approval_disables(data: Option<&[u8]>) -> bool {
    data.and_then(|bytes| bytes.first())
        .is_some_and(|b| b & 1 == 1)
}

/// The status given the stored `Run` value (`None` if absent) and the `StartupApproved` data.
#[must_use]
pub fn autostart_status(run_value: Option<&str>, approved: Option<&[u8]>) -> AutostartStatus {
    match run_value {
        None => AutostartStatus::Off,
        Some(_) if approval_disables(approved) => AutostartStatus::SwitchedOffInWindows,
        Some(_) => AutostartStatus::On,
    }
}

/// Whether the stored `Run` value must be rewritten at start-up: the setting is on, a value
/// exists, and it is not the command this process would register (a portable folder that was
/// moved, an old install, an older argument). A value that is absent is left absent: the user
/// removed it (the Settings switch then reads off), and start-up does not bring it back.
/// Windows paths are case-insensitive, so the comparison ignores case.
#[must_use]
pub fn stale_run_value(setting_on: bool, stored: Option<&str>, expected: &str) -> bool {
    setting_on && stored.is_some_and(|stored| !stored.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_the_path_and_appends_args() {
        assert_eq!(
            run_value(r"C:\Program Files\Snipdeck\snipdeck.exe", "--minimized").unwrap(),
            r#""C:\Program Files\Snipdeck\snipdeck.exe" --minimized"#
        );
    }

    #[test]
    fn blank_args_add_nothing() {
        for args in ["", "   ", "\t"] {
            assert_eq!(run_value(r"C:\a.exe", args).unwrap(), r#""C:\a.exe""#);
        }
    }

    #[test]
    fn args_are_trimmed_but_not_otherwise_changed() {
        assert_eq!(
            run_value(r"C:\a.exe", "  --x \"a b\"  ").unwrap(),
            r#""C:\a.exe" --x "a b""#
        );
    }

    #[test]
    fn rejects_unstorable_input() {
        assert_eq!(run_value("", ""), Err(CommandLineError::EmptyExe));
        assert_eq!(
            run_value("C:\\a\"b.exe", ""),
            Err(CommandLineError::BadCharacter)
        );
        assert_eq!(
            run_value("C:\\a\0.exe", ""),
            Err(CommandLineError::BadCharacter)
        );
        assert_eq!(
            run_value("C:\\a.exe", "--x\0"),
            Err(CommandLineError::BadCharacter)
        );
    }

    #[test]
    fn length_limit_counts_utf16_units_of_the_whole_value() {
        // `"` + exe + `"` is exe.len() + 2.
        let exe = format!("C:\\{}", "a".repeat(MAX_CHARS - 2 - 3));
        assert_eq!(run_value(&exe, "").unwrap().len(), MAX_CHARS);
        let exe = format!("C:\\{}", "a".repeat(MAX_CHARS - 2 - 3 + 1));
        assert_eq!(run_value(&exe, ""), Err(CommandLineError::TooLong));
        // An astral character is two UTF-16 units.
        let exe = format!("C:\\{}\u{1F600}", "a".repeat(MAX_CHARS - 2 - 3 - 1));
        assert_eq!(run_value(&exe, ""), Err(CommandLineError::TooLong));
    }

    #[test]
    fn approval_data_says_disabled_by_an_odd_first_byte() {
        let enabled = [0x02, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];
        let enabled_by_user = [0x06, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];
        let disabled = [0x03, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];
        assert!(!approval_disables(Some(&enabled)));
        assert!(!approval_disables(Some(&enabled_by_user)));
        assert!(approval_disables(Some(&disabled)));
        assert!(!approval_disables(Some(&[])), "empty: no record");
        assert!(!approval_disables(None), "no record means enabled");
    }

    #[test]
    fn status_has_three_states() {
        let on = [0x02, 0, 0, 0];
        let off = [0x03, 0, 0, 0];
        assert_eq!(autostart_status(None, None), AutostartStatus::Off);
        assert_eq!(autostart_status(None, Some(&off)), AutostartStatus::Off);
        assert_eq!(autostart_status(Some("x"), None), AutostartStatus::On);
        assert_eq!(autostart_status(Some("x"), Some(&on)), AutostartStatus::On);
        assert_eq!(
            autostart_status(Some("x"), Some(&off)),
            AutostartStatus::SwitchedOffInWindows
        );
    }

    #[test]
    fn a_stale_value_is_rewritten_only_when_the_setting_is_on() {
        let now = r#""C:\new\snipdeck.exe" --hidden"#;
        let old = r#""D:\old\snipdeck.exe" --hidden"#;
        assert!(stale_run_value(true, Some(old), now));
        assert!(!stale_run_value(false, Some(old), now), "setting off");
        assert!(!stale_run_value(true, Some(now), now), "already current");
        assert!(
            !stale_run_value(true, Some(&now.to_uppercase()), now),
            "paths ignore case"
        );
        assert!(!stale_run_value(true, None, now), "the user removed it");
        assert!(
            stale_run_value(true, Some(r#""C:\new\snipdeck.exe""#), now),
            "other arguments (the start-visible choice changed)"
        );
    }
}
