//! The item menu both windows offer (UX-19). The rows they share, in order, are defined once
//! here; each window's own rows are listed with the reason it has them.

use super::ItemMenuEntry;

/// A row both the Main window and the Panel offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shared {
    /// Use the item.
    Use,
    /// Copy the item.
    Copy,
    /// "Paste as...".
    PasteAs,
    /// "Open in Colours" (colour items, Colours tab on).
    OpenInColours,
    /// Edit.
    Edit,
    /// Duplicate.
    Duplicate,
    /// "Move to list...".
    MoveTo,
    /// Pin or Unpin.
    Pin,
    /// Move up.
    MoveUp,
    /// Move down.
    MoveDown,
    /// "Group...".
    Group,
    /// Move to Trash.
    Trash,
}

/// One shared row: its kind and the `MenuTexts` function that words it in both windows.
#[derive(Debug, Clone, Copy)]
pub struct SharedRow {
    /// Which row.
    pub kind: Shared,
    /// The text function (in `menu_texts.slint` or `toast_texts.slint`).
    pub words: &'static str,
}

/// The shared rows in the order both menus draw them (separators sit after Open in Colours
/// and after Group).
pub const SHARED: &[SharedRow] = &[
    SharedRow {
        kind: Shared::Use,
        words: "item-use",
    },
    SharedRow {
        kind: Shared::Copy,
        words: "item-copy",
    },
    SharedRow {
        kind: Shared::PasteAs,
        words: "item-paste-as",
    },
    SharedRow {
        kind: Shared::OpenInColours,
        words: "item-open-in-colours",
    },
    SharedRow {
        kind: Shared::Edit,
        words: "item-edit",
    },
    SharedRow {
        kind: Shared::Duplicate,
        words: "item-duplicate",
    },
    SharedRow {
        kind: Shared::MoveTo,
        words: "item-move-to",
    },
    SharedRow {
        kind: Shared::Pin,
        words: "item-pin",
    },
    SharedRow {
        kind: Shared::MoveUp,
        words: "menu-move-up",
    },
    SharedRow {
        kind: Shared::MoveDown,
        words: "menu-move-down",
    },
    SharedRow {
        kind: Shared::Group,
        words: "item-group",
    },
    SharedRow {
        kind: Shared::Trash,
        words: "item-trash",
    },
];

/// Rows only the Main window has, with the reason.
pub const MAIN_ONLY: &[(&str, &str)] = &[(
    "Tags…",
    "tags are chosen from a chooser the Panel does not have; the Panel shows no tag UI",
)];

/// Rows only the Panel has, with the reason.
pub const PANEL_ONLY: &[(&str, &str)] = &[
    (
        "Colour",
        "Main sets the colour label in the item editor and the label filter",
    ),
    ("Sensitive", "Main sets the flag in the item editor"),
    (
        "Stack",
        "the paste stack is the Panel's staging area; Main has no stack",
    ),
];

/// Which shared row `entry` is, if it is one.
#[must_use]
pub const fn shared_of(entry: ItemMenuEntry) -> Option<Shared> {
    Some(match entry {
        ItemMenuEntry::Use => Shared::Use,
        ItemMenuEntry::Copy => Shared::Copy,
        ItemMenuEntry::PasteAs => Shared::PasteAs,
        ItemMenuEntry::OpenInColours => Shared::OpenInColours,
        ItemMenuEntry::Edit => Shared::Edit,
        ItemMenuEntry::Duplicate => Shared::Duplicate,
        ItemMenuEntry::MoveTo => Shared::MoveTo,
        ItemMenuEntry::Pin { .. } => Shared::Pin,
        ItemMenuEntry::MoveUp { .. } => Shared::MoveUp,
        ItemMenuEntry::MoveDown { .. } => Shared::MoveDown,
        ItemMenuEntry::Group => Shared::Group,
        ItemMenuEntry::Trash => Shared::Trash,
        _ => return None,
    })
}
