//! The user's regional format: how this PC writes a short date and a short time (Settings,
//! Time & language, Region). Templates take the numeric order and the clock style from it
//! instead of from the interface language (a German Windows with English (UK) formats writes
//! `17/10/2026`, not `17.10.2026`).
//!
//! The patterns are Windows' own (`dd/MM/yyyy`, `h:mm tt`); `snipdeck_core::template::Locale::with_regional`
//! reads them. Only the decoding of the answer is portable; the call itself is
//! [`windows::regional`](crate::windows). Off Windows [`read`] returns [`RegionalPatterns::NONE`], and
//! the template engine keeps the style of the language table.
//!
//! **Unverified on Windows:** `GetLocaleInfoEx` has never run. The owner must check on a PC whose
//! region format differs from its display language (`tools/windows-test`).

/// The short date and short time patterns of the user's regional format, as Windows writes
/// them. A pattern that could not be read is `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RegionalPatterns {
    /// `LOCALE_SSHORTDATE`, such as `dd/MM/yyyy`.
    pub short_date: Option<String>,
    /// `LOCALE_SSHORTTIME`, such as `HH:mm` or `h:mm tt`.
    pub short_time: Option<String>,
}

impl RegionalPatterns {
    /// Nothing known: the language table decides.
    pub const NONE: Self = Self {
        short_date: None,
        short_time: None,
    };
}

/// The text `GetLocaleInfoEx` wrote to `buffer` when it returned `written` (the length with the
/// terminating zero, or 0 for a failure). `None` for a failure or an empty answer.
#[must_use]
pub fn pattern_from_utf16(buffer: &[u16], written: i32) -> Option<String> {
    let len = usize::try_from(written).ok().filter(|n| *n > 1)?;
    let units = buffer.get(..len)?;
    let units = units.strip_suffix(&[0]).unwrap_or(units);
    let text = String::from_utf16(units).ok()?;
    let text = text.trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// The user's regional short date and time patterns.
#[must_use]
pub fn read() -> RegionalPatterns {
    #[cfg(windows)]
    {
        crate::windows::regional::read()
    }
    #[cfg(not(windows))]
    {
        RegionalPatterns::NONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(text: &str) -> Vec<u16> {
        text.encode_utf16().chain([0]).collect()
    }

    #[test]
    fn the_answer_is_the_text_before_the_terminator() {
        let mut buffer = utf16("dd/MM/yyyy");
        let written = i32::try_from(buffer.len()).unwrap();
        buffer.resize(80, 0xFFFF);
        assert_eq!(
            pattern_from_utf16(&buffer, written).as_deref(),
            Some("dd/MM/yyyy")
        );
    }

    #[test]
    fn a_failure_or_an_empty_answer_is_nothing() {
        assert_eq!(pattern_from_utf16(&[0; 8], 0), None);
        assert_eq!(pattern_from_utf16(&[0; 8], -1), None);
        assert_eq!(pattern_from_utf16(&utf16(""), 1), None);
        assert_eq!(pattern_from_utf16(&utf16("   "), 4), None);
        assert_eq!(
            pattern_from_utf16(&utf16("HH:mm"), 99),
            None,
            "past the buffer"
        );
        assert_eq!(
            pattern_from_utf16(&[0xD800, 0], 2),
            None,
            "a broken surrogate"
        );
    }

    #[test]
    fn off_windows_nothing_is_known() {
        #[cfg(not(windows))]
        assert_eq!(read(), RegionalPatterns::NONE);
    }
}
