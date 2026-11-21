//! The filter beside a section (SPEC §5.3): which of the section's items are shown.
//!
//! One model for the Main window's filter chips and for the Picker's scope. Categories combine
//! with AND (a tag *and* a kind *and* a flag); inside a category the choices are alternatives,
//! except tags, which the person can ask to match all or any. `Untagged` and a tag choice are
//! exclusive: asking for tags drops "untagged" and the other way round, so no state is empty by
//! construction.

use std::collections::BTreeSet;

use crate::{Item, ItemKind, ItemLabel, TagId, TagTree};

/// How a set of chosen tags is read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TagMatch {
    /// The item has at least one of the tags.
    #[default]
    Any,
    /// The item has every one of the tags.
    All,
}

/// A filter over the items of a view.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent flags the person ticks one by one"
)]
pub struct ViewFilter {
    /// The chosen tags (empty: no tag filter).
    pub tags: BTreeSet<TagId>,
    /// Whether the chosen tags are alternatives or all required.
    pub tag_match: TagMatch,
    /// Only items with no tag at all.
    pub untagged: bool,
    /// Only these kinds (empty: every kind).
    pub kinds: Vec<ItemKind>,
    /// Only items with a typed trigger.
    pub has_trigger: bool,
    /// Only Sensitive items.
    pub sensitive: bool,
    /// Only pinned items.
    pub pinned: bool,
    /// Only items with this colour label.
    pub label: Option<ItemLabel>,
}

impl ViewFilter {
    /// A filter that lets everything through.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Only pinned items (the Picker's *Pinned* scope).
    #[must_use]
    pub fn pinned_only() -> Self {
        Self {
            pinned: true,
            ..Self::default()
        }
    }

    /// Whether nothing is filtered.
    #[must_use]
    pub fn is_none(&self) -> bool {
        *self == Self::default()
    }

    /// How many separate choices are active (a chip each): each tag, untagged, each kind, and
    /// each flag and the label.
    #[must_use]
    pub fn active(&self) -> usize {
        self.tags.len()
            + usize::from(self.untagged)
            + self.kinds.len()
            + usize::from(self.has_trigger)
            + usize::from(self.sensitive)
            + usize::from(self.pinned)
            + usize::from(self.label.is_some())
    }

    /// Ticks or unticks a tag; ticking one clears "untagged".
    pub fn toggle_tag(&mut self, tag: TagId) {
        if !self.tags.remove(&tag) {
            self.tags.insert(tag);
            self.untagged = false;
        }
    }

    /// Ticks or unticks "untagged"; ticking it clears the chosen tags.
    pub fn toggle_untagged(&mut self) {
        self.untagged = !self.untagged;
        if self.untagged {
            self.tags.clear();
        }
    }

    /// Ticks or unticks a kind.
    pub fn toggle_kind(&mut self, kind: ItemKind) {
        if let Some(at) = self.kinds.iter().position(|k| *k == kind) {
            self.kinds.remove(at);
        } else {
            self.kinds.push(kind);
        }
    }

    /// Whether `item` passes. `has_trigger` says whether the item has a typed trigger (the
    /// item does not carry that itself). A chosen tag is carried by an item that has it or any
    /// tag nested under it in `tree`.
    #[must_use]
    pub fn matches(&self, item: &Item, has_trigger: bool, tree: &TagTree) -> bool {
        if self.untagged && !item.tags.is_empty() {
            return false;
        }
        if !self.tags.is_empty() {
            let carries = |t: &TagId| {
                item.tags.contains(t) || item.tags.iter().any(|own| tree.is_under(*own, *t))
            };
            let ok = match self.tag_match {
                TagMatch::Any => self.tags.iter().any(carries),
                TagMatch::All => self.tags.iter().all(carries),
            };
            if !ok {
                return false;
            }
        }
        (self.kinds.is_empty() || self.kinds.contains(&item.kind))
            && (!self.has_trigger || has_trigger)
            && (!self.sensitive || item.sensitive)
            && (!self.pinned || item.pinned)
            && self.label.is_none_or(|l| item.label == Some(l))
    }

    /// Forgets a tag that no longer exists.
    pub fn forget_tag(&mut self, tag: TagId) {
        self.tags.remove(&tag);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ListId, Timestamp};

    fn item(kind: ItemKind, tags: &[TagId]) -> Item {
        let mut i = Item::new(
            ListId::generate(),
            "t".to_owned(),
            kind,
            Timestamp::from_second(1).unwrap(),
        );
        i.tags = tags.iter().copied().collect();
        i
    }

    #[test]
    fn an_empty_filter_lets_everything_through() {
        let f = ViewFilter::none();
        assert!(f.is_none());
        assert_eq!(f.active(), 0);
        assert!(f.matches(&item(ItemKind::Image, &[]), false, &TagTree::default()));
    }

