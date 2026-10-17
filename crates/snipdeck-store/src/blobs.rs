//! The content-addressed blob folder.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use snipdeck_core::{BlobRef, Content, INLINE_LIMIT, Inline};

use crate::{Error, Result};

/// The content-addressed blob folder (`blobs/` in the data folder, ADR-0002).
///
/// This is the only place that decides between inline and blob storage, using
/// [`INLINE_LIMIT`](snipdeck_core::INLINE_LIMIT), and the only place that hashes (BLAKE3).
pub trait BlobStore {
    /// Stores `bytes`: returns [`Content::Inline`] when they fit within the inline limit,
    /// otherwise writes them once under their hash (identical bytes share one file) and returns
    /// [`Content::Blob`].
    fn put(&mut self, bytes: Vec<u8>) -> Result<Content>;

    /// Reads `content`'s bytes, borrowing them when they are inline.
    ///
    /// Fails with [`Error::NotFound`](crate::Error::NotFound) if a blob file is missing.
    fn read<'a>(&self, content: &'a Content) -> Result<Cow<'a, [u8]>>;
}

/// A blob found by [`Blobs::list`].
#[derive(Debug, Clone)]
pub(crate) struct StoredBlob {
    pub blob: BlobRef,
    pub len: u64,
    pub modified: SystemTime,
}

/// Everything [`Blobs::list`] found that is not a finished blob: leftovers of an interrupted
/// write.
#[derive(Debug, Clone)]
pub(crate) struct Leftover {
    pub path: PathBuf,
    pub modified: SystemTime,
}

/// The blob folder, or an in-memory stand-in for [`Store::open_in_memory`](crate::Store).
#[derive(Debug)]
pub(crate) enum Blobs {
    /// Files under `root/ab/cdef...` (hash in lowercase hex).
    Disk { root: PathBuf },
    /// Nothing touches the disk.
    Memory(HashMap<BlobRef, Vec<u8>>),
}

/// Prefix of in-flight files; a crash can leave them behind and [`Blobs::list`] reports them.
const TEMP_PREFIX: &str = ".tmp-";

impl Blobs {
    pub(crate) fn on_disk(root: PathBuf) -> Self {
        Self::Disk { root }
    }

    pub(crate) fn in_memory() -> Self {
        Self::Memory(HashMap::new())
    }

    /// Inline or blob, by [`INLINE_LIMIT`]: the single place the threshold is applied.
    pub(crate) fn put(&mut self, bytes: Vec<u8>) -> Result<Content> {
        if bytes.len() <= INLINE_LIMIT {
            return Ok(Content::Inline(Inline::new(bytes)?));
        }
        let blob = BlobRef::from_bytes(*blake3::hash(&bytes).as_bytes());
        let len = bytes.len() as u64;
        match self {
            Self::Memory(map) => {
                map.insert(blob, bytes);
            }
            Self::Disk { .. } => {
                if self.has(&blob, len) {
                    // Identical bytes are already stored. Refresh the file's age so a sweep for
                    // orphans cannot take it between this call and the row that will use it.
                    self.touch(&blob);
                } else {
                    self.write_atomically(&blob, &bytes)?;
                }
            }
        }
        Ok(Content::Blob { blob, len })
    }

