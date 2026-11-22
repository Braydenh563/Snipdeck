//! Reads an executable's `FileDescription` from its version resource.
//!
//! UNVERIFIED on Windows (written on Linux; `tools/windows-test/` has no case for it yet). The
//! pure parts (which strings to ask for, which names are fit to show) are tested in
//! `app_names`.

use std::ffi::c_void;

use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows::core::{HSTRING, PCWSTR, w};

use crate::app_names;

/// Remembers the display name of the program at `path` unless it was asked before. Reads the
/// file's version resource once per program; a file without one is remembered as nameless.
pub fn remember_description(path: &str) {
    if app_names::asked(path) {
        return;
    }
    let description = read(path).unwrap_or_default();
    app_names::remember(path, &description);
}

fn read(path: &str) -> Option<String> {
    let file = HSTRING::from(path);
    // SAFETY: `file` is a valid, NUL-terminated wide string that outlives the call.
    let size = unsafe { GetFileVersionInfoSizeW(&file, None) };
    if size == 0 {
        return None;
    }
    let mut block = vec![0u8; usize::try_from(size).ok()?];
    // SAFETY: `block` is writable for `size` bytes and outlives the call.
    unsafe { GetFileVersionInfoW(&file, None, size, block.as_mut_ptr().cast()) }.ok()?;
    let translations = translations(&block);
    app_names::description_paths(&translations)
        .iter()
        .find_map(|sub| string_at(&block, sub))
}

/// The `(language, code page)` pairs of the file's `\VarFileInfo\Translation`.
fn translations(block: &[u8]) -> Vec<(u16, u16)> {
    let Some((ptr, bytes)) = query(block, w!("\\VarFileInfo\\Translation")) else {
        return Vec::new();
    };
    // SAFETY: `VerQueryValueW` returned a pointer into `block` to `bytes` bytes of
    // (language, code page) pairs; they are read unaligned.
    let raw = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), bytes) };
    raw.as_chunks::<4>()
        .0
        .iter()
        .map(|c| {
            (
                u16::from_le_bytes([c[0], c[1]]),
                u16::from_le_bytes([c[2], c[3]]),
            )
        })
        .collect()
}

/// The string at version-resource path `sub`, without its terminating NUL.
fn string_at(block: &[u8], sub: &str) -> Option<String> {
    let sub = HSTRING::from(sub);
    let (ptr, chars) = query(block, PCWSTR(sub.as_ptr()))?;
    let mut units = Vec::with_capacity(chars);
    for i in 0..chars {
        // SAFETY: for a string value `VerQueryValueW` returns a pointer into `block` to `chars`
        // UTF-16 units (the NUL included), possibly unaligned, so each is read unaligned.
        let unit = unsafe { ptr.cast::<u16>().add(i).read_unaligned() };
        if unit == 0 {
            break;
        }
        units.push(unit);
    }
    let text = String::from_utf16_lossy(&units);
    (!text.trim().is_empty()).then_some(text)
}

/// `VerQueryValueW` for `sub`: the pointer into `block` and its length (bytes for a binary
/// value, UTF-16 units for a string).
fn query(block: &[u8], sub: PCWSTR) -> Option<(*const c_void, usize)> {
    let mut ptr: *mut c_void = std::ptr::null_mut();
    let mut len = 0u32;
    // SAFETY: `block` is a version block from `GetFileVersionInfoW`, `sub` is NUL-terminated,
    // and both out-pointers are valid for the call.
    let found = unsafe { VerQueryValueW(block.as_ptr().cast(), sub, &raw mut ptr, &raw mut len) };
    (found.as_bool() && !ptr.is_null())
        .then(|| (ptr.cast_const(), usize::try_from(len).unwrap_or(0)))
}
