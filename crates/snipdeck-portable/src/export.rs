//! Writing a `.snipdeck` archive (SPEC §5.9). The format is specified in `docs/format.md`.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Seek, Write};
use std::path::{Path, PathBuf};

use snipdeck_core::{BlobRef, Content, Format, FormatKind, ItemId, TagId, Timestamp};
use snipdeck_store::crypto::{DataKey, Slot, encrypt_payload};

use crate::archive::{ASSET_DIR, ArchiveWriter, ITEMS, MANIFEST};
use crate::dto::{
    BoardDto, Counts, ENCRYPTION, FORMAT_NAME, FileDto, FormatDto, ItemDto, ItemsDoc, ListDto,
    MAJOR, MINOR, Manifest, TagDto,
};
use crate::error::{Error, Result};
use crate::limits::Limits;
use crate::plan::Library;

/// What to do with sensitive items. There is deliberately no "write them in plain text" choice:
/// an archive is a file people e-mail and upload.
#[derive(Debug, Clone, Copy, Default)]
pub enum SensitivePolicy<'k> {
    /// Leave sensitive items out entirely (the default). The count is recorded in the manifest
    /// and shown as a warning on import.
    #[default]
    Skip,
    /// Include them with every payload encrypted under `key` as a `SNENC2` envelope (see
    /// `docs/crypto.md`). The key is the library's data key, so the archive can be imported
    /// only where that key is available (normally the same OS user); plan restores accordingly.
    Encrypt(&'k DataKey),
}

/// Options for an export.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExportOptions<'k> {
    /// How sensitive items are handled. Defaults to [`SensitivePolicy::Skip`].
    pub sensitive: SensitivePolicy<'k>,
    /// The limits the archive must stay within, so that it can be imported again.
    pub limits: Limits,
}

/// What an export wrote.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExportReport {
    /// Boards written.
    pub boards: usize,
    /// Lists written.
    pub lists: usize,
    /// Items written, sensitive ones included when encrypted.
    pub items: usize,
    /// Tags written (only those used by written items).
    pub tags: usize,
    /// Distinct asset files written.
    pub assets: usize,
    /// Sensitive items written encrypted.
    pub sensitive_encrypted: u64,
    /// Sensitive items left out.
    pub sensitive_omitted: u64,
    /// Total uncompressed size of the archive's entries.
    pub uncompressed_bytes: u64,
}

/// Writes the library as a `.snipdeck` zip to `out`.
///
/// Only live records are exported (the trash is not). Boards, lists and items are written in the
/// user's order. `now` is recorded as the creation time.
///
/// # Errors
///
/// Storage errors, I/O errors, [`Error::LimitExceeded`] if the library is too large for
/// `options.limits`, and [`Error::Crypto`] if encryption fails.
pub fn export_to_writer<L: Library, W: Write + Seek>(
    lib: &L,
    now: Timestamp,
    options: &ExportOptions<'_>,
    out: W,
) -> Result<ExportReport> {
    let mut exporter = Exporter {
        lib,
        options,
        writer: ArchiveWriter::new(out, options.limits),
        written: HashSet::new(),
        report: ExportReport::default(),
    };
    let doc = exporter.build_doc()?;
    exporter.finish(now, &doc)
}

/// Writes the library to `path`, atomically: the archive is written to `<path>.part` and renamed
/// over `path` only when complete, so a failure never leaves a half-written file and never
/// damages an existing one.
///
/// # Errors
///
/// As [`export_to_writer`], plus [`Error::File`] for the destination.
pub fn export_to_path<L: Library>(
    lib: &L,
    now: Timestamp,
    options: &ExportOptions<'_>,
    path: &Path,
) -> Result<ExportReport> {
    let part = part_path(path);
    let _ = fs::remove_file(&part);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&part)
        .map_err(|source| Error::File {
            path: part.clone(),
            source,
        })?;
    let result = (|| {
        let mut buffered = BufWriter::new(file);
        let report = export_to_writer(lib, now, options, &mut buffered)?;
        let file = buffered
            .into_inner()
            .map_err(std::io::IntoInnerError::into_error)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&part, path).map_err(|source| Error::File {
            path: path.to_owned(),
            source,
        })?;
        Ok(report)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}

