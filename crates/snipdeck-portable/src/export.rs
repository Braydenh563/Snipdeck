//! Writing a `.snipdeck` archive (SPEC §5.9). The format is specified in `docs/format.md`.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Seek, Write};
use std::path::{Path, PathBuf};

use snipdeck_core::{BlobRef, Content, Format, FormatKind, ItemId, TagId, Timestamp};
use snipdeck_store::crypto::{DataKey, Slot, encrypt_payload};

use crate::archive::{ASSET_DIR, ArchiveWriter, ITEMS, MANIFEST};
use crate::dto::{
    BoardDto, Counts, ENCRYPTION, FORMAT_NAME, FileDto, FormatDto, GroupDto, ItemDto, ItemsDoc,
    ListDto, MAJOR, MINOR, Manifest, TagDto,
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
    /// Write every tag, not only the ones an exported item uses: what a backup of the whole
    /// library does, so a restore brings the tag list back as it was.
    pub every_tag: bool,
}

/// What an export wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    /// Items left out because an attachment could not be read or was over a limit. The
    /// archive holds everything else (a backup degrades instead of stopping).
    pub skipped: Vec<SkippedItem>,
}

/// An item an export could not write, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedItem {
    /// The item's title.
    pub title: String,
    /// The list it is in.
    pub list: String,
    /// What went wrong, for a log or the details view.
    pub reason: String,
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
        sensitive_bytes: 0,
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

/// A payload read and ready to be written as an asset.
struct Staged {
    hash: BlobRef,
    bytes: Vec<u8>,
    compress: bool,
}

/// Whether an error is the item's own (an unreadable attachment, an item over a size limit), so
/// the item is left out and the rest is written. Everything else stops the export.
fn left_out(error: &Error) -> bool {
    matches!(error, Error::Store(_) | Error::ItemTooLarge { .. })
}

struct Exporter<'a, L, W: Write + Seek> {
    lib: &'a L,
    options: &'a ExportOptions<'a>,
    writer: ArchiveWriter<W>,
    written: HashSet<BlobRef>,
    /// Bytes of sensitive content written so far (against `Limits::max_sensitive_bytes`).
    sensitive_bytes: u64,
    report: ExportReport,
}

/// An entry over the entry limit becomes an error that names the item that holds it. The total
/// is not the fault of the item that happened to cross it, so that error stays as it is.
fn name_the_item(error: Error, title: &str) -> Error {
    match error {
        Error::LimitExceeded {
            what: what @ "uncompressed entry size",
            limit,
        } => Error::ItemTooLarge {
            title: title.to_owned(),
            what,
            limit,
        },
        other => other,
    }
}

