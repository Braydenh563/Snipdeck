//! The error shared by the Windows backend services (hotkeys, single instance, DPAPI).
//!
//! It is portable (no Windows types) so the callers compile and are testable everywhere. It never
//! carries secrets: only the name of the failed call and the numeric error code.

/// An operating-system call failed, or a service could not be started.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// A Win32 call failed.
    #[error("{op} failed (OS error {code:#010x})")]
    Os {
        /// The Win32 function that failed.
        op: &'static str,
        /// The Win32 error code (`GetLastError`), or the raw `HRESULT` bits when the failure was
        /// not a Win32 error.
        code: u32,
    },
    /// A service could not be started or reached (for example its thread has stopped).
    #[error("{0}")]
    Unavailable(&'static str),
}

impl Error {
    /// The OS error code, if this is an [`Error::Os`].
    #[must_use]
    pub fn code(&self) -> Option<u32> {
        match self {
            Self::Os { code, .. } => Some(*code),
            Self::Unavailable(_) => None,
        }
    }
}

#[cfg(windows)]
impl Error {
    /// Wraps a `windows` crate error from `op`.
    pub(crate) fn from_windows(op: &'static str, error: &windows::core::Error) -> Self {
        Self::Os {
            op,
            code: win32_code(error.code().0),
        }
    }
}

/// The Win32 error code inside an `HRESULT`, or the raw bits when it is not a Win32 mapping.
#[cfg(any(windows, test))]
pub(crate) fn win32_code(hresult: i32) -> u32 {
    // `HRESULT_FROM_WIN32(x)` is `0x8007_0000 | x`; the facility is bits 16..=28.
    let bits = hresult.cast_unsigned();
    if bits & 0xFFFF_0000 == 0x8007_0000 {
        bits & 0xFFFF
    } else {
        bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn win32_hresults_unwrap_to_the_win32_code() {
        // HRESULT_FROM_WIN32(ERROR_HOTKEY_ALREADY_REGISTERED = 1409)
        assert_eq!(win32_code(0x8007_0581_u32.cast_signed()), 1409);
        assert_eq!(win32_code(0x8007_0005_u32.cast_signed()), 5);
    }

    #[test]
    fn other_hresults_keep_their_bits() {
        assert_eq!(win32_code(0x8000_4005_u32.cast_signed()), 0x8000_4005);
    }

    #[test]
    fn display_names_the_call_and_code_only() {
        let e = Error::Os {
            op: "RegisterHotKey",
            code: 1409,
        };
        assert_eq!(e.to_string(), "RegisterHotKey failed (OS error 0x00000581)");
        assert_eq!(e.code(), Some(1409));
        assert_eq!(Error::Unavailable("x").code(), None);
    }
}
