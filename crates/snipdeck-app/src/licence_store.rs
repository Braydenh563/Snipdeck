//! The licence key on disk: one text file, `licence.key`, in the data folder next to the
//! library. The key is checked offline against the public key compiled into this build
//! (`snipdeck_core::licence`) every time Snipdeck starts, so the file is never trusted: a missing,
//! damaged or foreign key simply means the Free plan.

use std::{fs, io, path::Path};

use snipdeck_core::{
    Entitlements, Timestamp,
    licence::{LicenceError, verify_embedded},
};

use crate::toast::LicenceProblem;

/// The file's name in the data folder.
pub const FILE: &str = "licence.key";

/// The key text saved in `folder`, if there is a readable one.
#[must_use]
pub fn read(folder: &Path) -> Option<String> {
    let text = fs::read_to_string(folder.join(FILE)).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Saves `token` in `folder`, replacing the file whole (written beside it and renamed, so a crash
/// never leaves half a key).
///
/// # Errors
///
/// The file could not be written.
pub fn write(folder: &Path, token: &str) -> io::Result<()> {
    let target = folder.join(FILE);
    let part = folder.join(format!("{FILE}.part"));
    fs::create_dir_all(folder)?;
    fs::write(&part, token.trim())?;
    fs::rename(&part, &target).inspect_err(|_| {
        let _ = fs::remove_file(&part);
    })
}

/// What the saved key entitles the user to: Pro for a good key, Free for none or a bad one.
#[must_use]
pub fn entitlements(folder: &Path, now: Timestamp) -> Entitlements {
    read(folder)
        .and_then(|token| verify_embedded(&token, now).ok())
        .unwrap_or_default()
}

/// Which toast a refused key gets.
#[must_use]
pub const fn problem_of(error: LicenceError) -> LicenceProblem {
    match error {
        LicenceError::Malformed(_) => LicenceProblem::NotAKey,
        LicenceError::BadSignature => LicenceProblem::BadSignature,
        LicenceError::UnsupportedVersion(_) => LicenceProblem::NewerFormat,
        LicenceError::WrongTier => LicenceProblem::WrongTier,
    }
}

#[cfg(test)]
mod tests {
    use snipdeck_core::{
        Tier,
        licence::signing::{SigningKey, pro_payload, sign_token},
    };

    use super::*;

    fn now() -> Timestamp {
        "2026-10-18T09:00:00Z".parse().expect("timestamp")
    }

    /// A key the development public key accepts (its seed is published in `snipdeck-core`).
    /// `None` in a build that embeds the production key, which no test can sign for.
    fn good_key() -> Option<String> {
        if !snipdeck_core::licence::IS_DEVELOPMENT_KEY {
            return None;
        }
        let signer = SigningKey::from_bytes(b"snipdeck development licence key");
        let payload = pro_payload("TEST-1", 1_760_000_000, None).expect("payload");
        Some(sign_token(&signer, &payload))
    }

    #[test]
    fn no_file_or_a_bad_one_is_the_free_plan() {
        let dir = tempfile::tempdir().expect("dir");
        assert_eq!(entitlements(dir.path(), now()).tier(), Tier::Free);
        write(dir.path(), "SNIP1.not.a-key").expect("write");
        assert_eq!(read(dir.path()).as_deref(), Some("SNIP1.not.a-key"));
        assert_eq!(entitlements(dir.path(), now()).tier(), Tier::Free);
        write(dir.path(), "  \n").expect("write");
        assert_eq!(read(dir.path()), None, "blank means none");
    }

    #[test]
    fn a_saved_good_key_is_pro_on_the_next_start() {
        let Some(key) = good_key() else { return };
        let dir = tempfile::tempdir().expect("dir");
        write(dir.path(), &format!("  {key}\n")).expect("write");
        assert_eq!(read(dir.path()).as_deref(), Some(key.as_str()));
        assert_eq!(entitlements(dir.path(), now()).tier(), Tier::Pro);
        assert!(
            !dir.path().join(format!("{FILE}.part")).exists(),
            "no leftover"
        );
    }

    #[test]
    fn every_licence_error_has_its_own_problem() {
        let problems = [
            problem_of(LicenceError::Malformed("x")),
            problem_of(LicenceError::BadSignature),
            problem_of(LicenceError::UnsupportedVersion(2)),
            problem_of(LicenceError::WrongTier),
        ];
        for (i, a) in problems.iter().enumerate() {
            for b in &problems[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}
