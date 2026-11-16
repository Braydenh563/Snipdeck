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
}

impl Tag {
    /// A new tag with a fresh id.
    pub fn new(name: Name) -> Self {
        Self {
            id: TagId::generate(),
            name,
            colour: None,
        }
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
}
