//! Reading the Windows clipboard history (Win+V) for the "import" in Settings.
//!
//! The decisions are portable and unit-tested here: converting a `WinRT` `DateTime` to a
//! [`Timestamp`], and which history texts are worth importing. The `WinRT` calls are in
//! `windows::clipboard_history` (`cfg(windows)`); off Windows [`read_current_session`] reports
//! [`HistoryRead::Unsupported`].
//!
//! Only what Windows still holds for this sign-in session is available (it is cleared at
//! sign-out and by the user), and only text entries are read. Pinned entries are included.
//!
//! **Unverified:** the Windows side has never run. It compiles against the `windows` crate only;
//! the owner must run it on Windows 10 1809+ / 11 with history on and off (see the Windows test
//! wizard) before this is believed.

use snipdeck_core::Timestamp;

use crate::error::Error;

/// One text entry of the clipboard history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardHistoryEntry {
    /// The text, unchanged.
    pub text: String,
    /// When Windows recorded the copy, if the value is a valid time.
    pub timestamp: Option<Timestamp>,
}

/// What reading the history produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryRead {
    /// The text entries, in the order Windows lists them (newest first). May be empty.
    Entries(Vec<ClipboardHistoryEntry>),
    /// The user has clipboard history off. The UI tells them to turn it on in
    /// Settings > System > Clipboard.
    HistoryDisabled,
    /// Windows refused access to the history (for example by policy).
    AccessDenied,
    /// This platform has no clipboard history to read.
    Unsupported,
}

/// Reads the text entries of the current session's clipboard history.
///
/// Blocks until Windows answers, so call it off the UI thread.
///
/// # Errors
///
/// [`Error::Os`] when a `WinRT` call fails for a reason other than the history being off or
/// access denied.
#[cfg(windows)]
pub fn read_current_session() -> Result<HistoryRead, Error> {
    crate::windows::clipboard_history::read_current_session()
}

/// Reads the text entries of the current session's clipboard history.
///
/// There is no clipboard history off Windows: this always returns [`HistoryRead::Unsupported`].
///
/// # Errors
///
/// Never, off Windows.
#[cfg(not(windows))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "same signature as the Windows implementation"
)]
pub fn read_current_session() -> Result<HistoryRead, Error> {
    Ok(HistoryRead::Unsupported)
}

/// 100-nanosecond ticks between 1601-01-01 (the `WinRT` / FILETIME epoch) and 1970-01-01.
const TICKS_AT_UNIX_EPOCH: i64 = 116_444_736_000_000_000;
const TICKS_PER_SECOND: i64 = 10_000_000;

/// A `WinRT` `DateTime.UniversalTime` (ticks since 1601-01-01 UTC) as a [`Timestamp`], rounded
/// down to the second. `None` for zero (unset) or a value outside the representable range.
#[must_use]
pub fn timestamp_from_ticks(ticks: i64) -> Option<Timestamp> {
    if ticks <= 0 {
        return None;
    }
    let seconds = ticks
        .checked_sub(TICKS_AT_UNIX_EPOCH)?
        .div_euclid(TICKS_PER_SECOND);
    Timestamp::from_second(seconds).ok()
}

/// Whether a history text is worth importing: it has something other than whitespace.
#[must_use]
pub fn keep_text(text: &str) -> bool {
    !text.trim().is_empty()
}

/// Builds an entry from a history item's text and ticks, or `None` when the text is not kept.
#[must_use]
pub fn entry_from(text: String, ticks: i64) -> Option<ClipboardHistoryEntry> {
    keep_text(&text).then(|| ClipboardHistoryEntry {
        text,
        timestamp: timestamp_from_ticks(ticks),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_convert_to_unix_seconds() {
        // 2021-01-01T00:00:00Z is 1_609_459_200 s after 1970.
        let ticks = TICKS_AT_UNIX_EPOCH + 1_609_459_200 * TICKS_PER_SECOND + 9_999_999;
        assert_eq!(
            timestamp_from_ticks(ticks),
            Some(Timestamp::from_second(1_609_459_200).unwrap())
        );
        assert_eq!(
            timestamp_from_ticks(TICKS_AT_UNIX_EPOCH),
            Some(Timestamp::from_second(0).unwrap())
        );
    }

    #[test]
    fn times_before_1970_round_down() {
        let ticks = TICKS_AT_UNIX_EPOCH - 1;
        assert_eq!(
            timestamp_from_ticks(ticks),
            Some(Timestamp::from_second(-1).unwrap())
        );
    }

    #[test]
    fn unset_and_out_of_range_ticks_give_none() {
        assert_eq!(timestamp_from_ticks(0), None);
        assert_eq!(timestamp_from_ticks(-5), None);
        assert_eq!(timestamp_from_ticks(i64::MAX), None);
    }

    #[test]
    fn blank_text_is_dropped_and_other_text_is_kept_verbatim() {
        assert!(entry_from(" \r\n\t".to_owned(), 1).is_none());
        assert!(entry_from(String::new(), 1).is_none());
        let e = entry_from("  keep me \n".to_owned(), 0).unwrap();
        assert_eq!(e.text, "  keep me \n");
        assert_eq!(e.timestamp, None);
    }

    #[cfg(not(windows))]
    #[test]
    fn off_windows_the_history_is_unsupported() {
        assert_eq!(read_current_session(), Ok(HistoryRead::Unsupported));
    }
}
