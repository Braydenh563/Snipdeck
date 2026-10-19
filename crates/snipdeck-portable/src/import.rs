//! Reading a `.snipdeck` archive (SPEC §5.9) and the options shared by every importer.
//!
//! The archive is untrusted. Importing is split so that nothing touches the library until
//! everything has been checked:
//!
//! 1. open the zip and check every entry against [`Limits`] (`archive`);
//! 2. read the manifest and decide whether the version is readable at all;
//! 3. parse `items.json` and validate every id, name, enum key, timestamp, count and reference
//!    into a [`Plan`];
//! 4. verify every asset: size, BLAKE3 hash against its name, UTF-8 and size limits for text,
//!    and decryption of encrypted payloads;
//! 5. decide duplicates, then write the plan (`plan::apply`) in one store transaction.
//!
//! Steps 1 to 4 are read-only, so an archive that fails them changes nothing.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;
use std::str::FromStr;

use snipdeck_core::Content;
use snipdeck_core::{
    BlobRef, BoardId, FileEntry, Format, FormatKind, Formats, Inline, ItemId, ItemKind, ListAccent,
    ListId, ListSort, ListView, Name, TagId, Timestamp,
};
use snipdeck_store::crypto::{DataKey, Slot, decrypt_payload};
use snipdeck_store::name_key;

use crate::archive::{ITEMS, MANIFEST, SafeArchive};
use crate::dto::{
    ENCRYPTION, FORMAT_NAME, FormatDto, Header, ItemDto, ItemsDoc, ListDto, MAJOR, MINOR, Manifest,
    TagDto,
};
use crate::error::{Error, Result};
use crate::importers::MEMORY_BUDGET;
use crate::limits::{Limits, TEXT_LIMIT};
use crate::plan::{
    AssetRef, ContentHasher, Library, Plan, PlanBoard, PlanFile, PlanFormat, PlanItem, PlanList,
    Source, apply, existing_hashes,
};
use crate::report::{ImportReport, Warning};

/// What to do with items that are already in the library.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MergeStrategy {
    /// Import everything as new items with new ids (the default). Importing the same file twice
    /// gives two copies.
    #[default]
    ImportAsNew,
    /// Skip an item whose content (every format, byte for byte; not its title or flags) equals
    /// an item already in the library or earlier in the same import. If nothing is left the
    /// board is not created.
    SkipDuplicates,
}

/// How a `.snipdeck` archive's boards and lists are laid out on import.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BoardLayout {
    /// Everything goes under one new board named after the file (the default). The archive's
    /// lists become that board's lists; when the archive has several boards, each list is named
    /// `Board / List`.
    #[default]
    SingleBoard,
    /// Recreate each of the archive's boards under its own name, with new ids. This is the
    /// layout for restoring a backup.
    KeepBoards,
}

/// Where an import gets the data key for encrypted payloads.
#[derive(Debug, Clone, Copy, Default)]
pub enum KeySource<'k> {
    /// No key: encrypted items are skipped with a warning.
    #[default]
    None,
    /// The library's own unlocked key ([`Library::data_key`]). This is how a restore into the
    /// library an archive came from works: a key borrowed from the library cannot be passed
    /// in while the library is also borrowed mutably for writing.
    Library,
    /// A key the caller holds.
    Given(&'k DataKey),
}

/// Options shared by every importer. Fields an importer has no use for are ignored.
#[derive(Debug, Clone, Default)]
pub struct ImportOptions<'k> {
    /// Duplicate handling. Applies to every importer.
    pub merge: MergeStrategy,
    /// Archive layout. Applies to `.snipdeck` imports only.
    pub layout: BoardLayout,
    /// The data key for decrypting sensitive items in a `.snipdeck`. Without one, encrypted items
    /// are skipped with a warning.
    pub key: KeySource<'k>,
    /// The name of the new board, instead of one derived from the file or folder name.
    pub board_name: Option<String>,
    /// When set, a warning is reported if the import leaves more boards than this (the Free
    /// tier's limit). Nothing is blocked: the caller decides. Use [`ImportOptions::max_boards`]
    /// to enforce a limit.
    pub free_board_limit: Option<usize>,
    /// The most live boards the library may hold once the import is done (the Free tier's 3).
    /// An import that would go past it is refused inside its transaction with
    /// `ProRequired(UnlimitedBoards)` and changes nothing; the caller passes `None` on Pro. A
    /// restore of boards the user already has cannot grow past the limit this way.
    pub max_boards: Option<usize>,
    /// Limits for reading untrusted input.
    pub limits: Limits,
    /// Run the whole import (validation, duplicate skipping, the board limit) and report what it
    /// would add, then roll the transaction back so the library is unchanged. This is the
    /// preview the Settings import and Restore show before anything is written.
    pub dry_run: bool,
}

