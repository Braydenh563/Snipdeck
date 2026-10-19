//! Domain types and pure logic for Snipdeck.
//!
//! This crate is the contract every other Snipdeck crate builds on. It has no I/O and no
//! OS-specific code: anything that needs the clock, the disk or the clipboard receives the
//! value it needs (for example `now: Timestamp`) from its caller.
//!
//! The model, from the top: a [`Board`] holds ordered [`List`]s, a list holds ordered
//! [`Item`]s, and an item's content is a set of clipboard [`Formats`]. Order is not a field:
//! storage returns things in the user's order and takes a [`Placement`] to change it.
//! See `docs/architecture.md` for the reasoning behind each choice.

#![forbid(unsafe_code)]

#[macro_use]
mod key;

mod board;
mod content;
mod entitlements;
mod error;
mod format;
mod ids;
mod item;
mod list;
mod name;
mod placement;
mod tag;
mod usage;

pub mod licence;
pub mod search;
pub mod template;

pub use board::Board;
pub use content::{BlobRef, Content, INLINE_LIMIT, Inline};
pub use entitlements::{Entitlements, Feature, Tier};
pub use error::Error;
pub use format::{FileEntry, Format, FormatKind, Formats};
pub use ids::{BoardId, ItemId, ListId, TagId};
pub use item::{Item, ItemKind, ItemLabel};
pub use jiff::Timestamp;
pub use list::{List, ListAccent, ListIcon, ListSort, ListView};
pub use name::Name;
pub use placement::Placement;
pub use tag::Tag;
pub use usage::{PasteFailure, UseMode, UseOutcome};

/// The product name shown to users. The working name lives only here (SPEC header).
pub const APP_NAME: &str = "Snipdeck";
