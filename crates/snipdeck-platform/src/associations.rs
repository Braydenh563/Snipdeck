//! The file type and URL scheme Snipdeck registers with Windows, in one table.
//!
//! The installer (`snipdeck.iss`) and the MSIX manifest (`AppxManifest.xml`) must register exactly
//! these values. What the running app does with an argument handed over by the shell is in
//! [`crate::handover`].

/// The archive file extension, with the dot: `.snipdeck` backups and exports.
pub const EXTENSION: &str = ".snipdeck";

/// The extension without the dot.
pub const EXTENSION_BARE: &str = "snipdeck";

/// The registry `ProgID` the extension points to.
pub const PROG_ID: &str = "Snipdeck.Archive";

/// The URL scheme: `snipdeck://open`, `snipdeck://panel`, `snipdeck://settings` (see
/// [`crate::handover::parse_url`]).
pub const URL_SCHEME: &str = "snipdeck";

/// The text the shell's "open" verb and the URL handler run after the quoted executable path: the
/// one argument the shell substitutes (the file or the URL).
pub const OPEN_ARGUMENT: &str = "\"%1\"";

/// The registry command for the "open" verb and for the URL protocol: `"<exe>" "%1"`.
#[must_use]
pub fn open_command(exe: &str) -> String {
    format!("\"{exe}\" {OPEN_ARGUMENT}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_what_the_packages_register() {
        assert_eq!(EXTENSION, ".snipdeck");
        assert_eq!(format!(".{EXTENSION_BARE}"), EXTENSION);
        assert_eq!(PROG_ID, "Snipdeck.Archive");
        assert_eq!(URL_SCHEME, "snipdeck");
    }

    #[test]
    fn the_open_command_quotes_the_path_and_the_argument() {
        assert_eq!(
            open_command(r"C:\Program Files\Snipdeck\snipdeck.exe"),
            r#""C:\Program Files\Snipdeck\snipdeck.exe" "%1""#
        );
    }
}
