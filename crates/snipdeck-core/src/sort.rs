//! How a view orders its items: one [`Sort`] for every section (SPEC §5.1, §5.3).
//!
//! A list stores its own sort; *All items*, *Pinned* and each tag view keep theirs in the
//! settings. Manual order is the order storage returns, so sorting is stable and every other key
//! falls back to it for items that compare equal.

use std::cmp::Ordering;

use crate::search::fold;
use crate::{Error, Item};

keyed_enum! {
    /// What a [`Sort`] orders by.
    pub enum SortKey ("sort key") {
        /// The user's drag order.
        Manual = "manual",
        /// The title (the first line of the preview for an untitled item), ignoring case and accents.
        Name = "name",
        /// When the item was made.
        Created = "created",
        /// When its title, content, tags or flags last changed.
        Modified = "modified",
        /// How many times it was used.
        MostUsed = "most-used",
        /// When it was last used; items never used go last.
        RecentlyUsed = "recently-used",
    }
}

impl SortKey {
    /// Whether the natural direction runs from the largest value down: newest, most, latest.
    /// Only *Name* (A to Z) and *Manual* run the other way.
    #[must_use]
    pub const fn natural_descending(self) -> bool {
        !matches!(self, Self::Manual | Self::Name)
    }

    /// Whether the key can be reversed (Manual is the user's order and has no direction).
    #[must_use]
    pub const fn has_direction(self) -> bool {
        !matches!(self, Self::Manual)
    }
}

/// An order: a key and a direction. `reversed` flips the key's natural direction (see
/// [`SortKey::natural_descending`]), so the stored words of the three older orders (`manual`,
/// `most-used`, `recently-used`) keep their meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sort {
    /// What it orders by.
    pub key: SortKey,
    /// The opposite of the key's natural direction. Always false for Manual.
    pub reversed: bool,
}

/// The stored suffix of a reversed sort.
const REVERSED_SUFFIX: &str = "-reversed";

impl Sort {
    /// The user's own order.
    pub const MANUAL: Self = Self::of(SortKey::Manual);
    /// Highest use count first.
    pub const MOST_USED: Self = Self::of(SortKey::MostUsed);
    /// Latest use first.
    pub const RECENTLY_USED: Self = Self::of(SortKey::RecentlyUsed);

    /// `key` in its natural direction.
    #[must_use]
    pub const fn of(key: SortKey) -> Self {
        Self {
            key,
            reversed: false,
        }
    }

    /// `key`, reversed when `reversed` (ignored for Manual).
    #[must_use]
    pub const fn new(key: SortKey, reversed: bool) -> Self {
        Self {
            key,
            reversed: reversed && key.has_direction(),
        }
    }

    /// Whether this is the user's own order.
    #[must_use]
    pub const fn is_manual(self) -> bool {
        matches!(self.key, SortKey::Manual)
    }

    /// Whether the values run from largest to smallest (newest first, most used first, Z to A).
    #[must_use]
    pub const fn descending(self) -> bool {
        self.key.natural_descending() != self.reversed
    }

    /// The same key in the other direction (Manual stays Manual).
    #[must_use]
    pub const fn flipped(self) -> Self {
        Self::new(self.key, !self.reversed)
    }

    /// The stable word for storage and settings: the key, then `-reversed` when flipped.
    #[must_use]
    pub fn key(self) -> String {
        if self.reversed {
            format!("{}{REVERSED_SUFFIX}", self.key.key())
        } else {
            self.key.key().to_owned()
        }
    }

    /// Reads [`Sort::key`] back.
    ///
    /// # Errors
    ///
    /// [`Error::Parse`] for a word that is not a sort.
    pub fn from_key(word: &str) -> Result<Self, Error> {
        let (base, reversed) = word
            .strip_suffix(REVERSED_SUFFIX)
            .map_or((word, false), |b| (b, true));
        match base.parse::<SortKey>() {
            Ok(key) if reversed && !key.has_direction() => Err(Error::Parse {
                what: "sort",
                value: word.to_owned(),
            }),
            Ok(key) => Ok(Self::new(key, reversed)),
            Err(_) => Err(Error::Parse {
                what: "sort",
                value: word.to_owned(),
            }),
        }
    }

