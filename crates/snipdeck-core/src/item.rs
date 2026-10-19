use std::collections::BTreeSet;

use crate::{ItemId, ListId, TagId, Timestamp, UseMode};

keyed_enum! {
    /// What an item holds, which decides its leading element in the panel (DESIGN.md §4).
    ///
    /// Sensitivity is not a kind: it is [`Item::sensitive`], orthogonal to content, and the UI
    /// draws the lock glyph whenever it is set. Keeping it out of this enum means the two can
    /// never disagree, and un-marking an item restores its real kind.
    pub enum ItemKind ("item kind") {
        /// Plain text.
        Text = "text",
        /// Text with placeholders, resolved at use time.
        Template = "template",
        /// Source code, previewed in mono.
        Code = "code",
        /// A URL.
        Link = "link",
        /// An image.
        Image = "image",
        /// One or more files.
        File = "file",
    }
}

keyed_enum! {
    /// An item's colour label: a 4px bar on its card, and a filter in the Main window. The label
    /// is always shown with its name (tooltip, accessible description, filter chip text), so
    /// colour is never the only cue.
    pub enum ItemLabel ("item label") {
        /// Blue.
        Blue = "blue",
        /// Teal.
        Teal = "teal",
        /// Green.
        Green = "green",
        /// Amber.
        Amber = "amber",
        /// Orange.
        Orange = "orange",
        /// Red.
        Red = "red",
        /// Violet.
        Violet = "violet",
        /// Slate.
        Slate = "slate",
    }
}

/// One saved thing the user can paste (SPEC §4), without its content.
///
/// Content lives in [`Formats`](crate::Formats), which storage loads separately and only
/// when needed (use, edit, export): a board of 10 000 items must fit the 40 MB idle budget
/// (SPEC §9), and inline content alone can reach 64 KiB per format. Manual order within the
/// list is held by storage, not by this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Identity.
    pub id: ItemId,
    /// The list this item belongs to.
    pub list_id: ListId,
    /// Title shown on the card. May be empty, in which case the UI shows the preview.
    pub title: String,
    /// Content kind.
    pub kind: ItemKind,
    /// Start of the item's text, built with [`Item::preview_of`]; used for card previews and
    /// in-memory search. Empty for items without text.
    pub preview: String,
    /// Tags, in id order.
    pub tags: BTreeSet<TagId>,
    /// Shown in the favourites strip.
    pub pinned: bool,
    /// Masked, kept out of history, auto-cleared from the clipboard after use, and encrypted at
    /// rest with Supporter (SPEC §5.7).
    pub sensitive: bool,
    /// Always paste as plain text (SPEC §5.1a).
    pub always_plain: bool,
    /// How many times the item has been used.
    pub use_count: u32,
    /// When the item was last used.
    pub last_used_at: Option<Timestamp>,
    /// The colour label, if the user set one.
    pub label: Option<ItemLabel>,
    /// Name of the app the content was captured from, if known.
    pub source_app: Option<String>,
    /// When the item was created.
    pub created_at: Timestamp,
    /// When the item's title, content, tags or flags last changed. Using it doesn't count.
    pub updated_at: Timestamp,
}

impl Item {
    /// The most characters [`Item::preview_of`] keeps. At least the 64 that in-memory
    /// search matches against (SPEC §5.1), plus enough for three comfortable-view lines.
    pub const PREVIEW_CHARS: usize = 200;

    /// A new, unused, untagged item with a fresh id and an empty preview.
    pub fn new(list_id: ListId, title: String, kind: ItemKind, now: Timestamp) -> Self {
        Self {
            id: ItemId::generate(),
            list_id,
            title,
            kind,
            preview: String::new(),
            tags: BTreeSet::new(),
            pinned: false,
            sensitive: false,
            always_plain: false,
            use_count: 0,
            last_used_at: None,
            label: None,
            source_app: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// The preview to store for an item whose text is `text`: up to
    /// [`Item::PREVIEW_CHARS`] characters with leading whitespace dropped, or nothing at all
    /// for a sensitive item, so no plaintext of it is stored outside its (encrypted) content.
    pub fn preview_of(text: &str, sensitive: bool) -> String {
        if sensitive {
            return String::new();
        }
        text.trim_start()
            .chars()
            .take(Self::PREVIEW_CHARS)
            .collect()
    }

    /// The mode to use when the user asks for `requested`, honouring
    /// [`Item::always_plain`]: a paste becomes a plain-text paste; a copy stays a copy.
    pub fn effective_mode(&self, requested: UseMode) -> UseMode {
        match requested {
            UseMode::Paste if self.always_plain => UseMode::PastePlain,
            mode => mode,
        }
    }

    /// Counts one use at `at`. Does not touch [`Item::updated_at`].
    pub fn record_use(&mut self, at: Timestamp) {
        self.use_count = self.use_count.saturating_add(1);
        self.last_used_at = Some(at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Timestamp {
        Timestamp::from_second(1_800_000_000).unwrap()
    }

    fn item() -> Item {
        Item::new(
            ListId::generate(),
            "Signature".to_owned(),
            ItemKind::Text,
            now(),
        )
    }

    #[test]
    fn item_kind_keys_round_trip() {
        crate::key::assert_keys_round_trip(ItemKind::ALL, ItemKind::key);
    }

    #[test]
    fn item_label_keys_round_trip_and_there_are_eight() {
        crate::key::assert_keys_round_trip(ItemLabel::ALL, ItemLabel::key);
        assert_eq!(ItemLabel::ALL.len(), 8);
    }

    #[test]
    fn new_item_is_unused_and_unflagged() {
        let item = item();
        assert_eq!((item.use_count, item.last_used_at), (0, None));
        assert!(!item.pinned && !item.sensitive && !item.always_plain);
        assert!(item.tags.is_empty() && item.preview.is_empty() && item.label.is_none());
        assert_eq!((item.created_at, item.updated_at), (now(), now()));
    }

    #[test]
    fn preview_skips_leading_whitespace_and_caps_characters() {
        assert_eq!(
            Item::preview_of("\n\n  Dear team,\nThanks", false),
            "Dear team,\nThanks"
        );
        let long = "é".repeat(Item::PREVIEW_CHARS + 50);
        assert_eq!(
            Item::preview_of(&long, false).chars().count(),
            Item::PREVIEW_CHARS
        );
    }

    #[test]
    fn sensitive_items_have_no_preview() {
        assert_eq!(Item::preview_of("hunter2", true), "");
    }

    #[test]
    fn always_plain_turns_paste_into_plain_paste_only() {
        let mut item = item();
        assert_eq!(item.effective_mode(UseMode::Paste), UseMode::Paste);
        item.always_plain = true;
        assert_eq!(item.effective_mode(UseMode::Paste), UseMode::PastePlain);
        assert_eq!(
            item.effective_mode(UseMode::PastePlain),
            UseMode::PastePlain
        );
        assert_eq!(item.effective_mode(UseMode::Copy), UseMode::Copy);
    }

    #[test]
    fn record_use_counts_and_stamps_without_touching_updated_at() {
        let mut item = item();
        let later = Timestamp::from_second(1_800_000_060).unwrap();
        item.record_use(later);
        assert_eq!((item.use_count, item.last_used_at), (1, Some(later)));
        assert_eq!(item.updated_at, now());
    }

    #[test]
    fn use_count_saturates() {
        let mut item = item();
        item.use_count = u32::MAX;
        item.record_use(now());
        assert_eq!(item.use_count, u32::MAX);
    }
}
