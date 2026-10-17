/// Where to put a board, list or item among its siblings, e.g. after a drag-and-drop.
///
/// `Id` is the sibling type: `Placement<ItemId>` places an item within a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement<Id> {
    /// Before every sibling.
    First,
    /// After every sibling.
    Last,
    /// Directly after this sibling, which must be in the same container.
    After(Id),
}
