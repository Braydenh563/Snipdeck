use std::collections::HashMap;

use crate::{GroupId, ListAccent, ListId, Name};

/// How many levels of groups a list can hold: a top-level group (level 1), a group inside it
/// (level 2) and one inside that (level 3). A deeper tree would not fit the Main window's
/// indent at its narrowest width, and nothing needs more than a folder, a subfolder and a
/// topic.
pub const MAX_GROUP_DEPTH: usize = 3;

/// A named, collapsible run of items inside one list.
///
/// The group holds no order of its own: its members are the items that name it, and they sit
/// next to each other in the list, the header being drawn above the first. A group exists
/// only while it has a member or a group nested under it, so there is no empty header.
///
/// A group can sit inside another group of the same list (`parent`): its members are then a
/// contiguous part of the parent's run, and its header is drawn one level in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// The group's id.
    pub id: GroupId,
    /// The list the group lives in; never changes.
    pub list_id: ListId,
    /// The header text.
    pub name: Name,
    /// Whether the members are hidden under the header.
    pub collapsed: bool,
    /// An optional colour, one of the nine list accents: a dot beside the header's name, never
    /// alone (the name is always there).
    pub colour: Option<ListAccent>,
    /// The group this one is nested in (`None`: a top-level group). It is a group of the same
    /// list, the tree has no cycle and no more than [`MAX_GROUP_DEPTH`] levels.
    pub parent: Option<GroupId>,
}

impl Group {
    /// A new, expanded group in `list_id`.
    pub fn new(list_id: ListId, name: Name) -> Self {
        Self {
            id: GroupId::generate(),
            list_id,
            name,
            collapsed: false,
            colour: None,
            parent: None,
        }
    }
}

/// The deepest nesting that is followed when walking up: a cycle that slipped into stored data
/// cannot loop.
const WALK_LIMIT: usize = 32;

/// The nesting of the groups of one list: who is inside whom.
#[derive(Debug, Clone, Default)]
pub struct GroupTree {
    parents: HashMap<GroupId, Option<GroupId>>,
}