    /// Orders `items` (given in manual order) by this sort. Equal items stay in manual order,
    /// and an item that was never used goes last in both directions of the use-based keys.
    pub fn apply(self, items: &mut [&Item]) {
        let reversed = self.descending();
        match self.key {
            SortKey::Manual => {}
            SortKey::Name => {
                items.sort_by_cached_key(|i| fold(display_title(i)));
                if reversed {
                    items.reverse();
                    // `reverse` flips ties too: put equal names back in manual order.
                    reorder_ties(items);
                }
            }
            SortKey::Created => {
                items.sort_by(|a, b| directed(a.created_at.cmp(&b.created_at), reversed));
            }
            SortKey::Modified => {
                items.sort_by(|a, b| directed(a.updated_at.cmp(&b.updated_at), reversed));
            }
            SortKey::MostUsed => {
                items.sort_by(|a, b| directed(a.use_count.cmp(&b.use_count), reversed));
            }
            SortKey::RecentlyUsed => items.sort_by(|a, b| match (a.last_used_at, b.last_used_at) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(x), Some(y)) => directed(x.cmp(&y), reversed),
            }),
        }
    }
}

impl std::fmt::Display for Sort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.key())
    }
}

/// `order` for ascending values, flipped for descending (largest first).
fn directed(order: Ordering, descending: bool) -> Ordering {
    if descending { order.reverse() } else { order }
}

/// The title the user sees: the title, else the first line of the preview.
fn display_title(item: &Item) -> &str {
    if item.title.trim().is_empty() {
        item.preview.lines().next().unwrap_or_default()
    } else {
        &item.title
    }
}

