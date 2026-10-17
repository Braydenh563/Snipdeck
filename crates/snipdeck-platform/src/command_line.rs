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
}
