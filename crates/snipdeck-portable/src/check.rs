//! Reading a finished archive back to see that it is whole: the "last backup" line, the
//! read-back after every new backup, and the test for "today's backup is already there".

use std::fs::File;
use std::path::Path;

use snipdeck_core::BlobRef;

use crate::archive::SafeArchive;
use crate::dto::{ItemsDoc, Manifest, parse_items_doc};
use crate::error::{Error, Result};
use crate::limits::Limits;

/// What a readable archive holds, from its manifest and `items.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveSummary {
    /// Boards in the archive.
    pub boards: usize,
    /// Lists in the archive.
    pub lists: usize,
    /// Items in the archive.
    pub items: usize,
    /// Tags in the archive.
    pub tags: usize,
    /// Asset files in the archive.
    pub assets: usize,
    /// The creation time the manifest records.
    pub created_at: String,
    /// Items the exporter left out because an attachment could not be read.
    pub skipped: u64,
    /// Sensitive items the exporter left out (no key).
    pub sensitive_omitted: u64,
}

/// How much of an archive [`check_archive_path`] reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    /// The zip directory, the manifest and `items.json`; the counts must agree. Cheap.
    Counts,
    /// As `Counts`, and every asset is read and hashed against its name. Reads the whole file.
    Everything,
}

/// Opens the archive at `path` and checks that it can be read back.
///
/// # Errors
///
/// [`Error::File`] if it cannot be opened, [`Error::MissingEntry`] / [`Error::Malformed`] /
/// [`Error::AssetMismatch`] if it is cut off, damaged or its counts disagree with its contents.
pub fn check_archive_path(path: &Path, depth: Depth) -> Result<ArchiveSummary> {
    let file = File::open(path).map_err(|source| Error::File {
        path: path.to_owned(),
        source,
    })?;
    let limits = Limits::default();
    let mut archive = SafeArchive::open(file, limits)?;
    let manifest: Manifest = serde_json::from_slice(&archive.read_manifest()?)
        .map_err(|e| Error::malformed("manifest.json", format!("not readable: {e}")))?;
    let items = archive.read_items()?;
    let doc: ItemsDoc = parse_items_doc(&items, limits.max_items)
        .map_err(|e| Error::malformed("items.json", format!("not readable: {e:?}")))?;

    let lists = doc.boards.iter().map(|b| b.lists.len()).sum::<usize>();
    let items = doc
        .boards
        .iter()
        .flat_map(|b| &b.lists)
        .map(|l| l.items.len())
        .sum::<usize>();
    let disagree = |what: &str, found: usize, said: u64| {
        (found as u64 != said).then(|| format!("{what}: {found} found, {said} in the manifest"))
    };
    let problem = disagree("boards", doc.boards.len(), manifest.counts.boards)
        .or_else(|| disagree("lists", lists, manifest.counts.lists))
        .or_else(|| disagree("items", items, manifest.counts.items))
        .or_else(|| disagree("assets", archive.asset_count(), manifest.counts.assets));
    if let Some(problem) = problem {
        return Err(Error::malformed("manifest.json", problem));
    }

    if depth == Depth::Everything {
        let hashes: Vec<BlobRef> = archive.asset_hashes().copied().collect();
        for asset in hashes {
            let actual = BlobRef::from_bytes(archive.hash_asset(&asset)?);
            if actual != asset {
                return Err(Error::AssetMismatch {
                    asset: asset.to_string(),
                    reason: "its bytes do not match its name".to_owned(),
                });
            }
        }
    }
    Ok(ArchiveSummary {
        boards: doc.boards.len(),
        lists,
        items,
        tags: doc.tags.len(),
        assets: usize::try_from(manifest.counts.assets).unwrap_or(usize::MAX),
        created_at: manifest.created_at,
        skipped: manifest.skipped_unreadable,
        sensitive_omitted: manifest.sensitive_omitted,
    })
}