    pub(crate) fn read<'a>(&self, content: &'a Content) -> Result<Cow<'a, [u8]>> {
        let (blob, len) = match content {
            Content::Inline(inline) => return Ok(Cow::Borrowed(inline.as_bytes())),
            Content::Blob { blob, len } => (blob, *len),
        };
        let bytes = match self {
            Self::Memory(map) => map.get(blob).cloned().ok_or_else(|| not_found(blob))?,
            Self::Disk { .. } => {
                let path = self.file_path(blob);
                let found = fs::metadata(&path)
                    .map_err(|e| io_or_missing(e, blob))?
                    .len();
                if found != len {
                    return Err(damaged(
                        blob,
                        format!("it has {found} bytes, expected {len}"),
                    ));
                }
                fs::read(&path).map_err(|e| io_or_missing(e, blob))?
            }
        };
        if bytes.len() as u64 != len {
            return Err(damaged(
                blob,
                format!("it has {} bytes, expected {len}", bytes.len()),
            ));
        }
        if blake3::hash(&bytes).as_bytes() != blob.as_bytes() {
            return Err(damaged(blob, "its hash does not match its name".to_owned()));
        }
        Ok(Cow::Owned(bytes))
    }

    /// Whether a blob of `len` bytes is stored under `blob` (metadata only, no hashing).
    pub(crate) fn has(&self, blob: &BlobRef, len: u64) -> bool {
        match self {
            Self::Memory(map) => map.get(blob).is_some_and(|b| b.len() as u64 == len),
            Self::Disk { .. } => {
                fs::metadata(self.file_path(blob)).is_ok_and(|m| m.is_file() && m.len() == len)
            }
        }
    }

    /// Deletes a blob; a blob that is already gone is fine.
    pub(crate) fn remove(&mut self, blob: &BlobRef) -> Result<()> {
        match self {
            Self::Memory(map) => {
                map.remove(blob);
                Ok(())
            }
            Self::Disk { .. } => match fs::remove_file(self.file_path(blob)) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.into()),
            },
        }
    }

    /// Deletes a leftover temp file reported by [`Blobs::list`].
    pub(crate) fn remove_leftover(leftover: &Leftover) -> Result<()> {
        match fs::remove_file(&leftover.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Every finished blob and every leftover temp file. Files that are neither are ignored.
    pub(crate) fn list(&self) -> Result<(Vec<StoredBlob>, Vec<Leftover>)> {
        let root = match self {
            Self::Memory(map) => {
                let blobs = map
                    .iter()
                    .map(|(blob, bytes)| StoredBlob {
                        blob: *blob,
                        len: bytes.len() as u64,
                        modified: SystemTime::UNIX_EPOCH,
                    })
                    .collect();
                return Ok((blobs, Vec::new()));
            }
            Self::Disk { root } => root,
        };
        let mut blobs = Vec::new();
        let mut leftovers = Vec::new();
        let top = match fs::read_dir(root) {
            Ok(top) => top,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((blobs, leftovers)),
            Err(e) => return Err(e.into()),
        };
        for shard in top {
            let shard = shard?;
            let shard_name = shard.file_name().to_string_lossy().into_owned();
            if shard_name.len() != 2 || !shard.file_type()?.is_dir() {
                continue;
            }
            for entry in fs::read_dir(shard.path())? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                let meta = entry.metadata()?;
                if !meta.is_file() {
                    continue;
                }
                let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                if name.starts_with(TEMP_PREFIX) {
                    leftovers.push(Leftover {
                        path: entry.path(),
                        modified,
                    });
                } else if let Ok(blob) = format!("{shard_name}{name}").parse::<BlobRef>() {
                    blobs.push(StoredBlob {
                        blob,
                        len: meta.len(),
                        modified,
                    });
                }
            }
        }
        Ok((blobs, leftovers))
    }

    /// Where `blob` lives on disk; `None` for the in-memory store.
    #[cfg(test)]
    pub(crate) fn path_of(&self, blob: &BlobRef) -> Option<PathBuf> {
        match self {
            Self::Memory(_) => None,
            Self::Disk { .. } => Some(self.file_path(blob)),
        }
    }

    fn file_path(&self, blob: &BlobRef) -> PathBuf {
        let Self::Disk { root } = self else {
            return PathBuf::new();
        };
        let hex = blob.to_string();
        root.join(&hex[..2]).join(&hex[2..])
    }

    fn touch(&self, blob: &BlobRef) {
        if let Ok(file) = fs::OpenOptions::new()
            .write(true)
            .open(self.file_path(blob))
        {
            // Best effort: the worst case is a blob that a later sweep may collect early.
            let _ = file.set_modified(SystemTime::now());
        }
    }

    /// Temp file in the destination folder, flushed, then renamed over the final name, so a
    /// reader (or a crash) never sees a partial blob.
    fn write_atomically(&self, blob: &BlobRef, bytes: &[u8]) -> Result<()> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let target = self.file_path(blob);
        let Some(folder) = target.parent() else {
            return Err(io::Error::other("blob path has no folder").into());
        };
        fs::create_dir_all(folder)?;
        let temp = folder.join(format!(
            "{TEMP_PREFIX}{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let written = (|| -> io::Result<()> {
            let mut file = fs::File::create(&temp)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temp, &target)
        })();
        if let Err(e) = written {
            let _ = fs::remove_file(&temp);
            return Err(e.into());
        }
        sync_folder(folder);
        Ok(())
    }
}

