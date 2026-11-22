use std::collections::HashMap;

use crate::{ItemLabel, Name, TagId};

/// A free-form label that can be put on items across boards (SPEC §4).
///
/// Tag names are unique ignoring case; storage enforces that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    /// Identity.
    pub id: TagId,
    /// Display name.
    pub name: Name,
    /// An optional colour, one of the eight item colours: shown beside the name wherever the
    /// tag is listed, never alone (the name is always there).
    pub colour: Option<ItemLabel>,
    /// The tag this one is nested under (`None`: a top-level tag). Filtering by a tag also
    /// finds the items carrying any tag nested under it; the tree never has a cycle.
    pub parent: Option<TagId>,
}

impl Tag {
    /// A new tag with a fresh id.
    pub fn new(name: Name) -> Self {
        Self {
            id: TagId::generate(),
            name,
            colour: None,
            parent: None,
        }
    }
}

/// The deepest nesting that is followed: a tree this deep is read as far as it goes, so a
/// cycle that slipped into stored data cannot loop.
const MAX_DEPTH: usize = 32;

/// The nesting of every tag, read once from the tags: who is under whom, and the paths
/// (`work/invoices`) a search matches by.
#[derive(Debug, Clone, Default)]
pub struct TagTree {
    tags: HashMap<TagId, (Name, Option<TagId>)>,
}