impl<L: Library, W: Write + Seek> Exporter<'_, L, W> {
    /// One item as it is written, or `None` when it is left out (sensitive and not kept, or
    /// unreadable and named in the report).
    fn item_dto(
        &mut self,
        item: &snipdeck_core::Item,
        list_name: &str,
        triggers: &HashMap<ItemId, snipdeck_core::Trigger>,
        group_of: &HashMap<ItemId, snipdeck_core::GroupId>,
        written_groups: &mut HashSet<snipdeck_core::GroupId>,
        used_tags: &mut BTreeSet<TagId>,
    ) -> Result<Option<ItemDto>> {
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
                return Ok(None);
            }
            (true, SensitivePolicy::Encrypt(key)) => Some(key),
        };
        let format_dtos = match self.item_formats(item.id, &item.title, key) {
            Ok(dtos) => dtos,
            Err(error) if left_out(&error) => {
                self.report.skipped.push(SkippedItem {
                    title: item.title.clone(),
                    list: list_name.to_owned(),
                    reason: error.to_string(),
                });
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        if key.is_some() {
            self.report.sensitive_encrypted += 1;
        }
        used_tags.extend(item.tags.iter().copied());
        let dto = ItemDto {
            id: item.id.to_string(),
            title: item.title.clone(),
            kind: item.kind.key().to_owned(),
            pinned: item.pinned,
            sensitive: item.sensitive,
            always_plain: item.always_plain,
            label: item.label.map(|l| l.key().to_owned()),
            trigger: triggers.get(&item.id).map(ToString::to_string),
            group: group_of.get(&item.id).map(|g| {
                written_groups.insert(*g);
                g.to_string()
            }),
            use_count: item.use_count,
            last_used_at: item.last_used_at.map(|t| t.to_string()),
            source_app: item.source_app.clone(),
            created_at: item.created_at.to_string(),
            updated_at: item.updated_at.to_string(),
            tags: item.tags.iter().map(ToString::to_string).collect(),
            preview: (!item.sensitive).then(|| item.preview.clone()),
            formats: format_dtos,
        };
        self.report.items += 1;
        Ok(Some(dto))
    }

    fn build_doc(&mut self) -> Result<ItemsDoc> {
        let tag_names: HashMap<TagId, String> = self
            .lib
            .tags()?
            .into_iter()
            .map(|t| (t.id, t.name.to_string()))
            .collect();
        let mut used_tags: BTreeSet<TagId> = BTreeSet::new();
        let mut boards = Vec::new();
        let triggers = trigger_map(self.lib)?;

        for board in self.lib.boards()? {
            let mut lists = Vec::new();
            for list in self.lib.lists(board.id)? {
                let mut items = Vec::new();
                let runs = self.lib.groups_of(list.id)?;
                let group_of = group_map(&runs);
                let mut written_groups = std::collections::HashSet::new();
                for item in self.lib.items(list.id)? {
                    if let Some(dto) = self.item_dto(
                        &item,
                        list.name.as_str(),
                        &triggers,
                        &group_of,
                        &mut written_groups,
                        &mut used_tags,
                    )? {
                        items.push(dto);
                    }
                }
                lists.push(ListDto {
                    id: list.id.to_string(),
                    name: list.name.to_string(),
                    accent: list.accent.map(|a| a.key().to_owned()),
                    icon: list.icon.map(|i| i.key().to_owned()),
                    groups: group_dtos(&runs, &written_groups),
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

        if self.options.every_tag {
            used_tags.extend(tag_names.keys().copied());
        }
        // Tags in name order: those some written item uses (every tag for a backup).
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

    /// Every format of one item, with an archive limit error naming the item. Every payload is
    /// read before any is written, so an item that cannot be read leaves nothing behind in the
    /// archive.
    fn item_formats(
        &mut self,
        item: ItemId,
        title: &str,
        key: Option<&DataKey>,
    ) -> Result<Vec<FormatDto>> {
        let formats = self.lib.formats(item)?;
        let mut staged = Vec::new();
        let mut dtos = Vec::with_capacity(formats.iter().count());
        for format in &formats {
            let dto = self
                .format_dto(item, format, key, &mut staged)
                .map_err(|e| name_the_item(e, title))?;
            dtos.push(dto);
        }
        // The reader decrypts at most `max_sensitive_bytes` of one archive: an item past it would
        // make the whole backup unreadable, so it is left out and named instead.
        if key.is_some() {
            let size: u64 = staged.iter().map(|p| p.bytes.len() as u64).sum();
            let limit = self.options.limits.max_sensitive_bytes;
            if self.sensitive_bytes.saturating_add(size) > limit {
                return Err(Error::ItemTooLarge {
                    title: title.to_owned(),
                    what: "decrypted size of sensitive content",
                    limit,
                });
            }
            self.sensitive_bytes += size;
        }
        for payload in staged {
            if self.written.insert(payload.hash) {
                self.writer
                    .add(
                        &format!("{ASSET_DIR}{}", payload.hash),
                        &payload.bytes,
                        payload.compress,
                    )
                    .map_err(|e| name_the_item(e, title))?;
            }
        }
        Ok(dtos)
    }

    fn format_dto(
        &mut self,
        item: ItemId,
        format: &Format,
        key: Option<&DataKey>,
        staged: &mut Vec<Staged>,
    ) -> Result<FormatDto> {
        let kind = format.kind();
        match format {
            Format::Text(c) | Format::Html(c) | Format::Rtf(c) | Format::Png(c) => {
                let (asset, size, encrypted) =
                    self.payload(item, kind, Slot::Format(kind), c, key, staged)?;
                Ok(FormatDto {
                    kind: kind.key().to_owned(),
                    asset: Some(asset),
                    size: Some(size),
                    encrypted,
                    files: None,
                })
            }
            Format::Files(files) => {
                let mut out = Vec::with_capacity(files.len());
                for (index, file) in files.iter().enumerate() {
                    let slot = Slot::File(u32::try_from(index).unwrap_or(u32::MAX));
                    let (asset, size, encrypted) =
                        self.payload(item, FormatKind::Files, slot, file.content(), key, staged)?;
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

    /// Reads one payload and stages it as an asset (written once per distinct content, when
    /// the whole item has been read); returns its hash name, stored size and whether it is
    /// encrypted.
    fn payload(
        &mut self,
        item: ItemId,
        kind: FormatKind,
        slot: Slot,
        content: &Content,
        key: Option<&DataKey>,
        staged: &mut Vec<Staged>,
    ) -> Result<(String, u64, bool)> {
        let plain = self.lib.read(content)?;
        let (bytes, encrypted) = match key {
            Some(key) => (encrypt_payload(key, &item, slot, &plain)?, true),
            None => (plain.into_owned(), false),
        };
        let hash = BlobRef::from_bytes(*blake3::hash(&bytes).as_bytes());
        let size = bytes.len() as u64;
        staged.push(Staged {
            hash,
            bytes,
            compress: kind != FormatKind::Png,
        });
        Ok((hash.to_string(), size, encrypted))
    }

    fn finish(mut self, now: Timestamp, doc: &ItemsDoc) -> Result<ExportReport> {
        let json = serde_json::to_vec(doc)
            .map_err(|e| Error::malformed(ITEMS, format!("could not serialise: {e}")))?;
        let cap = self.options.limits.export_json_cap();
        if json.len() as u64 > cap {
            return Err(Error::LimitExceeded {
                what: "items.json size",
                limit: cap,
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
            skipped_unreadable: self.report.skipped.len() as u64,
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

/// The groups of a list that have a member in the archive.
fn group_dtos(
    runs: &[snipdeck_store::GroupRun],
    written: &std::collections::HashSet<snipdeck_core::GroupId>,
) -> Vec<GroupDto> {
    runs.iter()
        .filter(|run| written.contains(&run.group.id))
        .map(|run| GroupDto {
            id: run.group.id.to_string(),
            name: run.group.name.to_string(),
            collapsed: run.group.collapsed,
        })
        .collect()
}

/// Each item's trigger, by item.
fn trigger_map<L: Library>(
    lib: &L,
) -> Result<std::collections::HashMap<snipdeck_core::ItemId, snipdeck_core::Trigger>> {
    Ok(lib
        .triggers()?
        .into_iter()
        .map(|(trigger, item)| (item, trigger))
        .collect())
}

/// The group of each grouped item.
fn group_map(
    runs: &[snipdeck_store::GroupRun],
) -> std::collections::HashMap<snipdeck_core::ItemId, snipdeck_core::GroupId> {
    runs.iter()
        .flat_map(|run| run.items.iter().map(|id| (*id, run.group.id)))
        .collect()
}
