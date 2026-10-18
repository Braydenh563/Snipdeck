//! "Paste next": the hotkey that pastes the following item of the pinned strip or the shown
//! list, one press at a time.
//!
//! The order is copied when a run starts and kept until it ends, because pasting counts a use
//! and a list sorted by use would otherwise reshuffle under the run. A run ends when the order
//! comes to its end (the next press says so and the one after starts again) or when the source
//! changes (another board's pinned strip, another list).

use snipdeck_core::{BoardId, ItemId, ListId};

/// What a run walks through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The pinned items of a board (when it has any).
    Pinned(BoardId),
    /// The items of a list, in the order the Panel shows them.
    List(ListId),
}

/// What a press should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// Paste this item, then call [`PasteQueue::used`].
    Item(ItemId),
    /// The run had already pasted everything; the next press starts again.
    Finished,
    /// The source has no items.
    Empty,
}

/// The position of the current run.
#[derive(Debug, Default)]
pub struct PasteQueue {
    source: Option<Source>,
    order: Vec<ItemId>,
    next: usize,
    /// The item handed out by the last [`peek`](Self::peek), waiting to be reported used.
    offered: Option<ItemId>,
}

impl PasteQueue {
    /// The item the next press pastes. `order` is called only when a run starts (the first
    /// press, a changed `source`, or after a run ended) and gives the source's items in order.
    pub fn peek(&mut self, source: Source, order: impl FnOnce() -> Vec<ItemId>) -> Next {
        if self.source != Some(source) {
            self.source = Some(source);
            self.order = order();
            self.next = 0;
        }
        self.offered = None;
        match self.order.get(self.next) {
            Some(&item) => {
                self.offered = Some(item);
                Next::Item(item)
            }
            None if self.order.is_empty() => {
                self.reset();
                Next::Empty
            }
            None => {
                self.reset();
                Next::Finished
            }
        }
    }

    /// `item` was pasted. Advances the run if it is the item [`peek`](Self::peek) offered;
    /// anything else (a paste that failed is never reported, a click in the Panel is someone
    /// else's) leaves the position alone.
    pub fn used(&mut self, item: ItemId) {
        if self.offered == Some(item) {
            self.offered = None;
            self.next += 1;
        }
    }

    /// Forgets the run; the next press starts again from the first item.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(n: usize) -> Vec<ItemId> {
        (0..n).map(|_| ItemId::generate()).collect()
    }

    fn press(queue: &mut PasteQueue, source: Source, order: &[ItemId]) -> Next {
        let next = queue.peek(source, || order.to_vec());
        if let Next::Item(item) = next {
            queue.used(item);
        }
        next
    }

    #[test]
    fn presses_walk_the_order_then_say_finished_then_start_again() {
        let source = Source::List(ListId::generate());
        let order = ids(2);
        let mut queue = PasteQueue::default();
        assert_eq!(press(&mut queue, source, &order), Next::Item(order[0]));
        assert_eq!(press(&mut queue, source, &order), Next::Item(order[1]));
        assert_eq!(press(&mut queue, source, &order), Next::Finished);
        assert_eq!(press(&mut queue, source, &order), Next::Item(order[0]));
    }

    #[test]
    fn a_paste_that_did_not_happen_is_offered_again() {
        let source = Source::List(ListId::generate());
        let order = ids(2);
        let mut queue = PasteQueue::default();
        assert_eq!(queue.peek(source, || order.clone()), Next::Item(order[0]));
        // No `used`: the clipboard was busy.
        assert_eq!(queue.peek(source, || order.clone()), Next::Item(order[0]));
    }

    #[test]
    fn the_order_is_copied_once_per_run() {
        let source = Source::List(ListId::generate());
        let first = ids(2);
        let mut queue = PasteQueue::default();
        assert_eq!(press(&mut queue, source, &first), Next::Item(first[0]));
        // The list was re-sorted by use meanwhile: the run keeps its own order.
        let reshuffled = vec![first[1], first[0]];
        assert_eq!(press(&mut queue, source, &reshuffled), Next::Item(first[1]));
    }

    #[test]
    fn another_source_starts_a_new_run() {
        let a = Source::List(ListId::generate());
        let b = Source::Pinned(BoardId::generate());
        let (in_a, in_b) = (ids(2), ids(2));
        let mut queue = PasteQueue::default();
        assert_eq!(press(&mut queue, a, &in_a), Next::Item(in_a[0]));
        assert_eq!(press(&mut queue, b, &in_b), Next::Item(in_b[0]));
        assert_eq!(press(&mut queue, b, &in_b), Next::Item(in_b[1]));
        // Back to the first source: a fresh run, not where it was left.
        assert_eq!(press(&mut queue, a, &in_a), Next::Item(in_a[0]));
    }

    #[test]
    fn an_empty_source_is_reported_and_a_click_elsewhere_does_not_advance() {
        let source = Source::List(ListId::generate());
        let mut queue = PasteQueue::default();
        assert_eq!(press(&mut queue, source, &[]), Next::Empty);

        let order = ids(2);
        assert_eq!(queue.peek(source, || order.clone()), Next::Item(order[0]));
        queue.used(order[1]);
        assert_eq!(queue.peek(source, || order.clone()), Next::Item(order[0]));
        queue.reset();
        assert_eq!(queue.peek(source, || order.clone()), Next::Item(order[0]));
    }
}
