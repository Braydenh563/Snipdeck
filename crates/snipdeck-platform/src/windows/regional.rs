//! The user's regional short date and short time patterns, from `GetLocaleInfoEx` for the user's
//! default locale (`LOCALE_SSHORTDATE`, `LOCALE_SSHORTTIME`). See [`crate::regional`].
//!
//! **Unverified on Windows:** compiled only; the owner runs it on a PC whose region format
//! differs from its display language.

use windows::Win32::Globalization::{GetLocaleInfoEx, LOCALE_SSHORTDATE, LOCALE_SSHORTTIME};
use windows::core::PCWSTR;

use crate::regional::{RegionalPatterns, pattern_from_utf16};

/// The longest answer asked for: a pattern is under 80 units.
const MAX_UNITS: usize = 85;

fn info(kind: u32) -> Option<String> {
    let mut buffer = [0u16; MAX_UNITS];
    // SAFETY: `buffer` is writable for `MAX_UNITS` units and the call writes at most that many;
    // a null locale name means the user's default locale.
    let written = unsafe { GetLocaleInfoEx(PCWSTR::null(), kind, Some(&mut buffer)) };
    pattern_from_utf16(&buffer, written)
}

/// Reads both patterns; one that cannot be read is `None`.
pub fn read() -> RegionalPatterns {
    RegionalPatterns {
        short_date: info(LOCALE_SSHORTDATE),
        short_time: info(LOCALE_SSHORTTIME),
    }
}
