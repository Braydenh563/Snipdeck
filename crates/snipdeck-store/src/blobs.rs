//! The content-addressed blob folder.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use snipdeck_core::{BlobRef, Content, INLINE_LIMIT, Inline};
use zeroize::Zeroizing;

use crate::{Error, Result};

/// The content-addressed blob folder (`blobs/` in the data folder, ADR-0002).
///
/// This is the only place that decides between inline and blob storage, using
/// [`INLINE_LIMIT`](snipdeck_core::INLINE_LIMIT), and the only place that hashes (BLAKE3).
///
/// A blob returned by `put` stays protected until an item that uses it is written, even if
/// other writes release the same bytes meanwhile; one never attached is collected by
/// [`Store::gc_blobs`](crate::Store::gc_blobs). For sensitive content use
/// [`Store::put_sensitive`](crate::Store::put_sensitive), which keeps it off the disk.
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

/// The blob folder, or an in-memory stand-in for [`Store::open_in_memory`](crate::Store), plus
/// two pieces of session state:
///
/// * `pending`: how many times `put` handed out each blob that no committed row has used
///   since. Releasing a blob skips pending ones (review M3), so the documented `put`, then
///   `insert_item` sequence cannot lose its blob to an unrelated write in between.
/// * `plain`: decrypted payloads of encrypted items that are too large to be inline, and
///   sensitive captures staged with [`Blobs::put_transient`]. Memory only: never written to
///   disk unless they end up in an item that is not encrypted. Dropped by `clear_transient`
///   (when the key ring is locked) and with the store.
pub(crate) struct Blobs {
    backing: Backing,
    pending: HashMap<BlobRef, usize>,
    plain: RefCell<HashMap<BlobRef, Zeroizing<Vec<u8>>>>,
    /// While a transaction is open: the blob files it created (they did not exist before), so
    /// a rollback can delete them again. `None` outside a transaction.
    created: Option<Vec<BlobRef>>,
    /// While a transaction is open: the transient payloads a write took into an item. They are
    /// plaintext in RAM that nothing needs once the write is over, so the transaction's end (commit
    /// or rollback) drops them; see [`Blobs::take_transient`].
    taken: Vec<BlobRef>,
}

/// Where a transaction (or a savepoint inside one) stood when it began; see
/// [`Blobs::mark`] and [`Blobs::rewind`].
pub(crate) struct BlobMark {
    created: usize,
    pending: HashMap<BlobRef, usize>,
    /// How many transient payloads had been taken, and which ones existed at all.
    taken: usize,
    transient: HashSet<BlobRef>,
}

enum Backing {
    /// Files under `root/ab/cdef...` (hash in lowercase hex).
    Disk { root: PathBuf },
    /// Nothing touches the disk.
    Memory(HashMap<BlobRef, Vec<u8>>),
}

impl fmt::Debug for Blobs {
    /// Counts and sizes only: blob bytes are user content (review L5).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = f.debug_struct("Blobs");
        match &self.backing {
            Backing::Disk { root } => out.field("root", root),
            Backing::Memory(map) => out
                .field("in_memory", &map.len())
                .field("bytes", &map.values().map(Vec::len).sum::<usize>()),
        };
        out.field("pending", &self.pending.len())
            .field("transient", &self.plain.borrow().len())
            .finish_non_exhaustive()
    }
}

/// Prefix of in-flight files; a crash can leave them behind and [`Blobs::list`] reports them.
const TEMP_PREFIX: &str = ".tmp-";

#[cfg(test)]
thread_local! {
    /// Test hook: makes every blob deletion on this thread fail.
    pub(crate) static FAIL_REMOVES: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
}

impl Blobs {
    pub(crate) fn on_disk(root: PathBuf) -> Self {
        Self::with(Backing::Disk { root })
    }

    pub(crate) fn in_memory() -> Self {
        Self::with(Backing::Memory(HashMap::new()))
    }

    fn with(backing: Backing) -> Self {
        Self {
            backing,
            pending: HashMap::new(),
            plain: RefCell::new(HashMap::new()),
            created: None,
            taken: Vec::new(),
        }
    }

    /// Starts (or joins) recording which blob files get created, and notes the pending counts.
    pub(crate) fn mark(&mut self) -> BlobMark {
        let created = self.created.get_or_insert_with(Vec::new).len();
        BlobMark {
            created,
            pending: self.pending.clone(),
            taken: self.taken.len(),
            transient: self.plain.borrow().keys().copied().collect(),
        }
    }

