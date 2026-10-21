//! Making items from what the user pasted or dropped: what kind each thing becomes, and whether
//! text looks like a secret.
//!
//! Pure functions, so the rules are tested here and the Windows clipboard and drop code only
//! has to hand over what it found. The controller turns the pieces into items
//! (`controller::create_ctl`).

use std::path::PathBuf;

use snipdeck_core::ItemKind;

/// What the clipboard or a drop held, as far as Snipdeck reads it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Clip {
    /// Plain text (a browser link arrives as its address).
    pub text: Option<String>,
    /// A PNG picture (a screenshot, a copied image, a dropped picture from a browser).
    pub png: Option<Vec<u8>>,
    /// Files and folders from Explorer.
    pub files: Vec<PathBuf>,
}

impl Clip {
    /// A clip of just this text.
    #[must_use]
    pub fn of_text(text: &str) -> Self {
        Self {
            text: Some(text.to_owned()),
            ..Self::default()
        }
    }
}

/// One item to make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    /// Text, or a link when the text is one web address.
    Text {
        /// [`ItemKind::Text`] or [`ItemKind::Link`].
        kind: ItemKind,
        /// The text.
        text: String,
    },
    /// A PNG picture.
    Image(Vec<u8>),
    /// A file to read from disk (a PNG file becomes an Image item, any other a File item).
    File(PathBuf),
}

/// Why text is held back for the user to look at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Secret {
    /// A private key block.
    PrivateKey,
    /// A token or key with a well-known prefix (cloud keys, GitHub tokens, JSON web tokens).
    Token,
    /// A line that says `password: ...` or similar.
    Password,
    /// One long run of mixed letters and digits with no spaces.
    LooksRandom,
}

/// What to make from `clip`, in order. Files win over a picture and a picture over text, as
/// Explorer and browsers put a thumbnail or a name next to the real thing. Empty when there is
/// nothing to make (blank text counts as nothing).
#[must_use]
pub fn classify(clip: &Clip) -> Vec<Piece> {
    if !clip.files.is_empty() {
        return clip.files.iter().cloned().map(Piece::File).collect();
    }
    if let Some(png) = clip.png.as_ref().filter(|p| !p.is_empty()) {
        return vec![Piece::Image(png.clone())];
    }
    match clip.text.as_deref().filter(|t| !t.trim().is_empty()) {
        Some(text) => vec![Piece::Text {
            kind: kind_of_text(text),
            text: text.to_owned(),
        }],
        None => Vec::new(),
    }
}

/// [`ItemKind::Link`] for one `http` or `https` address with nothing else, otherwise Text.
#[must_use]
pub fn kind_of_text(text: &str) -> ItemKind {
    let t = text.trim();
    let web = t.starts_with("http://") || t.starts_with("https://");
    if web && !t.contains(char::is_whitespace) && t.len() > "https://".len() {
        ItemKind::Link
    } else {
        ItemKind::Text
    }
}

/// Whether `text` looks like a secret Snipdeck should not save without asking. Deliberately
/// cautious about what it flags: a wrong "yes" costs one extra click, a wrong "no" saves a
/// password in plain sight.
#[must_use]
pub fn looks_secret(text: &str) -> Option<Secret> {
    let t = text.trim();
    if t.contains("-----BEGIN") && t.contains("PRIVATE KEY") {
        return Some(Secret::PrivateKey);
    }
    if has_token(t) {
        return Some(Secret::Token);
    }
    if says_password(t) {
        return Some(Secret::Password);
    }
    if kind_of_text(t) == ItemKind::Text && looks_random(t) {
        return Some(Secret::LooksRandom);
    }
    None
}

fn has_token(t: &str) -> bool {
    t.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '=' | ':' | ',')).any(|w| {
        let long = |prefix: &str, min: usize| w.starts_with(prefix) && w.len() >= min;
        long("AKIA", 20)
            || long("ghp_", 30)
            || long("gho_", 30)
            || long("github_pat_", 30)
            || long("sk-", 24)
            || long("xoxb-", 20)
            || long("xoxp-", 20)
            || is_jwt(w)
    })
}

