//! What the user's tier unlocks (SPEC §6). Pure data: the licence module turns a verified
//! licence key into [`Entitlements`]; the UI asks them before offering a Pro action.

use crate::Error;

/// The user's licence tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// No licence key.
    Free,
    /// A valid Pro licence key.
    Pro,
}

keyed_enum! {
    /// A capability Free users don't have. The key names the upsell sheet to show.
    pub enum Feature ("feature") {
        /// More than [`Entitlements::FREE_BOARD_LIMIT`] boards.
        UnlimitedBoards = "unlimited-boards",
        /// Prompted template placeholders: `{field:…}` and `{choice:…}`.
        PromptedFields = "prompted-fields",
        /// Encrypting sensitive items at rest, optionally behind a master password.
        SensitiveEncryption = "sensitive-encryption",
        /// Detaching a list into its own floating panel.
        FloatingLists = "floating-lists",
        /// Queueing items and pasting them one by one with the Paste next hotkey.
        PasteStack = "paste-stack",
    }
}

/// What the current user may do.
///
/// Losing Pro never hides or deletes data: a Free user with more boards than the limit keeps
/// them all and only can't create more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entitlements {
    tier: Tier,
}

impl Entitlements {
    /// How many boards a Free user can create.
    pub const FREE_BOARD_LIMIT: usize = 3;

    /// The entitlements of `tier`.
    pub const fn new(tier: Tier) -> Self {
        Self { tier }
    }

    /// The tier these entitlements come from.
    pub const fn tier(self) -> Tier {
        self.tier
    }

    /// Whether `feature` is available.
    pub const fn allows(self, feature: Feature) -> bool {
        match self.tier {
            Tier::Pro => true,
            // Every `Feature` is Pro-only today; list them so a new one is a deliberate choice.
            Tier::Free => match feature {
                Feature::UnlimitedBoards
                | Feature::PromptedFields
                | Feature::SensitiveEncryption
                | Feature::FloatingLists
                | Feature::PasteStack => false,
            },
        }
    }

    /// Like [`Entitlements::allows`], as a `Result` for `?`.
    ///
    /// # Errors
    ///
    /// [`Error::ProRequired`] if `feature` is not available.
    pub const fn require(self, feature: Feature) -> Result<(), Error> {
        if self.allows(feature) {
            Ok(())
        } else {
            Err(Error::ProRequired(feature))
        }
    }

    /// Whether another board may be created when `existing` live boards exist.
    pub const fn can_create_board(self, existing: usize) -> bool {
        existing < Self::FREE_BOARD_LIMIT || self.allows(Feature::UnlimitedBoards)
    }
}

impl Default for Entitlements {
    /// Free, until a licence key says otherwise.
    fn default() -> Self {
        Self::new(Tier::Free)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FREE: Entitlements = Entitlements::new(Tier::Free);
    const PRO: Entitlements = Entitlements::new(Tier::Pro);

    #[test]
    fn feature_keys_round_trip() {
        crate::key::assert_keys_round_trip(Feature::ALL, Feature::key);
    }

    #[test]
    fn default_is_free() {
        assert_eq!(Entitlements::default().tier(), Tier::Free);
    }

    #[test]
    fn free_has_no_pro_features_and_pro_has_all() {
        for &feature in Feature::ALL {
            assert!(!FREE.allows(feature));
            assert_eq!(FREE.require(feature), Err(Error::ProRequired(feature)));
            assert!(PRO.allows(feature));
            assert_eq!(PRO.require(feature), Ok(()));
        }
    }

    #[test]
    fn free_can_create_three_boards() {
        assert!(FREE.can_create_board(0));
        assert!(FREE.can_create_board(2));
        assert!(!FREE.can_create_board(3));
        assert!(!FREE.can_create_board(7));
        assert!(PRO.can_create_board(1000));
    }
}
