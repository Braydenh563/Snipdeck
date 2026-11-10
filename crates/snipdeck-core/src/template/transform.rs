//! Paste-time text transforms: the "Paste as..." choices of the item menu.
//!
//! A transform is applied to the text a paste would send (a template is rendered first), and
//! the result is pasted as plain text. Nothing here reads the locale: case mapping is
//! Unicode's default, so `ß` becomes `SS` in upper case and the result can be longer than the
//! text it came from.

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
}

impl Transform {
    /// Every choice, in the order the menu lists them.
    pub const ALL: [Self; 6] = [
        Self::Plain,
        Self::Upper,
        Self::Lower,
        Self::Title,
        Self::Trimmed,
        Self::Slug,
    ];
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
    }
}

fn title(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word_start = true;
    for c in text.chars() {
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
    for c in text.chars() {
        if c.is_alphanumeric() {
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
            (Slug, "", ""),
        ] {
            assert_eq!(apply(transform, input), want, "{transform:?} {input:?}");
        }
    }

    #[test]
    fn every_choice_is_listed_once() {
        for (i, a) in Transform::ALL.iter().enumerate() {
            assert!(!Transform::ALL[..i].contains(a), "{a:?}");
        }
    }
}
