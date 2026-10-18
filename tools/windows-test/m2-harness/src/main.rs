//! M2 test harness: drives the Windows backend of `snipdeck-platform` from global hotkeys so the
//! owner can check it by hand with the wizard (`tools/windows-test/tests/m2-platform.json`).
//!
//! Console lines the wizard captures all start with `[m2]`. Clipboard content is never printed.

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod items;
#[cfg(windows)]
mod sys;

#[cfg(windows)]
fn main() {
    std::process::exit(app::run());
}

#[cfg(not(windows))]
fn main() {
    println!("Windows only");
}
