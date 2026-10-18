//! [`ItemRepo`] for [`Store`], and the encoding of an item's formats.

use std::collections::{BTreeSet, HashMap};

use rusqlite::{Connection, ToSql, params};
use snipdeck_core::{
    BlobRef, Content, FileEntry, Format, FormatKind, Formats, Item, ItemId, ListId, Placement,
    TagId, Timestamp,
};
use zeroize::Zeroizing;

use crate::batch::Ctx;
use crate::blobs::Blobs;
use crate::codec::{
    content_from, flag, from_nanos, from_opt_nanos, id_bytes, id_from, nanos, not_found, payload,
    raw_placement, require_item, require_list, require_new,
};
use crate::crypto::{DataKey, Slot, decrypt_payload, encrypt_payload};
use crate::rank::{self, ITEMS};
use crate::{Error, Result, fts, keys};

/// The tables a live item is read through: the item, its list and its board.
const FROM_LIVE: &str =
    "items i JOIN lists l ON l.id = i.list_id JOIN boards b ON b.id = l.board_id";

/// The same joins, for queries that start from another table that refers to `items i`.
const JOIN_LIVE: &str = "JOIN lists l ON l.id = i.list_id JOIN boards b ON b.id = l.board_id";

/// A live item is out of the trash, and so are its list and board.
const WHERE_LIVE: &str = "i.deleted_at IS NULL AND l.deleted_at IS NULL AND b.deleted_at IS NULL";

const COLUMNS: &str = "i.id, i.list_id, i.title, i.kind, i.preview, i.pinned, i.sensitive, \
     i.always_plain, i.use_count, i.last_used_at, i.source_app, i.created_at, i.updated_at";

struct ItemRow {
    id: Vec<u8>,
    list_id: Vec<u8>,
    title: String,
    kind: String,
    preview: String,
    pinned: bool,
    sensitive: bool,
    always_plain: bool,
    use_count: i64,
    last_used_at: Option<i64>,
    source_app: Option<String>,
    created_at: i64,
    updated_at: i64,
}

impl ItemRow {
    fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            list_id: row.get(1)?,
            title: row.get(2)?,
            kind: row.get(3)?,
            preview: row.get(4)?,
            pinned: row.get(5)?,
            sensitive: row.get(6)?,
            always_plain: row.get(7)?,
            use_count: row.get(8)?,
            last_used_at: row.get(9)?,
            source_app: row.get(10)?,
            created_at: row.get(11)?,
            updated_at: row.get(12)?,
        })
    }

    fn into_item(self, tags: &mut HashMap<Vec<u8>, BTreeSet<TagId>>) -> Result<Item> {
        let tags = tags.remove(&self.id).unwrap_or_default();
        Ok(Item {
            id: id_from(&self.id)?,
            list_id: id_from(&self.list_id)?,
            title: self.title,
            kind: self.kind.parse()?,
            preview: self.preview,
            tags,
            pinned: self.pinned,
            sensitive: self.sensitive,
            always_plain: self.always_plain,
            use_count: u32::try_from(self.use_count)
                .map_err(|_| Error::Corrupt("a use count is out of range".to_owned()))?,
            last_used_at: from_opt_nanos(self.last_used_at)?,
            source_app: self.source_app,
            created_at: from_nanos(self.created_at)?,
            updated_at: from_nanos(self.updated_at)?,
        })
    }
}

/// Live items matching `extra` (an `AND ...` clause), in `order`, with their tags: two queries
/// however many items there are.
fn fetch(conn: &Connection, extra: &str, args: &[&dyn ToSql], order: &str) -> Result<Vec<Item>> {
    let mut tags: HashMap<Vec<u8>, BTreeSet<TagId>> = HashMap::new();
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT it.item_id, it.tag_id FROM item_tags it
         JOIN items i ON i.id = it.item_id {JOIN_LIVE}
         WHERE {WHERE_LIVE} {extra}"
    ))?;
    let links = stmt.query_map(args, |r| {
        Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?))
    })?;
    for link in links {
        let (item, tag) = link?;
        tags.entry(item).or_default().insert(id_from(&tag)?);
    }

    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM {FROM_LIVE} WHERE {WHERE_LIVE} {extra} ORDER BY {order}"
    ))?;
    let rows = stmt.query_map(args, ItemRow::read)?;
    rows.map(|row| row?.into_item(&mut tags)).collect()
}

