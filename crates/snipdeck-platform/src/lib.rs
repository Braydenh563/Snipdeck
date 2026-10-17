//! The OS layer: what Snipdeck needs from the desktop to paste an item into another app.
//!
//! The crate is split so that the decisions are portable and testable on Linux CI, while only the
//! calls into Windows are not:
//!
//! - [`keys`]: which keys to inject to paste, given the modifiers the user is holding.
//! - [`payload`]: clipboard content with its bytes resolved (the platform has no store).
//! - [`route`]: whether a Target can be pasted into at all, and with which paste key.
//! - [`pipeline`]: the paste sequence (snapshot, write, inject, wait for the target, restore)
//!   written against the [`pipeline::Clipboard`], [`pipeline::Keyboard`] and
//!   [`pipeline::Timing`] traits, so it runs under test with fakes and a fixed clock.
//!
//! The Windows implementation of those traits lives in a `windows` module (`cfg(windows)`), the
//! only place `unsafe` is allowed (ADR-0003, `docs/architecture.md`).

pub mod keys;
pub mod payload;
pub mod pipeline;
pub mod route;

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Win32 calls; every unsafe block carries a SAFETY comment (ADR-0003)"
)]
pub mod windows;
