//! Persistence for Snipdeck: one SQLite database (WAL, FTS5) plus a content-addressed blob
//! folder, both inside the user's data folder (ADR-0002).
//!
//! The rest of the app talks to storage through the traits in this crate ([`BoardRepo`],
//! [`ListRepo`], [`ItemRepo`], [`TagRepo`], [`TrashRepo`], [`BlobStore`]), so tests and the UI
//! can run against fakes. The SQLite implementation of all of them is one type.
//!
//! The API is synchronous: SQLite on a local disk answers in well under a millisecond, the app
//! has no async runtime (it would cost memory and start-up time, SPEC §9), and the one writer
//! is the app itself. Reads take `&self`; writes take `&mut self`, which is what a
//! `rusqlite` transaction needs.

#![forbid(unsafe_code)]
#![allow(
    clippy::missing_errors_doc,
    reason = "every storage call can fail with I/O or SQLite errors; `repo` states the shared rules"
)]

// The `testing` feature lowers the Argon2 cost floor for stored key records (`crypto::keyring`) and
// exports an in-memory key provider. It is for tests only: a build with optimisations and no debug
// assertions (the release profile) must never carry it, so one that does not compile is the
// guard. Release-mode runs of the integration tests are therefore unsupported; use the debug
// profile. `tests/release_guard.rs` checks that the shipped app never enables the feature.
#[cfg(all(feature = "testing", not(debug_assertions), not(test)))]
compile_error!(
    "snipdeck-store's `testing` feature lowers the Argon2 floor and must not be built in a release \
     profile; it is enabled only by dev-dependencies (see tests/release_guard.rs)"
);

mod batch;
mod blobs;
mod boards;
mod codec;
pub mod crypto;
mod error;
mod fts;
mod items;
mod keys;
mod migrations;
mod rank;
mod recent;
mod repo;
mod schema;
mod store;
mod tags;
mod trash;

pub use batch::Batch;
pub use blobs::BlobStore;
pub use error::{Error, Result, Trouble};
pub use recent::{RecentEntry, RecentRepo};
pub use repo::{BoardRepo, ItemRepo, ListRepo, TagRepo, TrashEntry, TrashRepo, Trashable};
pub use store::{GcReport, Store};
pub use tags::name_key;