/// A board name from a file or folder name: trimmed, control characters replaced, shortened to
/// the 100-character name limit, with `fallback` when nothing is left.
pub(crate) fn board_name(raw: &str, fallback: &str) -> Result<Name> {
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(Name::MAX_CHARS)
        .collect();
    Ok(Name::new(&cleaned).or_else(|_| Name::new(fallback))?)
}

/// The common ending of every importer: skip duplicates, warn about the board limit, write the
/// plan. `plan` must hold only in-memory payloads.
pub(crate) fn finish<L: Library>(
    lib: &mut L,
    mut plan: Plan,
    options: &ImportOptions<'_>,
    mut report: ImportReport,
) -> Result<ImportReport> {
    if options.merge == MergeStrategy::SkipDuplicates {
        let mut known = existing_hashes(lib)?;
        report.items_skipped += plan.drop_duplicates(&mut known);
    }
    warn_board_limit(lib, &plan, options, &mut report)?;
    let mut no_assets = |_: &AssetRef| -> Result<Vec<u8>> {
        Err(Error::malformed("import", "internal: no assets expected"))
    };
    apply(
        lib,
        &plan,
        &mut no_assets,
        report,
        options.max_boards,
        options.dry_run,
    )
}

fn warn_board_limit<L: Library>(
    lib: &L,
    plan: &Plan,
    options: &ImportOptions<'_>,
    report: &mut ImportReport,
) -> Result<()> {
    if let Some(limit) = options.free_board_limit
        && !plan.boards.is_empty()
    {
        let total = lib.boards()?.len() + plan.boards.len();
        if total > limit {
            report.warn(Warning::BoardLimit { limit, total });
        }
    }
    Ok(())
}

/// Imports the `.snipdeck` file at `path`. The default new board is named after the file.
///
/// # Errors
///
/// See [`import_archive`].
pub fn import_archive_path<L: Library>(
    lib: &mut L,
    path: &Path,
    now: Timestamp,
    options: &ImportOptions<'_>,
) -> Result<ImportReport> {
    let file = File::open(path).map_err(|source| Error::File {
        path: path.to_owned(),
        source,
    })?;
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    import_archive(lib, BufReader::new(file), &stem, now, options)
}

