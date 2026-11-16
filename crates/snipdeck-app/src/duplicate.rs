//! Duplicate detection (SPEC 5.4): a text about to be saved as a new item that is already saved.
//!
//! Slint-free. The search is a scan of the live items' previews (the first 200 characters, kept
//! with every item and never for a Sensitive one) and a full comparison of the few that agree,
//! so it reads no text of the rest.

use snipdeck_core::{Item, ItemId, ItemKind, ListId};
use snipdeck_store::{BlobStore, ItemRepo, ListRepo};

use crate::{item_editor, usecase};

/// An item that already holds the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duplicate {
    /// The item that has it.
    pub item: ItemId,
    /// Its title.
    pub title: String,
    /// The list it is in.
    pub list: ListId,
    /// That list's name.
    pub list_name: String,
}

/// The text as compared: blanks at the ends and runs of blanks do not make a difference in a
/// preview, and the ends of the text do not either.
fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The live, non-sensitive text item that holds `text` (ends trimmed), if any.
///
/// A store that cannot be read finds none: a missed duplicate costs a copy, a failed Save costs
/// the user's text.
#[must_use]
pub fn find_text<S: ItemRepo + BlobStore + ListRepo>(store: &S, text: &str) -> Option<Duplicate> {
    let wanted = text.trim();
    if wanted.is_empty() {
        return None;
    }
    let head = squash(&Item::preview_of(wanted, false));
    let items = store.all_items().ok()?;
    let found = items.into_iter().find(|item| {
        if item.sensitive || !item_editor::is_text_kind(item.kind) || item.kind == ItemKind::Image {
            return false;
        }
        let theirs = squash(&item.preview);
        // Both are the start of their text: one must begin with the other.
        (theirs.starts_with(&head) || head.starts_with(&theirs))
            && usecase::item_text(store, item.id)
                .ok()
                .flatten()
                .is_some_and(|t| t.trim() == wanted)
    })?;
    let list = store.list(found.list_id).ok()?;
    // A title is optional: the card shows the start of the text instead.
    let title = if found.title.trim().is_empty() {
        squash(&found.preview).chars().take(40).collect()
    } else {
        found.title
    };
    Some(Duplicate {
        item: found.id,
        title,
        list: found.list_id,
        list_name: list.name.as_str().to_owned(),
    })
}
