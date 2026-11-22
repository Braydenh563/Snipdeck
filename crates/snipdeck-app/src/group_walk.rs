//! Walking a list's items under their groups: which headers go above an item, how deep it sits,
//! and whether a folded group hides it. The Panel and the Main window share it, so a folded group
//! hides the groups nested in it in both.

use std::collections::HashMap;

use snipdeck_core::{GroupId, ItemId, MAX_GROUP_DEPTH};
use snipdeck_store::GroupRun;

/// What one item needs from the walk.
#[derive(Debug)]
pub(crate) struct Step<'a> {
    /// The headers that open above the item, outermost first, each with its depth (0 at the top
    /// level). A header under a folded group is not listed: it is hidden with the rest.
    pub(crate) headers: Vec<(&'a GroupRun, usize)>,
    /// The item sits under a folded group.
    pub(crate) hidden: bool,
    /// How many groups the item is nested in (0: loose).
    pub(crate) depth: usize,
}

/// A walk over the items of one list in order.
pub(crate) struct GroupWalk<'a> {
    by_id: HashMap<GroupId, &'a GroupRun>,
    of_item: HashMap<ItemId, &'a GroupRun>,
    open: Vec<GroupId>,
}

impl<'a> GroupWalk<'a> {
    /// A walk over the groups `runs` of one list.
    pub(crate) fn new(runs: &'a [GroupRun]) -> Self {
        Self {
            by_id: runs.iter().map(|r| (r.group.id, r)).collect(),
            of_item: runs
                .iter()
                .flat_map(|r| r.items.iter().map(move |i| (*i, r)))
                .collect(),
            open: Vec::new(),
        }
    }

    /// The innermost group of `item`.
    pub(crate) fn innermost(&self, item: ItemId) -> Option<&'a GroupRun> {
        self.of_item.get(&item).copied()
    }

    /// The groups `item` is in, outermost first.
    pub(crate) fn chain(&self, item: ItemId) -> Vec<&'a GroupRun> {
        let mut chain = Vec::new();
        let mut at = self.innermost(item);
        while let Some(run) = at {
            chain.push(run);
            if chain.len() > MAX_GROUP_DEPTH {
                break;
            }
            at = run.group.parent.and_then(|p| self.by_id.get(&p).copied());
        }
        chain.reverse();
        chain
    }

    /// The next item of the list in order.
    pub(crate) fn step(&mut self, item: ItemId) -> Step<'a> {
        let chain = self.chain(item);
        let shared = self
            .open
            .iter()
            .zip(&chain)
            .take_while(|(open, run)| **open == run.group.id)
            .count();
        self.open.truncate(shared);
        let mut headers = Vec::new();
        let mut hidden = chain[..shared].iter().any(|r| r.group.collapsed);
        for (depth, run) in chain.iter().enumerate().skip(shared) {
            if !hidden {
                headers.push((*run, depth));
            }
            hidden |= run.group.collapsed;
            self.open.push(run.group.id);
        }
        Step {
            headers,
            hidden,
            depth: chain.len(),
        }
    }
}