    #[test]
    fn tags_match_any_or_all() {
        let (a, b) = (TagId::generate(), TagId::generate());
        let only_a = item(ItemKind::Text, &[a]);
        let both = item(ItemKind::Text, &[a, b]);
        let none = item(ItemKind::Text, &[]);
        let mut f = ViewFilter::none();
        f.toggle_tag(a);
        f.toggle_tag(b);
        assert_eq!(f.active(), 2);
        assert!(
            f.matches(&only_a, false, &TagTree::default()),
            "any: one tag is enough"
        );
        assert!(f.matches(&both, false, &TagTree::default()));
        assert!(!f.matches(&none, false, &TagTree::default()));
        f.tag_match = TagMatch::All;
        assert!(
            !f.matches(&only_a, false, &TagTree::default()),
            "all: needs both"
        );
        assert!(f.matches(&both, false, &TagTree::default()));
    }

    #[test]
    fn untagged_and_tags_exclude_each_other() {
        let a = TagId::generate();
        let mut f = ViewFilter::none();
        f.toggle_tag(a);
        f.toggle_untagged();
        assert!(f.untagged);
        assert_eq!(f.tags.len(), 0);
        assert!(f.matches(&item(ItemKind::Text, &[]), false, &TagTree::default()));
        assert!(!f.matches(&item(ItemKind::Text, &[a]), false, &TagTree::default()));
        f.toggle_tag(a);
        assert!(!f.untagged, "choosing a tag ends the untagged filter");
        f.toggle_tag(a);
        assert!(f.is_none());
    }

    #[test]
    fn kinds_are_alternatives_and_categories_combine_with_and() {
        let a = TagId::generate();
        let mut f = ViewFilter::none();
        f.toggle_kind(ItemKind::Link);
        f.toggle_kind(ItemKind::Image);
        assert!(f.matches(&item(ItemKind::Link, &[]), false, &TagTree::default()));
        assert!(f.matches(&item(ItemKind::Image, &[]), false, &TagTree::default()));
        assert!(!f.matches(&item(ItemKind::Text, &[]), false, &TagTree::default()));
        f.toggle_tag(a);
        assert!(
            !f.matches(&item(ItemKind::Link, &[]), false, &TagTree::default()),
            "tag and kind"
        );
        assert!(f.matches(&item(ItemKind::Link, &[a]), false, &TagTree::default()));
        f.toggle_kind(ItemKind::Link);
        assert!(!f.matches(&item(ItemKind::Link, &[a]), false, &TagTree::default()));
    }

    #[test]
    fn flags_trigger_and_label_each_narrow_the_view() {
        let mut plain = item(ItemKind::Text, &[]);
        let mut f = ViewFilter {
            has_trigger: true,
            ..ViewFilter::none()
        };
        assert!(!f.matches(&plain, false, &TagTree::default()));
        assert!(f.matches(&plain, true, &TagTree::default()));
        f = ViewFilter {
            sensitive: true,
            ..ViewFilter::none()
        };
        assert!(!f.matches(&plain, false, &TagTree::default()));
        plain.sensitive = true;
        assert!(f.matches(&plain, false, &TagTree::default()));
        f = ViewFilter::pinned_only();
        assert!(!f.matches(&plain, false, &TagTree::default()));
        plain.pinned = true;
        assert!(f.matches(&plain, false, &TagTree::default()));
        f = ViewFilter {
            label: Some(ItemLabel::Red),
            ..ViewFilter::none()
        };
        assert!(!f.matches(&plain, false, &TagTree::default()));
        plain.label = Some(ItemLabel::Red);
        assert!(f.matches(&plain, false, &TagTree::default()));
    }

    #[test]
    fn a_chosen_tag_also_finds_items_tagged_with_a_tag_nested_under_it() {
        use crate::{Name, Tag};
        let work = Tag::new(Name::new("work").unwrap());
        let invoices = Tag {
            parent: Some(work.id),
            ..Tag::new(Name::new("invoices").unwrap())
        };
        let other = Tag::new(Name::new("other").unwrap());
        let tree = TagTree::new([&work, &invoices, &other]);
        let under = item(ItemKind::Text, &[invoices.id]);
        let mut f = ViewFilter::none();
        f.toggle_tag(work.id);
        assert!(f.matches(&under, false, &tree), "work finds work/invoices");
        assert!(
            !f.matches(&under, false, &TagTree::default()),
            "flat: it does not"
        );
        let mut f = ViewFilter::none();
        f.toggle_tag(invoices.id);
        assert!(
            !f.matches(&item(ItemKind::Text, &[work.id]), false, &tree),
            "not upwards"
        );
        f.toggle_tag(other.id);
        f.tag_match = TagMatch::All;
        assert!(!f.matches(&under, false, &tree), "all: other is missing");
    }
}