    /// Undoes the blob side of a rolled-back transaction: deletes the files created since
    /// `mark` (no committed row can use a file that did not exist before) and restores the
    /// pending counts. Transient plaintext a rolled-back write took, or that was staged since
    /// `mark`, is dropped (zeroed) too. The files are scrubbed first: a payload staged there may be plaintext. A
    /// file that cannot be deleted now is left for `Store::gc_blobs`.
    pub(crate) fn rewind(&mut self, mark: BlobMark) {
        self.pending = mark.pending;
        let taken = self.taken.split_off(mark.taken.min(self.taken.len()));
        let mut plain = self.plain.borrow_mut();
        for blob in taken {
            plain.remove(&blob);
        }
        plain.retain(|blob, _| mark.transient.contains(blob));
        drop(plain);
        let doomed = self
            .created
            .as_mut()
            .map(|log| log.split_off(mark.created.min(log.len())))
            .unwrap_or_default();
        for blob in doomed {
            let _ = self.remove(&blob, true);
        }
    }

    /// Stops recording created files (the outermost transaction ended).
    /// The transient payloads the transaction's writes took are dropped (zeroed): the rows
    /// they went into hold the sealed or stored copy now.
    pub(crate) fn end_log(&mut self) {
        self.created = None;
        let mut plain = self.plain.borrow_mut();
        for blob in self.taken.drain(..) {
            plain.remove(&blob);
        }
    }

    /// Notes that a write is taking `content` into an item: if it is a transient payload it is
    /// dropped when the transaction ends (or, if that is rolled back, at the rollback). Not at
    /// once, because another row of the same transaction may carry the same payload.
    pub(crate) fn take_transient(&mut self, content: &Content) {
        if let Content::Blob { blob, .. } = content
            && self.plain.borrow().contains_key(blob)
        {
            self.taken.push(*blob);
        }
    }

    /// Inline or blob, by [`INLINE_LIMIT`]: the single place the threshold is applied. A new
    /// blob is pending until a committed row uses it ([`Blobs::attached`]).
    pub(crate) fn put(&mut self, bytes: Vec<u8>) -> Result<Content> {
        let content = self.store(bytes)?;
        if let Content::Blob { blob, .. } = &content {
            *self.pending.entry(*blob).or_default() += 1;
        }
        Ok(content)
    }

    /// Like [`Blobs::put`] but not pending: for payloads the store writes itself inside a
    /// transaction (envelopes, materialised plaintext), which the transaction's rows use.
    pub(crate) fn store(&mut self, bytes: Vec<u8>) -> Result<Content> {
        if bytes.len() <= INLINE_LIMIT {
            return Ok(Content::Inline(Inline::new(bytes)?));
        }
        let blob = BlobRef::from_bytes(*blake3::hash(&bytes).as_bytes());
        let len = bytes.len() as u64;
        match &mut self.backing {
            Backing::Memory(map) => {
                let new = map.insert(blob, bytes).is_none();
                if new {
                    self.note_created(blob);
                }
            }
            Backing::Disk { .. } => {
                if self.holds_exactly(&blob, &bytes) {
                    // Identical bytes are already stored. Refresh the file's age so a sweep for
                    // orphans cannot take it between this call and the row that will use it.
                    self.touch(&blob);
                } else {
                    // Missing, truncated, or damaged with the right length (review L4).
                    let existed = self.file_path(&blob).exists();
                    self.write_atomically(&blob, &bytes)?;
                    if !existed {
                        self.note_created(blob);
                    }
                }
            }
        }
        Ok(Content::Blob { blob, len })
    }

    fn note_created(&mut self, blob: BlobRef) {
        if let Some(log) = &mut self.created {
            log.push(blob);
        }
    }

    /// Keeps `bytes` in memory only and returns content that refers to them; see the type's
    /// docs. Small payloads are inline as usual.
    pub(crate) fn put_transient(&self, bytes: Vec<u8>) -> Result<Content> {
        if bytes.len() <= INLINE_LIMIT {
            return Ok(Content::Inline(Inline::new(bytes)?));
        }
        let blob = BlobRef::from_bytes(*blake3::hash(&bytes).as_bytes());
        let len = bytes.len() as u64;
        self.plain.borrow_mut().insert(blob, Zeroizing::new(bytes));
        Ok(Content::Blob { blob, len })
    }

    /// Whether `blob` is held in memory by [`Blobs::put_transient`].
    pub(crate) fn is_transient(&self, blob: &BlobRef) -> bool {
        self.plain.borrow().contains_key(blob)
    }

    /// Forgets every transient payload (they are zeroed on drop).
    pub(crate) fn clear_transient(&self) {
        self.plain.borrow_mut().clear();
    }

