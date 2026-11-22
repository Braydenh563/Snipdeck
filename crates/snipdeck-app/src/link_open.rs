//! Opening a Link item's address in the system's handler ("Open in browser").
//!
//! A Link item can hold any address that has a scheme: an imported library carries links written
//! by other people, so `file:///C:/x`, `javascript://x`, `ms-settings:` or `\\server\share` must
//! never reach `ShellExecute`. The check is made here, where the address is opened, not when it is
//! saved: only [`ALLOWED_SCHEMES`] pass.

use std::ffi::OsStr;

/// The schemes "Open in browser" opens: web pages, an email draft and a phone number.
pub const ALLOWED_SCHEMES: [&str; 4] = ["http", "https", "mailto", "tel"];

/// Why an address was not opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenLinkError {
    /// The scheme is not one of [`ALLOWED_SCHEMES`], or the address is malformed.
    NotAllowed,
    /// The system could not open it.
    Failed(String),
}

/// `text` trimmed, when it is an address this app may open; `None` otherwise.
#[must_use]
pub fn allowed_target(text: &str) -> Option<&str> {
    let text = text.trim();
    if text.is_empty() || text.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    let (scheme, rest) = text.split_once(':')?;
    let scheme = scheme.to_ascii_lowercase();
    if !ALLOWED_SCHEMES.contains(&scheme.as_str()) {
        return None;
    }
    let ok = match scheme.as_str() {
        "mailto" | "tel" => !rest.is_empty(),
        _ => {
            // `http://host/...`: a name after `//`, never another slash or a backslash (a UNC path).
            rest.strip_prefix("//")
                .and_then(|r| r.chars().next())
                .is_some_and(|c| c.is_alphanumeric() || c == '[')
        }
    };
    ok.then_some(text)
}

/// Opens `text` with `opener` when it passes [`allowed_target`].
///
/// # Errors
///
/// [`OpenLinkError::NotAllowed`] before `opener` is called, or [`OpenLinkError::Failed`].
pub fn open_with(
    text: &str,
    opener: impl FnOnce(&str) -> Result<(), String>,
) -> Result<(), OpenLinkError> {
    let target = allowed_target(text).ok_or(OpenLinkError::NotAllowed)?;
    opener(target).map_err(OpenLinkError::Failed)
}

/// The system's opener (`ShellExecuteW` on Windows).
///
/// # Errors
///
/// The system's message when the address could not be opened.
pub fn system_opener(target: &str) -> Result<(), String> {
    crate::help_binding::launch(OsStr::new(target))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_mail_and_phone_addresses_open() {
        for ok in [
            "https://example.com/a?b=1#c",
            "HTTP://EXAMPLE.COM",
            "  https://example.com  ",
            "http://[::1]:8080/",
            "http://192.168.0.1",
            "mailto:me@example.com?subject=Hi",
            "tel:+4930123456",
            "MAILTO:a@b.c",
        ] {
            assert_eq!(allowed_target(ok), Some(ok.trim()), "{ok:?}");
        }
    }

    #[test]
    fn every_other_scheme_and_path_is_refused() {
        for bad in [
            "file:///C:/Windows/System32/calc.exe",
            "file://server/share/x.exe",
            "FILE:///etc/passwd",
            "javascript://x%0Aalert(1)",
            "javascript:alert(1)",
            "ms-settings:privacy",
            "ms-msdt:/id PCWDiagnostic",
            "search-ms:query=x",
            "obsidian://open?vault=a",
            "ftp://host/file",
            "data:text/html,<script>",
            "\\\\server\\share\\x.exe",
            "//server/share",
            "C:\\Windows\\System32\\calc.exe",
            "cmd.exe",
            "https:///x",
            "https:////server/share",
            "https:\\\\server\\share",
            "https://",
            "https:",
            "mailto:",
            "tel:",
            "https://a b",
            "https://a\nb",
            "https://a\u{0}b",
            "",
            "   ",
        ] {
            assert_eq!(allowed_target(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_refused_address_never_reaches_the_opener() {
        let mut called = false;
        let out = open_with("file:///C:/x", |_| {
            called = true;
            Ok(())
        });
        assert_eq!(out, Err(OpenLinkError::NotAllowed));
        assert!(!called);
        let mut got = String::new();
        let out = open_with(" https://example.com ", |t| {
            t.clone_into(&mut got);
            Ok(())
        });
        assert_eq!(out, Ok(()));
        assert_eq!(got, "https://example.com");
        assert_eq!(
            open_with("tel:112", |_| Err("no handler".into())),
            Err(OpenLinkError::Failed("no handler".into()))
        );
    }
}