/// Imports a `.snipdeck` archive. `source_name` (normally the file name without extension) names
/// the new board unless the options say otherwise.
///
/// Validation covers the zip (limits, names, links), the manifest (format, version, counts),
/// `items.json` (ids, names, keys, timestamps, references, duplicates) and every asset (size and
/// hash). Only then is the library written, in one store transaction (see `plan`).
///
/// # Errors
///
/// An [`Error`] naming the first problem; the library is unchanged. Warnings (sensitive items
/// that need a key, unknown entries, the Free-tier board limit) go in the report instead.
pub fn import_archive<L: Library, R: Read + Seek>(
    lib: &mut L,
    reader: R,
    source_name: &str,
    now: Timestamp,
    options: &ImportOptions<'_>,
) -> Result<ImportReport> {
    let mut archive = SafeArchive::open(reader, options.limits)?;
    let mut report = ImportReport::default();

    let manifest = read_manifest(&archive.read_manifest()?, &mut report)?;
    let doc: ItemsDoc = serde_json::from_slice(&archive.read_items()?)
        .map_err(|e| Error::malformed(ITEMS, e.to_string()))?;

    let name = board_name(
        options.board_name.as_deref().unwrap_or(source_name),
        "Imported",
    )?;
    let built = build_plan(&doc, &manifest, &archive, options, name, now)?;
    let mut plan = built.plan;
    if archive.unknown_entries() > 0 {
        report.warn(Warning::UnknownEntries {
            count: archive.unknown_entries(),
        });
    }
    if built.unreferenced > 0 {
        report.warn(Warning::UnreferencedAssets {
            count: built.unreferenced,
        });
    }
    if manifest.sensitive_omitted > 0 {
        report.warn(Warning::SensitiveOmittedByExport {
            count: manifest.sensitive_omitted,
        });
    }

    // Decrypted payloads are moved into the plan here, so the key (which may be borrowed from
    // the library) is no longer needed when the library is borrowed mutably for writing.
    let key = match options.key {
        KeySource::None => None,
        KeySource::Given(key) => Some(key),
        KeySource::Library => lib.data_key(),
    };
    verify(&mut archive, &mut plan, key, &mut report)?;

    if options.merge == MergeStrategy::SkipDuplicates {
        let mut known = existing_hashes(lib)?;
        report.items_skipped += plan.drop_duplicates(&mut known);
    }
    warn_board_limit(lib, &plan, options, &mut report)?;

    let mut fetch = |asset: &AssetRef| -> Result<Vec<u8>> {
        // Encrypted payloads were decrypted into the plan; only plain assets are fetched.
        let bytes = archive.read_asset(&asset.hash)?;
        // The file was verified before writing began; check again in case it changed
        // underneath us since.
        if blake3::hash(&bytes).as_bytes() != asset.hash.as_bytes() {
            return Err(Error::AssetMismatch {
                asset: asset.hash.to_string(),
                reason: "it changed while the import was running".to_owned(),
            });
        }
        Ok(bytes)
    };
    apply(
        lib,
        &plan,
        &mut fetch,
        report,
        options.max_boards,
        options.dry_run,
    )
}

/// Reads `manifest.json`: checks it is a Snipdeck manifest of a readable version before looking
/// at anything else in it.
fn read_manifest(bytes: &[u8], report: &mut ImportReport) -> Result<Manifest> {
    let header: Header =
        serde_json::from_slice(bytes).map_err(|e| Error::malformed(MANIFEST, e.to_string()))?;
    if header.format.as_deref() != Some(FORMAT_NAME) {
        return Err(Error::UnsupportedFormat(
            "`format` is not `snipdeck-archive`".to_owned(),
        ));
    }
    let version = header
        .format_version
        .ok_or_else(|| Error::UnsupportedFormat("`format_version` is missing".to_owned()))?;
    let (major, minor) = version
        .split_once('.')
        .and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)))
        .ok_or_else(|| Error::UnsupportedFormat(format!("`{version}` is not `major.minor`")))?;
    if major == 0 {
        return Err(Error::UnsupportedFormat(
            "major version 0 does not exist".to_owned(),
        ));
    }
    if major > MAJOR {
        return Err(Error::FormatTooNew {
            found: version,
            supported_major: MAJOR,
        });
    }
    if minor > MINOR {
        report.warn(Warning::NewerMinorVersion { found: version });
    }
    serde_json::from_slice(bytes).map_err(|e| Error::malformed(MANIFEST, e.to_string()))
}

struct Built {
    plan: Plan,
    /// Assets in the archive that no item refers to.
    unreferenced: usize,
}

/// An id in the canonical 36-character hyphenated form (`docs/format.md`); the parsers behind
/// `FromStr` also take braces, `urn:uuid:` and unhyphenated forms, which are not the format.
fn parse_id<T: FromStr<Err = snipdeck_core::Error>>(text: &str, what: &str) -> Result<T> {
    let parsed = if text.len() == 36 {
        text.parse().ok()
    } else {
        None
    };
    parsed.ok_or_else(|| {
        Error::malformed(
            ITEMS,
            format!(
                "`{}` is not a valid {what} id",
                crate::error::display_name(text)
            ),
        )
    })
}