fn require_tags(conn: &Connection, tags: &BTreeSet<TagId>) -> Result<()> {
    for tag in tags {
        crate::codec::exists(conn, "tags", id_bytes(*tag))?
            .then_some(())
            .ok_or_else(|| not_found(*tag))?;
    }
    Ok(())
}

fn write_tags(conn: &Connection, item: [u8; 16], tags: &BTreeSet<TagId>) -> Result<()> {
    conn.execute("DELETE FROM item_tags WHERE item_id = ?1", [item])?;
    for tag in tags {
        conn.execute(
            "INSERT INTO item_tags (item_id, tag_id) VALUES (?1, ?2)",
            params![item, id_bytes(*tag)],
        )?;
    }
    Ok(())
}

/// The stored state `update_item` compares the new item with.
struct Before {
    seq: i64,
    list: Vec<u8>,
    title: String,
    sensitive: bool,
    encrypted: bool,
}

/// The writes `update_item` refuses while the key ring is locked: clearing the flag of an
/// encrypted item (that means decrypting first), and, once encryption is set up, storing new
/// plaintext of a sensitive item (new content, or marking it sensitive).
fn refuse_while_locked(
    conn: &Connection,
    item: &Item,
    before: &Before,
    new_content: bool,
) -> Result<()> {
    if before.encrypted && !item.sensitive {
        return Err(Error::EncryptionLocked);
    }
    if item.sensitive && (new_content || !before.sensitive) && keys::ring_exists(conn)? {
        return Err(Error::EncryptionLocked);
    }
    Ok(())
}

/// Whether the item's payloads are stored as envelopes.
pub(crate) fn is_encrypted(conn: &Connection, id: [u8; 16]) -> Result<bool> {
    Ok(
        conn.query_row("SELECT encrypted FROM items WHERE id = ?1", [id], |r| {
            r.get(0)
        })?,
    )
}

fn check_item(item: &Item) -> Result<()> {
    if item.sensitive && !item.preview.is_empty() {
        return Err(Error::Invalid(
            "a sensitive item must not have a preview".to_owned(),
        ));
    }
    Ok(())
}

/// Every blob hash the item's formats refer to.
fn blob_hashes(conn: &Connection, id: [u8; 16]) -> Result<Vec<BlobRef>> {
    let mut stmt = conn.prepare_cached(
        "SELECT blob_hash FROM item_formats WHERE item_id = ?1 AND blob_hash IS NOT NULL
         UNION SELECT blob_hash FROM item_files WHERE item_id = ?1 AND blob_hash IS NOT NULL",
    )?;
    let hashes = stmt.query_map([id], |r| r.get::<_, Vec<u8>>(0))?;
    hashes
        .map(|hash| {
            <[u8; 32]>::try_from(hash?)
                .map(BlobRef::from_bytes)
                .map_err(|_| Error::Corrupt("a blob hash is not 32 bytes".to_owned()))
        })
        .collect()
}

/// What [`write_formats`] did with the blobs it was given.
#[derive(Default)]
pub(crate) struct Written {
    /// Blobs the new rows refer to as given (a `put` each stops being pending on commit).
    pub used: Vec<BlobRef>,
    /// Blobs whose bytes were sealed into envelopes instead: no row refers to them, so they are
    /// released (and scrubbed) after the commit unless something else uses them.
    pub consumed: Vec<BlobRef>,
}

