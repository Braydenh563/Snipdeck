//! Windows Hello through `KeyCredentialManager` (the OS half of [`crate::hello`]).
//!
//! The credential is an RSA key pair kept by Windows and unlocked by the user's PIN, face or
//! fingerprint. `RequestSignAsync` signs with PKCS#1 v1.5, which is deterministic, so the same
//! challenge always gives the same signature: the portable layer hashes it into a KEK.
//!
//! Every call runs on its own short-lived thread that joins the MTA, and blocks the caller until
//! the `WinRT` async operation finishes (the Hello prompt may be on screen), the same shape as the
//! app's `master_job` worker. Call these methods from a worker, never the UI thread.
//!
//! **Unverified on Windows**: this compiles in CI; the prompt, its focus and the status mapping
//! have not been run. No key material is logged.

use windows::Security::Credentials::{
    KeyCredential, KeyCredentialCreationOption, KeyCredentialManager, KeyCredentialStatus,
};
use windows::Security::Cryptography::CryptographicBuffer;
use windows::Storage::Streams::DataReader;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::core::HSTRING;

use crate::hello::{HelloAuthenticator, HelloAvailability, HelloError};

/// The credential name Windows stores the key pair under.
const CREDENTIAL_NAME: &str = "Snipdeck";

/// The real Hello.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsHello;

/// Runs `job` on a fresh MTA thread and waits for it.
fn on_worker<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, HelloError> + Send + 'static,
) -> Result<T, HelloError> {
    std::thread::spawn(move || {
        // SAFETY: balanced with `CoUninitialize` below on this same thread; a failure (for
        // example the thread already had another apartment) is ignored and the job still runs.
        let initialised = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        let out = job();
        if initialised {
            // SAFETY: pairs the successful `CoInitializeEx` above.
            unsafe { CoUninitialize() };
        }
        out
    })
    .join()
    .map_err(|_| HelloError::Failed)?
}

fn status_error(status: KeyCredentialStatus) -> HelloError {
    match status {
        KeyCredentialStatus::UserCanceled => HelloError::Cancelled,
        KeyCredentialStatus::NotFound | KeyCredentialStatus::UserPrefersPassword => {
            HelloError::Removed
        }
        _ => HelloError::Failed,
    }
}

fn open_credential() -> Result<KeyCredential, HelloError> {
    let name = HSTRING::from(CREDENTIAL_NAME);
    let result = KeyCredentialManager::OpenAsync(&name)
        .and_then(|op| op.join())
        .map_err(|_| HelloError::Failed)?;
    match result.Status().map_err(|_| HelloError::Failed)? {
        KeyCredentialStatus::Success => result.Credential().map_err(|_| HelloError::Failed),
        other => Err(status_error(other)),
    }
}

impl HelloAuthenticator for WindowsHello {
    fn availability(&self) -> HelloAvailability {
        let supported = on_worker(|| {
            KeyCredentialManager::IsSupportedAsync()
                .and_then(|op| op.join())
                .map_err(|_| HelloError::Failed)
        });
        match supported {
            Ok(true) => HelloAvailability::Ready,
            // The API answers "false" both for no hardware and for no PIN set up; Windows gives
            // no cheaper way to tell them apart, so the app shows the same hint for both.
            Ok(false) => HelloAvailability::NotSetUp,
            Err(_) => HelloAvailability::Unsupported,
        }
    }

    fn ensure_credential(&self) -> Result<(), HelloError> {
        on_worker(|| {
            let name = HSTRING::from(CREDENTIAL_NAME);
            let result = KeyCredentialManager::RequestCreateAsync(
                &name,
                KeyCredentialCreationOption::FailIfExists,
            )
            .and_then(|op| op.join())
            .map_err(|_| HelloError::Failed)?;
            match result.Status().map_err(|_| HelloError::Failed)? {
                KeyCredentialStatus::Success | KeyCredentialStatus::CredentialAlreadyExists => {
                    Ok(())
                }
                other => Err(status_error(other)),
            }
        })
    }

    fn sign_challenge(&self, challenge: &[u8]) -> Result<Vec<u8>, HelloError> {
        let challenge = challenge.to_vec();
        on_worker(move || {
            let credential = open_credential()?;
            let buffer = CryptographicBuffer::CreateFromByteArray(&challenge)
                .map_err(|_| HelloError::Failed)?;
            let result = credential
                .RequestSignAsync(&buffer)
                .and_then(|op| op.join())
                .map_err(|_| HelloError::Failed)?;
            match result.Status().map_err(|_| HelloError::Failed)? {
                KeyCredentialStatus::Success => {}
                other => return Err(status_error(other)),
            }
            let signed = result.Result().map_err(|_| HelloError::Failed)?;
            let reader = DataReader::FromBuffer(&signed).map_err(|_| HelloError::Failed)?;
            let len = reader
                .UnconsumedBufferLength()
                .map_err(|_| HelloError::Failed)?;
            let mut bytes = vec![0_u8; len as usize];
            reader
                .ReadBytes(&mut bytes)
                .map_err(|_| HelloError::Failed)?;
            Ok(bytes)
        })
    }
}