fn is_jwt(w: &str) -> bool {
    let mut parts = w.split('.');
    let (Some(a), Some(b), Some(c), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    a.starts_with("eyJ") && b.starts_with("eyJ") && c.len() >= 8
}

fn says_password(t: &str) -> bool {
    t.lines().any(|line| {
        let l = line.trim().to_ascii_lowercase();
        ["password", "passwd", "pwd", "secret", "api key", "api_key", "apikey", "token"]
            .iter()
            .any(|word| {
                l.strip_prefix(word)
                    .is_some_and(|rest| rest.trim_start().starts_with([':', '=']))
            })
    })
}

fn looks_random(t: &str) -> bool {
    t.len() >= 20
        && !t.contains(char::is_whitespace)
        && t.chars().all(|c| c.is_ascii_alphanumeric() || "-_+/=!@#$%^&*".contains(c))
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().any(|c| c.is_ascii_lowercase())
        && t.chars().any(|c| c.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(t: &str) -> Vec<Piece> {
        classify(&Clip::of_text(t))
    }

    fn kind(pieces: &[Piece]) -> Option<ItemKind> {
        match pieces {
            [Piece::Text { kind, .. }] => Some(*kind),
            _ => None,
        }
    }

    #[test]
    fn the_kind_detection_table() {
        let png = vec![0x89, b'P', b'N', b'G'];
        let file = |p: &str| PathBuf::from(p);
        // (what was on the clipboard, what becomes of it)
        let table: Vec<(Clip, Vec<Piece>)> = vec![
            (Clip::default(), vec![]),
            (Clip::of_text(""), vec![]),
            (Clip::of_text("  \n "), vec![]),
            (
                Clip::of_text("hello there"),
                vec![Piece::Text {
                    kind: ItemKind::Text,
                    text: "hello there".into(),
                }],
            ),
            (
                Clip::of_text("https://example.com/a?b=1"),
                vec![Piece::Text {
                    kind: ItemKind::Link,
                    text: "https://example.com/a?b=1".into(),
                }],
            ),
            (
                Clip::of_text("  http://example.com  "),
                vec![Piece::Text {
                    kind: ItemKind::Link,
                    text: "  http://example.com  ".into(),
                }],
            ),
            (
                Clip::of_text("see https://example.com now"),
                vec![Piece::Text {
                    kind: ItemKind::Text,
                    text: "see https://example.com now".into(),
                }],
            ),
            (
                Clip::of_text("https://a.com https://b.com"),
                vec![Piece::Text {
                    kind: ItemKind::Text,
                    text: "https://a.com https://b.com".into(),
                }],
            ),
            (
                Clip::of_text("https://"),
                vec![Piece::Text {
                    kind: ItemKind::Text,
                    text: "https://".into(),
                }],
            ),
            (
                Clip {
                    png: Some(png.clone()),
                    ..Clip::default()
                },
                vec![Piece::Image(png.clone())],
            ),
            (
                Clip {
                    png: Some(png.clone()),
                    text: Some("a name".into()),
                    ..Clip::default()
                },
                vec![Piece::Image(png.clone())],
            ),
            (
                Clip {
                    files: vec![file("a.txt"), file("b.png")],
                    ..Clip::default()
                },
                vec![Piece::File(file("a.txt")), Piece::File(file("b.png"))],
            ),
            (
                Clip {
                    files: vec![file("a.txt")],
                    png: Some(png),
                    text: Some("a.txt".into()),
                },
                vec![Piece::File(file("a.txt"))],
            ),
        ];
        for (clip, want) in table {
            assert_eq!(classify(&clip), want, "for {clip:?}");
        }
    }

    #[test]
    fn a_single_address_is_a_link_and_prose_is_not() {
        assert_eq!(kind(&text("https://example.com")), Some(ItemKind::Link));
        assert_eq!(kind(&text("ftp://example.com")), Some(ItemKind::Text));
        assert_eq!(kind(&text("example.com")), Some(ItemKind::Text));
    }

    #[test]
    fn secrets_are_flagged_and_ordinary_text_is_not() {
        let table = [
            ("-----BEGIN RSA PRIVATE KEY-----\nabc\n-----END RSA PRIVATE KEY-----", Some(Secret::PrivateKey)),
            ("AKIAIOSFODNN7EXAMPLE", Some(Secret::Token)),
            ("ghp_abcdefghijklmnopqrstuvwxyz0123456789", Some(Secret::Token)),
            ("export KEY=sk-abcdefghijklmnopqrstuvwxyz", Some(Secret::Token)),
            ("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.abcdefghij", Some(Secret::Token)),
            ("password: hunter2", Some(Secret::Password)),
            ("user: me\nPassword = x", Some(Secret::Password)),
            ("Zk3pQ9vL2mX8wR5tY1nB7", Some(Secret::LooksRandom)),
            ("Hello, how are you today?", None),
            ("https://example.com/Zk3pQ9vL2mX8wR5tY1nB7", None),
            ("The password reset page is at the bottom", None),
            ("supercalifragilisticexpialidocious", None),
            ("1234567890123456789012", None),
            ("", None),
        ];
        for (input, want) in table {
            assert_eq!(looks_secret(input), want, "for {input:?}");
        }
    }
}
