//! Making an item from what the user pasted (Ctrl+V on a list): what kind the text becomes, and
//! whether it looks like a secret.
//!
//! Pure functions, so the rules are tested here. The controller makes the item
//! (`controller::create_ctl`).

use snipdeck_core::ItemKind;

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
    if says_password(t) {
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
