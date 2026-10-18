//! [`TagRepo`] for [`Store`].
//!
//! Uniqueness is on `name_key` ([`name_key`]): the name with its case folded fully and in
//! Unicode normalisation form C (review N5). SQLite's own `NOCASE` folds ASCII only, and plain
//! lower-casing keeps `Straße` and `STRASSE`, or a composed and a decomposed `École`, apart.

use rusqlite::{Connection, OptionalExtension, params};
use snipdeck_core::{Name, Tag, TagId};
use unicode_normalization::UnicodeNormalization as _;

use crate::Error;
use crate::Result;
use crate::batch::Ctx;
use crate::codec::{id_bytes, id_from, not_found, require_new, stored_name};

/// The key two tag names clash on. Importers use it to decide whether a name is already a tag,
/// so that they agree with the store and never trip its `TagExists` check.
///
/// Decompose, fold the case, compose: the canonical caseless match of Unicode §3.13, with
/// the case folded as upper-case then lower-case. That equals full case folding for every
/// cased letter that matters here (`ß` to `ss`, final `ς` to `σ`, ligatures such as `ﬁ`), and
/// it is the same mapping whatever the input's case, which is all a uniqueness key needs.
#[must_use]
pub fn name_key(name: &str) -> String {
    let decomposed: String = name.nfd().collect();
    decomposed.to_uppercase().to_lowercase().nfc().collect()
}

fn key(name: &Name) -> String {
    name_key(name.as_str())
}

/// Migration step to schema version 2: recomputes every `name_key` with [`name_key`].
///
/// Tags that were distinct before and clash now (`Straße` and `STRASSE`) both stay: the later
/// one (by id) gets its key suffixed with its id, and `check_integrity` reports it so the user
/// can rename one. Keys are first parked on unique placeholders so the unique index never sees
/// a temporary clash.
pub(crate) fn rekey(conn: &Connection) -> Result<()> {
    let tags: Vec<(Vec<u8>, String)> = conn
        .prepare("SELECT id, name FROM tags ORDER BY id")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    conn.execute("UPDATE tags SET name_key = char(1) || hex(id)", [])?;
    let mut taken = std::collections::HashSet::new();
    for (id, name) in tags {
        let mut key = name_key(&name);
        if key.is_empty() || !taken.insert(key.clone()) {
            key = format!("{key}\u{1}{}", hex(&id));
            taken.insert(key.clone());
        }
        conn.execute(
            "UPDATE tags SET name_key = ?2 WHERE id = ?1",
            params![id, key],
        )?;
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    use core::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

pub(crate) fn tags(conn: &Connection) -> Result<Vec<Tag>> {
    let mut stmt = conn.prepare_cached("SELECT id, name FROM tags ORDER BY name_key, id")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?))
    })?;
    rows.map(|row| {
        let (id, name) = row?;
        Ok(Tag {
            id: id_from(&id)?,
            name: stored_name(&name)?,
        })
    })
    .collect()
}

pub(crate) fn insert_tag(cx: &mut Ctx<'_>, tag: &Tag) -> Result<()> {
    let conn = cx.conn;
    require_new(conn, "tags", tag.id)?;
    clash_check(conn, tag)?;
    conn.execute(
        "INSERT INTO tags (id, name, name_key) VALUES (?1, ?2, ?3)",
        params![id_bytes(tag.id), tag.name.as_str(), key(&tag.name)],
    )?;
    Ok(())
}

pub(crate) fn update_tag(cx: &mut Ctx<'_>, tag: &Tag) -> Result<()> {
    let conn = cx.conn;
    if !crate::codec::exists(conn, "tags", id_bytes(tag.id))? {
        return Err(not_found(tag.id));
    }
    clash_check(conn, tag)?;
    conn.execute(
        "UPDATE tags SET name = ?2, name_key = ?3 WHERE id = ?1",
        params![id_bytes(tag.id), tag.name.as_str(), key(&tag.name)],
    )?;
    Ok(())
}

pub(crate) fn delete_tag(cx: &mut Ctx<'_>, id: TagId) -> Result<()> {
    let deleted = cx
        .conn
        .execute("DELETE FROM tags WHERE id = ?1", [id_bytes(id)])?;
    if deleted == 0 {
        return Err(not_found(id));
    }
    Ok(())
}

/// Fails with [`Error::TagExists`] if another tag has the same name ignoring case.
fn clash_check(conn: &rusqlite::Connection, tag: &Tag) -> Result<()> {
    let clash = conn
        .query_row(
            "SELECT 1 FROM tags WHERE name_key = ?1 AND id <> ?2",
            params![key(&tag.name), id_bytes(tag.id)],
            |_| Ok(()),
        )
        .optional()?;
    if clash.is_some() {
        return Err(Error::TagExists(tag.name.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_fold_case_fully_and_normalise() {
        assert_eq!(name_key("Straße"), name_key("STRASSE"));
        assert_eq!(name_key("\u{c9}cole"), name_key("e\u{301}cole"));
        assert_eq!(name_key("\u{c9}COLE"), "\u{e9}cole", "stored composed");
        assert_eq!(name_key("ΣΟΣ"), name_key("σος"));
        assert_eq!(name_key("ﬁle"), name_key("FILE"));
        assert_ne!(name_key("Work"), name_key("Word"));
    }

    #[test]
    fn rekeying_keeps_tags_that_now_clash_and_parks_no_duplicate() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::schema::V1).unwrap();
        for (id, name, old) in [
            (1_u8, "Straße", "straße"),
            (2, "STRASSE", "strasse"),
            (3, "Work", "work"),
        ] {
            conn.execute(
                "INSERT INTO tags (id, name, name_key) VALUES (?1, ?2, ?3)",
                params![vec![id; 16], name, old],
            )
            .unwrap();
        }
        rekey(&conn).unwrap();
        let keys: Vec<String> = conn
            .prepare("SELECT name_key FROM tags ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(keys[0], "strasse");
        assert!(keys[1].starts_with("strasse\u{1}"), "{:?}", keys[1]);
        assert_eq!(keys[2], "work");
    }
}
