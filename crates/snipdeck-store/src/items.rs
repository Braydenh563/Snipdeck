//! [`ItemRepo`] for [`Store`], and the encoding of an item's formats.

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};

use rusqlite::{Connection, ToSql, params};
use snipdeck_core::{
    BlobRef, Content, FileEntry, Format, FormatKind, Formats, Item, ItemId, ListId, Placement,
    TagId, Timestamp,
};

use crate::blobs::Blobs;
use crate::codec::{
    content_from, flag, from_nanos, from_opt_nanos, id_bytes, id_from, nanos, not_found, payload,
    raw_placement, require_item, require_list, require_new,
};
use crate::rank::{self, ITEMS};
use crate::store::begin;
use crate::{BlobStore, Error, ItemRepo, Result, Store, fts};

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

/// Replaces the item's formats with `formats`, checking that every blob they name exists.
fn write_formats(conn: &Connection, blobs: &Blobs, id: [u8; 16], formats: &Formats) -> Result<()> {
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

impl ItemRepo for Store {
    fn items(&self, list: ListId) -> Result<Vec<Item>> {
        fetch(
            &self.conn,
            "AND i.list_id = ?1",
            &[&id_bytes(list)],
            "i.rank",
        )
    }

    fn all_items(&self) -> Result<Vec<Item>> {
        fetch(&self.conn, "", &[], "b.rank, l.rank, i.rank")
    }

    fn item(&self, id: ItemId) -> Result<Item> {
        fetch(&self.conn, "AND i.id = ?1", &[&id_bytes(id)], "i.rank")?
            .pop()
            .ok_or_else(|| not_found(id))
    }

    fn formats(&self, id: ItemId) -> Result<Formats> {
        require_item(&self.conn, id)?;
        load_formats(&self.conn, id_bytes(id))
    }

    fn insert_item(&mut self, item: &Item, formats: &Formats, at: Placement<ItemId>) -> Result<()> {
        check_item(item)?;
        let (created, updated) = (nanos(item.created_at)?, nanos(item.updated_at)?);
        let last_used = item.last_used_at.map(nanos).transpose()?;
        let tx = begin(&mut self.conn)?;
        require_new(&tx, "items", item.id)?;
        require_list(&tx, item.list_id)?;
        require_tags(&tx, &item.tags)?;
        let (id, list) = (id_bytes(item.id), id_bytes(item.list_id));
        let rank = rank::place(&tx, ITEMS, Some(list), raw_placement(at), None)?;
        tx.execute(
            "INSERT INTO items (id, list_id, title, kind, preview, pinned, sensitive, always_plain,
                                use_count, last_used_at, source_app, rank, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
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
                updated
            ],
        )?;
        let seq = tx.last_insert_rowid();
        write_formats(&tx, &self.blobs, id, formats)?;
        write_tags(&tx, id, &item.tags)?;
        fts::index(&tx, &self.blobs, seq, &item.title, item.sensitive, formats)?;
        Ok(tx.commit()?)
    }

    fn update_item(&mut self, item: &Item, formats: Option<&Formats>) -> Result<()> {
        check_item(item)?;
        let (created, updated) = (nanos(item.created_at)?, nanos(item.updated_at)?);
        let last_used = item.last_used_at.map(nanos).transpose()?;
        let tx = begin(&mut self.conn)?;
        require_item(&tx, item.id)?;
        require_tags(&tx, &item.tags)?;
        let (id, list) = (id_bytes(item.id), id_bytes(item.list_id));
        let (seq, current): (i64, Vec<u8>) =
            tx.query_row("SELECT seq, list_id FROM items WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        if current != list {
            // A new parent: the item goes to the end of it.
            require_list(&tx, item.list_id)?;
            let rank = rank::place(&tx, ITEMS, Some(list), Placement::Last, Some(id))?;
            tx.execute(
                "UPDATE items SET list_id = ?2, rank = ?3 WHERE id = ?1",
                params![id, list, rank],
            )?;
        }
        tx.execute(
            "UPDATE items SET title = ?2, kind = ?3, preview = ?4, pinned = ?5, sensitive = ?6,
                    always_plain = ?7, use_count = ?8, last_used_at = ?9, source_app = ?10,
                    created_at = ?11, updated_at = ?12
             WHERE id = ?1",
            params![
                id,
                item.title,
                item.kind.key(),
                item.preview,
                flag(item.pinned),
                flag(item.sensitive),
                flag(item.always_plain),
                item.use_count,
                last_used,
                item.source_app,
                created,
                updated
            ],
        )?;
        write_tags(&tx, id, &item.tags)?;
        let mut candidates = Vec::new();
        if let Some(formats) = formats {
            candidates = blob_hashes(&tx, id)?;
            write_formats(&tx, &self.blobs, id, formats)?;
            fts::index(&tx, &self.blobs, seq, &item.title, item.sensitive, formats)?;
        } else {
            fts::reindex(&tx, &self.blobs, id)?;
        }
        tx.commit()?;
        self.release_blobs(candidates)
    }

    fn move_item(&mut self, id: ItemId, list: ListId, at: Placement<ItemId>) -> Result<()> {
        let tx = begin(&mut self.conn)?;
        require_item(&tx, id)?;
        require_list(&tx, list)?;
        let (bytes, list_bytes) = (id_bytes(id), id_bytes(list));
        let rank = rank::place(&tx, ITEMS, Some(list_bytes), raw_placement(at), Some(bytes))?;
        tx.execute(
            "UPDATE items SET list_id = ?2, rank = ?3 WHERE id = ?1",
            params![bytes, list_bytes, rank],
        )?;
        Ok(tx.commit()?)
    }

    fn record_use(&mut self, id: ItemId, at: Timestamp) -> Result<()> {
        let at = nanos(at)?;
        let tx = begin(&mut self.conn)?;
        require_item(&tx, id)?;
        tx.execute(
            "UPDATE items SET use_count = MIN(use_count + 1, 4294967295), last_used_at = ?2
             WHERE id = ?1",
            params![id_bytes(id), at],
        )?;
        Ok(tx.commit()?)
    }

    fn search(&self, query: &str, limit: usize) -> Result<Vec<ItemId>> {
        let Some(expression) = fts::match_expression(query) else {
            return Ok(Vec::new());
        };
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT i.id FROM item_fts JOIN items i ON i.seq = item_fts.rowid {JOIN_LIVE}
             WHERE item_fts MATCH ?1 AND {WHERE_LIVE}
             ORDER BY bm25(item_fts, 10.0, 1.0), i.seq
             LIMIT ?2"
        ))?;
        let ids = stmt.query_map(params![expression, limit], |r| r.get::<_, Vec<u8>>(0))?;
        ids.map(|id| id_from(&id?)).collect()
    }
}

impl BlobStore for Store {
    fn put(&mut self, bytes: Vec<u8>) -> Result<Content> {
        self.blobs.put(bytes)
    }

    fn read<'a>(&self, content: &'a Content) -> Result<Cow<'a, [u8]>> {
        self.blobs.read(content)
    }
}
