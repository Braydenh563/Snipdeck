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
mod repo;
mod schema;
mod store;
mod tags;
mod trash;

pub use blobs::BlobStore;
pub use error::{Error, Result};
pub use repo::{BoardRepo, ItemRepo, ListRepo, TagRepo, TrashEntry, TrashRepo, Trashable};
pub use store::{GcReport, Store};