/// Replaces the item's formats with `formats`. With `key`, every payload is first encrypted
/// for its slot (`SNENC2`, bound to the item, format and file) and the envelope is what is
/// stored; the caller sets `items.encrypted` to match. Without, payloads are stored as given,
/// and a transient payload (see `Blobs`) is written to the blob folder.
pub(crate) fn write_formats(
    conn: &Connection,
    blobs: &mut Blobs,
    item: ItemId,
    formats: &Formats,
    key: Option<&DataKey>,
) -> Result<Written> {
    let mut written = Written::default();
    let stored = map_payloads(formats, |content, slot| {
        // A staged or decrypted payload is plaintext in RAM that nothing needs once this
        // transaction ends (the row holds the sealed or stored copy): drop it then.
        blobs.take_transient(content);
        if let Some(key) = key {
            let plain = Zeroizing::new(blobs.read_any(content)?.into_owned());
            let envelope = encrypt_payload(key, &item, slot, &plain)?;
            if let Content::Blob { blob, .. } = content {
                written.consumed.push(*blob);
            }
            return blobs.store(envelope);
        }
        match content {
            Content::Blob { blob, len } if !blobs.has(blob, *len) && blobs.is_transient(blob) => {
                // Decrypted or staged in memory, now part of an item that is not encrypted.
                let bytes = blobs.read_any(content)?.into_owned();
                blobs.store(bytes)
            }
            Content::Blob { blob, .. } => {
                written.used.push(*blob);
                Ok(content.clone())
            }
            Content::Inline(_) => Ok(content.clone()),
        }
    })?;
    write_rows(conn, blobs, id_bytes(item), &stored)?;
    Ok(written)
}

/// The decrypted formats of an encrypted item. Payloads too large to be inline are kept in
/// memory only (see `Blobs`), never written to disk in plain text.
pub(crate) fn decrypt_formats(
    blobs: &Blobs,
    key: &DataKey,
    item: ItemId,
    stored: &Formats,
) -> Result<Formats> {
    map_payloads(stored, |content, slot| {
        let envelope = blobs.read(content)?;
        let plain = decrypt_payload(key, &item, slot, &envelope)?;
        blobs.put_transient(plain.to_vec())
    })
}

/// Rebuilds `formats` with every payload replaced by `f(payload, its slot)`, keeping order and
/// file names.
fn map_payloads(
    formats: &Formats,
    mut f: impl FnMut(&Content, Slot) -> Result<Content>,
) -> Result<Formats> {
    let mut out = Vec::new();
    for format in formats {
        let slot = Slot::Format(format.kind());
        out.push(match format {
            Format::Text(c) => Format::Text(f(c, slot)?),
            Format::Html(c) => Format::Html(f(c, slot)?),
            Format::Rtf(c) => Format::Rtf(f(c, slot)?),
            Format::Png(c) => Format::Png(f(c, slot)?),
            Format::Files(files) => {
                let mut entries = Vec::with_capacity(files.len());
                for (index, file) in files.iter().enumerate() {
                    let index = u32::try_from(index)
                        .map_err(|_| Error::Invalid("too many files".to_owned()))?;
                    entries.push(FileEntry::new(
                        file.name(),
                        f(file.content(), Slot::File(index))?,
                    )?);
                }
                Format::Files(entries)
            }
        });
    }
    Ok(Formats::new(out)?)
}