fn parse_time(text: &str, what: &str) -> Result<Timestamp> {
    let at: Timestamp = text.parse().map_err(|_| {
        Error::malformed(
            ITEMS,
            format!(
                "`{}` is not a valid {what} time",
                crate::error::display_name(text)
            ),
        )
    })?;
    // Storage holds nanoseconds in an i64: the years 1677 to 2262.
    i64::try_from(at.as_nanosecond()).map_err(|_| {
        Error::malformed(ITEMS, format!("{what} time {at} is outside 1677 to 2262"))
    })?;
    Ok(at)
}

fn parse_name(text: &str, what: &str) -> Result<Name> {
    Name::new(text).map_err(|e| Error::malformed(ITEMS, format!("{what} name: {e}")))
}

fn parse_key<T: FromStr<Err = snipdeck_core::Error>>(text: &str, what: &str) -> Result<T> {
    text.parse().map_err(|_| {
        Error::malformed(
            ITEMS,
            format!(
                "`{}` is not a known {what}",
                crate::error::display_name(text)
            ),
        )
    })
}

fn unique<T: Eq + std::hash::Hash + std::fmt::Display + Copy>(
    seen: &mut HashSet<T>,
    id: T,
    kind: &'static str,
) -> Result<()> {
    if seen.insert(id) {
        Ok(())
    } else {
        Err(Error::DuplicateId {
            kind,
            id: id.to_string(),
        })
    }
}

/// Validates the whole document against the manifest and the archive's entries and turns it
/// into a plan. Nothing is read from assets yet.
fn build_plan<R: Read + Seek>(
    doc: &ItemsDoc,
    manifest: &Manifest,
    archive: &SafeArchive<R>,
    options: &ImportOptions<'_>,
    single_board: Name,
    now: Timestamp,
) -> Result<Built> {
    check_counts(
        doc,
        manifest,
        archive.asset_count(),
        options.limits.max_items,
    )?;

    let mut builder = Builder {
        archive,
        encrypted_declared: manifest.encryption.as_deref() == Some(ENCRYPTION),
        referenced: HashSet::new(),
        tag_index: HashMap::new(),
        seen_lists: HashSet::new(),
        seen_items: HashSet::new(),
    };
    let tags = builder.read_tags(&doc.tags)?;
    let mut plan = Plan {
        tags,
        boards: Vec::new(),
    };

    let keep_boards = options.layout == BoardLayout::KeepBoards;
    let multi = doc.boards.len() > 1;
    if !keep_boards {
        plan.boards.push(PlanBoard {
            name: single_board,
            created_at: now,
            updated_at: now,
            lists: Vec::new(),
        });
    }
    let mut seen_boards = HashSet::new();
    for board in &doc.boards {
        let board_id: BoardId = parse_id(&board.id, "board")?;
        unique(&mut seen_boards, board_id, "board")?;
        let name = parse_name(&board.name, "board")?;
        let created_at = parse_time(&board.created_at, "board creation")?;
        let updated_at = parse_time(&board.updated_at, "board update")?;

        let prefix = (!keep_boards && multi).then_some(&name);
        let mut lists = Vec::with_capacity(board.lists.len());
        for list in &board.lists {
            lists.push(builder.list(list, prefix)?);
        }
        if keep_boards {
            plan.boards.push(PlanBoard {
                name,
                created_at,
                updated_at,
                lists,
            });
        } else if let Some(target) = plan.boards.first_mut() {
            target.lists.extend(lists);
        }
    }

    let unreferenced = archive
        .asset_hashes()
        .filter(|h| !builder.referenced.contains(h))
        .count();
    Ok(Built { plan, unreferenced })
}

/// The manifest's counts must equal what the document and the zip really hold.
fn check_counts(
    doc: &ItemsDoc,
    manifest: &Manifest,
    assets: usize,
    max_items: usize,
) -> Result<()> {
    let lists: usize = doc.boards.iter().map(|b| b.lists.len()).sum();
    let items: usize = doc
        .boards
        .iter()
        .flat_map(|b| &b.lists)
        .map(|l| l.items.len())
        .sum();
    let counts = [
        ("boards", manifest.counts.boards, doc.boards.len()),
        ("lists", manifest.counts.lists, lists),
        ("items", manifest.counts.items, items),
        ("tags", manifest.counts.tags, doc.tags.len()),
        ("assets", manifest.counts.assets, assets),
    ];
    for (what, declared, actual) in counts {
        if declared != actual as u64 {
            return Err(Error::CountMismatch {
                what,
                declared,
                actual: actual as u64,
            });
        }
    }
    if items > max_items {
        return Err(Error::LimitExceeded {
            what: "item count",
            limit: max_items as u64,
        });
    }
    Ok(())
}

