//! Reading and writing the zip container, defensively.
//!
//! [`SafeArchive`] is the only code that touches an untrusted zip. It never extracts to disk
//! (so zip-slip has nothing to write to) and still rejects every dangerous name, because a name
//! that is dangerous here would be dangerous in any other tool the file is opened with.
//!
//! What it rejects, before decompressing anything: too many entries, entries or a total over
//! the declared size limits, absurd compression ratios, absolute or `..` paths, backslashes,
//! drive letters, control and non-ASCII characters, repeated names, symbolic links, devices,
//! encrypted entries, unsupported compression methods and overlapping entries (the "quine"
//! style of zip bomb). While decompressing it enforces the same limits against the bytes that
//! really come out, because headers can lie.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom, Write};

use snipdeck_core::BlobRef;
use zip::result::ZipError;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::{Error, Result};
use crate::limits::Limits;

pub(crate) const MANIFEST: &str = "manifest.json";
pub(crate) const ITEMS: &str = "items.json";
pub(crate) const ASSET_DIR: &str = "assets/";

/// The longest entry name accepted.
const MAX_NAME_LEN: usize = 255;
/// Read buffer size.
const CHUNK: usize = 64 * 1024;

/// Unix file type bits in a zip entry's external attributes.
const S_IFMT: u32 = 0o170_000;
const S_IFREG: u32 = 0o100_000;
const S_IFDIR: u32 = 0o040_000;

fn zip_error(error: ZipError) -> Error {
    match error {
        ZipError::Io(e) => io_error(e),
        other => Error::NotAnArchive(other.to_string()),
    }
}

/// Reading a damaged zip surfaces as `InvalidData`/`UnexpectedEof`; those are the archive's
/// fault, anything else is the disk's.
fn io_error(error: std::io::Error) -> Error {
    use std::io::ErrorKind::{InvalidData, InvalidInput, UnexpectedEof};
    match error.kind() {
        InvalidData | UnexpectedEof | InvalidInput => Error::NotAnArchive(error.to_string()),
        _ => Error::Io(error),
    }
}

/// Whether `name` is a well-formed entry name: relative, `/`-separated, ASCII letters, digits,
/// `.`, `_` and `-` only, no empty, `.` or `..` components. A trailing `/` marks a folder.
fn check_name(name: &str) -> core::result::Result<(), &'static str> {
    if name.is_empty() || name.len() > MAX_NAME_LEN {
        return Err("empty or too long");
    }
    if name.starts_with('/') {
        return Err("an absolute path");
    }
    if name.contains('\\') {
        return Err("using a backslash");
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'))
    {
        return Err("using characters outside letters, digits and `._-/`");
    }
    let body = name.strip_suffix('/').unwrap_or(name);
    for part in body.split('/') {
        match part {
            "" => return Err("containing an empty path component"),
            "." | ".." => return Err("a path that climbs out of the archive"),
            _ => {}
        }
    }
    Ok(())
}