/// Writes the format rows, checking that every blob they name exists.
fn write_rows(conn: &Connection, blobs: &Blobs, id: [u8; 16], formats: &Formats) -> Result<()> {
    conn.execute("DELETE FROM item_formats WHERE item_id = ?1", [id])?;
    let check = |content: &Content| -> Result<()> {
        match content {
            Content::Blob { blob, len } if !blobs.has(blob, *len) => Err(Error::NotFound {
                entity: "blob",
                id: blob.to_string(),
            }),
            _ => Ok(()),
        }
    };
    for (idx, format) in formats.iter().enumerate() {
        let idx = i64::try_from(idx).map_err(|_| Error::Invalid("too many formats".to_owned()))?;
        let kind = format.kind().key();
        match format {
            Format::Text(c) | Format::Html(c) | Format::Rtf(c) | Format::Png(c) => {
                check(c)?;
                let (inline, hash, len) = payload(c)?;
                conn.execute(
                    "INSERT INTO item_formats (item_id, idx, kind, inline_bytes, blob_hash, len)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![id, idx, kind, inline, hash, len],
                )?;
            }
            Format::Files(files) => {
                conn.execute(
                    "INSERT INTO item_formats (item_id, idx, kind) VALUES (?1, ?2, ?3)",
                    params![id, idx, kind],
                )?;
                for (file_idx, file) in files.iter().enumerate() {
                    let file_idx = i64::try_from(file_idx)
                        .map_err(|_| Error::Invalid("too many files".to_owned()))?;
                    check(file.content())?;
                    let (inline, hash, len) = payload(file.content())?;
                    conn.execute(
                        "INSERT INTO item_files (item_id, idx, name, inline_bytes, blob_hash, len)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![id, file_idx, file.name(), inline, hash, len],
                    )?;
                }
            }
        }
    }
    Ok(())
}

/// Reads an item's formats back, in preference order.
pub(crate) fn load_formats(conn: &Connection, id: [u8; 16]) -> Result<Formats> {
    type Raw = (String, Option<Vec<u8>>, Option<Vec<u8>>, Option<i64>);
    let mut stmt = conn.prepare_cached(
        "SELECT kind, inline_bytes, blob_hash, len FROM item_formats WHERE item_id = ?1 ORDER BY idx",
    )?;
    let rows: Vec<Raw> = stmt
        .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut formats = Vec::with_capacity(rows.len());
    for (kind, inline, hash, len) in rows {
        let kind: FormatKind = kind.parse()?;
        formats.push(match kind {
            FormatKind::Files => Format::Files(load_files(conn, id)?),
            FormatKind::Text => Format::Text(content_from(inline, hash, len)?),
            FormatKind::Html => Format::Html(content_from(inline, hash, len)?),
            FormatKind::Rtf => Format::Rtf(content_from(inline, hash, len)?),
            FormatKind::Png => Format::Png(content_from(inline, hash, len)?),
        });
    }
    Ok(Formats::new(formats)?)
}

fn load_files(conn: &Connection, id: [u8; 16]) -> Result<Vec<FileEntry>> {
    let mut stmt = conn.prepare_cached(
        "SELECT name, inline_bytes, blob_hash, len FROM item_files WHERE item_id = ?1 ORDER BY idx",
    )?;
    let rows = stmt.query_map([id], |r| {
        Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
    })?;
    rows.map(|row| {
        let (name, inline, hash, len) = row?;
        Ok(FileEntry::new(&name, content_from(inline, hash, len)?)?)
    })
    .collect()
}

pub(crate) fn items(conn: &Connection, list: ListId) -> Result<Vec<Item>> {
    // A list that is missing or not live is `NotFound`, not empty (review L10).
    require_list(conn, list)?;
    fetch(conn, "AND i.list_id = ?1", &[&id_bytes(list)], "i.rank")
}

pub(crate) fn all_items(conn: &Connection) -> Result<Vec<Item>> {
    fetch(conn, "", &[], "b.rank, l.rank, i.rank")
}

pub(crate) fn item(conn: &Connection, id: ItemId) -> Result<Item> {
    fetch(conn, "AND i.id = ?1", &[&id_bytes(id)], "i.rank")?
        .pop()
        .ok_or_else(|| not_found(id))
}

pub(crate) fn formats(
    conn: &Connection,
    blobs: &Blobs,
    key: Option<&DataKey>,
    id: ItemId,
) -> Result<Formats> {
    require_item(conn, id)?;
    let stored = load_formats(conn, id_bytes(id))?;
    if !is_encrypted(conn, id_bytes(id))? {
        return Ok(stored);
    }
    let key = key.ok_or(Error::EncryptionLocked)?;
    decrypt_formats(blobs, key, id, &stored)
}

