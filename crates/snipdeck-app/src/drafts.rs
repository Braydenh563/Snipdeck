//! Drafts of the item editor: what was typed and not yet saved, kept in `drafts/` under the data
//! folder so a crash, a power cut or a closed window does not lose it.
//!
//! One file per editor: `new.json` for the New item editor, `<item id>.json` for an item being
//! edited. The editor writes it every [`AUTOSAVE_EVERY`] while something changed (the clock and
//! the timer belong to the caller, so tests run on a frozen one), and removes it when the item
//! is saved or the changes are discarded.
//!
//! **A sensitive item never gets a draft.** A draft is plain text on disk, and the point of a
//! sensitive item is that its content is not (encrypted at rest with Supporter, masked on Free). The
//! editor therefore neither writes a draft while "Sensitive item" is on nor keeps one it wrote
//! before the switch was turned on: see [`Drafts::clear`].
//!
//! Nothing here logs or returns content in an error: failures are `io::Error`s about files.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use snipdeck_core::{ItemId, ItemKind};

/// How often the open editor saves its draft.
pub const AUTOSAVE_EVERY: Duration = Duration::from_secs(2);

/// The folder inside the data folder.
pub const FOLDER: &str = "drafts";

/// Drafts older than the format this code reads are ignored (and removed on the next save).
const VERSION: u32 = 1;

/// The most a draft file may weigh when it is read back: the editor's 16 KB of text plus the
/// fields around it, with room to spare. A bigger file is not one of ours.
const MAX_FILE_BYTES: u64 = 256 * 1024;

/// Which editor a draft belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftKey {
    /// The New item editor.
    New,
    /// The editor of this item.
    Item(ItemId),
}

impl DraftKey {
    fn file_name(self) -> String {
        match self {
            Self::New => "new.json".to_owned(),
            Self::Item(id) => format!("{id}.json"),
        }
    }
}

/// What the editor holds, as plain data. Never includes a sensitive item's content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    version: u32,
    /// For an existing item: its `updated_at` when the editor opened. A draft whose item has
    /// changed since is stale and is dropped by the caller.
    pub base: Option<String>,
    /// The kind (`ItemKind::key`).
    pub kind: String,
    /// The title field.
    pub title: String,
    /// The content field (text kinds).
    pub content: String,
    /// The list's id.
    pub list: String,
    /// The ticked tags' ids.
    pub tags: Vec<String>,
    /// Tags typed in the editor that do not exist yet.
    pub new_tags: Vec<String>,
    /// The path field (a new Image or File item).
    pub source_path: String,
}

impl Draft {
    /// A draft of these fields.
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "a plain record built in one place"
    )]
    pub fn new(
        base: Option<String>,
        kind: ItemKind,
        title: String,
        content: String,
        list: String,
        tags: Vec<String>,
        new_tags: Vec<String>,
        source_path: String,
    ) -> Self {
        Self {
            version: VERSION,
            base,
            kind: kind.key().to_owned(),
            title,
            content,
            list,
            tags,
            new_tags,
            source_path,
        }
    }

    /// The kind the draft was in, if it is one this version knows.
    #[must_use]
    pub fn item_kind(&self) -> Option<ItemKind> {
        ItemKind::ALL.iter().copied().find(|k| k.key() == self.kind)
    }
}

/// The `drafts/` folder.
#[derive(Debug, Clone)]
pub struct Drafts {
    dir: PathBuf,
}

impl Drafts {
    /// The drafts of the library whose data folder is `data_dir` (the folder is made on the
    /// first save).
    #[must_use]
    pub fn in_data_dir(data_dir: &Path) -> Self {
        Self {
            dir: data_dir.join(FOLDER),
        }
    }

    fn path(&self, key: DraftKey) -> PathBuf {
        self.dir.join(key.file_name())
    }