/// Validation state while turning the document into a plan.
struct Builder<'a, R: Read + Seek> {
    archive: &'a SafeArchive<R>,
    encrypted_declared: bool,
    /// Assets some item refers to.
    referenced: HashSet<BlobRef>,
    tag_index: HashMap<TagId, usize>,
    seen_lists: HashSet<ListId>,
    seen_items: HashSet<ItemId>,
}

impl<R: Read + Seek> Builder<'_, R> {
    /// Tags: ids unique, names valid. Tags whose names are equal ignoring case merge into one.
    fn read_tags(&mut self, tags: &[TagDto]) -> Result<Vec<Name>> {
        let mut seen = HashSet::new();
        let mut names: Vec<Name> = Vec::new();
        let mut by_key: HashMap<String, usize> = HashMap::new();
        for tag in tags {
            let id: TagId = parse_id(&tag.id, "tag")?;
            unique(&mut seen, id, "tag")?;
            let name = parse_name(&tag.name, "tag")?;
            let index = *by_key.entry(name_key(name.as_str())).or_insert_with(|| {
                names.push(name.clone());
                names.len() - 1
            });
            self.tag_index.insert(id, index);
        }
        Ok(names)
    }

    /// One list. `prefix` is the archive board's name when boards are being merged into one.
    fn list(&mut self, dto: &ListDto, prefix: Option<&Name>) -> Result<PlanList> {
        let id: ListId = parse_id(&dto.id, "list")?;
        unique(&mut self.seen_lists, id, "list")?;
        let mut name = parse_name(&dto.name, "list")?;
        if let Some(board) = prefix {
            name = board_name(&format!("{board} / {name}"), "List")?;
        }
        let mut list = PlanList::new(name, parse_time(&dto.created_at, "list creation")?);
        list.updated_at = parse_time(&dto.updated_at, "list update")?;
        list.accent = dto
            .accent
            .as_deref()
            .map(|a| parse_key::<ListAccent>(a, "list accent"))
            .transpose()?;
        list.view = parse_key::<ListView>(&dto.view, "list view")?;
        list.sort = parse_key::<ListSort>(&dto.sort, "list sort")?;
        for item in &dto.items {
            let item_id: ItemId = parse_id(&item.id, "item")?;
            unique(&mut self.seen_items, item_id, "item")?;
            list.items.push(self.item(item, item_id)?);
        }
        Ok(list)
    }

    fn item(&mut self, dto: &ItemDto, id: ItemId) -> Result<PlanItem> {
        let mut tags = BTreeSet::new();
        for tag in &dto.tags {
            let tag_id: TagId = parse_id(tag, "tag")?;
            let index = self.tag_index.get(&tag_id).ok_or_else(|| {
                Error::malformed(
                    ITEMS,
                    format!("item {id} uses tag {tag_id}, which is not in `tags`"),
                )
            })?;
            tags.insert(*index);
        }
        if dto.formats.is_empty() {
            return Err(Error::malformed(ITEMS, format!("item {id} has no formats")));
        }
        let mut kinds = HashSet::new();
        let mut formats = Vec::with_capacity(dto.formats.len());
        for format in &dto.formats {
            let kind: FormatKind = parse_key(&format.kind, "format kind")?;
            if !kinds.insert(kind) {
                return Err(Error::malformed(
                    ITEMS,
                    format!("item {id} repeats the `{kind}` format"),
                ));
            }
            formats.push(self.format(format, kind, id)?);
        }
        check_shape(&formats)?;
        Ok(PlanItem {
            title: dto.title.clone(),
            kind: parse_key::<ItemKind>(&dto.kind, "item kind")?,
            pinned: dto.pinned,
            sensitive: dto.sensitive,
            always_plain: dto.always_plain,
            use_count: dto.use_count,
            last_used_at: dto
                .last_used_at
                .as_deref()
                .map(|t| parse_time(t, "last use"))
                .transpose()?,
            source_app: dto.source_app.clone(),
            created_at: parse_time(&dto.created_at, "item creation")?,
            updated_at: parse_time(&dto.updated_at, "item update")?,
            tags,
            formats,
            preview: dto.preview.clone(),
            content_hash: [0; 32],
        })
    }

    fn format(&mut self, dto: &FormatDto, kind: FormatKind, item: ItemId) -> Result<PlanFormat> {
        let text = matches!(kind, FormatKind::Text | FormatKind::Html | FormatKind::Rtf);
        if kind == FormatKind::Files {
            let files = dto
                .files
                .as_deref()
                .filter(|f| !f.is_empty())
                .ok_or_else(|| {
                    Error::malformed(ITEMS, format!("item {item}: a `files` format needs files"))
                })?;
            if files.len() > Limits::MAX_FILES_PER_FORMAT {
                return Err(Error::LimitExceeded {
                    what: "files per item",
                    limit: Limits::MAX_FILES_PER_FORMAT as u64,
                });
            }
            let mut out = Vec::with_capacity(files.len());
            for (index, file) in files.iter().enumerate() {
                // Validates the name (one Windows-valid path component) as the store would.
                let placeholder = Content::Inline(Inline::new(Vec::new())?);
                FileEntry::new(&file.name, placeholder)?;
                let payload = Payload {
                    asset: &file.asset,
                    size: file.size,
                    encrypted: file.encrypted,
                    slot: Slot::File(u32::try_from(index).unwrap_or(u32::MAX)),
                    text: false,
                };
                out.push(PlanFile {
                    name: file.name.clone(),
                    source: Source::Asset(self.asset_ref(&payload, item)?),
                });
            }
            return Ok(PlanFormat::Files(out));
        }
        let (Some(asset), Some(size)) = (dto.asset.as_deref(), dto.size) else {
            return Err(Error::malformed(
                ITEMS,
                format!("item {item}: the `{kind}` format needs `asset` and `size`"),
            ));
        };
        let payload = Payload {
            asset,
            size,
            encrypted: dto.encrypted,
            slot: Slot::Format(kind),
            text,
        };
        Ok(PlanFormat::Single {
            kind,
            source: Source::Asset(self.asset_ref(&payload, item)?),
        })
    }

    /// Checks one payload reference against the archive and records it as used.
    fn asset_ref(&mut self, payload: &Payload<'_>, item: ItemId) -> Result<AssetRef> {
        let hash: BlobRef = payload.asset.parse().map_err(|_| {
            Error::malformed(
                ITEMS,
                format!(
                    "`{}` is not a lowercase 64-digit hash",
                    crate::error::display_name(payload.asset)
                ),
            )
        })?;
        match self.archive.asset_size(&hash) {
            None => {
                return Err(Error::AssetMismatch {
                    asset: hash.to_string(),
                    reason: "it is not in the archive".to_owned(),
                });
            }
            Some(actual) if actual != payload.size => {
                return Err(Error::AssetMismatch {
                    asset: hash.to_string(),
                    reason: format!(
                        "the item says {} bytes, the archive holds {actual}",
                        payload.size
                    ),
                });
            }
            Some(_) => {}
        }
        if payload.encrypted && !self.encrypted_declared {
            return Err(Error::malformed(
                MANIFEST,
                "an item has an encrypted payload but `encryption` is not `SNENC2`",
            ));
        }
        if payload.text && !payload.encrypted && payload.size > TEXT_LIMIT {
            return Err(Error::LimitExceeded {
                what: "text size",
                limit: TEXT_LIMIT,
            });
        }
        self.referenced.insert(hash);
        Ok(AssetRef {
            hash,
            encrypted_for: payload.encrypted.then_some((item, payload.slot)),
        })
    }
}