pub(crate) fn insert_item(
    cx: &mut Ctx<'_>,
    item: &Item,
    formats: &Formats,
    at: Placement<ItemId>,
) -> Result<()> {
    check_item(item)?;
    let (created, updated) = (nanos(item.created_at)?, nanos(item.updated_at)?);
    let last_used = item.last_used_at.map(nanos).transpose()?;
    let (conn, key) = (cx.conn, cx.key);
    require_new(conn, "items", item.id)?;
    require_list(conn, item.list_id)?;
    require_tags(conn, &item.tags)?;
    if item.sensitive && key.is_none() && keys::ring_exists(conn)? {
        // Encryption is set up: never store a new secret in plain text.
        return Err(Error::EncryptionLocked);
    }
    let encrypt = item.sensitive && key.is_some();
    let (id, list) = (id_bytes(item.id), id_bytes(item.list_id));
    let rank = rank::place(conn, ITEMS, Some(list), raw_placement(at), None)?;
    conn.execute(
        "INSERT INTO items (id, list_id, title, kind, preview, pinned, sensitive, always_plain,
                            use_count, last_used_at, source_app, rank, created_at, updated_at,
                            encrypted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            id,
            list,
            item.title,
            item.kind.key(),
            item.preview,
            flag(item.pinned),
            flag(item.sensitive),
            flag(item.always_plain),
            item.use_count,
            last_used,
            item.source_app,
            rank,
            created,
            updated,
            flag(encrypt)
        ],
    )?;
    let seq = conn.last_insert_rowid();
    let written = write_formats(conn, cx.blobs, item.id, formats, key.filter(|_| encrypt))?;
    write_tags(conn, id, &item.tags)?;
    fts::index(conn, cx.blobs, seq, &item.title, item.sensitive, formats)?;
    // A plaintext blob `put` for this sensitive item was sealed: scrub it.
    let consumed_plaintext = item.sensitive && !written.consumed.is_empty();
    cx.post.written(written, Vec::new(), consumed_plaintext);
    Ok(())
}

