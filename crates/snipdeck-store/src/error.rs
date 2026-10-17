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
    /// A record with this id already exists (live or in the trash).
    #[error("{entity} {id} already exists")]
    AlreadyExists {
        /// `"board"`, `"list"`, `"item"` or `"tag"`.
        entity: &'static str,
        /// The duplicate id.
        id: String,
    },
    /// Another process is using this data folder (the database is single-writer, ADR-0002).
    #[error(
        "another Snipdeck is already using the data folder `{}`; close it and try again",
        folder.display()
    )]
    Locked {
        /// The data folder.
        folder: std::path::PathBuf,
    },
    /// The database was written by a newer Snipdeck than this one.
    #[error(
        "this data was saved by a newer Snipdeck (database format {found}, this version \
         understands up to {supported}); update Snipdeck instead of opening it with this version"
    )]
    SchemaTooNew {
        /// The version recorded in the database.
        found: u32,
        /// The newest version this build can open.
        supported: u32,
    },
    /// The file is a SQLite database, but not one of Snipdeck's.
    #[error("`{}` is not a Snipdeck database", path.display())]
    ForeignDatabase {
        /// The offending file.
        path: std::path::PathBuf,
    },
    /// A migration failed; the database was left at the previous version.
    #[error("migration to database format {version} ({name}) failed: {reason}")]
    Migration {
        /// The version being migrated to.
        version: u32,
        /// The migration's name.
        name: &'static str,
        /// What went wrong.
        reason: String,
    },
    /// A blob file does not match the hash or length recorded for it.
    #[error("blob {blob} is damaged: {reason}")]
    BlobCorrupt {
        /// The blob's hash.
        blob: String,
        /// What did not match.
        reason: String,
    },
    /// A value cannot be stored, for example a timestamp outside 1677 to 2262 or a sensitive
    /// item that carries a plaintext preview.
    #[error("cannot store this: {0}")]
    Invalid(String),
    /// The database holds something Snipdeck could not have written.
    #[error("the database is inconsistent: {0}")]
    Corrupt(String),
    /// SQLite reported an error.
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// The file system reported an error.
    #[error("file system error: {0}")]
    Io(#[from] std::io::Error),
}
