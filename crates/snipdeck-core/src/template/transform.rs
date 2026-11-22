//! Paste-time text transforms: the "Paste as..." choices of the item menu.
//!
//! A transform is applied to the text a paste would send (a template is rendered first), and
//! the result is pasted as plain text. Nothing here reads the locale: case mapping is
//! Unicode's default, so `ß` becomes `SS` in upper case and the result can be longer than the
//! text it came from.

use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

/// One "Paste as..." choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Transform {
    /// The text as it is, without any formatting.
    Plain,
    /// `UPPERCASE`.
    Upper,
    /// `lowercase`.
    Lower,
    /// `Title Case`: each whitespace-separated word has its first letter up and the rest down.
    Title,
    /// Without leading and trailing whitespace.
    Trimmed,
    /// `a-url-friendly-slug`: lower case, letters and digits of any script kept, every other
    /// run of characters one `-`, none at the ends.
    Slug,
    /// `Sentence case.`: the text lower case, then the first letter of each sentence up.
    Sentence,
    /// `camelCase`: words joined, the first one in lower case and the others capitalised.
    Camel,
    /// `snake_case`: words in lower case joined by `_`.
    Snake,
    /// `url%20encoded`: every byte but `A-Z a-z 0-9 - . _ ~` as `%XX` (UTF-8).
    UrlEncode,
    /// Only the first whitespace-separated word.
    FirstWord,
    /// Only the last whitespace-separated word.
    LastWord,
}

impl Transform {
    /// Every choice, in the order the menu and the editor help list them.
    pub const ALL: [Self; 12] = [
        Self::Plain,
        Self::Upper,
        Self::Lower,
        Self::Title,
        Self::Trimmed,
        Self::Slug,
        Self::Sentence,
        Self::Camel,
        Self::Snake,
        Self::UrlEncode,
        Self::FirstWord,
        Self::LastWord,
    ];

    /// The name a placeholder uses: `{clipboard|upper}`, `{value:Name|snake}`.
    #[must_use]
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Upper => "upper",
            Self::Lower => "lower",
            Self::Title => "title",
            Self::Trimmed => "trim",
            Self::Slug => "slug",
            Self::Sentence => "sentence",
            Self::Camel => "camel",
            Self::Snake => "snake",
            Self::UrlEncode => "urlencode",
            Self::FirstWord => "firstword",
            Self::LastWord => "lastword",
        }
    }

    /// The transform a placeholder names (ignoring case and spaces), if there is one.
    #[must_use]
    pub fn from_keyword(word: &str) -> Option<Self> {
        let word = word.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|t| t.keyword() == word)
    }

    /// The keywords, comma separated, for messages and the editor help.
    #[must_use]
    pub fn keyword_list() -> String {
        Self::ALL.map(Self::keyword).join(", ")
    }
}

/// `text` with `transform` applied.
#[must_use]
pub fn apply(transform: Transform, text: &str) -> String {
    match transform {
        Transform::Plain => text.to_owned(),
        Transform::Upper => text.to_uppercase(),
        Transform::Lower => text.to_lowercase(),
        Transform::Trimmed => text.trim().to_owned(),
        Transform::Title => title(text),
        Transform::Slug => slug(text),
        Transform::Sentence => sentence(text),
        Transform::Camel => camel(text),
        Transform::Snake => words(text).join("_").to_lowercase(),
        Transform::UrlEncode => url_encode(text),
        Transform::FirstWord => text
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_owned(),
        Transform::LastWord => text
            .split_whitespace()
            .next_back()
            .unwrap_or_default()
            .to_owned(),
    }
}

