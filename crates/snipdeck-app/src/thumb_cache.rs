//! Decoded picture thumbnails of image items, kept for the Main window's rows (MAIN-21) and its
//! Detail pane.
//!
//! A row asks for its thumbnail when the list draws it (the window's list is virtualised, so
//! only rows on screen ask), one decode per item, and the answer is kept by item id for as long
//! as the item is unchanged. The cache holds at most [`ROW_CAPACITY`] pictures of at most
//! [`snipdeck_store::THUMB_MAX_BYTES`] bytes each; the oldest entry goes first. A picture that
//! cannot be decoded is remembered too, so a bad one is not decoded again on every redraw.

use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use snipdeck_core::{ItemId, Timestamp};
use snipdeck_store::RecentThumb;

/// How many row thumbnails are kept (about 3.5 MB at the largest size).
pub const ROW_CAPACITY: usize = 96;
/// How many pictures one pass of the window's queue decodes before it lets the window draw.
pub const DECODES_PER_PASS: usize = 8;
/// The longest side of the Detail pane's picture, in pixels.
pub const DETAIL_SIDE: u32 = 320;

/// A thumbnail shared between the cache and the rows that show it. Two are equal when they are
/// the same picture (a row is redrawn when its picture changed, never by comparing pixels).
#[derive(Debug, Clone)]
pub struct Thumb(Rc<RecentThumb>);

impl Thumb {
    /// Wraps a decoded picture.
    #[must_use]
    pub fn new(thumb: RecentThumb) -> Self {
        Self(Rc::new(thumb))
    }

    /// The pixels.
    #[must_use]
    pub fn pixels(&self) -> &RecentThumb {
        &self.0
    }
}

impl PartialEq for Thumb {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Thumb {}

/// What the cache knows about one item.
#[derive(Debug)]
struct Entry {
    /// The item's `updated_at` when the picture was decoded: a changed item decodes again.
    stamp: Timestamp,
    /// `None`: the item has no picture that can be shown.
    thumb: Option<Thumb>,
}

/// A bounded cache of thumbnails by item id.
#[derive(Debug)]
pub struct ThumbCache {
    capacity: usize,
    entries: HashMap<ItemId, Entry>,
    /// Item ids, oldest first.
    order: VecDeque<ItemId>,
}

impl ThumbCache {
    /// A cache that keeps at most `capacity` items (at least one).
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    /// What is known about `id` as of `stamp`: `None` when it was never decoded (or the item
    /// changed since), `Some(None)` when it has no picture to show.
    #[must_use]
    pub fn get(&self, id: ItemId, stamp: Timestamp) -> Option<Option<&Thumb>> {
        self.entries
            .get(&id)
            .filter(|e| e.stamp == stamp)
            .map(|e| e.thumb.as_ref())
    }

    /// Keeps the answer for `id` as of `stamp`, dropping the oldest item when the cache is full.
    pub fn put(&mut self, id: ItemId, stamp: Timestamp, thumb: Option<Thumb>) {
        if self.entries.insert(id, Entry { stamp, thumb }).is_some() {
            self.order.retain(|kept| *kept != id);
        }
        self.order.push_back(id);
        while self.entries.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            } else {
                break;
            }
        }
    }

    /// How many items are kept.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is kept.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pic(side: u32) -> Thumb {
        Thumb::new(RecentThumb {
            width: side,
            height: side,
            rgba: vec![0; (side * side * 4) as usize],
        })
    }

    fn stamp(second: i64) -> Timestamp {
        Timestamp::from_second(second).expect("a valid second")
    }

    #[test]
    fn a_decoded_picture_is_found_until_the_item_changes() {
        let mut cache = ThumbCache::new(4);
        let id = ItemId::generate();
        assert!(cache.get(id, stamp(1)).is_none(), "never decoded");
        cache.put(id, stamp(1), Some(pic(2)));
        assert!(matches!(cache.get(id, stamp(1)), Some(Some(_))));
        assert!(
            cache.get(id, stamp(2)).is_none(),
            "an item changed since the decode decodes again"
        );
    }

    #[test]
    fn a_picture_that_cannot_be_shown_is_remembered_as_none() {
        let mut cache = ThumbCache::new(4);
        let id = ItemId::generate();
        cache.put(id, stamp(1), None);
        assert!(matches!(cache.get(id, stamp(1)), Some(None)));
    }

    #[test]
    fn the_cache_never_holds_more_than_its_capacity_and_drops_the_oldest() {
        let mut cache = ThumbCache::new(3);
        let ids: Vec<ItemId> = (0..5).map(|_| ItemId::generate()).collect();
        for (n, id) in ids.iter().enumerate() {
            cache.put(*id, stamp(1), Some(pic(1)));
            assert!(cache.len() <= 3, "after {n}");
        }
        assert_eq!(cache.len(), 3);
        assert!(cache.get(ids[0], stamp(1)).is_none());
        assert!(cache.get(ids[1], stamp(1)).is_none());
        assert!(cache.get(ids[4], stamp(1)).is_some());
    }

    #[test]
    fn putting_an_item_again_keeps_one_entry_and_makes_it_the_newest() {
        let mut cache = ThumbCache::new(2);
        let (a, b, c) = (ItemId::generate(), ItemId::generate(), ItemId::generate());
        cache.put(a, stamp(1), Some(pic(1)));
        cache.put(b, stamp(1), Some(pic(1)));
        cache.put(a, stamp(2), Some(pic(1)));
        assert_eq!(cache.len(), 2);
        cache.put(c, stamp(1), Some(pic(1)));
        assert!(cache.get(b, stamp(1)).is_none(), "b was the oldest");
        assert!(cache.get(a, stamp(2)).is_some());
    }

    #[test]
    fn two_thumbs_are_equal_only_when_they_are_the_same_picture() {
        let a = pic(1);
        assert_eq!(a, a.clone());
        assert_ne!(a, pic(1));
    }

    #[test]
    fn the_row_cache_stays_within_its_memory_bound() {
        // The bound the module promises: ROW_CAPACITY pictures of at most THUMB_MAX_BYTES.
        assert!(ROW_CAPACITY * snipdeck_store::THUMB_MAX_BYTES <= 8 * 1024 * 1024);
    }
}
