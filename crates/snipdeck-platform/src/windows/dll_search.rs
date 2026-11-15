//! DLL search-order hardening (audit G20).
//!
//! Without it, a DLL planted beside the exe (or in the current folder) can be loaded in place of
//! a system one when the app is installed to a folder the user can write. After
//! [`restrict_dll_search`] a library is looked for in the application folder and System32 only,
//! plus folders added by `AddDllDirectory`.
//!
//! **Unverified on Windows**: compiles for the Windows target only; the owner must run the
//! GitHub build on Windows to confirm it still starts.

use windows::Win32::System::LibraryLoader::{
    LOAD_LIBRARY_SEARCH_APPLICATION_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32, SetDefaultDllDirectories,
};

/// Limits where later `LoadLibrary` calls look. Call first thing in `main`, before anything
/// loads a library. Returns `false` if Windows refused (an old Windows without the API), in
/// which case the default search order stays.
pub fn restrict_dll_search() -> bool {
    // SAFETY: takes a documented flag mask and no pointers; it changes only the process-wide
    // default search order and fails (rather than misbehaves) when unsupported.
    unsafe {
        SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_APPLICATION_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32)
    }
    .is_ok()
}