impl GroupTree {
    /// The tree of `groups`. A parent that is not among them is ignored.
    #[must_use]
    pub fn new<'a>(groups: impl IntoIterator<Item = &'a Group>) -> Self {
        let mut parents: HashMap<GroupId, Option<GroupId>> =
            groups.into_iter().map(|g| (g.id, g.parent)).collect();
        let known: Vec<GroupId> = parents.keys().copied().collect();
        for id in known {
            if let Some(Some(parent)) = parents.get(&id).copied()
                && !parents.contains_key(&parent)
            {
                parents.insert(id, None);
            }
        }
        Self { parents }
    }

    /// The group `id` is nested in.
    #[must_use]
    pub fn parent(&self, id: GroupId) -> Option<GroupId> {
        self.parents.get(&id).copied().flatten()
    }

    /// The groups above `id`, nearest first.
    pub fn ancestors(&self, id: GroupId) -> impl Iterator<Item = GroupId> + '_ {
        let mut next = self.parent(id);
        let mut steps = 0;
        std::iter::from_fn(move || {
            let current = next?;
            steps += 1;
            if steps > WALK_LIMIT {
                return None;
            }
            next = self.parent(current);
            Some(current)
        })
    }

    /// How many groups are above `id` (0 for a top-level group).
    #[must_use]
    pub fn depth(&self, id: GroupId) -> usize {
        self.ancestors(id).count()
    }

    /// Whether `id` is nested, at any depth, in `ancestor`.
    #[must_use]
    pub fn is_under(&self, id: GroupId, ancestor: GroupId) -> bool {
        self.ancestors(id).any(|a| a == ancestor)
    }

    /// The groups nested directly in `id`.
    #[must_use]
    pub fn children(&self, id: GroupId) -> Vec<GroupId> {
        self.parents
            .iter()
            .filter(|(_, p)| **p == Some(id))
            .map(|(k, _)| *k)
            .collect()
    }

    /// How many levels `id` and the groups nested in it take (1 for a group with none).
    #[must_use]
    pub fn height(&self, id: GroupId) -> usize {
        let mut level = vec![id];
        let mut levels = 0;
        while !level.is_empty() && levels < WALK_LIMIT {
            levels += 1;
            level = level.into_iter().flat_map(|g| self.children(g)).collect();
        }
        levels
    }

    /// Whether `id` may be nested in `under` (`None`: taken out to the top level): not in
    /// itself or in one of its own groups, and with its own groups not deeper than
    /// [`MAX_GROUP_DEPTH`] levels. A group already in `under` can be nested there again (the
    /// move to the end of its run).
    #[must_use]
    pub fn can_nest(&self, id: GroupId, under: Option<GroupId>) -> bool {
        if !self.parents.contains_key(&id) {
            return false;
        }
        let Some(under) = under else {
            return true;
        };
        id != under
            && self.parents.contains_key(&under)
            && !self.is_under(under, id)
            && self.depth(under) + 1 + self.height(id) <= MAX_GROUP_DEPTH
    }

    /// `id` and every group nested in it, at any depth.
    #[must_use]
    pub fn subtree(&self, id: GroupId) -> Vec<GroupId> {
        let mut out = vec![id];
        let mut at = 0;
        while let Some(&next) = out.get(at) {
            at += 1;
            for child in self.children(next) {
                if !out.contains(&child) {
                    out.push(child);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(parent: Option<&Group>) -> Group {
        Group {
            parent: parent.map(|p| p.id),
            ..Group::new(ListId::generate(), Name::new("Group").unwrap())
        }
    }

    #[test]
    fn a_group_inside_another_has_a_depth_and_ancestors() {
        let a = group(None);
        let b = group(Some(&a));
        let c = group(Some(&b));
        let tree = GroupTree::new([&a, &b, &c]);
        assert_eq!(tree.depth(a.id), 0);
        assert_eq!(tree.depth(c.id), 2);
        assert_eq!(tree.ancestors(c.id).collect::<Vec<_>>(), vec![b.id, a.id]);
        assert!(tree.is_under(c.id, a.id));
        assert!(!tree.is_under(a.id, c.id));
        assert_eq!(tree.height(a.id), 3);
        assert_eq!(tree.height(c.id), 1);
        assert_eq!(tree.subtree(a.id).len(), 3);
    }

    #[test]
    fn nesting_refuses_a_cycle_itself_and_a_tree_deeper_than_the_limit() {
        let root = group(None);
        let mid = group(Some(&root));
        let leaf = group(Some(&mid));
        let other = group(None);
        let other_child = group(Some(&other));
        let tree = GroupTree::new([&root, &mid, &leaf, &other, &other_child]);
        assert!(!tree.can_nest(root.id, Some(root.id)));
        assert!(!tree.can_nest(root.id, Some(leaf.id)));
        // `other` with a child is two levels; under `leaf` (depth 2) it would be five.
        assert!(!tree.can_nest(other.id, Some(leaf.id)));
        assert!(tree.can_nest(other.id, Some(root.id)));
        // A single group fits under `mid` (depth 1 + 1 + height 1 = 3), a pair does not.
        assert!(!tree.can_nest(other.id, Some(mid.id)));
        assert!(tree.can_nest(other_child.id, Some(mid.id)));
        assert!(tree.can_nest(leaf.id, None));
        assert!(tree.can_nest(leaf.id, Some(mid.id)));
    }

    #[test]
    fn a_parent_that_is_not_listed_is_ignored_and_a_stored_loop_ends() {
        let ghost = group(None);
        let orphan = group(Some(&ghost));
        let mut x = group(None);
        let mut y = group(None);
        x.parent = Some(y.id);
        y.parent = Some(x.id);
        let tree = GroupTree::new([&orphan, &x, &y]);
        assert_eq!(tree.parent(orphan.id), None);
        assert!(tree.depth(x.id) > 0);
        assert!(tree.ancestors(x.id).count() <= 32);
    }
}