/// Makes the rename durable where the platform allows it (directories cannot be opened for
/// syncing on Windows).
fn sync_folder(folder: &Path) {
    #[cfg(unix)]
    if let Ok(dir) = fs::File::open(folder) {
        let _ = dir.sync_all();
    }
    #[cfg(not(unix))]
    let _ = folder;
}

fn not_found(blob: &BlobRef) -> Error {
    Error::NotFound {
        entity: "blob",
        id: blob.to_string(),
    }
}

fn io_or_missing(e: io::Error, blob: &BlobRef) -> Error {
    if e.kind() == io::ErrorKind::NotFound {
        not_found(blob)
    } else {
        e.into()
    }
}

fn damaged(blob: &BlobRef, reason: String) -> Error {
    Error::BlobCorrupt {
        blob: blob.to_string(),
        reason,
    }
}

#[cfg(test)]
#[allow(
    unknown_lints,
    clippy::assert_is_empty,
    reason = "`assert!(x.is_empty())` reads fine; the lint is newer than our pinned toolchain"
)]
mod tests {
    use super::*;
    use crate::Error;
    use snipdeck_core::INLINE_LIMIT;

    fn blobs() -> (tempfile::TempDir, Blobs) {
        let dir = tempfile::tempdir().unwrap();
        let blobs = Blobs::on_disk(dir.path().join("blobs"));
        (dir, blobs)
    }

    fn big(fill: u8) -> Vec<u8> {
        vec![fill; INLINE_LIMIT + 1]
    }

