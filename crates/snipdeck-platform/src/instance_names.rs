//! Names of the kernel objects that make Snipdeck a single-instance app.
//!
//! Portable so the naming rules are tested everywhere; `windows::single_instance` uses them.
//! All objects live in the per-session `Local\` namespace. The id in every name comes from
//! [`instance_id`]: a hash of the user's SID and the data folder, so two users in one session
//! (`runas`) and two copies on different data folders (`--data-dir`) each get their own instance
//! and cannot collide on, or guess, each other's names. The objects also carry an explicit
//! owner-only DACL (`windows::single_instance`).

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

/// The named auto-reset event `snipdeck.exe --quit` signals: `Local\Snipdeck.quit-<app_id>`.
///
/// # Errors
///
/// See [`AppIdError`].
pub fn quit_event_name(app_id: &str) -> Result<String, AppIdError> {
    check(app_id)?;
    Ok(format!(r"Local\Snipdeck.quit-{app_id}"))
}

/// The named auto-reset event the first instance sets once it has handled a hand-over:
/// `Local\Snipdeck.ack-<app_id>`.
///
/// # Errors
///
/// See [`AppIdError`].
pub fn ack_event_name(app_id: &str) -> Result<String, AppIdError> {
    check(app_id)?;
    Ok(format!(r"Local\Snipdeck.ack-{app_id}"))
}

/// The named mutex second launches take while they use the mailbox: `Local\Snipdeck.lock-<app_id>`.
///
/// # Errors
///
/// See [`AppIdError`].
pub fn lock_mutex_name(app_id: &str) -> Result<String, AppIdError> {
    check(app_id)?;
    Ok(format!(r"Local\Snipdeck.lock-{app_id}"))
}

/// The named shared-memory section that holds the mailbox: `Local\Snipdeck.box-<app_id>`.
///
/// # Errors
///
/// See [`AppIdError`].
pub fn section_name(app_id: &str) -> Result<String, AppIdError> {
    check(app_id)?;
    Ok(format!(r"Local\Snipdeck.box-{app_id}"))
}

/// The folder as a comparison key: Windows paths are case-insensitive and have many spellings of
/// one folder (`/` for `\`, a trailing separator, the `\\?\` verbatim prefix `canonicalize` adds).
/// Pass the canonical path (`std::fs::canonicalize`, or its nearest existing parent plus the rest).
#[must_use]
pub fn normalize_dir_key(path: &str) -> String {
    let path = path.replace('/', "\\");
    let path = if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = path.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        path
    };
    let mut key = path.to_lowercase();
    // "C:\" keeps its separator (it is the folder); any other trailing one goes.
    while key.len() > 3 && key.ends_with('\\') {
        key.pop();
    }
    key
}

/// The fixed mutex name every first instance also holds, for Setup's `AppMutex=Snipdeck-main`
/// (the installer cannot compute the per-user, per-folder [`instance_id`]).
pub const INSTALLER_MARKER_MUTEX: &str = r"Local\Snipdeck-main";

/// The id every kernel object name of one instance is built from: 32 hex characters, a hash of
/// `user_sid` (the SID string of the logged-on user) and the data folder's [`normalize_dir_key`].
///
/// Same user and folder: same id, so the launches meet. Another folder (`--data-dir`) or another
/// user: another id, so they run side by side.
#[must_use]
pub fn instance_id(user_sid: &str, dir_key: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"snipdeck-instance-v1\0");
    hasher.update(user_sid.as_bytes());
    hasher.update(b"\0");
    hasher.update(dir_key.as_bytes());
    let digest = hasher.finalize();
    digest.as_bytes()[..16]
        .iter()
        .fold(String::with_capacity(32), |mut id, byte| {
            use std::fmt::Write as _;
            let _ = write!(id, "{byte:02x}");
            id
        })
}

#[cfg(test)]
mod tests {
    #[test]
    fn installer_marker_matches_the_inno_app_mutex() {
        assert_eq!(super::INSTALLER_MARKER_MUTEX, r"Local\Snipdeck-main");
    }

