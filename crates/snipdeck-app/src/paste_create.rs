//! Making items from what the user pasted (Ctrl+V on a list) or dropped (files from Explorer):
//! which pieces there are, what kind a text becomes, and whether it looks like a secret.
//!
//! Pure functions, so the rules are tested here. The controller makes the items
//! (`controller::create_ctl`).

use std::path::PathBuf;

use snipdeck_core::ItemKind;

/// What the clipboard or a drop held, as far as Snipdeck reads it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Clip {
    /// Plain text (a browser link arrives as its address).
    pub text: Option<String>,
    /// A PNG picture (a screenshot, a copied image).
    pub png: Option<Vec<u8>>,
    /// Files from Explorer.
    pub files: Vec<PathBuf>,
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

/// What to make from `clip`, in order. Files win over text and text over a picture: Explorer
/// puts a file name next to the files, and spreadsheets and word processors put a bitmap next to
/// copied text, but nothing puts text next to a screenshot. Empty when there is nothing to make
/// (blank text counts as nothing).
#[must_use]
pub fn classify(clip: &Clip) -> Vec<Piece> {
    if !clip.files.is_empty() {
        return clip.files.iter().cloned().map(Piece::File).collect();
    }
    if let Some(text) = clip.text.as_deref().filter(|t| !t.trim().is_empty()) {
        return vec![Piece::Text {
            kind: kind_of_text(text),
            text: text.to_owned(),
        }];
    }
    match clip.png.as_ref().filter(|p| !p.is_empty()) {
        Some(png) => vec![Piece::Image(png.clone())],
        None => Vec::new(),
    }
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
    if says_password(t) || has_secret_assignment(t) || has_secret_json_key(t) {
        return Some(Secret::Password);
    }
    if kind_of_text(t) == ItemKind::Text && looks_random(t) {
        return Some(Secret::LooksRandom);
    }
    None
}

fn has_token(t: &str) -> bool {
    t.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '=' | ':' | ','))
        .any(|w| {
            let long = |prefix: &str, min: usize| w.starts_with(prefix) && w.len() >= min;
            long("AKIA", 20)
                || long("ghp_", 30)
                || long("gho_", 30)
                || long("github_pat_", 30)
                || long("sk-", 24)
                || long("xoxb-", 20)
                || long("xoxp-", 20)
                || long("sk_live_", 20)
                || long("sk_test_", 20)
                || long("rk_live_", 20)
                || long("glpat-", 20)
                || long("AIza", 39)
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
        [
            "password", "passwd", "pwd", "secret", "api key", "api_key", "apikey", "token",
        ]
        .iter()
        .any(|word| {
            l.strip_prefix(word)
                .is_some_and(|rest| rest.trim_start().starts_with([':', '=']))
        })
    })
}

/// Whether a name says it holds a secret.
fn secret_name(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    ["PASSWORD", "PASSWD", "SECRET", "TOKEN", "API_KEY", "APIKEY"]
        .iter()
        .any(|word| n.contains(word))
}

/// A `.env` style line: `KEY=value` (with an optional `export `) whose key is one plain name
/// that says it holds a secret and whose value is not empty.
fn has_secret_assignment(t: &str) -> bool {
    t.lines().any(|line| {
        let l = line.trim();
        let l = l.strip_prefix("export ").unwrap_or(l).trim_start();
        let Some((key, value)) = l.split_once('=') else {
            return false;
        };
        let key = key.trim();
        !value.trim().is_empty()
            && !key.is_empty()
            && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && secret_name(key)
    })
}

/// A JSON member such as `"password": "x"`.
fn has_secret_json_key(t: &str) -> bool {
    t.lines().any(|line| {
        let parts: Vec<&str> = line.split('"').collect();
        parts.iter().enumerate().any(|(i, key)| {
            i % 2 == 1
                && secret_name(key)
                && parts.get(i + 1).is_some_and(|after| {
                    after
                        .trim_start()
                        .strip_prefix(':')
                        .is_some_and(|value| !value.trim().is_empty() || parts.get(i + 2).is_some())
                })
        })
    })
}

