//! The licence key on disk: one text file, `licence.key`, in the data folder next to the
//! library. The key is checked offline against the public key compiled into this build
//! (`snipdeck_core::licence`) every time Snipdeck starts, so the file is never trusted: a missing,
//! damaged or foreign key simply means the Free plan.

use std::{
    fs,
    io::{self, Read as _, Write as _},
    path::Path,
};

use snipdeck_core::{
    Entitlements, Timestamp,
    licence::{LicenceError, verify_embedded},
};

use crate::toast::LicenceProblem;

/// The most a licence key file may weigh: a key is a few hundred characters, so a bigger file
/// is not one (and is not read whole into memory).
pub const MAX_TOKEN_BYTES: u64 = 16 * 1024;

/// The file's name in the data folder.
pub const FILE: &str = "licence.key";

/// The key text saved in `folder`, if there is a readable one.
#[must_use]
pub fn read(folder: &Path) -> Option<String> {
    let file = fs::File::open(folder.join(FILE)).ok()?;
    let mut bytes = Vec::new();
    // One byte over the cap tells a file that is too big from one that is exactly the cap.
    file.take(MAX_TOKEN_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_TOKEN_BYTES {
        return None;
    }
    let text = String::from_utf8(bytes).ok()?;
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
    fs::create_dir_all(folder)?;
    let (mut file, part) = crate::settings_store::create_temp(folder, FILE)?;
    file.write_all(token.trim().as_bytes())
        .and_then(|()| file.sync_all())
        .and_then(|()| {
            drop(file);
            fs::rename(&part, &target)
        })
        .inspect_err(|_| {
            let _ = fs::remove_file(&part);
        })
}

/// What the saved key entitles the user to: Supporter for a good key, Free for none or a bad one.
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

    /// The published development seed, assembled at run time (public by design).
    fn development_seed() -> [u8; 32] {
        let text = ["snipdeck", "development", "licence", "key"].join(" ");
        text.as_bytes().try_into().expect("32 bytes")
    }

    fn now() -> Timestamp {
        "2026-10-18T09:00:00Z".parse().expect("timestamp")
    }

    /// A key the development public key accepts (its seed is published in `snipdeck-core`).
    /// `None` in a build that embeds the production key, which no test can sign for.
    fn good_key() -> Option<String> {
        if !snipdeck_core::licence::IS_DEVELOPMENT_KEY {
            return None;
        }
        let signer = SigningKey::from_bytes(&development_seed());
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
    fn a_key_file_over_the_cap_is_not_read() {
        let dir = tempfile::tempdir().expect("dir");
        let big = "A".repeat(usize::try_from(MAX_TOKEN_BYTES).unwrap() + 1);
        fs::write(dir.path().join(FILE), &big).expect("write");
        assert_eq!(read(dir.path()), None, "one byte over");
        let exact = "A".repeat(usize::try_from(MAX_TOKEN_BYTES).unwrap());
        fs::write(dir.path().join(FILE), &exact).expect("write");
        assert_eq!(
            read(dir.path()).as_deref(),
            Some(exact.as_str()),
            "at the cap"
        );
    }

    #[test]
    fn a_saved_good_key_is_pro_on_the_next_start() {
        let Some(key) = good_key() else { return };
        let dir = tempfile::tempdir().expect("dir");
        write(dir.path(), &format!("  {key}\n")).expect("write");
        assert_eq!(read(dir.path()).as_deref(), Some(key.as_str()));
        assert_eq!(entitlements(dir.path(), now()).tier(), Tier::Pro);
        assert!(
            fs::read_dir(dir.path())
                .unwrap()
                .all(|e| e.unwrap().file_name() == FILE),
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
