//! DLL search-order hardening (audit G20).
//!
//! What this does, and what it does not (PLATFORM-16). After [`restrict_dll_search`] a library
//! loaded by name is looked for in the application folder and System32 only (plus folders added
//! by `AddDllDirectory`): the current directory and every `PATH` folder are no longer searched,
//! which closes the "DLL planted in the folder you opened a file from" route.
//!
//! It does **not** protect against a DLL beside the exe: `LOAD_LIBRARY_SEARCH_APPLICATION_DIR`
//! is kept on purpose (the app's own libraries live there), so an install folder the user can
//! write to is still trusted. The installer puts the app under `Program Files`, which they cannot.
//!
//! The call also runs after the loader has resolved the exe's static imports, so those are not
//! covered. The MSVC linker flag `/DEPENDENTLOADFLAG` (release build, MSVC target only: the GNU
//! linker has no such flag) closes that gap; release CI asserts the MSVC target.
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