fn part_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(".part");
    path.with_file_name(name)
}

struct Exporter<'a, L, W: Write + Seek> {
    lib: &'a L,
    options: &'a ExportOptions<'a>,
    writer: ArchiveWriter<W>,
    written: HashSet<BlobRef>,
    report: ExportReport,
}

impl<L: Library, W: Write + Seek> Exporter<'_, L, W> {
    fn build_doc(&mut self) -> Result<ItemsDoc> {
        let tag_names: HashMap<TagId, String> = self
            .lib
            .tags()?
            .into_iter()
            .map(|t| (t.id, t.name.to_string()))
            .collect();
        let mut used_tags: BTreeSet<TagId> = BTreeSet::new();
        let mut boards = Vec::new();

        for board in self.lib.boards()? {
            let mut lists = Vec::new();
            for list in self.lib.lists(board.id)? {
                let mut items = Vec::new();
                for item in self.lib.items(list.id)? {
                    if self.report.items >= self.options.limits.max_items {
                        return Err(Error::LimitExceeded {
                            what: "item count",
                            limit: self.options.limits.max_items as u64,
                        });
                    }
                    let key = match (item.sensitive, self.options.sensitive) {
                        (false, _) => None,
                        (true, SensitivePolicy::Skip) => {
                            self.report.sensitive_omitted += 1;
                            continue;
                        }
                        (true, SensitivePolicy::Encrypt(key)) => {
                            self.report.sensitive_encrypted += 1;
                            Some(key)
                        }
                    };
                    let formats = self.lib.formats(item.id)?;
                    let mut format_dtos = Vec::with_capacity(formats.iter().count());
                    for format in &formats {
                        format_dtos.push(self.format_dto(item.id, format, key)?);
                    }
                    used_tags.extend(item.tags.iter().copied());
                    items.push(ItemDto {
                        id: item.id.to_string(),
                        title: item.title.clone(),
                        kind: item.kind.key().to_owned(),
                        pinned: item.pinned,
                        sensitive: item.sensitive,
                        always_plain: item.always_plain,
                        label: item.label.map(|l| l.key().to_owned()),
                        use_count: item.use_count,
                        last_used_at: item.last_used_at.map(|t| t.to_string()),
                        source_app: item.source_app.clone(),
                        created_at: item.created_at.to_string(),
                        updated_at: item.updated_at.to_string(),
                        tags: item.tags.iter().map(ToString::to_string).collect(),
                        preview: (!item.sensitive).then(|| item.preview.clone()),
                        formats: format_dtos,
                    });
                    self.report.items += 1;
                }
                lists.push(ListDto {
                    id: list.id.to_string(),
                    name: list.name.to_string(),
                    accent: list.accent.map(|a| a.key().to_owned()),
                    icon: list.icon.map(|i| i.key().to_owned()),
                    view: list.view.key().to_owned(),
                    sort: list.sort.key().to_owned(),
                    created_at: list.created_at.to_string(),
                    updated_at: list.updated_at.to_string(),
                    items,
                });
                self.report.lists += 1;
            }
            boards.push(BoardDto {
                id: board.id.to_string(),
                name: board.name.to_string(),
                created_at: board.created_at.to_string(),
                updated_at: board.updated_at.to_string(),
                lists,
            });
            self.report.boards += 1;
        }

        // Tags in name order, only those some written item uses.
        let mut tags: Vec<TagDto> = used_tags
            .iter()
            .filter_map(|id| {
                tag_names.get(id).map(|name| TagDto {
                    id: id.to_string(),
                    name: name.clone(),
                })
            })
            .collect();
        tags.sort_by_key(|t| t.name.to_lowercase());
        self.report.tags = tags.len();
        Ok(ItemsDoc { tags, boards })
    }

    fn format_dto(
        &mut self,
        item: ItemId,
        format: &Format,
        key: Option<&DataKey>,
    ) -> Result<FormatDto> {
        let kind = format.kind();
        let single = |content: &Content, this: &mut Self| -> Result<FormatDto> {
            let (asset, size, encrypted) =
                this.payload(item, kind, Slot::Format(kind), content, key)?;
            Ok(FormatDto {
                kind: kind.key().to_owned(),
                asset: Some(asset),
                size: Some(size),
                encrypted,
                files: None,
            })
        };
        match format {
            Format::Text(c) | Format::Html(c) | Format::Rtf(c) | Format::Png(c) => single(c, self),
            Format::Files(files) => {
                let mut out = Vec::with_capacity(files.len());
                for (index, file) in files.iter().enumerate() {
                    let slot = Slot::File(u32::try_from(index).unwrap_or(u32::MAX));
                    let (asset, size, encrypted) =
                        self.payload(item, FormatKind::Files, slot, file.content(), key)?;
                    out.push(FileDto {
                        name: file.name().to_owned(),
                        asset,
                        size,
                        encrypted,
                    });
                }
                Ok(FormatDto {
                    kind: kind.key().to_owned(),
                    asset: None,
                    size: None,
                    encrypted: false,
                    files: Some(out),
                })
            }
        }
    }

    /// Writes one payload as an asset (once per distinct content) and returns its hash name,
    /// stored size and whether it is encrypted.
    fn payload(
        &mut self,
        item: ItemId,
        kind: FormatKind,
        slot: Slot,
        content: &Content,
        key: Option<&DataKey>,
    ) -> Result<(String, u64, bool)> {
        let plain = self.lib.read(content)?;
        let (bytes, encrypted) = match key {
            Some(key) => (
                std::borrow::Cow::Owned(encrypt_payload(key, &item, slot, &plain)?),
                true,
            ),
            None => (plain, false),
        };
        let hash = BlobRef::from_bytes(*blake3::hash(&bytes).as_bytes());
        if self.written.insert(hash) {
            self.writer.add(
                &format!("{ASSET_DIR}{hash}"),
                &bytes,
                kind != FormatKind::Png,
            )?;
        }
        Ok((hash.to_string(), bytes.len() as u64, encrypted))
    }

    fn finish(mut self, now: Timestamp, doc: &ItemsDoc) -> Result<ExportReport> {
        let json = serde_json::to_vec(doc)
            .map_err(|e| Error::malformed(ITEMS, format!("could not serialise: {e}")))?;
        if json.len() as u64 > self.options.limits.max_json_bytes {
            return Err(Error::LimitExceeded {
                what: "items.json size",
                limit: self.options.limits.max_json_bytes,
            });
        }
        self.writer.add(ITEMS, &json, true)?;

        let manifest = Manifest {
            format: FORMAT_NAME.to_owned(),
            format_version: format!("{MAJOR}.{MINOR}"),
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            created_at: now.to_string(),
            counts: Counts {
                boards: self.report.boards as u64,
                lists: self.report.lists as u64,
                items: self.report.items as u64,
                tags: self.report.tags as u64,
                assets: self.written.len() as u64,
            },
            encryption: (self.report.sensitive_encrypted > 0).then(|| ENCRYPTION.to_owned()),
            sensitive_omitted: self.report.sensitive_omitted,
        };
        let manifest_json = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| Error::malformed(MANIFEST, format!("could not serialise: {e}")))?;
        self.writer.add(MANIFEST, &manifest_json, true)?;

        self.report.assets = self.written.len();
        self.report.uncompressed_bytes = self.writer.total_bytes();
        let report = self.report;
        let mut out = self.writer.finish()?;
        out.flush()?;
        Ok(report)
    }
}