/// Runs core's own checks on the shape of an item's formats (no repeated kind, no repeated file
/// name) with empty stand-in payloads, so that nothing core would refuse reaches the write.
fn check_shape(formats: &[PlanFormat]) -> Result<()> {
    let stand_in = Content::Inline(Inline::new(Vec::new())?);
    let mut shape = Vec::with_capacity(formats.len());
    for format in formats {
        shape.push(match format {
            PlanFormat::Single { kind, .. } => match kind {
                FormatKind::Text => Format::Text(stand_in.clone()),
                FormatKind::Html => Format::Html(stand_in.clone()),
                FormatKind::Rtf => Format::Rtf(stand_in.clone()),
                FormatKind::Png => Format::Png(stand_in.clone()),
                FormatKind::Files => {
                    return Err(Error::malformed(ITEMS, "`files` is not a single payload"));
                }
            },
            PlanFormat::Files(files) => Format::Files(
                files
                    .iter()
                    .map(|f| FileEntry::new(&f.name, stand_in.clone()))
                    .collect::<core::result::Result<_, _>>()?,
            ),
        });
    }
    Formats::new(shape)?;
    Ok(())
}

/// A payload reference as the document states it.
struct Payload<'a> {
    asset: &'a str,
    size: u64,
    encrypted: bool,
    slot: Slot,
    /// Whether it is a text-like format with the 5 MiB limit.
    text: bool,
}

