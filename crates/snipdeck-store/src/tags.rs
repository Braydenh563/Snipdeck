//! [`TagRepo`] for [`Store`].
//!
//! Uniqueness is on `name_key`, the name lower-cased by Rust's full Unicode `to_lowercase`
//! (SQLite's own `NOCASE` folds ASCII only, so `École` and `école` would be different tags).

use rusqlite::{OptionalExtension, params};
use snipdeck_core::{Name, Tag, TagId};

use crate::codec::{id_bytes, id_from, not_found, require_new};
use crate::store::begin;
use crate::{Error, Result, Store, TagRepo};

fn key(name: &Name) -> String {
    name.as_str().to_lowercase()
}

impl TagRepo for Store {
    fn tags(&self) -> Result<Vec<Tag>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT id, name FROM tags ORDER BY name_key, id")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?))
        })?;
        rows.map(|row| {
            let (id, name) = row?;
            Ok(Tag {
                id: id_from(&id)?,
                name: Name::new(&name)?,
            })
        })
        .collect()
    }

    fn insert_tag(&mut self, tag: &Tag) -> Result<()> {
        let tx = begin(&mut self.conn)?;
        require_new(&tx, "tags", tag.id)?;
        clash_check(&tx, tag)?;
        tx.execute(
            "INSERT INTO tags (id, name, name_key) VALUES (?1, ?2, ?3)",
            params![id_bytes(tag.id), tag.name.as_str(), key(&tag.name)],
        )?;
        Ok(tx.commit()?)
    }

    fn update_tag(&mut self, tag: &Tag) -> Result<()> {
        let tx = begin(&mut self.conn)?;
        if !crate::codec::exists(&tx, "tags", id_bytes(tag.id))? {
            return Err(not_found(tag.id));
        }
        clash_check(&tx, tag)?;
        tx.execute(
            "UPDATE tags SET name = ?2, name_key = ?3 WHERE id = ?1",
            params![id_bytes(tag.id), tag.name.as_str(), key(&tag.name)],
        )?;
        Ok(tx.commit()?)
    }

    fn delete_tag(&mut self, id: TagId) -> Result<()> {
        let tx = begin(&mut self.conn)?;
        let deleted = tx.execute("DELETE FROM tags WHERE id = ?1", [id_bytes(id)])?;
        if deleted == 0 {
            return Err(not_found(id));
        }
        Ok(tx.commit()?)
    }
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
