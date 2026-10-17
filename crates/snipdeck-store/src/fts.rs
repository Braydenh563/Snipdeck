//! The full-text index (`item_fts`): what goes in, and how user input becomes a query.

use rusqlite::{Connection, params};
use snipdeck_core::{Content, Format, Formats};

use crate::Result;
use crate::blobs::Blobs;
use crate::items::load_formats;

/// The most words of a query that are used; the rest are ignored.
const MAX_TERMS: usize = 32;

/// Turns what the user typed into an FTS5 `MATCH` expression, or `None` if there is nothing to
/// search for.
///
/// Every word becomes a quoted phrase with a prefix star (`"quart"*`), all of them required.
/// Inside quotes FTS5 treats operators (`AND`, `NEAR`, `title:`, `-`, `*`) as ordinary text,
/// and the only character that could end the quote, `"`, is doubled, so no input can produce
/// a syntax error. Words without a letter or digit would tokenise to nothing and are dropped.
pub(crate) fn match_expression(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .take(MAX_TERMS)
        .map(|word| {
            let clean: String = word
                .chars()
                .filter(|c| !c.is_control())
                .map(|c| {
                    if c == '"' {
                        "\"\"".to_owned()
                    } else {
                        c.to_string()
                    }
                })
                .collect();
            format!("\"{clean}\"*")
        })
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
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
    Ok(String::from_utf8_lossy(&blobs.read(content)?).into_owned())
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
        params![seq, title, body],
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
    use super::match_expression;

    #[test]
    fn words_become_quoted_prefix_phrases() {
        assert_eq!(
            match_expression("quart rep").as_deref(),
            Some("\"quart\"* \"rep\"*")
        );
    }

    #[test]
    fn quotes_are_doubled_and_operators_stay_inside_them() {
        assert_eq!(match_expression("a\"b").as_deref(), Some("\"a\"\"b\"*"));
        assert_eq!(
            match_expression("NEAR(a b)").as_deref(),
            Some("\"NEAR(a\"* \"b)\"*")
        );
        assert_eq!(match_expression("title:x").as_deref(), Some("\"title:x\"*"));
    }

    #[test]
    fn punctuation_only_input_is_nothing() {
        for q in ["", "  ", "-", "\"\"", "* ^ ()"] {
            assert_eq!(match_expression(q), None, "{q:?}");
        }
    }

    #[test]
    fn control_characters_are_dropped_and_length_is_capped() {
        assert_eq!(match_expression("a\0b").as_deref(), Some("\"ab\"*"));
        let many = "w ".repeat(100);
        assert_eq!(match_expression(&many).unwrap().matches('*').count(), 32);
    }
}