impl TagTree {
    /// The tree of `tags`. A parent that is not among them is ignored.
    #[must_use]
    pub fn new<'a>(tags: impl IntoIterator<Item = &'a Tag>) -> Self {
        let mut tree = Self {
            tags: tags
                .into_iter()
                .map(|t| (t.id, (t.name.clone(), t.parent)))
                .collect(),
        };
        let known: Vec<TagId> = tree.tags.keys().copied().collect();
        for id in known {
            if let Some(entry) = tree.tags.get(&id)
                && let Some(parent) = entry.1
                && !tree.tags.contains_key(&parent)
                && let Some(entry) = tree.tags.get_mut(&id)
            {
                entry.1 = None;
            }
        }
        tree
    }

    /// The tag `id` is nested under.
    #[must_use]
    pub fn parent(&self, id: TagId) -> Option<TagId> {
        self.tags.get(&id).and_then(|t| t.1)
    }

    /// How many tags are above `id` (0 for a top-level tag).
    #[must_use]
    pub fn depth(&self, id: TagId) -> usize {
        self.ancestors(id).count()
    }

    /// The tags above `id`, nearest first.
    pub fn ancestors(&self, id: TagId) -> impl Iterator<Item = TagId> + '_ {
        let mut next = self.parent(id);
        let mut steps = 0;
        std::iter::from_fn(move || {
            let current = next?;
            steps += 1;
            if steps > MAX_DEPTH {
                return None;
            }
            next = self.parent(current);
            Some(current)
        })
    }

    /// Whether `id` is nested, at any depth, under `ancestor`.
    #[must_use]
    pub fn is_under(&self, id: TagId, ancestor: TagId) -> bool {
        self.ancestors(id).any(|a| a == ancestor)
    }

    /// Whether `id` may be nested under `under`: not under itself and not under one of its own
    /// descendants.
    #[must_use]
    pub fn can_nest(&self, id: TagId, under: TagId) -> bool {
        id != under && !self.is_under(under, id) && self.tags.contains_key(&under)
    }

    /// The tags nested directly under `id`, by name.
    #[must_use]
    pub fn children(&self, id: TagId) -> Vec<TagId> {
        let mut out: Vec<(&str, TagId)> = self
            .tags
            .iter()
            .filter(|(_, t)| t.1 == Some(id))
            .map(|(k, t)| (t.0.as_str(), *k))
            .collect();
        out.sort_by(|a, b| {
            a.0.to_lowercase()
                .cmp(&b.0.to_lowercase())
                .then(a.1.cmp(&b.1))
        });
        out.into_iter().map(|(_, k)| k).collect()
    }

    /// Every tag in sidebar order: each top-level tag by name, with the tags nested under it
    /// right after it (and theirs after them), by name at every level.
    #[must_use]
    pub fn walk(&self) -> Vec<TagId> {
        let mut roots: Vec<(&str, TagId)> = self
            .tags
            .iter()
            .filter(|(_, t)| t.1.is_none())
            .map(|(k, t)| (t.0.as_str(), *k))
            .collect();
        roots.sort_by(|a, b| {
            a.0.to_lowercase()
                .cmp(&b.0.to_lowercase())
                .then(a.1.cmp(&b.1))
        });
        let mut out = Vec::with_capacity(self.tags.len());
        let mut stack: Vec<TagId> = roots.into_iter().rev().map(|(_, k)| k).collect();
        while let Some(id) = stack.pop() {
            if out.contains(&id) {
                continue;
            }
            out.push(id);
            stack.extend(self.children(id).into_iter().rev());
        }
        // A stored loop has no top-level tag to start from: show it flat rather than lose it.
        let mut rest: Vec<(&str, TagId)> = self
            .tags
            .iter()
            .filter(|(k, _)| !out.contains(k))
            .map(|(k, t)| (t.0.as_str(), *k))
            .collect();
        rest.sort_by(|a, b| {
            a.0.to_lowercase()
                .cmp(&b.0.to_lowercase())
                .then(a.1.cmp(&b.1))
        });
        out.extend(rest.into_iter().map(|(_, k)| k));
        out
    }

    /// `id`'s name with the names above it: `work/invoices`.
    #[must_use]
    pub fn path(&self, id: TagId) -> String {
        let mut names: Vec<&str> = self
            .ancestors(id)
            .filter_map(|a| self.tags.get(&a).map(|t| t.0.as_str()))
            .collect();
        names.reverse();
        if let Some(own) = self.tags.get(&id) {
            names.push(own.0.as_str());
        }
        names.join("/")
    }

    /// The names a search matches an item carrying `id` by: its own path and name and those of
    /// every tag above it, so `tag:work` finds an item tagged `work/invoices`, `tag:invoices`
    /// finds it by its own name and `tag:work/inv` by the path.
    #[must_use]
    pub fn search_paths(&self, id: TagId) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for tag in std::iter::once(id).chain(self.ancestors(id)) {
            let Some((name, _)) = self.tags.get(&tag) else {
                continue;
            };
            for text in [self.path(tag), name.as_str().to_owned()] {
                if !out.contains(&text) {
                    out.push(text);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_tags_get_distinct_ids() {
        let name = Name::new("invoices").unwrap();
        assert_ne!(Tag::new(name.clone()).id, Tag::new(name).id);
    }

    fn named(name: &str, parent: Option<&Tag>) -> Tag {
        Tag {
            parent: parent.map(|p| p.id),
            ..Tag::new(Name::new(name).unwrap())
        }
    }

    #[test]
    fn a_tag_under_another_has_a_path_a_depth_and_ancestors() {
        let work = named("work", None);
        let invoices = named("invoices", Some(&work));
        let paid = named("paid", Some(&invoices));
        let tree = TagTree::new([&work, &invoices, &paid]);
        assert_eq!(tree.path(paid.id), "work/invoices/paid");
        assert_eq!(tree.depth(paid.id), 2);
        assert_eq!(tree.depth(work.id), 0);
        assert!(tree.is_under(paid.id, work.id));
        assert!(!tree.is_under(work.id, paid.id));
        assert_eq!(
            tree.search_paths(paid.id),
            vec![
                "work/invoices/paid",
                "paid",
                "work/invoices",
                "invoices",
                "work"
            ]
        );
        assert_eq!(tree.children(work.id), vec![invoices.id]);
    }

    #[test]
    fn the_walk_lists_each_tag_once_under_its_parent_and_keeps_a_loop() {
        let work = named("Work", None);
        let paid = named("paid", Some(&work));
        let invoices = named("invoices", Some(&work));
        let alpha = named("alpha", None);
        let tree = TagTree::new([&work, &paid, &invoices, &alpha]);
        assert_eq!(tree.walk(), vec![alpha.id, work.id, invoices.id, paid.id]);
        let mut a = named("a", None);
        let mut b = named("b", Some(&a));
        a.parent = Some(b.id);
        b.parent = Some(a.id);
        assert_eq!(TagTree::new([&a, &b]).walk(), vec![a.id, b.id]);
    }

    #[test]
    fn a_tag_cannot_be_nested_under_itself_or_its_own_descendant() {
        let a = named("a", None);
        let b = named("b", Some(&a));
        let c = named("c", None);
        let tree = TagTree::new([&a, &b, &c]);
        assert!(!tree.can_nest(a.id, a.id));
        assert!(!tree.can_nest(a.id, b.id), "b is under a");
        assert!(tree.can_nest(b.id, c.id));
        assert!(tree.can_nest(a.id, c.id));
    }

    #[test]
    fn a_parent_that_is_gone_is_ignored_and_a_stored_cycle_does_not_loop() {
        let ghost = named("ghost", None);
        let orphan = named("orphan", Some(&ghost));
        assert_eq!(TagTree::new([&orphan]).depth(orphan.id), 0);
        let mut a = named("a", None);
        let mut b = named("b", Some(&a));
        a.parent = Some(b.id);
        b.parent = Some(a.id);
        let tree = TagTree::new([&a, &b]);
        assert_eq!(tree.depth(a.id), 32, "followed as far as the bound");
    }
}
