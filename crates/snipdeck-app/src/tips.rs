//! The Tips tab of the Help window: ten cards, each with a picture and a "Try it" that runs one
//! [`Command`]. Slint-free and pure. The words (title, two lines, the button and the picture's
//! description) live in the UI crate's `TipTexts` global in the order of [`TIPS`], so they are
//! translated there; the pictures are drawn in the UI crate's `tip_art.slint`
//! (wordless, in the theme's colours), the card at index `i` showing art `i`.

use crate::commands::Command;

/// What a card's "Try it" does. Each maps to one [`Command`] the palette also offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipAction {
    /// Show the Panel.
    ShowPanel,
    /// Open All items in the Main window (where typing in the search box filters every list).
    Browse,
    /// Open Pinned.
    GoPinned,
    /// Paste the next item.
    PasteNext,
    /// Start a new snippet.
    NewItem,
    /// Quick capture.
    QuickCapture,
    /// The Panel's Symbols tab.
    Symbols,
    /// The keyboard shortcuts (the palette itself is not a command).
    Shortcuts,
    /// Tiles.
    ViewTile,
    /// Settings.
    Settings,
}

impl TipAction {
    /// The command this action runs.
    #[must_use]
    pub const fn command(self) -> Command {
        match self {
            Self::ShowPanel => Command::ShowPanel,
            Self::Browse => Command::GoAll,
            Self::GoPinned => Command::GoPinned,
            Self::PasteNext => Command::PasteNext,
            Self::NewItem => Command::NewItem,
            Self::QuickCapture => Command::QuickCapture,
            Self::Symbols => Command::Symbols,
            Self::Shortcuts => Command::Shortcuts,
            Self::ViewTile => Command::ViewTile,
            Self::Settings => Command::Settings,
        }
    }

    /// Whether the command shows something in the Main window, so the Main window must come
    /// forward for the user to see it work.
    #[must_use]
    pub const fn opens_main(self) -> bool {
        matches!(self, Self::Browse | Self::GoPinned | Self::ViewTile)
    }
}

/// One card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tip {
    /// What "Try it" does.
    pub action: TipAction,
}

/// The cards, in the order a new user meets the features (the order of `TipTexts`).
pub const TIPS: [Tip; 11] = [
    Tip {
        action: TipAction::ShowPanel,
    },
    Tip {
        action: TipAction::Browse,
    },
    Tip {
        action: TipAction::GoPinned,
    },
    Tip {
        action: TipAction::PasteNext,
    },
    Tip {
        action: TipAction::NewItem,
    },
    Tip {
        action: TipAction::QuickCapture,
    },
    Tip {
        action: TipAction::Symbols,
    },
    Tip {
        action: TipAction::Shortcuts,
    },
    Tip {
        action: TipAction::ViewTile,
    },
    Tip {
        action: TipAction::Settings,
    },
    Tip {
        action: TipAction::Settings,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every card has its own drawing: `TipArt` has one branch per index, so the index of the last card
    /// must be the last branch (checked in the UI crate's `tip_art` test).
    #[test]
    fn there_is_one_picture_per_card() {
        assert_eq!(TIPS.len(), 11);
    }

    #[test]
    fn every_action_maps_to_a_command() {
        for tip in TIPS {
            let command = tip.action.command();
            assert!(
                Command::ALL.contains(&command),
                "{command:?} is not offered"
            );
            assert!(
                !command.needs_item(),
                "{command:?} needs an item under the cursor"
            );
            assert_eq!(Command::from_id(command.id()), Some(command));
        }
    }
}
