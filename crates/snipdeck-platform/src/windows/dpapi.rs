//! `CryptProtectData` / `CryptUnprotectData` wrappers: the OS binding for the sensitive-items key.
//!
//! The app wraps these in the store crate's `KeyProvider` (the platform crate does not depend on
//! the store). They follow `docs/crypto.md`:
//!
//! - **Scope: current user.** `CRYPTPROTECT_LOCAL_MACHINE` is never set, so only the same Windows
//!   user on the same machine profile can unprotect a blob.
//! - **No UI.** `CRYPTPROTECT_UI_FORBIDDEN`: a failure is an error, never a dialog.
//! - **Authenticated.** DPAPI blobs carry an integrity MAC; a foreign, truncated or modified blob
//!   makes [`unprotect`] fail (typically `ERROR_INVALID_DATA`, 13, or an `NTE_*` code; the caller
//!   treats any [`Error::Os`] here as "rejected").
//! - **Entropy** is an extra secret mixed into the key (the store passes a fixed application
//!   string, so another program running as the same user must also know it). Empty entropy means
//!   none, on both sides.
//! - **Hygiene.** The plaintext returned by Windows is copied into a [`Zeroizing`] buffer, the
//!   Windows-owned buffer is overwritten with zeros, then freed with `LocalFree`. Errors carry only
//!   the call name and code, never data.
//!
//! **Unverified on Windows.** The `#[cfg(test)]` tests below run only there.

use std::ptr;

use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};
use windows::core::{PCWSTR, w};
use zeroize::{Zeroize, Zeroizing};

use crate::error::Error;

/// Description stored (unencrypted) in the blob. Not secret, not checked on unprotect.
const DESCRIPTION: PCWSTR = w!("Snipdeck");

/// A read-only view of `bytes` as a blob. The pointer is only valid while `bytes` is.
///
/// `None` for an empty slice (no blob), or for one too long for a `DWORD` length.
fn blob_of(bytes: &[u8]) -> Result<Option<CRYPT_INTEGER_BLOB>, Error> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let len =
        u32::try_from(bytes.len()).map_err(|_| Error::Unavailable("DPAPI input is too large"))?;
    Ok(Some(CRYPT_INTEGER_BLOB {
        cbData: len,
        // The API takes `*mut` for historical reasons but only reads the input blob.
        pbData: bytes.as_ptr().cast_mut(),
    }))
}

/// Takes ownership of a Windows-allocated output blob: copies it out, wipes it, frees it.
///
/// `wipe` should be `true` when the content is secret.
fn take_output(out: CRYPT_INTEGER_BLOB, wipe: bool) -> Result<Zeroizing<Vec<u8>>, Error> {
    if out.pbData.is_null() {
        return Err(Error::Unavailable("DPAPI returned no data"));
    }
    // SAFETY: on success `pbData` points to `cbData` readable and writable bytes allocated by
    // DPAPI with `LocalAlloc`; nothing else references them, and `cbData` fits `usize`.
    let bytes = unsafe { std::slice::from_raw_parts_mut(out.pbData, out.cbData as usize) };
    let copy = Zeroizing::new(bytes.to_vec());
    if wipe {
        bytes.zeroize();
    }
    // SAFETY: `pbData` was allocated by DPAPI with `LocalAlloc` and the documentation says to free
    // it with `LocalFree`; `bytes` is not used after this point.
    unsafe {
        // `LocalFree` returns null on success; there is nothing to do if it somehow failed.
        let _ = LocalFree(Some(HLOCAL(out.pbData.cast())));
    }
    Ok(copy)
}