/// The number of central directory entries the end-of-central-directory record declares
/// (the zip64 record when the plain one says `0xFFFF`), or `None` if no record signature is
/// found; the zip library will then reject the file itself.
///
/// The record that counts is the one that ends exactly at the end of the file, its comment
/// filling the rest. A second signature inside that comment (a decoy that would make the count
/// disagree with the directory the zip library reads) or a record followed by other bytes is
/// refused, so the checks below cannot be steered by planted bytes.
fn declared_entry_count<R: Read + Seek>(reader: &mut R) -> Result<Option<u64>> {
    const EOCD: [u8; 4] = *b"PK\x05\x06";
    const LOCATOR: [u8; 4] = *b"PK\x06\x07";
    const EOCD64: [u8; 4] = *b"PK\x06\x06";
    // The record is 22 bytes followed by a comment of at most 65 535 bytes, and the zip64
    // locator (20 bytes) sits just before it.
    const SEARCH: u64 = 20 + 22 + 65_535;
    let len = reader.seek(SeekFrom::End(0))?;
    let start = len.saturating_sub(SEARCH);
    reader.seek(SeekFrom::Start(start))?;
    let mut tail = Vec::new();
    reader.by_ref().take(SEARCH).read_to_end(&mut tail)?;
    let signatures: Vec<usize> = tail
        .windows(4)
        .enumerate()
        .filter_map(|(at, w)| (w == EOCD).then_some(at))
        .collect();
    let ends_the_file = |at: usize| {
        tail.get(at + 20..at + 22)
            .is_some_and(|c| at + 22 + usize::from(u16::from_le_bytes([c[0], c[1]])) == tail.len())
    };
    let mut exact = signatures.iter().copied().filter(|at| ends_the_file(*at));
    let (Some(pos), None) = (exact.next(), exact.next()) else {
        return if signatures.is_empty() {
            Ok(None)
        } else if signatures.iter().any(|at| ends_the_file(*at)) {
            Err(Error::unsafe_entry(
                "(archive)",
                "has more than one end-of-central-directory record",
            ))
        } else {
            Err(Error::NotAnArchive(
                "the end-of-central-directory record does not end the file".to_owned(),
            ))
        };
    };
    if signatures.iter().any(|at| *at > pos) {
        return Err(Error::unsafe_entry(
            "(archive)",
            "has an end-of-central-directory record inside the archive comment",
        ));
    }
    let record = &tail[pos..];
    let total = u64::from(u16::from_le_bytes([record[10], record[11]]));
    if total != 0xFFFF {
        return Ok(Some(total));
    }
    // zip64: a locator sits just before the record and points at the zip64 record.
    let Some(locator) = pos.checked_sub(20).map(|p| &tail[p..pos]) else {
        return Ok(None);
    };
    if locator[..4] != LOCATOR {
        return Ok(None);
    }
    let offset = u64::from_le_bytes(locator[8..16].try_into().unwrap_or([0; 8]));
    reader.seek(SeekFrom::Start(offset))?;
    let mut head = [0_u8; 56];
    if reader.read_exact(&mut head).is_err() || head[..4] != EOCD64 {
        return Ok(None);
    }
    Ok(Some(u64::from_le_bytes(
        head[32..40].try_into().unwrap_or([0; 8]),
    )))
}

/// The asset an entry named `assets/<hash>` holds.
fn asset_hash(name: &str) -> Option<BlobRef> {
    name.strip_prefix(ASSET_DIR)?.parse().ok()
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    index: usize,
    size: u64,
}

/// A validated, read-only view of a `.snipdeck` zip.
pub(crate) struct SafeArchive<R: Read + Seek> {
    zip: ZipArchive<R>,
    manifest: Option<Entry>,
    items: Option<Entry>,
    assets: HashMap<BlobRef, Entry>,
    unknown: usize,
    limits: Limits,
}

impl<R: Read + Seek> SafeArchive<R> {
    /// Opens `reader` and validates every entry's metadata against `limits`.
    pub(crate) fn open(mut reader: R, limits: Limits) -> Result<Self> {
        let len = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(0))?;
        let allowed = limits
            .max_total_bytes
            .saturating_add((limits.max_entries as u64).saturating_mul(1024))
            .saturating_add(1024 * 1024);
        if len > allowed {
            return Err(Error::LimitExceeded {
                what: "archive file size",
                limit: allowed,
            });
        }

        let declared = declared_entry_count(&mut reader)?;
        // Before the zip library reads the directory into memory: a file that declares millions
        // of entries must not be allowed to allocate for them.
        if declared.is_some_and(|n| n > limits.max_entries as u64) {
            return Err(Error::LimitExceeded {
                what: "entry count",
                limit: limits.max_entries as u64,
            });
        }
        reader.seek(SeekFrom::Start(0))?;
        let mut zip = ZipArchive::new(reader).map_err(zip_error)?;
        // The zip library keeps one entry per name. If the directory lists more entries than it
        // kept, some name appears twice, and different tools would disagree about which wins.
        if declared.is_some_and(|n| n != zip.len() as u64) {
            return Err(Error::unsafe_entry(
                "(archive)",
                "listed more than once (the entry count and the names disagree)",
            ));
        }
        if zip.len() > limits.max_entries {
            return Err(Error::LimitExceeded {
                what: "entry count",
                limit: limits.max_entries as u64,
            });
        }

