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
mod error;
mod format;
mod group;
mod ids;
mod item;
mod list;
mod name;
mod placement;
mod sort;
mod tag;
mod trigger;
mod usage;
mod view_filter;

pub mod search;
pub mod template;
pub mod text_tools;

pub use board::Board;
pub use content::{BlobRef, Content, INLINE_LIMIT, Inline};
pub use error::Error;
pub use format::{FileEntry, Format, FormatKind, Formats};
pub use group::{Group, GroupTree, MAX_GROUP_DEPTH};
pub use ids::{BoardId, GroupId, ItemId, ListId, TagId};
pub use item::{Item, ItemKind, ItemLabel};
pub use jiff::Timestamp;
pub use list::{List, ListAccent, ListIcon, ListView};
pub use name::Name;
pub use placement::Placement;
pub use sort::{Sort, SortKey};
pub use tag::{Tag, TagTree};
pub use trigger::{
    BUFFER_CHARS, Candidate, CaseMode, CaseShape, Fired, MIN_PLAIN_CHARS, Matcher,
    SUGGEST_MIN_CHARS, Suggestion, Trigger, TriggerEnd, TriggerOptions, prefix_of,
};
pub use usage::{PasteFailure, UseMode, UseOutcome};
pub use view_filter::{TagMatch, ViewFilter};

/// The product name shown to users. The working name lives only here (SPEC header).
pub const APP_NAME: &str = "Snipdeck";