    #[test]
    fn every_object_has_its_own_name_and_all_are_local() {
        let id = "ab12";
        let names = [
            mutex_name(id).unwrap(),
            raise_event_name(id).unwrap(),
            quit_event_name(id).unwrap(),
            ack_event_name(id).unwrap(),
            lock_mutex_name(id).unwrap(),
            section_name(id).unwrap(),
        ];
        for (i, a) in names.iter().enumerate() {
            assert!(a.starts_with(r"Local\Snipdeck"), "{a}");
            for b in &names[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_eq!(ack_event_name("a b"), Err(AppIdError::Character));
        assert_eq!(lock_mutex_name(""), Err(AppIdError::Length));
        assert_eq!(section_name("a\\b"), Err(AppIdError::Character));
    }

    #[test]
    fn one_folder_has_one_key_whatever_its_spelling() {
        let key = r"c:\users\ann\snipdeck";
        for spelling in [
            r"C:\Users\Ann\Snipdeck",
            r"C:\Users\Ann\Snipdeck\",
            r"C:/Users/Ann/Snipdeck",
            r"\\?\C:\Users\Ann\Snipdeck",
            r"\\?\c:\USERS\ann\snipdeck\\",
        ] {
            assert_eq!(normalize_dir_key(spelling), key, "{spelling}");
        }
        assert_eq!(normalize_dir_key(r"\\?\UNC\Srv\Share\D"), r"\\srv\share\d");
        assert_eq!(
            normalize_dir_key(r"C:\"),
            r"c:\",
            "a drive root keeps its separator"
        );
        assert_ne!(normalize_dir_key(r"C:\a"), normalize_dir_key(r"C:\b"));
    }

    #[test]
    fn the_id_follows_the_user_and_the_folder() {
        let sid = "S-1-5-21-1-2-3-1001";
        let id = instance_id(sid, r"c:\users\ann\snipdeck");
        assert_eq!(id.len(), 32);
        assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(id, instance_id(sid, r"c:\users\ann\snipdeck"), "stable");
        assert_ne!(id, instance_id(sid, r"d:\other"), "another data folder");
        assert_ne!(
            id,
            instance_id("S-1-5-21-1-2-3-1002", r"c:\users\ann\snipdeck"),
            "another user"
        );
        // The two fields cannot run into each other.
        assert_ne!(instance_id("ab", "c"), instance_id("a", "bc"));
        // It is a valid app id for every name.
        assert!(section_name(&id).is_ok() && mutex_name(&id).is_ok());
    }

    use super::*;

    #[test]
    fn names_are_local_and_distinct() {
        assert_eq!(mutex_name("main").unwrap(), r"Local\Snipdeck-main");
        assert_eq!(
            raise_event_name("main").unwrap(),
            r"Local\Snipdeck.raise-main"
        );
        assert_eq!(
            quit_event_name("main").unwrap(),
            r"Local\Snipdeck.quit-main"
        );
        assert_ne!(mutex_name("main"), raise_event_name("main"));
        assert_ne!(raise_event_name("main"), quit_event_name("main"));
    }

    #[test]
    fn rejects_bad_ids() {
        assert_eq!(mutex_name(""), Err(AppIdError::Length));
        assert_eq!(mutex_name(&"a".repeat(65)), Err(AppIdError::Length));
        assert!(mutex_name(&"a".repeat(64)).is_ok());
        for bad in ["a\\b", "a b", "a/b", "é", "a\0", "Global\\x"] {
            assert_eq!(raise_event_name(bad), Err(AppIdError::Character), "{bad:?}");
            assert_eq!(quit_event_name(bad), Err(AppIdError::Character), "{bad:?}");
        }
        assert!(mutex_name("Dev.1_x-y").is_ok());
    }

    #[test]
    fn the_mutex_and_event_names_never_collide() {
        // Mutexes and events share one kernel namespace; the prefixes differ (`Snipdeck-`,
        // `Snipdeck.raise-`, `Snipdeck.quit-`), so no pair of app ids can produce the same string.
        let ids = [
            "x", "x-raise", "raise-x", "quit-x", "x-quit", "a.b", ".raise-x", ".quit-x",
        ];
        for a in ids {
            for b in ids {
                let m = mutex_name(a).unwrap();
                let r = raise_event_name(b).unwrap();
                let q = quit_event_name(b).unwrap();
                assert_ne!(m, r);
                assert_ne!(m, q);
                assert_ne!(r, q);
            }
        }
    }
}
