//! The licence and third-party notices that ship inside the program.
//!
//! The GPL text, the font licence (SIL OFL 1.1, which requires its notice in every copy), the
//! symbol data licences and every linked crate's licence are compiled into the exe, so every
//! package (installer, zip, Store) carries them without a copy step that could be forgotten.
//! Help > About writes them to a `.txt` file (a stock Windows opens `.txt`, not `.md`) and opens it.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// The licence of Snipdeck itself.
pub const LICENSE: &str = include_str!("../../../LICENSE");
/// The notices for everything Snipdeck bundles or links.
pub const THIRD_PARTY: &str = include_str!("../../../THIRD_PARTY_NOTICES.md");
/// The name of the file written for the user.
pub const FILE_NAME: &str = "Snipdeck licence and third-party notices.txt";

/// The text of the file: the licence, then the third-party notices.
#[must_use]
pub fn text() -> String {
    format!("{LICENSE}\n\n{}\n\n{THIRD_PARTY}", "=".repeat(78))
}

/// Writes the file into `dir` (created if needed) unless it already holds this text, and returns
/// its path.
///
/// # Errors
/// The folder or file cannot be written.
pub fn write(dir: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join(FILE_NAME);
    let text = text();
    if fs::read_to_string(&path).is_ok_and(|have| have == text) {
        return Ok(path);
    }
    fs::write(&path, text)?;
    Ok(path)
}

/// Where [`write`] puts the file for the running program: a `Snipdeck` folder in the temporary
/// folder (the notices are not user data, so the data folder stays theirs).
#[must_use]
pub fn default_dir() -> PathBuf {
    std::env::temp_dir().join("Snipdeck")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_carries_the_gpl_the_font_licence_and_the_symbol_data_licences() {
        let text = text();
        assert!(text.contains("GNU GENERAL PUBLIC LICENSE"));
        assert!(
            text.contains("SIL OPEN FONT LICENSE Version 1.1"),
            "OFL for the IBM Plex fonts"
        );
        assert!(text.contains("IBM Corp."));
        assert!(text.contains("Microsoft Fluent Emoji") && text.contains("Phosphor"));
        assert!(text.contains("Unicode License v3"));
        assert!(text.contains("## Rust crates (including Slint)"));
    }

    #[test]
    fn writing_twice_gives_one_unchanged_file() {
        let dir = tempfile::tempdir().expect("dir");
        let first = write(dir.path()).expect("write");
        assert_eq!(first.extension().and_then(|e| e.to_str()), Some("txt"));
        let stamp = fs::metadata(&first)
            .expect("meta")
            .modified()
            .expect("time");
        let second = write(&dir.path().join(".")).expect("again");
        assert_eq!(
            fs::metadata(&second)
                .expect("meta")
                .modified()
                .expect("time"),
            stamp
        );
        assert_eq!(fs::read_to_string(first).expect("read"), text());
    }
}