fn looks_random(t: &str) -> bool {
    t.len() >= 20
        && !t.contains(char::is_whitespace)
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_+/=!@#$%^&*".contains(c))
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().any(|c| c.is_ascii_lowercase())
        && t.chars().any(|c| c.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_prefers_files_then_text_then_a_picture() {
        let clip = Clip {
            text: Some("name.txt".into()),
            png: Some(vec![1]),
            files: vec![PathBuf::from("a"), PathBuf::from("b")],
        };
        assert_eq!(
            classify(&clip),
            [Piece::File("a".into()), Piece::File("b".into())]
        );
        let clip = Clip {
            files: Vec::new(),
            ..clip
        };
        assert_eq!(
            classify(&clip),
            [Piece::Text {
                kind: ItemKind::Text,
                text: "name.txt".into()
            }]
        );
        let clip = Clip {
            text: Some("  \n".into()),
            ..clip
        };
        assert_eq!(classify(&clip), [Piece::Image(vec![1])]);
        assert_eq!(classify(&Clip::default()), Vec::<Piece>::new());
        let clip = Clip {
            png: Some(Vec::new()),
            ..Clip::default()
        };
        assert_eq!(classify(&clip), Vec::<Piece>::new());
    }

    #[test]
    fn the_kind_detection_table() {
        let table = [
            ("hello there", ItemKind::Text),
            ("", ItemKind::Text),
            ("https://example.com/a?b=1", ItemKind::Link),
            ("  http://example.com  ", ItemKind::Link),
            ("https://example.com\n", ItemKind::Link),
            ("see https://example.com now", ItemKind::Text),
            ("https://a.com https://b.com", ItemKind::Text),
            ("https://", ItemKind::Text),
            ("ftp://example.com", ItemKind::Text),
        ];
        for (input, want) in table {
            assert_eq!(kind_of_text(input), want, "for {input:?}");
        }
    }

    #[test]
    fn a_single_address_is_a_link_and_prose_is_not() {
        assert_eq!(kind_of_text("https://example.com"), ItemKind::Link);
        assert_eq!(kind_of_text("example.com"), ItemKind::Text);
        assert_eq!(kind_of_text("open https://example.com"), ItemKind::Text);
    }

    #[test]
    fn secrets_are_flagged_and_ordinary_text_is_not() {
        let table = [
            (
                "-----BEGIN RSA PRIVATE KEY-----\nabc\n-----END RSA PRIVATE KEY-----",
                Some(Secret::PrivateKey),
            ),
            ("AKIAIOSFODNN7EXAMPLE", Some(Secret::Token)),
            (
                "ghp_abcdefghijklmnopqrstuvwxyz0123456789",
                Some(Secret::Token),
            ),
            (
                "export KEY=sk-abcdefghijklmnopqrstuvwxyz",
                Some(Secret::Token),
            ),
            (
                "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.abcdefghij",
                Some(Secret::Token),
            ),
            ("password: hunter2", Some(Secret::Password)),
            ("DB_PASSWORD=hunter2", Some(Secret::Password)),
            ("export api_key=abc123", Some(Secret::Password)),
            (
                "HOST=localhost\nAUTH_TOKEN=abc\nPORT=80",
                Some(Secret::Password),
            ),
            ("  CLIENT_SECRET = \"xyz\"", Some(Secret::Password)),
            (
                "{\"user\": \"me\", \"password\": \"x\"}",
                Some(Secret::Password),
            ),
            ("\"apiKey\" : \"abc\"", Some(Secret::Password)),
            ("sk_live_abcdefghijklmnopqrstuv", Some(Secret::Token)),
            ("sk_test_abcdefghijklmnopqrstuv", Some(Secret::Token)),
            ("rk_live_abcdefghijklmnopqrstuv", Some(Secret::Token)),
            ("glpat-abcdefghijklmnopqrstu", Some(Secret::Token)),
            (
                "key=AIzaSyA1234567890abcdefghijklmnopqrstuv",
                Some(Secret::Token),
            ),
            ("Remember the password for the wifi", None),
            ("Reset your password = easy", None),
            ("HOST=localhost\nPORT=80", None),
            ("\"password\" is a word in this quote", None),
            ("https://example.com/login?password=1", None),
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