    /// Marks blobs as used by a committed row: one `put` each stops being pending.
    pub(crate) fn attached(&mut self, blobs: impl IntoIterator<Item = BlobRef>) {
        for blob in blobs {
            if let Some(count) = self.pending.get_mut(&blob) {
                *count -= 1;
                if *count == 0 {
                    self.pending.remove(&blob);
                }
            }
        }
    }

    /// Whether a `put` handed out `blob` and no committed row has used it since.
    pub(crate) fn is_pending(&self, blob: &BlobRef) -> bool {
        self.pending.contains_key(blob)
    }

    /// Reads `content`'s bytes from the blob folder (or inline), checking length and hash.
    /// Transient payloads are not found here; see [`Blobs::read_any`].
    pub(crate) fn read<'a>(&self, content: &'a Content) -> Result<Cow<'a, [u8]>> {
        let (blob, len) = match content {
            Content::Inline(inline) => return Ok(Cow::Borrowed(inline.as_bytes())),
            Content::Blob { blob, len } => (blob, *len),
        };
        let bytes = match &self.backing {
            Backing::Memory(map) => map.get(blob).cloned().ok_or_else(|| not_found(blob))?,
            Backing::Disk { .. } => {
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

    /// [`Blobs::read`], but a transient payload is served from memory first.
    pub(crate) fn read_any<'a>(&self, content: &'a Content) -> Result<Cow<'a, [u8]>> {
        if let Content::Blob { blob, len } = content
            && let Some(bytes) = self.plain.borrow().get(blob)
            && bytes.len() as u64 == *len
        {
            return Ok(Cow::Owned(bytes.to_vec()));
        }
        self.read(content)
    }

    /// Whether a blob of `len` bytes is stored under `blob` (metadata only, no hashing).
    /// Transient payloads do not count: rows may only refer to stored blobs.
    pub(crate) fn has(&self, blob: &BlobRef, len: u64) -> bool {
        match &self.backing {
            Backing::Memory(map) => map.get(blob).is_some_and(|b| b.len() as u64 == len),
            Backing::Disk { .. } => {
                fs::metadata(self.file_path(blob)).is_ok_and(|m| m.is_file() && m.len() == len)
            }
        }
    }

    /// Whether the stored file holds exactly `bytes` (a stored blob of the right length whose
    /// content differs is damaged and must be rewritten).
    fn holds_exactly(&self, blob: &BlobRef, bytes: &[u8]) -> bool {
        self.has(blob, bytes.len() as u64)
            && fs::read(self.file_path(blob)).is_ok_and(|stored| stored == bytes)
    }

    /// Whether the stored blob starts with `prefix` (reads only that many bytes).
    pub(crate) fn starts_with(&self, blob: &BlobRef, prefix: &[u8]) -> Result<bool> {
        match &self.backing {
            Backing::Memory(map) => Ok(map.get(blob).is_some_and(|b| b.starts_with(prefix))),
            Backing::Disk { .. } => {
                let mut file =
                    fs::File::open(self.file_path(blob)).map_err(|e| io_or_missing(e, blob))?;
                let mut head = vec![0; prefix.len()];
                match file.read_exact(&mut head) {
                    Ok(()) => Ok(head == prefix),
                    Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(false),
                    Err(e) => Err(e.into()),
                }
            }
        }
    }

    /// Deletes a blob; a blob that is already gone is fine. With `scrub`, the file is first
    /// overwritten with zeros and flushed: best effort against recovery of plaintext, which
    /// the file system may still defeat (copy-on-write, SSD wear levelling, snapshots).
    pub(crate) fn remove(&mut self, blob: &BlobRef, scrub: bool) -> Result<()> {
        #[cfg(test)]
        if FAIL_REMOVES.with(core::cell::Cell::get) {
            return Err(io::Error::other("injected failure").into());
        }
        match &mut self.backing {
            Backing::Memory(map) => {
                map.remove(blob);
                Ok(())
            }
            Backing::Disk { .. } => {
                let path = self.file_path(blob);
                if scrub {
                    overwrite_with_zeros(&path);
                }
                match fs::remove_file(path) {
                    Ok(()) => Ok(()),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                    Err(e) => Err(e.into()),
                }
            }
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
        let root = match &self.backing {
            Backing::Memory(map) => {
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
            Backing::Disk { root } => root,
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
        match self.backing {
            Backing::Memory(_) => None,
            Backing::Disk { .. } => Some(self.file_path(blob)),
        }
    }

    fn file_path(&self, blob: &BlobRef) -> PathBuf {
        let Backing::Disk { root } = &self.backing else {
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

/// Overwrites a file in place with zeros and flushes it. Best effort: errors are ignored
/// because the delete that follows is what the caller relies on.
fn overwrite_with_zeros(path: &Path) {
    let Ok(mut file) = fs::OpenOptions::new().write(true).open(path) else {
        return;
    };
    let Ok(len) = file.metadata().map(|m| m.len()) else {
        return;
    };
    let zeros = vec![0_u8; 64 * 1024];
    let mut left = len;
    while left > 0 {
        let n = usize::try_from(left.min(zeros.len() as u64)).unwrap_or(zeros.len());
        if file.write_all(&zeros[..n]).is_err() {
            return;
        }
        left -= n as u64;
    }
    let _ = file.sync_all();
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
        assert_eq!(files_under(dir.path()).len(), 0);
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

    /// Review L4: a file of the right length with different bytes is rewritten by a re-put.
    #[test]
    fn putting_again_repairs_a_file_damaged_without_changing_length() {
        let (_dir, mut blobs) = blobs();
        let content = blobs.put(big(5)).unwrap();
        let Content::Blob { blob, .. } = &content else {
            panic!()
        };
        let path = blobs.path_of(blob).unwrap();
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[0] ^= 0xFF;
        std::fs::write(&path, bytes).unwrap();
        assert!(matches!(
            blobs.read(&content),
            Err(Error::BlobCorrupt { .. })
        ));
        assert_eq!(blobs.put(big(5)).unwrap(), content);
        assert_eq!(blobs.read(&content).unwrap().as_ref(), big(5).as_slice());
    }

    #[test]
    fn debug_shows_counts_not_bytes() {
        let mut blobs = Blobs::in_memory();
        blobs.put(big(0xAB)).unwrap();
        let shown = format!("{blobs:?}");
        assert!(shown.len() < 200 && !shown.contains("171"), "{shown}");
    }

    #[test]
    fn a_blob_is_pending_from_put_until_attached_once_per_put() {
        let mut blobs = Blobs::in_memory();
        let Content::Blob { blob, .. } = blobs.put(big(1)).unwrap() else {
            panic!()
        };
        blobs.put(big(1)).unwrap();
        assert!(blobs.is_pending(&blob));
        blobs.attached([blob]);
        assert!(blobs.is_pending(&blob), "the second put is still waiting");
        blobs.attached([blob, blob]);
        assert!(!blobs.is_pending(&blob));
    }

    #[test]
    fn transient_payloads_stay_in_memory_and_are_not_stored_blobs() {
        let (dir, blobs) = blobs();
        let content = blobs.put_transient(big(4)).unwrap();
        let Content::Blob { blob, len } = &content else {
            panic!()
        };
        assert_eq!(files_under(dir.path()).len(), 0);
        assert!(!blobs.has(blob, *len));
        assert!(blobs.is_transient(blob));
        assert_eq!(
            blobs.read_any(&content).unwrap().as_ref(),
            big(4).as_slice()
        );
        assert!(matches!(blobs.read(&content), Err(Error::NotFound { .. })));
        blobs.clear_transient();
        assert!(matches!(
            blobs.read_any(&content),
            Err(Error::NotFound { .. })
        ));
    }

    #[test]
    fn a_scrubbed_remove_deletes_the_file() {
        let (dir, mut blobs) = blobs();
        let Content::Blob { blob, len } = blobs.put(big(6)).unwrap() else {
            panic!()
        };
        blobs.remove(&blob, true).unwrap();
        assert!(!blobs.has(&blob, len));
        assert_eq!(files_under(dir.path()).len(), 0);
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
        blobs.remove(&blob, false).unwrap();
        assert!(!blobs.has(&blob, len));
        blobs.remove(&blob, false).unwrap();
    }

    /// Security review 2, finding 1: a rolled-back transaction must scrub the files it created
    /// (a payload staged there may be plaintext), not just unlink them. A hard link keeps the
    /// contents reachable after the unlink, so it shows whether they were overwritten.
    #[test]
    fn rewind_scrubs_the_files_it_deletes() {
        let (dir, mut blobs) = blobs();
        let secret = vec![0xA5; INLINE_LIMIT + 10];
        let mark = blobs.mark();
        let Content::Blob { blob, .. } = blobs.put(secret.clone()).unwrap() else {
            panic!()
        };
        let link = dir.path().join("kept-by-hard-link");
        std::fs::hard_link(blobs.path_of(&blob).unwrap(), &link).unwrap();
        blobs.rewind(mark);
        assert!(!blobs.path_of(&blob).unwrap().exists(), "unlinked");
        let left = std::fs::read(&link).unwrap();
        assert!(
            left.iter().all(|b| *b == 0),
            "the rolled-back file still held its bytes"
        );
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
        blobs.remove(blob, false).unwrap();
        assert!(matches!(
            blobs.read(&content),
            Err(Error::NotFound { entity: "blob", .. })
        ));
    }
}
