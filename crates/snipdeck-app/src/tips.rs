//! The Tips tab of the Help window: ten cards, each with a picture and a "Try it" that runs one
//! [`Command`]. Slint-free and pure. The words (title, two lines, the button and the picture's
//! description) live in the UI crate's `TipTexts` global in the order of [`TIPS`], so they are
//! translated there; the pictures are crops of the golden renders under
//! `crates/snipdeck-ui/ui/images/tips/` (`scripts/tips_images.py` regenerates them).

use snipdeck_core::Feature;

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
    /// The picture's file name under `snipdeck-ui/ui/images/tips/`.
    pub image: &'static str,
    /// What "Try it" does.
    pub action: TipAction,
    /// The Supporter feature the card is about, when the Free plan lacks it: the card is marked and "Try it" opens
    /// the licence page instead of running into a wall.
    pub feature: Option<Feature>,
}

/// The cards, in the order a new user meets the features (the order of `TipTexts`).
pub const TIPS: [Tip; 10] = [
    Tip {
        image: "01-open-panel.png",
        action: TipAction::ShowPanel,

        feature: None,
    },
    Tip {
        image: "02-search.png",
        action: TipAction::Browse,

        feature: None,
    },
    Tip {
        image: "03-pinned.png",
        action: TipAction::GoPinned,

        feature: None,
    },
    Tip {
        image: "04-paste-next.png",
        action: TipAction::PasteNext,

        feature: Some(Feature::PasteStack),
    },
    Tip {
        image: "05-templates.png",
        action: TipAction::NewItem,

        feature: Some(Feature::PromptedFields),
    },
    Tip {
        image: "06-capture.png",
        action: TipAction::QuickCapture,

        feature: None,
    },
    Tip {
        image: "07-symbols.png",
        action: TipAction::Symbols,

        feature: None,
    },
    Tip {
        image: "08-commands.png",
        action: TipAction::Shortcuts,

        feature: None,
    },
    Tip {
        image: "09-view.png",
        action: TipAction::ViewTile,

        feature: None,
    },
    Tip {
        image: "10-sensitive.png",
        action: TipAction::Settings,

        feature: Some(Feature::MasterPassword),
    },
];

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn every_tip_has_an_image_that_exists() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../snipdeck-ui/ui/images/tips");
        let mut seen = Vec::new();
        for tip in TIPS {
            assert!(
                dir.join(tip.image).is_file(),
                "missing picture {}",
                tip.image
            );
            assert!(!seen.contains(&tip.image), "{} is used twice", tip.image);
            seen.push(tip.image);
        }
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
