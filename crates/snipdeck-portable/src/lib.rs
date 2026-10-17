//! Data portability for Snipdeck (SPEC §5.9, §5.7a): the `.snipdeck` archive, importers for other
//! tools, and the daily local backup.
//!
//! * [`export_to_writer`] / [`export_to_path`] write a library as a `.snipdeck` zip;
//!   [`import_archive`] / [`import_archive_path`] read one. The format is specified in
//!   `docs/format.md`.
//! * [`import_folder`], [`import_ditto`] and [`import_copyq_json`] import other sources and
//!   return the same [`ImportReport`].
//! * [`backup`] writes one backup per local day and prunes old ones; restoring is an import.
//!
//! Everything works on any store implementing the repository traits (the [`Library`] bound),
//! takes the clock as an argument, and does its own file I/O only where a path is passed in.
//!
//! # Security model
//!
//! Archives, databases and folders are **untrusted input**. Nothing is extracted to disk; every
//! zip entry is checked against [`Limits`] before it is decompressed and again while it is;
//! hostile names, links, encrypted entries and overlapping data are refused; and an import
//! validates the whole file before it writes the first record, then writes atomically (it undoes
//! itself on failure; see `plan`). Sensitive items are never written in plain text: they are
//! left out of an export, or encrypted under the library's data key.
//!
//! # Errors versus warnings
//!
//! An [`Error`] means the input is unusable and the library is unchanged. A [`Warning`] in the
//! [`ImportReport`] means the import went ahead but left something out or changed something:
//! sensitive items that need a key, lossy text decoding, skipped files, the Free-tier board
//! limit. The caller decides what to tell the user.

#![forbid(unsafe_code)]
#![allow(
    clippy::missing_errors_doc,
    reason = "every call can fail with storage or I/O errors; the `Error` docs list them"
)]

mod archive;
pub mod backup;
mod dto;
mod error;
mod export;
mod import;
mod importers;
mod limits;
mod plan;
mod report;

pub use error::{Error, Result};
pub use export::{ExportOptions, ExportReport, SensitivePolicy, export_to_path, export_to_writer};
pub use import::{
    BoardLayout, ImportOptions, KeySource, MergeStrategy, import_archive, import_archive_path,
};
pub use importers::{MEMORY_BUDGET, import_copyq_json, import_ditto, import_folder};
pub use limits::{Limits, TEXT_LIMIT};
pub use plan::Library;
pub use report::{ImportReport, MAX_WARNINGS, Warning};