/// Encrypts `data` for the current Windows user, mixing in `entropy` (empty means none).
///
/// # Errors
///
/// [`Error::Os`] if `CryptProtectData` fails; [`Error::Unavailable`] if `data` or `entropy` is
/// larger than 4 GiB.
pub fn protect(data: &[u8], entropy: &[u8]) -> Result<Vec<u8>, Error> {
    // Empty input still needs a valid (empty) blob: DPAPI accepts `cbData == 0`.
    let input = blob_of(data)?.unwrap_or(CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    });
    let entropy_blob = blob_of(entropy)?;
    let mut out = CRYPT_INTEGER_BLOB::default();
    // SAFETY: `input` and `entropy_blob` point into `data` / `entropy`, which outlive the call and
    // are only read; `out` is a valid out-pointer. No prompt struct is passed and
    // `CRYPTPROTECT_UI_FORBIDDEN` is set, so no UI can appear. The scope is the current user
    // because `CRYPTPROTECT_LOCAL_MACHINE` is not set.
    let result = unsafe {
        CryptProtectData(
            &raw const input,
            DESCRIPTION,
            entropy_blob.as_ref().map(ptr::from_ref),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &raw mut out,
        )
    };
    result.map_err(|e| Error::from_windows("CryptProtectData", &e))?;
    // The output is ciphertext, so it is not secret, but wiping it costs nothing.
    let blob = take_output(out, true)?;
    Ok(blob.to_vec())
}

/// Decrypts a blob made by [`protect`] with the same `entropy`, for the same Windows user.
///
/// # Errors
///
/// [`Error::Os`] if `CryptUnprotectData` fails: a blob from another user or machine, with other
/// entropy, truncated or modified. [`Error::Unavailable`] if `blob` or `entropy` is larger than
/// 4 GiB.
pub fn unprotect(blob: &[u8], entropy: &[u8]) -> Result<Zeroizing<Vec<u8>>, Error> {
    let Some(input) = blob_of(blob)? else {
        // An empty blob is never a valid DPAPI blob; do not hand it to the API.
        return Err(Error::Unavailable("empty DPAPI blob"));
    };
    let entropy_blob = blob_of(entropy)?;
    let mut out = CRYPT_INTEGER_BLOB::default();
    // SAFETY: as in `protect`: the input and entropy blobs point into slices that outlive the call
    // and are only read, `out` is a valid out-pointer, the description out-pointer is omitted, no
    // prompt struct is passed and `CRYPTPROTECT_UI_FORBIDDEN` is set.
    let result = unsafe {
        CryptUnprotectData(
            &raw const input,
            None,
            entropy_blob.as_ref().map(ptr::from_ref),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &raw mut out,
        )
    };
    result.map_err(|e| Error::from_windows("CryptUnprotectData", &e))?;
    take_output(out, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTROPY: &[u8] = b"snipdeck-test-entropy";

    #[test]
    fn round_trip() {
        let secret = b"a 32 byte data encryption key..!";
        let blob = protect(secret, ENTROPY).unwrap();
        assert_ne!(blob.as_slice(), secret.as_slice());
        assert!(!blob.windows(secret.len()).any(|w| w == secret));
        assert_eq!(
            unprotect(&blob, ENTROPY).unwrap().as_slice(),
            secret.as_slice()
        );
    }

    #[test]
    fn round_trip_without_entropy_and_with_empty_data() {
        let blob = protect(b"x", b"").unwrap();
        assert_eq!(unprotect(&blob, b"").unwrap().as_slice(), b"x");
        let blob = protect(b"", ENTROPY).unwrap();
        assert_eq!(
            unprotect(&blob, ENTROPY).unwrap().as_slice(),
            b"".as_slice()
        );
    }

    #[test]
    fn wrong_entropy_is_rejected() {
        let blob = protect(b"secret", ENTROPY).unwrap();
        assert!(unprotect(&blob, b"other").is_err());
        assert!(unprotect(&blob, b"").is_err());
    }

    #[test]
    fn modified_truncated_and_foreign_blobs_are_rejected() {
        let blob = protect(b"secret", ENTROPY).unwrap();
        for i in [0, blob.len() / 2, blob.len() - 1] {
            let mut bad = blob.clone();
            bad[i] ^= 1;
            assert!(unprotect(&bad, ENTROPY).is_err(), "flipped byte {i}");
        }
        assert!(unprotect(&blob[..blob.len() - 1], ENTROPY).is_err());
        assert!(unprotect(b"not a dpapi blob", ENTROPY).is_err());
        assert!(unprotect(b"", ENTROPY).is_err());
    }
}
