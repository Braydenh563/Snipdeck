//! Search folding: "cafe" finds "café", the katakana ネコ finds the hiragana ねこ, "strasse" finds
//! "straße".

use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

/// The kana voiced and semi-voiced sound marks (dakuten, handakuten): they change the letter, so
/// folding keeps them (`か` and `が` stay different).
const KANA_MARKS: [char; 2] = ['\u{3099}', '\u{309A}'];

/// Letters that Unicode normalisation does not take apart but that have a plain ASCII spelling.
fn spelled_out(c: char) -> Option<&'static str> {
    Some(match c {
        'ß' => "ss",
        'œ' => "oe",
        'æ' => "ae",
        'ø' => "o",
        'ł' => "l",
        'đ' | 'ð' => "d",
        'þ' => "th",
        _ => return None,
    })
}

fn fold_with(s: &str, german: bool) -> String {
    let mut spelled = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        match c {
            'ä' if german => spelled.push_str("ae"),
            'ö' if german => spelled.push_str("oe"),
            'ü' if german => spelled.push_str("ue"),
            _ => match spelled_out(c) {
                Some(t) => spelled.push_str(t),
                None => spelled.push(c),
            },
        }
    }
    spelled
        .nfkd()
        .filter(|c| KANA_MARKS.contains(c) || !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            // Katakana letters ァ..ヶ sit exactly 0x60 above their hiragana twins.
            '\u{30A1}'..='\u{30F6}' => char::from_u32(u32::from(c) - 0x60).unwrap_or(c),
            _ => c,
        })
        .collect()
}

/// Lowercase, NFKD with accents removed (so full-width letters become ASCII and `é` becomes `e`),
/// `ß`, `œ` and `æ` spelled out (`ss`, `oe`, `ae`), and katakana mapped to hiragana. The kana
/// voicing marks stay: `か` and `が` are different letters.
#[must_use]
pub fn fold(s: &str) -> String {
    fold_with(s, false)
}

/// [`fold`] with the German transliteration `ä` to `ae`, `ö` to `oe`, `ü` to `ue`, so "laecheln"
/// finds "lächeln". Used as an alternate spelling of every name and keyword.
#[must_use]
pub(crate) fn fold_german(s: &str) -> String {
    fold_with(s, true)
}

/// A word character: a letter or digit, or a combining mark that belongs to the letter before it.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || is_combining_mark(c)
}

/// The words of an already folded string: runs of letters and digits.
pub(crate) fn words(folded: &str) -> impl Iterator<Item = &str> {
    folded
        .split(|c: char| !is_word(c))
        .filter(|w| !w.is_empty())
}

/// True for a letter of a script that is written without spaces (Han, kana, Hangul): a query of one
/// such character is already a word.
pub(crate) fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{3040}'..='\u{30FF}' | '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}'
        | '\u{AC00}'..='\u{D7AF}' | '\u{F900}'..='\u{FAFF}' | '\u{FF66}'..='\u{FF9F}'
        | '\u{20000}'..='\u{2FFFF}')
}

/// `haystack` starts with `needle` and the match does not stop inside a voiced kana: `ひ` does not
/// start `ぴえろ` (ピエロ), which is a different letter.
pub(crate) fn starts_with(haystack: &str, needle: &str) -> bool {
    haystack
        .strip_prefix(needle)
        .is_some_and(|rest| !rest.starts_with(KANA_MARKS))
}

/// [`str::contains`] with the same rule as [`starts_with`].
pub(crate) fn contains(haystack: &str, needle: &str) -> bool {
    haystack
        .match_indices(needle)
        .any(|(i, m)| !haystack[i + m.len()..].starts_with(KANA_MARKS))
}