/// The letter-and-digit runs of `text`, split also where a lower case letter meets an upper
/// case one (`fooBar` is `foo`, `Bar`).
fn words(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut previous_lower = false;
    for c in text.nfc() {
        if c.is_alphanumeric() {
            if c.is_uppercase() && previous_lower && !current.is_empty() {
                out.push(core::mem::take(&mut current));
            }
            previous_lower = c.is_lowercase();
            current.push(c);
        } else {
            previous_lower = false;
            if !current.is_empty() {
                out.push(core::mem::take(&mut current));
            }
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn camel(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, word) in words(text).iter().enumerate() {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            if i == 0 {
                out.extend(first.to_lowercase());
            } else {
                out.extend(first.to_uppercase());
            }
            out.push_str(&chars.as_str().to_lowercase());
        }
    }
    out
}

fn sentence(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start = true;
    for c in text.nfc() {
        if c.is_alphabetic() {
            if start {
                out.extend(c.to_uppercase());
                start = false;
            } else {
                out.extend(c.to_lowercase());
            }
        } else {
            if matches!(c, '.' | '!' | '?' | '。' | '！' | '？') {
                start = true;
            }
            out.push(c);
        }
    }
    out
}

fn url_encode(text: &str) -> String {
    use core::fmt::Write as _;
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            // Writing to a `String` cannot fail.
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

fn title(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word_start = true;
    // NFC first, so a letter and its combining accent are one character.
    for c in text.nfc() {
        if c.is_whitespace() {
            out.push(c);
            word_start = true;
        } else if word_start {
            out.extend(c.to_uppercase());
            word_start = false;
        } else {
            out.extend(c.to_lowercase());
        }
    }
    out
}

fn slug(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_dash = false;
    // NFC first; a mark that stays separate (no precomposed form) still belongs to its word.
    for c in text.nfc() {
        if is_combining_mark(c) {
            if !out.is_empty() && !pending_dash {
                out.push(c);
            }
        } else if c.is_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.extend(c.to_lowercase());
        } else {
            pending_dash = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Transform::*, *};

    #[test]
    fn the_transform_table() {
        for (transform, input, want) in [
            (Plain, "Keep  this\r\n", "Keep  this\r\n"),
            (Upper, "straße", "STRASSE"),
            (Upper, "a\r\nb 👋", "A\r\nB 👋"),
            (Upper, "", ""),
            (Lower, "ÀB Ǆ", "àb ǆ"),
            (Lower, "STRASSE 👋", "strasse 👋"),
            (Title, "hello wORLD", "Hello World"),
            (
                Title,
                "  two  spaces\r\nnext line",
                "  Two  Spaces\r\nNext Line",
            ),
            (Title, "ßa éa 👋x 3rd", "SSa Éa 👋x 3rd"),
            (Title, "hello-world o'neil", "Hello-world O'neil"),
            (Trimmed, "  a b \r\n\t", "a b"),
            (Trimmed, "a\r\n b\r\n", "a\r\n b"),
            (Trimmed, "   ", ""),
            (Slug, "Héllo, Wörld! 👋 ok\r\nnext", "héllo-wörld-ok-next"),
            (Slug, "  --A__b--  ", "a-b"),
            (Slug, "Straße 42", "straße-42"),
            (Slug, "日本語 テキスト", "日本語-テキスト"),
            (Slug, "👋👋", ""),
            (Slug, "Cafe\u{301} au lait", "café-au-lait"),
            (Slug, "Caf\u{e9}", "café"),
            (Slug, "क\u{93f}ताब खोज", "किताब-खोज"),
            (Slug, "\u{301}a", "a"),
            (Title, "cafe\u{301} au lait", "Café Au Lait"),
            (Slug, "", ""),
            (
                Sentence,
                "hELLO there. how ARE you? fine",
                "Hello there. How are you? Fine",
            ),
            (Sentence, "  ünder 3 rules", "  Ünder 3 rules"),
            (Camel, "hello big_world-ok", "helloBigWorldOk"),
            (Camel, "Get HTTP Response", "getHttpResponse"),
            (Camel, "👋", ""),
            (Snake, "Hello Big World", "hello_big_world"),
            (Snake, "fooBar baz", "foo_bar_baz"),
            (Snake, "  --  ", ""),
            (UrlEncode, "a b&c=d/é~._-", "a%20b%26c%3Dd%2F%C3%A9~._-"),
            (UrlEncode, "", ""),
            (FirstWord, "  Ada Lovelace Byron ", "Ada"),
            (LastWord, "  Ada Lovelace Byron ", "Byron"),
            (FirstWord, "   ", ""),
            (LastWord, "", ""),
        ] {
            assert_eq!(apply(transform, input), want, "{transform:?} {input:?}");
        }
    }

    #[test]
    fn every_choice_has_a_keyword_that_finds_it_again() {
        for t in Transform::ALL {
            assert_eq!(Transform::from_keyword(t.keyword()), Some(t));
        }
        assert_eq!(Transform::from_keyword(" UPPER "), Some(Upper));
        assert_eq!(Transform::from_keyword("shout"), None);
    }

    #[test]
    fn every_choice_is_listed_once() {
        for (i, a) in Transform::ALL.iter().enumerate() {
            assert!(!Transform::ALL[..i].contains(a), "{a:?}");
        }
    }
}