        let mut scan = Scan::default();
        for index in 0..zip.len() {
            let file = zip.by_index_raw(index).map_err(zip_error)?;
            let facts = Facts {
                index,
                name: std::str::from_utf8(file.name_raw()).map(str::to_owned).ok(),
                size: file.size(),
                compressed: file.compressed_size(),
                encrypted: file.encrypted(),
                mode: file.unix_mode(),
                method: file.compression(),
            };
            drop(file);
            scan.add(&facts, &limits)?;
        }
        if zip.has_overlapping_entries().map_err(zip_error)? {
            return Err(Error::unsafe_entry(
                "(archive)",
                "stored in data that overlaps another entry",
            ));
        }

        Ok(Self {
            zip,
            manifest: scan.manifest,
            items: scan.items,
            assets: scan.assets,
            unknown: scan.unknown,
            limits,
        })
    }

    /// Entries this version does not know. They are never read.
    pub(crate) fn unknown_entries(&self) -> usize {
        self.unknown
    }

    /// How many assets the archive holds.
    pub(crate) fn asset_count(&self) -> usize {
        self.assets.len()
    }

    /// The declared (and, once read, verified) size of an asset, if the archive has it.
    pub(crate) fn asset_size(&self, asset: &BlobRef) -> Option<u64> {
        self.assets.get(asset).map(|e| e.size)
    }

    /// Every asset hash in the archive.
    pub(crate) fn asset_hashes(&self) -> impl Iterator<Item = &BlobRef> {
        self.assets.keys()
    }

    /// `manifest.json`, at most [`Limits::MAX_MANIFEST_BYTES`].
    pub(crate) fn read_manifest(&mut self) -> Result<Vec<u8>> {
        let entry = self.manifest.ok_or(Error::MissingEntry(MANIFEST))?;
        if entry.size > Limits::MAX_MANIFEST_BYTES {
            return Err(Error::LimitExceeded {
                what: "manifest size",
                limit: Limits::MAX_MANIFEST_BYTES,
            });
        }
        self.read_all(entry, MANIFEST)
    }

    /// `items.json`, at most [`Limits::max_json_bytes`].
    pub(crate) fn read_items(&mut self) -> Result<Vec<u8>> {
        let entry = self.items.ok_or(Error::MissingEntry(ITEMS))?;
        if entry.size > self.limits.max_json_bytes {
            return Err(Error::LimitExceeded {
                what: "items.json size",
                limit: self.limits.max_json_bytes,
            });
        }
        self.read_all(entry, ITEMS)
    }

    /// An asset's bytes.
    pub(crate) fn read_asset(&mut self, asset: &BlobRef) -> Result<Vec<u8>> {
        let entry = self.entry_of(asset)?;
        self.read_all(entry, &format!("assets/{asset}"))
    }

    /// The BLAKE3 hash of an asset's bytes, computed while streaming (nothing is kept).
    pub(crate) fn hash_asset(&mut self, asset: &BlobRef) -> Result<[u8; 32]> {
        let entry = self.entry_of(asset)?;
        let mut hasher = blake3::Hasher::new();
        self.pump(entry, &format!("assets/{asset}"), |chunk| {
            hasher.update(chunk);
        })?;
        Ok(*hasher.finalize().as_bytes())
    }

    fn entry_of(&self, asset: &BlobRef) -> Result<Entry> {
        self.assets
            .get(asset)
            .copied()
            .ok_or_else(|| Error::AssetMismatch {
                asset: asset.to_string(),
                reason: "it is not in the archive".to_owned(),
            })
    }

    fn read_all(&mut self, entry: Entry, label: &str) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(usize::try_from(entry.size).unwrap_or(0));
        self.pump(entry, label, |chunk| out.extend_from_slice(chunk))?;
        Ok(out)
    }

    /// Decompresses one entry in chunks, never producing more bytes than its header declared,
    /// and checks that it produced exactly that many.
    fn pump(&mut self, entry: Entry, label: &str, mut sink: impl FnMut(&[u8])) -> Result<()> {
        let mut file = self.zip.by_index(entry.index).map_err(zip_error)?;
        let mut buf = vec![0_u8; CHUNK];
        let mut produced = 0_u64;
        loop {
            let n = file.read(&mut buf).map_err(io_error)?;
            if n == 0 {
                break;
            }
            produced += n as u64;
            if produced > entry.size {
                return Err(Error::unsafe_entry(
                    label,
                    "larger when decompressed than its header says",
                ));
            }
            sink(&buf[..n]);
        }
        if produced != entry.size {
            return Err(Error::unsafe_entry(
                label,
                "shorter when decompressed than its header says",
            ));
        }
        Ok(())
    }
}