    fn files_under(root: &std::path::Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let mut stack = vec![root.to_owned()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    found.push(path);
                }
            }
        }
        found
    }

    #[test]
    fn payloads_up_to_the_limit_stay_inline_and_touch_no_file() {
        let (dir, mut blobs) = blobs();
        let content = blobs.put(vec![7; INLINE_LIMIT]).unwrap();
        assert!(matches!(content, Content::Inline(_)));
        assert_eq!(content.len(), INLINE_LIMIT as u64);
        assert!(files_under(dir.path()).is_empty());
        assert!(matches!(blobs.put(Vec::new()).unwrap(), Content::Inline(_)));
    }

    #[test]
    fn the_threshold_is_exactly_the_core_inline_limit() {
        let (_dir, mut blobs) = blobs();
        assert!(matches!(
            blobs.put(vec![1; INLINE_LIMIT]).unwrap(),
            Content::Inline(_)
        ));
        assert!(matches!(
            blobs.put(vec![1; INLINE_LIMIT + 1]).unwrap(),
            Content::Blob { .. }
        ));
    }

    #[test]
    fn larger_payloads_are_named_by_their_blake3_hash_under_a_two_digit_folder() {
        let (dir, mut blobs) = blobs();
        let bytes = big(3);
        let Content::Blob { blob, len } = blobs.put(bytes.clone()).unwrap() else {
            panic!("expected a blob");
        };
        assert_eq!(len, bytes.len() as u64);
        let hex = blake3::hash(&bytes).to_hex().to_string();
        assert_eq!(blob.to_string(), hex);
        let expected = dir.path().join("blobs").join(&hex[..2]).join(&hex[2..]);
        assert_eq!(files_under(dir.path()), std::slice::from_ref(&expected));
        assert_eq!(std::fs::read(expected).unwrap(), bytes);
        assert_eq!(blobs.path_of(&blob).unwrap().file_name().unwrap().len(), 62);
    }

    #[test]
    fn identical_bytes_share_one_file() {
        let (dir, mut blobs) = blobs();
        let a = blobs.put(big(5)).unwrap();
        let b = blobs.put(big(5)).unwrap();
        let c = blobs.put(big(6)).unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(files_under(dir.path()).len(), 2);
    }

    #[test]
    fn putting_again_repairs_a_truncated_file() {
        let (_dir, mut blobs) = blobs();
        let content = blobs.put(big(5)).unwrap();
        let Content::Blob { blob, .. } = &content else {
            panic!()
        };
        std::fs::write(blobs.path_of(blob).unwrap(), b"short").unwrap();
        assert_eq!(blobs.put(big(5)).unwrap(), content);
        assert_eq!(blobs.read(&content).unwrap().as_ref(), big(5).as_slice());
    }

    #[test]
    fn writes_leave_no_temporary_files() {
        let (dir, mut blobs) = blobs();
        for fill in 0..5 {
            blobs.put(big(fill)).unwrap();
        }
        let names: Vec<_> = files_under(dir.path())
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 5);
        assert!(
            names.iter().all(|n| !n.starts_with('.') && n.len() == 62),
            "{names:?}"
        );
    }

    #[test]
    fn reads_return_the_bytes_and_borrow_inline_ones() {
        let (_dir, mut blobs) = blobs();
        let small = blobs.put(b"hello".to_vec()).unwrap();
        assert!(matches!(
            blobs.read(&small).unwrap(),
            Cow::Borrowed(b"hello")
        ));
        let large = blobs.put(big(9)).unwrap();
        assert_eq!(blobs.read(&large).unwrap().as_ref(), big(9).as_slice());
    }

    #[test]
    fn reading_a_missing_blob_is_not_found() {
        let (_dir, mut blobs) = blobs();
        let content = blobs.put(big(1)).unwrap();
        let Content::Blob { blob, .. } = &content else {
            panic!()
        };
        std::fs::remove_file(blobs.path_of(blob).unwrap()).unwrap();
        assert!(matches!(
            blobs.read(&content),
            Err(Error::NotFound { entity: "blob", .. })
        ));
    }

    #[test]
    fn reading_verifies_length_and_content() {
        let (_dir, mut blobs) = blobs();
        let content = blobs.put(big(1)).unwrap();
        let Content::Blob { blob, .. } = &content else {
            panic!()
        };
        let path = blobs.path_of(blob).unwrap();

        std::fs::write(&path, vec![1; INLINE_LIMIT]).unwrap(); // shorter
        let err = blobs.read(&content).unwrap_err();
        assert!(
            matches!(&err, Error::BlobCorrupt { reason, .. } if reason.contains("bytes")),
            "{err:?}"
        );

        let mut flipped = big(1);
        flipped[100] ^= 1; // same length, different bytes
        std::fs::write(&path, flipped).unwrap();
        let err = blobs.read(&content).unwrap_err();
        assert!(
            matches!(&err, Error::BlobCorrupt { reason, .. } if reason.contains("hash")),
            "{err:?}"
        );
    }

    #[test]
    fn has_checks_presence_and_length_only() {
        let (_dir, mut blobs) = blobs();
        let Content::Blob { blob, len } = blobs.put(big(1)).unwrap() else {
            panic!()
        };
        assert!(blobs.has(&blob, len));
        assert!(!blobs.has(&blob, len + 1));
        assert!(!blobs.has(&BlobRef::from_bytes([0; 32]), len));
    }

    #[test]
    fn remove_deletes_the_file_and_tolerates_a_missing_one() {
        let (_dir, mut blobs) = blobs();
        let Content::Blob { blob, len } = blobs.put(big(1)).unwrap() else {
            panic!()
        };
        blobs.remove(&blob).unwrap();
        assert!(!blobs.has(&blob, len));
        blobs.remove(&blob).unwrap();
    }

    #[test]
    fn list_finds_blobs_and_interrupted_writes() {
        let (dir, mut blobs) = blobs();
        let Content::Blob { blob, len } = blobs.put(big(1)).unwrap() else {
            panic!()
        };
        let hex = blob.to_string();
        let stray = dir
            .path()
            .join("blobs")
            .join(&hex[..2])
            .join(".tmp-crashed");
        std::fs::write(&stray, b"half").unwrap();
        std::fs::write(dir.path().join("blobs").join("README.txt"), b"not ours").unwrap();

        let (found, leftovers) = blobs.list().unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].blob, found[0].len), (blob, len));
        assert_eq!(leftovers.len(), 1);
        assert_eq!(leftovers[0].path, stray);
    }

    #[test]
    fn listing_a_store_that_never_wrote_a_blob_is_empty() {
        let (_dir, blobs) = blobs();
        let (found, leftovers) = blobs.list().unwrap();
        assert!(found.is_empty() && leftovers.is_empty());
    }

    #[test]
    fn the_in_memory_store_behaves_the_same() {
        let mut blobs = Blobs::in_memory();
        let content = blobs.put(big(2)).unwrap();
        let Content::Blob { blob, len } = &content else {
            panic!()
        };
        assert_eq!(blobs.put(big(2)).unwrap(), content);
        assert_eq!(blobs.read(&content).unwrap().as_ref(), big(2).as_slice());
        assert!(blobs.has(blob, *len));
        assert_eq!(blobs.list().unwrap().0.len(), 1);
        blobs.remove(blob).unwrap();
        assert!(matches!(
            blobs.read(&content),
            Err(Error::NotFound { entity: "blob", .. })
        ));
    }
}
