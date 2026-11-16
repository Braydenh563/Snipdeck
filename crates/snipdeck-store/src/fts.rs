//! The full-text index (`item_fts`): what goes in, and how a parsed query is answered.
//!
//! Since version 10 the index uses SQLite's `trigram` tokenizer over *folded* text
//! ([`snipdeck_core::search::fold`]: NFKC, lower case, Latin diacritics removed), the same
//! folding every search surface applies to the query. A trigram index finds any run of three
//! or more characters, so mid-word text ("ature" in "Signature") and a word inside a Japanese
//! or Chinese sentence are found. Shorter words are answered by `LIKE` when they are written in
//! a script without spaces (one or two CJK characters are a word); other short words are left
//! to the in-memory fields (see [`snipdeck_core::search::Term::searches_body`]).

use std::collections::HashSet;

use rusqlite::{Connection, params};
use snipdeck_core::search::{BodyMatches, Query, fold};
use snipdeck_core::{Content, Format, Formats, ItemId};

use crate::Result;
use crate::blobs::Blobs;
use crate::items::{JOIN_LIVE, WHERE_LIVE, load_formats};

/// The SQL that (re)creates the index table.
pub(crate) const CREATE: &str = "CREATE VIRTUAL TABLE item_fts USING fts5 (
    title,
    body,
    tokenize = 'trigram'
);
INSERT INTO item_fts (item_fts, rank) VALUES ('secure-delete', 1);";

/// `text` as a quoted FTS5 string: inside quotes every operator is ordinary text, and the only
/// character that could end the quote, `"`, is doubled, so no input is a syntax error.
fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\"\""))
}

/// `text` as a `LIKE` pattern that matches it anywhere, with `\\` escaping `%`, `_` and itself.
fn like_anywhere(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('%');
    for c in text.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// For each term of `query`, the live items whose title or body contains it: see the module
/// documentation for which terms are looked up.
pub(crate) fn body_matches(conn: &Connection, query: &Query) -> Result<BodyMatches> {
    let live = format!("JOIN items i ON i.seq = f.rowid {JOIN_LIVE} WHERE {WHERE_LIVE}");
    let mut out = Vec::with_capacity(query.terms.len());
    for term in &query.terms {
        if !term.searches_body() {
            out.push(None);
            continue;
        }
        let (sql, arg) = if term.text.chars().count() >= 3 {
            (
                format!("SELECT i.id FROM item_fts f {live} AND item_fts MATCH ?1"),
                quoted(&term.text),
            )
        } else {
            (
                format!(
                    "SELECT i.id FROM item_fts f {live} \
                     AND (f.title LIKE ?1 ESCAPE '\\' OR f.body LIKE ?1 ESCAPE '\\')"
                ),
                like_anywhere(&term.text),
            )
        };
        let mut stmt = conn.prepare_cached(&sql)?;
        let ids = stmt.query_map([arg], |r| r.get::<_, Vec<u8>>(0))?;
        let mut found = HashSet::new();
        for id in ids {
            found.insert(crate::codec::id_from::<ItemId>(&id?)?);
        }
        out.push(Some(found));
    }
    Ok(BodyMatches::new(out))
}

/// The text an item is searched by: its `text` format. Other formats are not indexed (HTML and
/// RTF would add markup noise); an item without text is found by its title.
fn body_of(blobs: &Blobs, formats: &Formats) -> Result<String> {
    match formats.iter().find_map(|f| match f {
        Format::Text(content) => Some(content),
        _ => None,
    }) {
        Some(content) => text_of(blobs, content),
        None => Ok(String::new()),
    }
}

fn text_of(blobs: &Blobs, content: &Content) -> Result<String> {
    Ok(String::from_utf8_lossy(&blobs.read_any(content)?).into_owned())
}

/// Writes the item's row in the index, replacing any earlier one. A sensitive item is indexed
/// by title only.
pub(crate) fn index(
    conn: &Connection,
    blobs: &Blobs,
    seq: i64,
    title: &str,
    sensitive: bool,
    formats: &Formats,
) -> Result<()> {
    let body = if sensitive {
        String::new()
    } else {
        body_of(blobs, formats)?
    };
    write_row(conn, seq, title, &body)
}

fn write_row(conn: &Connection, seq: i64, title: &str, body: &str) -> Result<()> {
    conn.execute("DELETE FROM item_fts WHERE rowid = ?1", [seq])?;
    conn.execute(
        "INSERT INTO item_fts (rowid, title, body) VALUES (?1, ?2, ?3)",
        params![seq, fold(title), fold(body)],
    )?;
    Ok(())
}

/// Version 10: rebuilds the index with the trigram tokenizer over folded text. The old rows
/// hold the text that was indexed (a sensitive item's body is already empty), so no content is
/// read: each row is folded and written again.
pub(crate) fn rebuild_v10(conn: &Connection) -> Result<()> {
    let rows: Vec<(i64, String, String)> = conn
        .prepare("SELECT rowid, title, body FROM item_fts")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    conn.execute_batch(&format!("DROP TABLE item_fts; {CREATE}"))?;
    for (seq, title, body) in rows {
        write_row(conn, seq, &title, &body)?;
    }
    Ok(())
}

/// How many rows hold a title that is not the folded title of their item.
pub(crate) fn stale_titles(conn: &Connection) -> Result<usize> {
    let mut stmt =
        conn.prepare("SELECT f.title, i.title FROM item_fts f JOIN items i ON i.seq = f.rowid")?;
    let mut stale = 0;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (indexed, title) = row?;
        stale += usize::from(indexed != fold(&title));
    }
    Ok(stale)
}

/// Changes only the title of the item's row: the body stays as indexed, and the content is
/// not read (review L11).
pub(crate) fn retitle(conn: &Connection, seq: i64, title: &str) -> Result<()> {
    conn.execute(
        "UPDATE item_fts SET title = ?2 WHERE rowid = ?1",
        params![seq, fold(title)],
    )?;
    Ok(())
}

/// Rebuilds the item's row from what is stored, for edits that did not touch the content.
///
/// A content blob that cannot be read leaves the body empty rather than blocking the edit;
/// the next edit that supplies content repairs the entry.
pub(crate) fn reindex(conn: &Connection, blobs: &Blobs, id: [u8; 16]) -> Result<()> {
    let (seq, title, sensitive): (i64, String, bool) = conn.query_row(
        "SELECT seq, title, sensitive FROM items WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let body = if sensitive {
        String::new()
    } else {
        body_of(blobs, &load_formats(conn, id)?).unwrap_or_default()
    };
    write_row(conn, seq, &title, &body)
}

#[cfg(test)]
mod tests {
    use super::{like_anywhere, quoted};

    #[test]
    fn quotes_are_doubled_so_operators_stay_text() {
        assert_eq!(quoted("a\"b"), "\"a\"\"b\"");
        assert_eq!(quoted("NEAR(a b)"), "\"NEAR(a b)\"");
    }

    #[test]
    fn like_patterns_escape_their_wildcards() {
        assert_eq!(like_anywhere("日本"), "%日本%");
        assert_eq!(like_anywhere("a%_\\"), "%a\\%\\_\\\\%");
    }
}