/// Whether an item can be imported.
enum Verdict {
    Keep,
    /// It has encrypted payloads and no key was given.
    NeedsKey,
    /// Its encrypted payloads do not open with the key given.
    CannotDecrypt,
}

/// State shared by the asset checks of one import.
struct Verified {
    /// Plain assets already checked, with whether UTF-8 was required.
    plain: HashSet<(BlobRef, bool)>,
    /// Bytes of decrypted payload the plan may still hold in memory.
    decrypt_budget: u64,
}

/// Result of checking one asset.
enum Checked {
    /// Fine. `decrypted` is the plaintext of an encrypted payload, which replaces the asset in
    /// the plan.
    Ok {
        hash: [u8; 32],
        len: u64,
        decrypted: Option<Vec<u8>>,
    },
    NeedsKey,
    CannotDecrypt,
}

/// Reads and checks every asset the plan uses, computes each item's content hash, and drops the
/// items that cannot be imported because their encrypted payloads cannot be opened.
fn verify<R: Read + Seek>(
    archive: &mut SafeArchive<R>,
    plan: &mut Plan,
    key: Option<&DataKey>,
    report: &mut ImportReport,
) -> Result<()> {
    let mut verified = Verified {
        plain: HashSet::new(),
        decrypt_budget: MEMORY_BUDGET,
    };
    let mut needs_key = 0_usize;
    for list in plan.boards.iter_mut().flat_map(|b| &mut b.lists) {
        let items = std::mem::take(&mut list.items);
        for mut item in items {
            match verify_item(archive, &mut item, key, &mut verified)? {
                Verdict::Keep => list.items.push(item),
                Verdict::NeedsKey => {
                    needs_key += 1;
                    report.items_skipped += 1;
                }
                Verdict::CannotDecrypt => {
                    report.items_skipped += 1;
                    report.warn(Warning::CannotDecrypt { title: item.title });
                }
            }
        }
    }
    if needs_key > 0 {
        report.warn(Warning::SensitiveNeedKey { count: needs_key });
    }
    Ok(())
}