/// What the central directory says about one entry.
struct Facts {
    index: usize,
    name: Option<String>,
    size: u64,
    compressed: u64,
    encrypted: bool,
    mode: Option<u32>,
    method: CompressionMethod,
}

#[derive(Default)]
struct Scan {
    seen: HashSet<String>,
    total: u64,
    manifest: Option<Entry>,
    items: Option<Entry>,
    assets: HashMap<BlobRef, Entry>,
    unknown: usize,
}

impl Scan {
    fn add(&mut self, facts: &Facts, limits: &Limits) -> Result<()> {
        let name = facts.name.as_deref().ok_or_else(|| {
            Error::unsafe_entry("(not UTF-8)", "named with bytes that are not UTF-8")
        })?;
        check_name(name).map_err(|reason| Error::unsafe_entry(name, reason))?;
        if !self.seen.insert(name.to_owned()) {
            return Err(Error::unsafe_entry(name, "listed more than once"));
        }
        if facts.encrypted {
            return Err(Error::unsafe_entry(name, "password-protected"));
        }
        if !matches!(
            facts.method,
            CompressionMethod::Stored | CompressionMethod::Deflated
        ) {
            return Err(Error::unsafe_entry(
                name,
                "using an unsupported compression method",
            ));
        }
        let is_dir = name.ends_with('/');
        match facts.mode.map(|m| m & S_IFMT) {
            None | Some(0) => {}
            Some(S_IFREG) if !is_dir => {}
            Some(S_IFDIR) if is_dir => {}
            Some(0o120_000) => return Err(Error::unsafe_entry(name, "a symbolic link")),
            Some(_) => return Err(Error::unsafe_entry(name, "not a regular file")),
        }

        if facts.size > limits.max_entry_bytes {
            return Err(Error::LimitExceeded {
                what: "uncompressed entry size",
                limit: limits.max_entry_bytes,
            });
        }
        self.total = self.total.saturating_add(facts.size);
        if self.total > limits.max_total_bytes {
            return Err(Error::LimitExceeded {
                what: "total uncompressed size",
                limit: limits.max_total_bytes,
            });
        }
        if facts.size > Limits::RATIO_FLOOR
            && facts.size / facts.compressed.max(1) > limits.max_ratio
        {
            return Err(Error::unsafe_entry(
                name,
                "compressed suspiciously well (zip bomb)",
            ));
        }
        if is_dir {
            if facts.size != 0 {
                return Err(Error::unsafe_entry(name, "a folder with content"));
            }
            return Ok(());
        }

        let entry = Entry {
            index: facts.index,
            size: facts.size,
        };
        if name == MANIFEST {
            self.manifest = Some(entry);
        } else if name == ITEMS {
            self.items = Some(entry);
        } else if name.starts_with(ASSET_DIR) {
            let hash = asset_hash(name)
                .ok_or_else(|| Error::unsafe_entry(name, "an asset not named by a BLAKE3 hash"))?;
            self.assets.insert(hash, entry);
        } else {
            self.unknown += 1;
        }
        Ok(())
    }
}

/// Writes a `.snipdeck` zip under the same [`Limits`] the reader enforces, so an archive this
/// crate writes is one it can read back.
pub(crate) struct ArchiveWriter<W: Write + Seek> {
    zip: ZipWriter<W>,
    limits: Limits,
    entries: usize,
    total: u64,
}

impl<W: Write + Seek> ArchiveWriter<W> {
    pub(crate) fn new(out: W, limits: Limits) -> Self {
        Self {
            zip: ZipWriter::new(out),
            limits,
            entries: 0,
            total: 0,
        }
    }