pub(crate) fn update_item(cx: &mut Ctx<'_>, item: &Item, formats: Option<&Formats>) -> Result<()> {
    check_item(item)?;
    let (created, updated) = (nanos(item.created_at)?, nanos(item.updated_at)?);
    let (conn, key) = (cx.conn, cx.key);
    require_item(conn, item.id)?;
    require_tags(conn, &item.tags)?;
    let (id, list) = (id_bytes(item.id), id_bytes(item.list_id));
    let before: Before = conn.query_row(
        "SELECT seq, list_id, title, sensitive, encrypted FROM items WHERE id = ?1",
        [id],
        |r| {
            Ok(Before {
                seq: r.get(0)?,
                list: r.get(1)?,
                title: r.get(2)?,
                sensitive: r.get(3)?,
                encrypted: r.get(4)?,
            })
        },
    )?;
    if key.is_none() {
        refuse_while_locked(conn, item, &before, formats.is_some())?;
    }
    // Whether the content is encrypted once this call is done.
    let encrypted = match formats {
        Some(_) => item.sensitive && key.is_some(),
        None => item.sensitive && (before.encrypted || key.is_some()),
    };

    if before.list != list {
        // A new parent: the item goes to the end of it.
        require_list(conn, item.list_id)?;
        let rank = rank::place(conn, ITEMS, Some(list), Placement::Last, Some(id))?;
        conn.execute(
            "UPDATE items SET list_id = ?2, rank = ?3 WHERE id = ?1",
            params![id, list, rank],
        )?;
    }
    // `use_count` and `last_used_at` belong to `record_use`: an editor holding an item
    // loaded before a use must not roll the count back (review L12).
    conn.execute(
        "UPDATE items SET title = ?2, kind = ?3, preview = ?4, pinned = ?5, sensitive = ?6,
                always_plain = ?7, source_app = ?8, created_at = ?9, updated_at = ?10,
                encrypted = ?11
         WHERE id = ?1",
        params![
            id,
            item.title,
            item.kind.key(),
            item.preview,
            flag(item.pinned),
            flag(item.sensitive),
            flag(item.always_plain),
            item.source_app,
            created,
            updated,
            flag(encrypted)
        ],
    )?;
    write_tags(conn, id, &item.tags)?;

    let seal = key.filter(|_| encrypted);
    let mut old = Vec::new();
    let mut written = Written::default();
    if let Some(formats) = formats {
        old = blob_hashes(conn, id)?;
        written = write_formats(conn, cx.blobs, item.id, formats, seal)?;
        fts::index(
            conn,
            cx.blobs,
            before.seq,
            &item.title,
            item.sensitive,
            formats,
        )?;
    } else if encrypted != before.encrypted {
        // Marked sensitive (encrypt what is stored) or cleared (decrypt it first).
        let stored = load_formats(conn, id)?;
        let plain = match (before.encrypted, key) {
            (false, _) => stored,
            (true, Some(key)) => decrypt_formats(cx.blobs, key, item.id, &stored)?,
            (true, None) => return Err(Error::EncryptionLocked),
        };
        // The blobs this consumes or reuses are the item's own (in `old`), not from a
        // `put`, so they do not count as attached.
        old = blob_hashes(conn, id)?;
        write_formats(conn, cx.blobs, item.id, &plain, seal)?;
        fts::index(
            conn,
            cx.blobs,
            before.seq,
            &item.title,
            item.sensitive,
            &plain,
        )?;
    } else if before.sensitive != item.sensitive {
        fts::reindex(conn, cx.blobs, id)?;
    } else if before.title != item.title {
        // Metadata only: never re-read the content (review L11).
        fts::retitle(conn, before.seq, &item.title)?;
    }
    let cleared_plaintext =
        item.sensitive && (formats.is_some() || !before.sensitive || encrypted != before.encrypted);
    cx.post.written(written, old, cleared_plaintext);
    Ok(())
}

pub(crate) fn move_item(
    cx: &mut Ctx<'_>,
    id: ItemId,
    list: ListId,
    at: Placement<ItemId>,
) -> Result<()> {
    let conn = cx.conn;
    require_item(conn, id)?;
    require_list(conn, list)?;
    let (bytes, list_bytes) = (id_bytes(id), id_bytes(list));
    let rank = rank::place(
        conn,
        ITEMS,
        Some(list_bytes),
        raw_placement(at),
        Some(bytes),
    )?;
    conn.execute(
        "UPDATE items SET list_id = ?2, rank = ?3 WHERE id = ?1",
        params![bytes, list_bytes, rank],
    )?;
    Ok(())
}

pub(crate) fn record_use(cx: &mut Ctx<'_>, id: ItemId, at: Timestamp) -> Result<()> {
    let at = nanos(at)?;
    let conn = cx.conn;
    require_item(conn, id)?;
    conn.execute(
        "UPDATE items SET use_count = MIN(use_count + 1, 4294967295), last_used_at = ?2
         WHERE id = ?1",
        params![id_bytes(id), at],
    )?;
    Ok(())
}

pub(crate) fn search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<ItemId>> {
    let Some(expression) = fts::match_expression(query) else {
        return Ok(Vec::new());
    };
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT i.id FROM item_fts JOIN items i ON i.seq = item_fts.rowid {JOIN_LIVE}
         WHERE item_fts MATCH ?1 AND {WHERE_LIVE}
         ORDER BY bm25(item_fts, 10.0, 1.0), i.seq
         LIMIT ?2"
    ))?;
    let ids = stmt.query_map(params![expression, limit], |r| r.get::<_, Vec<u8>>(0))?;
    ids.map(|id| id_from(&id?)).collect()
}
