//! Search folding: "cafe" finds "café", the katakana ネコ finds the hiragana ねこ.

use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

/// Lowercase, NFKD with combining marks removed (so full-width letters become ASCII, accents and
/// dakuten drop), and katakana mapped to hiragana.
#[must_use]
pub fn fold(s: &str) -> String {
    s.nfkd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            // Katakana letters ァ..ヶ sit exactly 0x60 above their hiragana twins.
            '\u{30A1}'..='\u{30F6}' => char::from_u32(u32::from(c) - 0x60).unwrap_or(c),
            _ => c,
        })
        .collect()
}

/// The words of an already folded string: runs of letters and digits.
pub(crate) fn words(folded: &str) -> impl Iterator<Item = &str> {
    folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
}