/// After a reversed name sort: runs of equal names (same folded title) were reversed with the
/// rest, so put each run back in its original relative order.
fn reorder_ties(items: &mut [&Item]) {
    let mut start = 0;
    while start < items.len() {
        let name = fold(display_title(items[start]));
        let mut end = start + 1;
        while end < items.len() && fold(display_title(items[end])) == name {
            end += 1;
        }
        items[start..end].reverse();
        start = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ItemKind, ListId, Timestamp};

    fn at(second: i64) -> Timestamp {
        Timestamp::from_second(second).unwrap()
    }

    fn item(list: ListId, title: &str, created: i64) -> Item {
        let mut i = Item::new(list, title.to_owned(), ItemKind::Text, at(created));
        i.updated_at = at(created);
        i
    }

    fn titles(items: &[&Item]) -> Vec<String> {
        items.iter().map(|i| i.title.clone()).collect()
    }

    #[test]
    fn every_sort_word_round_trips_and_the_old_words_keep_their_meaning() {
        for key in SortKey::ALL {
            for reversed in [false, true] {
                let sort = Sort::new(*key, reversed);
                assert_eq!(Sort::from_key(&sort.key()), Ok(sort), "{}", sort.key());
            }
        }
        assert_eq!(Sort::from_key("manual"), Ok(Sort::MANUAL));
        assert_eq!(Sort::from_key("most-used"), Ok(Sort::MOST_USED));
        assert_eq!(Sort::from_key("recently-used"), Ok(Sort::RECENTLY_USED));
        assert!(Sort::MOST_USED.descending());
        assert!(!Sort::of(SortKey::Name).descending());
        assert!(Sort::from_key("manual-reversed").is_err());
        assert!(Sort::from_key("random").is_err());
        assert!(Sort::from_key("").is_err());
        // A reversed Manual is not a thing: the constructor drops it.
        assert_eq!(Sort::new(SortKey::Manual, true), Sort::MANUAL);
    }

    #[test]
    fn name_sorts_ignore_case_and_accents_and_use_the_preview_of_an_untitled_item() {
        let list = ListId::generate();
        let mut untitled = item(list, "", 3);
        untitled.preview = "Brie\nsecond line".to_owned();
        let zed = item(list, "zebra", 1);
        let eclair = item(list, "Éclair", 2);
        let apple = item(list, "apple", 4);
        let all = [&zed, &eclair, &untitled, &apple];

        let mut items = all.to_vec();
        Sort::of(SortKey::Name).apply(&mut items);
        let names: Vec<&str> = items
            .iter()
            .map(|i| {
                if i.title.is_empty() {
                    "Brie"
                } else {
                    i.title.as_str()
                }
            })
            .collect();
        assert_eq!(names, ["apple", "Brie", "Éclair", "zebra"]);

        let mut items = all.to_vec();
        Sort::new(SortKey::Name, true).apply(&mut items);
        assert_eq!(titles(&items), ["zebra", "Éclair", "", "apple"]);
    }

    #[test]
    fn equal_names_stay_in_manual_order_in_both_directions() {
        let list = ListId::generate();
        let a1 = item(list, "same", 1);
        let a2 = item(list, "SAME", 2);
        let b = item(list, "other", 3);
        let all = [&a1, &a2, &b];
        let mut items = all.to_vec();
        Sort::of(SortKey::Name).apply(&mut items);
        assert_eq!(items[1].id, a1.id);
        assert_eq!(items[2].id, a2.id);
        let mut items = all.to_vec();
        Sort::new(SortKey::Name, true).apply(&mut items);
        assert_eq!(items[0].id, a1.id);
        assert_eq!(items[1].id, a2.id);
    }

    #[test]
    fn dates_run_newest_first_and_reverse_to_oldest_first() {
        let list = ListId::generate();
        let old = item(list, "old", 10);
        let mid = item(list, "mid", 20);
        let mut new = item(list, "new", 5);
        new.updated_at = at(99);
        let all = [&old, &mid, &new];

        let mut items = all.to_vec();
        Sort::of(SortKey::Created).apply(&mut items);
        assert_eq!(titles(&items), ["mid", "old", "new"]);
        Sort::new(SortKey::Created, true).apply(&mut items);
        assert_eq!(titles(&items), ["new", "old", "mid"]);
        Sort::of(SortKey::Modified).apply(&mut items);
        assert_eq!(titles(&items), ["new", "mid", "old"]);
        Sort::new(SortKey::Modified, true).apply(&mut items);
        assert_eq!(titles(&items), ["old", "mid", "new"]);
    }

    #[test]
    fn use_sorts_put_never_used_items_last_in_both_directions() {
        let list = ListId::generate();
        let mut busy = item(list, "busy", 1);
        busy.use_count = 9;
        busy.last_used_at = Some(at(500));
        let mut some = item(list, "some", 2);
        some.use_count = 3;
        some.last_used_at = Some(at(900));
        let never = item(list, "never", 3);
        let all = [&never, &busy, &some];

        let mut items = all.to_vec();
        Sort::MOST_USED.apply(&mut items);
        assert_eq!(titles(&items), ["busy", "some", "never"]);
        Sort::new(SortKey::MostUsed, true).apply(&mut items);
        assert_eq!(titles(&items), ["never", "some", "busy"]);

        let mut items = all.to_vec();
        Sort::RECENTLY_USED.apply(&mut items);
        assert_eq!(titles(&items), ["some", "busy", "never"]);
        Sort::new(SortKey::RecentlyUsed, true).apply(&mut items);
        assert_eq!(titles(&items), ["busy", "some", "never"]);
    }

    #[test]
    fn manual_leaves_the_order_alone() {
        let list = ListId::generate();
        let b = item(list, "b", 2);
        let a = item(list, "a", 1);
        let mut items = vec![&b, &a];
        Sort::MANUAL.apply(&mut items);
        assert_eq!(titles(&items), ["b", "a"]);
        assert!(Sort::MANUAL.is_manual());
        assert_eq!(Sort::MANUAL.flipped(), Sort::MANUAL);
    }
}
