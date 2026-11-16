//! Embeds the app icon and the version information in the Windows executable.
//!
//! The icon is resource id 1 (`IDI_APP`): Explorer and the taskbar show it for the exe, and the
//! tray loads the same resource (`snipdeck-platform`'s `windows/tray.rs`). Only a Windows
//! *target* needs a resource compiler (`rc.exe` or `windres`); other targets, and so Linux CI,
//! build without one. Regenerate `snipdeck.ico` with `python3 scripts/make_logo.py`.
#![allow(
    clippy::print_stderr,
    clippy::print_stdout,
    reason = "a build script talks to cargo and the person building through stdout and stderr"
)]

/// The icon resource id the executable carries and the tray loads.
const ICON_ID: &str = "1";
/// The icon, relative to this crate.
const ICON: &str = "../../assets/logo/snipdeck.ico";

fn main() {
    println!("cargo:rerun-if-changed={ICON}");
    println!("cargo:rerun-if-changed=build.rs");
    // The target, not the machine building: a cross build from Linux to Windows needs the
    // resource, a native Windows build for another target does not.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // The DLLs the exe imports are resolved before `main`, so `SetDefaultDllDirectories` there
    // (audit G20) comes too late for them. `/DEPENDENTLOADFLAG:0x800` (LOAD_LIBRARY_SEARCH_
    // SYSTEM32) makes the loader take them from System32 only. MSVC linker only; unverified on
    // Windows until the owner runs the build there.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/DEPENDENTLOADFLAG:0x800");
    }
    let mut resource = winresource::WindowsResource::new();
    // The file and product versions come from `CARGO_PKG_VERSION`; the texts are set here.
    resource
        .set_icon_with_id(ICON, ICON_ID)
        .set("ProductName", "Snipdeck")
        // Explorer's Details tab and SmartScreen show these two.
        .set("CompanyName", "Snipdeck")
        .set(
            "LegalCopyright",
            "Copyright (c) Snipdeck contributors. Licensed under GPL-3.0-only.",
        )
        .set("FileDescription", "Snipdeck")
        .set("OriginalFilename", "snipdeck.exe")
        .set("InternalName", "snipdeck");
    if let Err(error) = resource.compile() {
        eprintln!("snipdeck-app: cannot embed the icon and version resource: {error}");
        std::process::exit(1);
    }
}