fn verify_item<R: Read + Seek>(
    archive: &mut SafeArchive<R>,
    item: &mut PlanItem,
    key: Option<&DataKey>,
    verified: &mut Verified,
) -> Result<Verdict> {
    let mut hasher = ContentHasher::new();
    for format in &mut item.formats {
        match format {
            PlanFormat::Single { kind, source } => {
                let text = matches!(kind, FormatKind::Text | FormatKind::Html | FormatKind::Rtf);
                let utf8 = matches!(kind, FormatKind::Text | FormatKind::Html);
                match check_source(archive, source, text, utf8, key, verified)? {
                    Checked::Ok {
                        hash,
                        len,
                        decrypted,
                    } => {
                        hasher.single(*kind, &hash, len);
                        if let Some(plain) = decrypted {
                            *source = Source::Bytes(plain);
                        }
                    }
                    Checked::NeedsKey => return Ok(Verdict::NeedsKey),
                    Checked::CannotDecrypt => return Ok(Verdict::CannotDecrypt),
                }
            }
            PlanFormat::Files(files) => {
                let mut parts = Vec::with_capacity(files.len());
                for file in files.iter_mut() {
                    match check_source(archive, &file.source, false, false, key, verified)? {
                        Checked::Ok {
                            hash,
                            len,
                            decrypted,
                        } => {
                            parts.push((file.name.clone(), hash, len));
                            if let Some(plain) = decrypted {
                                file.source = Source::Bytes(plain);
                            }
                        }
                        Checked::NeedsKey => return Ok(Verdict::NeedsKey),
                        Checked::CannotDecrypt => return Ok(Verdict::CannotDecrypt),
                    }
                }
                let parts: Vec<_> = parts.iter().map(|(n, h, l)| (n.as_str(), *h, *l)).collect();
                hasher.files(&parts);
            }
        }
    }
    item.content_hash = hasher.finish();
    Ok(Verdict::Keep)
}

fn check_source<R: Read + Seek>(
    archive: &mut SafeArchive<R>,
    source: &Source,
    text: bool,
    utf8: bool,
    key: Option<&DataKey>,
    verified: &mut Verified,
) -> Result<Checked> {
    let Source::Asset(asset) = source else {
        // Archive plans only hold assets until their encrypted payloads are decrypted below.
        return Err(Error::malformed(
            ITEMS,
            "internal: unexpected in-memory payload",
        ));
    };
    let name_mismatch = || Error::AssetMismatch {
        asset: asset.hash.to_string(),
        reason: "its bytes do not match its hash".to_owned(),
    };
    match asset.encrypted_for {
        None => {
            let size = archive.asset_size(&asset.hash).unwrap_or(0);
            if utf8 {
                if verified.plain.insert((asset.hash, true)) {
                    let bytes = archive.read_asset(&asset.hash)?;
                    if blake3::hash(&bytes).as_bytes() != asset.hash.as_bytes() {
                        return Err(name_mismatch());
                    }
                    if std::str::from_utf8(&bytes).is_err() {
                        return Err(Error::AssetMismatch {
                            asset: asset.hash.to_string(),
                            reason: "a text or HTML payload is not valid UTF-8".to_owned(),
                        });
                    }
                }
            } else if verified.plain.insert((asset.hash, false))
                && archive.hash_asset(&asset.hash)? != *asset.hash.as_bytes()
            {
                return Err(name_mismatch());
            }
            Ok(Checked::Ok {
                hash: *asset.hash.as_bytes(),
                len: size,
                decrypted: None,
            })
        }
        Some((item, slot)) => {
            let Some(key) = key else {
                return Ok(Checked::NeedsKey);
            };
            let bytes = archive.read_asset(&asset.hash)?;
            if blake3::hash(&bytes).as_bytes() != asset.hash.as_bytes() {
                return Err(name_mismatch());
            }
            let Ok(plain) = decrypt_payload(key, &item, slot, &bytes) else {
                return Ok(Checked::CannotDecrypt);
            };
            if text && plain.len() as u64 > TEXT_LIMIT {
                return Err(Error::LimitExceeded {
                    what: "text size",
                    limit: TEXT_LIMIT,
                });
            }
            if utf8 && std::str::from_utf8(&plain).is_err() {
                return Err(Error::AssetMismatch {
                    asset: asset.hash.to_string(),
                    reason: "a decrypted text or HTML payload is not valid UTF-8".to_owned(),
                });
            }
            let len = plain.len() as u64;
            verified.decrypt_budget =
                verified
                    .decrypt_budget
                    .checked_sub(len)
                    .ok_or(Error::LimitExceeded {
                        what: "decrypted size",
                        limit: MEMORY_BUDGET,
                    })?;
            Ok(Checked::Ok {
                hash: *blake3::hash(&plain).as_bytes(),
                len,
                decrypted: Some(plain.to_vec()),
            })
        }
    }
}
