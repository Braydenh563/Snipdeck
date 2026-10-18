//! Names of the kernel objects that make Snipdeck a single-instance app.
//!
//! Portable so the naming rules are tested everywhere; `windows::single_instance` uses them.
//! Both objects live in the per-session `Local\` namespace, so two users on one machine (fast user
//! switching, Remote Desktop) can each run their own copy.

/// The longest `app_id` accepted.
pub const MAX_APP_ID_LEN: usize = 64;

/// Why an `app_id` is unusable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AppIdError {
    /// Empty, or longer than [`MAX_APP_ID_LEN`].
    #[error("the app id must be 1 to {MAX_APP_ID_LEN} characters")]
    Length,
    /// Anything but ASCII letters, digits, `.`, `-` and `_` (kernel object names cannot contain
    /// `\`, and are compared case-insensitively, so look-alikes are not allowed).
    #[error("the app id may only contain ASCII letters, digits, '.', '-' and '_'")]
    Character,
}

fn check(app_id: &str) -> Result<(), AppIdError> {
    if app_id.is_empty() || app_id.len() > MAX_APP_ID_LEN {
        return Err(AppIdError::Length);
    }
    if !app_id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
    {
        return Err(AppIdError::Character);
    }
    Ok(())
}

/// The named mutex that decides who is first: `Local\Snipdeck-<app_id>`.
///
/// # Errors
///
/// See [`AppIdError`].
pub fn mutex_name(app_id: &str) -> Result<String, AppIdError> {
    check(app_id)?;
    Ok(format!(r"Local\Snipdeck-{app_id}"))
}

/// The named auto-reset event a later launch signals: `Local\Snipdeck.raise-<app_id>`.
///
/// # Errors
///
/// See [`AppIdError`].
pub fn raise_event_name(app_id: &str) -> Result<String, AppIdError> {
    check(app_id)?;
    Ok(format!(r"Local\Snipdeck.raise-{app_id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_local_and_distinct() {
        assert_eq!(mutex_name("main").unwrap(), r"Local\Snipdeck-main");
        assert_eq!(
            raise_event_name("main").unwrap(),
            r"Local\Snipdeck.raise-main"
        );
        assert_ne!(mutex_name("main"), raise_event_name("main"));
    }

    #[test]
    fn rejects_bad_ids() {
        assert_eq!(mutex_name(""), Err(AppIdError::Length));
        assert_eq!(mutex_name(&"a".repeat(65)), Err(AppIdError::Length));
        assert!(mutex_name(&"a".repeat(64)).is_ok());
        for bad in ["a\\b", "a b", "a/b", "é", "a\0", "Global\\x"] {
            assert_eq!(raise_event_name(bad), Err(AppIdError::Character), "{bad:?}");
        }
        assert!(mutex_name("Dev.1_x-y").is_ok());
    }

    #[test]
    fn the_mutex_and_event_names_never_collide() {
        // Mutexes and events share one kernel namespace; the prefixes differ (`Snipdeck-` against
        // `Snipdeck.raise-`), so no pair of app ids can produce the same string.
        for a in ["x", "x-raise", "raise-x", "a.b", ".raise-x"] {
            for b in ["x", "x-raise", "raise-x", "a.b", ".raise-x"] {
                assert_ne!(mutex_name(a).unwrap(), raise_event_name(b).unwrap());
            }
        }
    }
}