    /// The draft of `key`, if there is one this version can read. A damaged, oversized or
    /// foreign file reads as "no draft" (and stays for the user to look at).
    #[must_use]
    pub fn load(&self, key: DraftKey) -> Option<Draft> {
        let path = self.path(key);
        let meta = fs::metadata(&path).ok()?;
        if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
            return None;
        }
        let bytes = fs::read(&path).ok()?;
        let draft: Draft = serde_json::from_slice(&bytes).ok()?;
        (draft.version == VERSION).then_some(draft)
    }

    /// Writes the draft of `key`, replacing the last one. Written to a side file and renamed, so a
    /// crash in the middle leaves the previous draft whole.
    ///
    /// # Errors
    ///
    /// The folder or file could not be written.
    pub fn save(&self, key: DraftKey, draft: &Draft) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let bytes = serde_json::to_vec(draft).map_err(io::Error::other)?;
        let path = self.path(key);
        let side = path.with_extension("json.tmp");
        fs::write(&side, bytes)?;
        fs::rename(&side, &path)
    }

    /// Removes the draft of `key`; no draft is not an error.
    ///
    /// # Errors
    ///
    /// The file exists and could not be removed.
    pub fn clear(&self, key: DraftKey) -> io::Result<()> {
        match fs::remove_file(self.path(key)) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(content: &str) -> Draft {
        Draft::new(
            None,
            ItemKind::Code,
            "Title".into(),
            content.into(),
            "list".into(),
            vec!["tag".into()],
            vec!["fresh".into()],
            String::new(),
        )
    }

    #[test]
    fn a_draft_round_trips_and_clearing_removes_it() {
        let dir = tempfile::tempdir().expect("dir");
        let drafts = Drafts::in_data_dir(dir.path());
        assert_eq!(drafts.load(DraftKey::New), None);
        drafts
            .save(DraftKey::New, &draft("fn main() {}"))
            .expect("save");
        let back = drafts.load(DraftKey::New).expect("draft");
        assert_eq!(back, draft("fn main() {}"));
        assert_eq!(back.item_kind(), Some(ItemKind::Code));
        drafts.clear(DraftKey::New).expect("clear");
        assert_eq!(drafts.load(DraftKey::New), None);
        drafts
            .clear(DraftKey::New)
            .expect("clearing nothing is fine");
    }

    #[test]
    fn each_editor_has_its_own_file_and_a_newer_save_replaces_the_older() {
        let dir = tempfile::tempdir().expect("dir");
        let drafts = Drafts::in_data_dir(dir.path());
        let id = ItemId::generate();
        drafts.save(DraftKey::New, &draft("a")).expect("save");
        drafts.save(DraftKey::Item(id), &draft("b")).expect("save");
        drafts.save(DraftKey::Item(id), &draft("c")).expect("save");
        assert_eq!(drafts.load(DraftKey::New).expect("new").content, "a");
        assert_eq!(drafts.load(DraftKey::Item(id)).expect("item").content, "c");
        let mut names: Vec<String> = fs::read_dir(dir.path().join(FOLDER))
            .expect("dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names.len(), 2, "no side file is left behind: {names:?}");
    }

    #[test]
    fn damaged_foreign_and_oversized_files_read_as_no_draft() {
        let dir = tempfile::tempdir().expect("dir");
        let drafts = Drafts::in_data_dir(dir.path());
        drafts.save(DraftKey::New, &draft("x")).expect("save");
        let path = dir.path().join(FOLDER).join("new.json");
        fs::write(&path, b"{ not json").expect("write");
        assert_eq!(drafts.load(DraftKey::New), None);
        fs::write(&path, br#"{"version":99,"kind":"text","title":"","content":"","list":"","tags":[],"new_tags":[],"source_path":"","base":null}"#).expect("write");
        assert_eq!(drafts.load(DraftKey::New), None, "another version");
        fs::write(
            &path,
            vec![b' '; usize::try_from(MAX_FILE_BYTES).expect("fits") + 1],
        )
        .expect("write");
        assert_eq!(drafts.load(DraftKey::New), None, "too big to be ours");
    }
}
