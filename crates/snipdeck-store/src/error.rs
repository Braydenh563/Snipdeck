/// Shorthand for results of storage calls.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// Everything that can go wrong in storage.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No live record with this id; it never existed, was purged, or is in the trash.
    #[error("{entity} {id} not found")]
    NotFound {
        /// `"board"`, `"list"`, `"item"`, `"tag"` or `"blob"`.
        entity: &'static str,
        /// The id or hash that was looked up.
        id: String,
    },
    /// A tag with this name, ignoring case, already exists.
    #[error("a tag named `{0}` already exists")]
    TagExists(String),
    /// Stored data did not form a valid core value.
    #[error(transparent)]
    Core(#[from] snipdeck_core::Error),
}
