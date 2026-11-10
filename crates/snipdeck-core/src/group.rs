use crate::{GroupId, ListId, Name};

/// A named, collapsible run of items inside one list.
///
/// The group holds no order of its own: its members are the items that name it, and they sit
/// next to each other in the list, the header being drawn above the first. A group exists
/// only while it has a member, so there is no empty header.
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
}

impl Group {
    /// A new, expanded group in `list_id`.
    pub fn new(list_id: ListId, name: Name) -> Self {
        Self {
            id: GroupId::generate(),
            list_id,
            name,
            collapsed: false,
        }
    }
}