    /// Adds one file. `compress` is false for data that is already compressed (PNG).
    pub(crate) fn add(&mut self, name: &str, bytes: &[u8], compress: bool) -> Result<()> {
        let size = bytes.len() as u64;
        if self.entries + 1 > self.limits.max_entries {
            return Err(Error::LimitExceeded {
                what: "entry count",
                limit: self.limits.max_entries as u64,
            });
        }
        if size > self.limits.max_entry_bytes {
            return Err(Error::LimitExceeded {
                what: "uncompressed entry size",
                limit: self.limits.max_entry_bytes,
            });
        }
        if self.total.saturating_add(size) > self.limits.max_total_bytes {
            return Err(Error::LimitExceeded {
                what: "total uncompressed size",
                limit: self.limits.max_total_bytes,
            });
        }
        let method = if compress {
            CompressionMethod::Deflated
        } else {
            CompressionMethod::Stored
        };
        let options = SimpleFileOptions::default()
            .compression_method(method)
            .unix_permissions(0o644);
        self.zip.start_file(name, options).map_err(zip_error)?;
        self.zip.write_all(bytes)?;
        self.entries += 1;
        self.total += size;
        Ok(())
    }

    /// Bytes added so far, before compression.
    pub(crate) fn total_bytes(&self) -> u64 {
        self.total
    }

    pub(crate) fn finish(self) -> Result<W> {
        self.zip.finish().map_err(zip_error)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    /// An end-of-central-directory record declaring `total` entries, then `comment`.
    fn eocd(total: u16, comment_len: u16, comment: &[u8]) -> Vec<u8> {
        let mut out = b"PK\x05\x06".to_vec();
        out.extend_from_slice(&[0; 4]); // disk numbers
        out.extend_from_slice(&total.to_le_bytes()); // entries on this disk
        out.extend_from_slice(&total.to_le_bytes()); // entries
        out.extend_from_slice(&[0; 8]); // directory size and offset
        out.extend_from_slice(&comment_len.to_le_bytes());
        out.extend_from_slice(comment);
        out
    }

    fn declared(bytes: Vec<u8>) -> Result<Option<u64>> {
        declared_entry_count(&mut Cursor::new(bytes))
    }

    #[test]
    fn a_plain_record_is_read() {
        assert_eq!(declared(eocd(7, 0, b"")).unwrap(), Some(7));
        let mut with_comment = b"data".to_vec();
        with_comment.extend(eocd(9, 5, b"hello"));
        assert_eq!(declared(with_comment).unwrap(), Some(9));
    }

    #[test]
    fn no_record_is_left_to_the_zip_library() {
        assert_eq!(declared(b"not a zip".to_vec()).unwrap(), None);
    }

    /// Security review 2, finding 2: a record planted in the zip comment must not win.
    #[test]
    fn a_decoy_record_in_the_comment_is_refused() {
        let decoy = eocd(1, 0, b"");
        let bytes = eocd(5_000, u16::try_from(decoy.len()).unwrap(), &decoy);
        assert!(declared(bytes).is_err(), "decoy that ends the file");
        // A decoy whose own comment length is wrong (so it does not end the file) is still a
        // second record inside the real one's comment.
        let decoy = eocd(1, 3, b"");
        let bytes = eocd(5_000, u16::try_from(decoy.len()).unwrap(), &decoy);
        assert!(declared(bytes).is_err(), "decoy that ends elsewhere");
    }

    #[test]
    fn a_record_that_does_not_end_the_file_is_refused() {
        let mut bytes = eocd(2, 0, b"");
        bytes.extend_from_slice(b"trailing junk");
        assert!(declared(bytes).is_err());
        // Declares a longer comment than the file has.
        assert!(declared(eocd(2, 10, b"short")).is_err());
    }

    #[test]
    fn a_signature_inside_stored_data_is_not_a_record() {
        let mut bytes = b"stored file holding ".to_vec();
        bytes.extend(eocd(40_000, 0, b""));
        bytes.extend_from_slice(b" more file data");
        bytes.extend(eocd(3, 0, b""));
        assert_eq!(declared(bytes).unwrap(), Some(3));
    }
}
