//! Importing text clips from a Ditto database (`Data.db`).
//!
//! # The schema this reads
//!
//! Ditto (<https://github.com/sabrogden/Ditto>, `DatabaseUtilities.cpp`, `CreateDB`) keeps one
//! SQLite file with two tables that matter here:
//!
//! ```text
//! Main(lID INTEGER PRIMARY KEY, lDate INTEGER, mText TEXT, lShortCut INTEGER,
//!      lDontAutoDelete INTEGER, CRC INTEGER, bIsGroup INTEGER, lParentID INTEGER,
//!      QuickPasteText TEXT, clipOrder REAL, clipGroupOrder REAL, globalShortCut INTEGER,
//!      lastPasteDate INTEGER, stickyClipOrder REAL, ...)
//! Data(lID INTEGER PRIMARY KEY, lParentID INTEGER, strClipBoardFormat TEXT, ooData BLOB)
//! ```
//!
//! `Main` has one row per clip (or per group, `bIsGroup = 1`, whose `mText` is the group name);
//! `Data` has one row per clipboard format of a clip, `Data.lParentID = Main.lID`. `lDate` and
//! `lastPasteDate` are Unix seconds. `lDontAutoDelete` is the "keep forever" star. `lParentID`
//! is a group's `lID`, or negative for a clip in no group. `ooData` for `CF_UNICODETEXT` is
//! UTF-16LE with a terminating NUL; `CF_TEXT` is the ANSI code page with a NUL.
//!
//! # What is imported and what is not
//!
//! Imported: text clips. The full text comes from the `CF_UNICODETEXT` row (falling back to
//! `CF_TEXT`, then to `Main.mText`, which Ditto truncates to its description length). Groups
//! become lists (nested groups are named `Outer / Inner`); clips outside any group go to a list
//! named `Ditto history`. Starred clips (`lDontAutoDelete`) are pinned. `lDate` is the creation
//! time and `lastPasteDate` the last use. Order within a list is newest first.
//!
//! Skipped, and reported as one [`Warning::NonTextSkipped`]: clips with no text format (images,
//! file lists, ...). Not read at all: other formats of a text clip (HTML, RTF), shortcuts, quick
//! paste text, sticky ordering, and Ditto's own export files.
//!
//! The file is opened read-only, with triggers and untrusted-schema functions disabled; it is
//! never modified. `Main` and `Data` must be ordinary tables (no views, virtual tables or
//! computed columns, which could expand a small file without bound), values over 10 MB are
//! never loaded, and what is held is counted against [`MEMORY_BUDGET`] as rows are read.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::limits::Limit;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags, OptionalExtension, Row};
use snipdeck_core::Timestamp;

use crate::error::{Error, Result};
use crate::import::{ImportOptions, board_name, finish};
use crate::limits::TEXT_LIMIT;
use crate::plan::{Library, Plan, PlanBoard, PlanItem, PlanList};
use crate::report::{ImportReport, Warning};

use super::{Decoded, MEMORY_BUDGET, clean_label, decode_text, note_lossy, title_from};

/// Required columns, by table. Anything else Ditto added over the years is ignored.
const REQUIRED: [(&str, &[&str]); 2] = [
    (
        "Main",
        &[
            "lID",
            "lDate",
            "mText",
            "lDontAutoDelete",
            "bIsGroup",
            "lParentID",
        ],
    ),
    ("Data", &["lParentID", "strClipBoardFormat", "ooData"]),
];

/// The most bytes of one value read from the file. A text clip can be 5 MB of UTF-8, so up to
/// twice that (UTF-16) plus the terminator is read; anything longer is not loaded at all
/// (`length()` is asked first) and the clip is reported as too large.
const VALUE_LIMIT: u64 = 2 * TEXT_LIMIT + 4;

/// [`VALUE_LIMIT`] as an SQL parameter.
fn value_limit() -> i64 {
    i64::try_from(VALUE_LIMIT).unwrap_or(i64::MAX)
}

/// SQLite's own cap on a value or row ([`Limit::SQLITE_LIMIT_LENGTH`]), set on the connection as
/// a backstop. It is far above [`VALUE_LIMIT`] so that a legitimately huge clip is skipped with
/// a warning (its row must still be readable to find that out) instead of failing the import.
const ROW_LIMIT: i32 = 64 * 1024 * 1024;

/// Nested groups deeper than this are not followed (and a cycle in `lParentID` cannot loop).
const MAX_GROUP_DEPTH: usize = 8;

/// Imports the text clips of the Ditto database at `db_path` into a new board (named
/// `Imported from Ditto` unless the options say otherwise).
///
/// # Errors
///
/// [`Error::File`] if the file does not exist, [`Error::NotDitto`] if it is not a Ditto
/// database, [`Error::Sqlite`] for a damaged one, and storage errors.
pub fn import_ditto<L: Library>(
    lib: &mut L,
    db_path: &Path,
    now: Timestamp,
    options: &ImportOptions<'_>,
) -> Result<ImportReport> {
    let conn = open_read_only(db_path)?;
    check_schema(&conn, db_path)?;

    let mut report = ImportReport::default();
    let groups = read_groups(&conn)?;
    let mut clips = read_clips(&conn, options.limits.max_items, MEMORY_BUDGET, &mut report)?;
    fill_texts(&conn, &mut clips, MEMORY_BUDGET, &mut report)?;

    let mut by_list: HashMap<Option<i64>, Vec<&Clip>> = HashMap::new();
    let mut non_text = 0_usize;
    let mut total_bytes = 0_u64;
    let mut truncated = false;
    for clip in &clips {
        if let Some(size) = clip.too_large {
            report.items_skipped += 1;
            report.warn(Warning::TooLarge {
                name: format!("Ditto clip {}", clip.id),
                size,
            });
            continue;
        }
        let Some(text) = clip.text.as_deref() else {
            non_text += 1;
            continue;
        };
        if text.trim().is_empty() {
            report.items_skipped += 1;
            continue;
        }
        if text.len() as u64 > TEXT_LIMIT {
            report.items_skipped += 1;
            report.warn(Warning::TooLarge {
                name: format!("Ditto clip {}", clip.id),
                size: text.len() as u64,
            });
            continue;
        }
        total_bytes += text.len() as u64;
        if total_bytes > MEMORY_BUDGET {
            truncated = true;
            break;
        }
        let list = groups.contains_key(&clip.parent).then_some(clip.parent);
        by_list.entry(list).or_default().push(clip);
    }
    if truncated {
        report.warn(Warning::Truncated {
            reason: "reached the import memory limit".to_owned(),
        });
    }
    if non_text > 0 {
        report.items_skipped += non_text;
        report.warn(Warning::NonTextSkipped { count: non_text });
    }

    let mut lists = Vec::new();
    // Clips in no group first, then groups in id order (Ditto's creation order).
    let mut keys: Vec<Option<i64>> = by_list.keys().copied().collect();
    keys.sort_by_key(|k| (k.is_some(), *k));
    for key in keys {
        let mut members = by_list.remove(&key).unwrap_or_default();
        // Newest first, ties by id.
        members.sort_by_key(|c| std::cmp::Reverse((c.date, c.id)));
        let name = match key {
            None => "Ditto history".to_owned(),
            Some(id) => group_path(&groups, id),
        };
        let mut list = PlanList::new(board_name(&name, "Ditto group")?, now);
        list.items = members.into_iter().map(|c| plan_item(c, now)).collect();
        lists.push(list);
    }

    let mut plan = Plan::default();
    if !lists.is_empty() {
        plan.boards.push(PlanBoard {
            name: board_name(
                options
                    .board_name
                    .as_deref()
                    .unwrap_or("Imported from Ditto"),
                "Imported from Ditto",
            )?,
            created_at: now,
            updated_at: now,
            lists,
        });
    }
    finish(lib, plan, options, report)
}

fn open_read_only(path: &Path) -> Result<Connection> {
    if !path.is_file() {
        return Err(Error::File {
            path: path.to_owned(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"),
        });
    }
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| Error::NotDitto {
        path: path.to_owned(),
        reason: e.to_string(),
    })?;
    // A database from outside is untrusted: no functions or virtual tables from its schema,
    // no triggers, and no writes even by accident.
    conn.set_db_config(
        rusqlite::config::DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA,
        false,
    )?;
    conn.set_db_config(
        rusqlite::config::DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER,
        false,
    )?;
    conn.set_limit(Limit::SQLITE_LIMIT_LENGTH, ROW_LIMIT)?;
    conn.pragma_update(None, "query_only", true)?;
    conn.busy_timeout(std::time::Duration::from_secs(2))?;
    Ok(conn)
}

fn check_schema(conn: &Connection, path: &Path) -> Result<()> {
    let not_ditto = |reason: String| Error::NotDitto {
        path: path.to_owned(),
        reason,
    };
    for (table, required) in REQUIRED {
        // Only real tables: a view, a virtual table or a generated column computes its rows
        // from a few bytes of schema, so a small file could expand without bound.
        let (kind, sql): (String, Option<String>) = conn
            .query_row(
                "SELECT type, sql FROM sqlite_master
                 WHERE lower(name) = lower(?1) AND type IN ('table', 'view')",
                [table],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| not_ditto(e.to_string()))?
            .ok_or_else(|| not_ditto(format!("there is no `{table}` table")))?;
        if kind != "table" {
            return Err(not_ditto(format!("`{table}` is a {kind}, not a table")));
        }
        if sql.is_some_and(|sql| {
            sql.trim_start()
                .get(..14)
                .is_some_and(|head| head.eq_ignore_ascii_case("CREATE VIRTUAL"))
        }) {
            return Err(not_ditto(format!(
                "`{table}` is a virtual table, not a table"
            )));
        }
        // Table names are literals above; PRAGMA cannot take a parameter.
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_xinfo({table})"))
            .map_err(|e| not_ditto(e.to_string()))?;
        // (name, hidden): `hidden` is 0 for an ordinary column, 2 or 3 for a generated one.
        let columns: Vec<(String, i64)> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, i64>(6)?)))
            .map_err(|e| not_ditto(e.to_string()))?
            .collect::<core::result::Result<_, _>>()
            .map_err(|e| not_ditto(e.to_string()))?;
        if columns.is_empty() {
            return Err(not_ditto(format!("there is no `{table}` table")));
        }
        for column in required {
            let Some((_, hidden)) = columns.iter().find(|(c, _)| c.eq_ignore_ascii_case(column))
            else {
                return Err(not_ditto(format!("`{table}` has no `{column}` column")));
            };
            if *hidden != 0 {
                return Err(not_ditto(format!(
                    "`{table}.{column}` is a computed column"
                )));
            }
        }
    }
    Ok(())
}

struct Clip {
    id: i64,
    date: i64,
    parent: i64,
    pinned: bool,
    last_used: i64,
    text: Option<String>,
    unicode: bool,
    /// The size of a text value over [`VALUE_LIMIT`], which was not loaded.
    too_large: Option<u64>,
}

/// An integer-valued column, tolerating real numbers and NULL (SQLite columns are loosely typed
/// and the file is untrusted).
fn int_at(row: &Row<'_>, index: usize) -> Option<i64> {
    match row.get_ref(index).ok()? {
        ValueRef::Integer(n) => Some(n),
        #[expect(
            clippy::cast_possible_truncation,
            reason = "saturating float cast is intended"
        )]
        ValueRef::Real(x) if x.is_finite() => Some(x as i64),
        _ => None,
    }
}

fn bytes_at(row: &Row<'_>, index: usize) -> Option<Vec<u8>> {
    match row.get_ref(index).ok()? {
        ValueRef::Text(b) | ValueRef::Blob(b) => Some(b.to_vec()),
        _ => None,
    }
}

fn read_groups(conn: &Connection) -> Result<HashMap<i64, (String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT lID, mText, lParentID FROM Main WHERE bIsGroup <> 0 AND bIsGroup IS NOT NULL",
    )?;
    let mut rows = stmt.query([])?;
    let mut groups = HashMap::new();
    while let Some(row) = rows.next()? {
        let Some(id) = int_at(row, 0) else { continue };
        let name = bytes_at(row, 1)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        groups.insert(id, (clean_label(&name, 60), int_at(row, 2).unwrap_or(-1)));
    }
    Ok(groups)
}

/// `Outer / Inner` for a group, following `lParentID` at most [`MAX_GROUP_DEPTH`] levels.
fn group_path(groups: &HashMap<i64, (String, i64)>, id: i64) -> String {
    let mut parts = Vec::new();
    let mut seen = Vec::new();
    let mut cursor = Some(id);
    while let Some(current) = cursor {
        // A cycle in `lParentID` (or a very deep tree) ends the path instead of looping.
        if parts.len() >= MAX_GROUP_DEPTH || seen.contains(&current) {
            break;
        }
        seen.push(current);
        let Some((name, parent)) = groups.get(&current) else {
            break;
        };
        parts.push(if name.is_empty() {
            "Group"
        } else {
            name.as_str()
        });
        cursor = groups.contains_key(parent).then_some(*parent);
    }
    parts.reverse();
    parts.join(" / ")
}

fn read_clips(
    conn: &Connection,
    max: usize,
    budget: u64,
    report: &mut ImportReport,
) -> Result<Vec<Clip>> {
    // `lastPasteDate` exists from Ditto 3.x on; older files get NULL.
    let has_last_paste = conn
        .prepare("SELECT 1 FROM pragma_table_info('Main') WHERE name = 'lastPasteDate'")?
        .exists([])?;
    let last_paste = if has_last_paste {
        "lastPasteDate"
    } else {
        "NULL"
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT lID, lDate, CASE WHEN length(mText) > ?1 THEN NULL ELSE mText END,
                lDontAutoDelete, lParentID, {last_paste}, length(mText)
         FROM Main WHERE bIsGroup = 0 OR bIsGroup IS NULL ORDER BY lID DESC"
    ))?;
    let mut rows = stmt.query([value_limit()])?;
    let mut clips = Vec::new();
    let mut held = 0_u64;
    while let Some(row) = rows.next()? {
        if clips.len() >= max {
            report.warn(Warning::Truncated {
                reason: format!("only the newest {max} clips were read"),
            });
            break;
        }
        let Some(id) = int_at(row, 0) else { continue };
        let too_large = int_at(row, 6)
            .and_then(|n| u64::try_from(n).ok())
            .filter(|n| *n > VALUE_LIMIT);
        let text = bytes_at(row, 2)
            .filter(|b| !b.is_empty())
            .map(|b| decode_text(&b).0);
        held += text.as_ref().map_or(0, |t| t.len() as u64);
        if held > budget {
            report.warn(Warning::Truncated {
                reason: "reached the import memory limit".to_owned(),
            });
            break;
        }
        clips.push(Clip {
            id,
            date: int_at(row, 1).unwrap_or(0),
            parent: int_at(row, 4).unwrap_or(-1),
            pinned: int_at(row, 3).unwrap_or(0) != 0,
            last_used: int_at(row, 5).unwrap_or(0),
            text,
            unicode: false,
            too_large,
        });
    }
    Ok(clips)
}

/// Replaces each clip's `mText` (possibly truncated by Ditto) with the full text of its
/// `CF_UNICODETEXT` row, or `CF_TEXT` if there is none.
fn fill_texts(
    conn: &Connection,
    clips: &mut [Clip],
    budget: u64,
    report: &mut ImportReport,
) -> Result<()> {
    let index: HashMap<i64, usize> = clips.iter().enumerate().map(|(i, c)| (c.id, i)).collect();
    let mut stmt = conn.prepare(
        "SELECT lParentID, strClipBoardFormat,
                CASE WHEN length(ooData) > ?1 THEN NULL ELSE ooData END, length(ooData)
         FROM Data WHERE strClipBoardFormat IN ('CF_UNICODETEXT', 'CF_TEXT')",
    )?;
    let mut rows = stmt.query([value_limit()])?;
    let mut held: u64 = clips
        .iter()
        .map(|c| c.text.as_ref().map_or(0, |t| t.len() as u64))
        .sum();
    while let Some(row) = rows.next()? {
        let Some(&i) = int_at(row, 0).and_then(|id| index.get(&id)) else {
            continue;
        };
        let format = bytes_at(row, 1).unwrap_or_default();
        let size = int_at(row, 3).and_then(|n| u64::try_from(n).ok());
        if size.is_some_and(|n| n > VALUE_LIMIT) {
            // Too big to hold, and never loaded: the clip is reported by its real size, unless
            // better text is (or comes) from another row.
            if !clips[i].unicode {
                clips[i].too_large = size;
            }
            continue;
        }
        let Some(mut data) = bytes_at(row, 2) else {
            continue;
        };
        let unicode = format == b"CF_UNICODETEXT";
        if clips[i].unicode {
            continue;
        }
        // Both formats end with a terminating NUL (a wide one for Unicode).
        let (text, how) = if unicode {
            while data.len() >= 2 && data[data.len() - 2..] == [0, 0] {
                data.truncate(data.len() - 2);
            }
            utf16le(&data)
        } else {
            while data.last() == Some(&0) {
                data.pop();
            }
            decode_text(&data)
        };
        note_lossy(how, &format!("Ditto clip {}", clips[i].id), report);
        held = held - clips[i].text.as_ref().map_or(0, |t| t.len() as u64) + text.len() as u64;
        if held > budget {
            report.warn(Warning::Truncated {
                reason: "reached the import memory limit".to_owned(),
            });
            break;
        }
        clips[i].text = Some(text);
        clips[i].too_large = None;
        clips[i].unicode = unicode;
    }
    Ok(())
}

fn utf16le(data: &[u8]) -> (String, Decoded) {
    let units: Vec<u16> = data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| u16::from_le_bytes(*p))
        .collect();
    let exact = data.len().is_multiple_of(2) && String::from_utf16(&units).is_ok();
    (
        String::from_utf16_lossy(&units),
        if exact {
            Decoded::Utf16
        } else {
            Decoded::Lossy
        },
    )
}

fn plan_item(clip: &Clip, now: Timestamp) -> PlanItem {
    let text = clip.text.clone().unwrap_or_default();
    let created = Timestamp::from_second(clip.date).unwrap_or(now);
    // Storage holds 1677 to 2262; `from_second` accepts a wider range.
    let created = if i64::try_from(created.as_nanosecond()).is_ok() {
        created
    } else {
        now
    };
    let mut item = PlanItem::text(title_from(&text), text, created);
    item.pinned = clip.pinned;
    if clip.last_used > 0 {
        item.last_used_at = Timestamp::from_second(clip.last_used)
            .ok()
            .filter(|t| i64::try_from(t.as_nanosecond()).is_ok());
    }
    item
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database(clips: usize, text: &str) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE Main(lID INTEGER PRIMARY KEY, lDate INTEGER, mText TEXT,
                lDontAutoDelete INTEGER, bIsGroup INTEGER, lParentID INTEGER);
             CREATE TABLE Data(lID INTEGER PRIMARY KEY, lParentID INTEGER,
                strClipBoardFormat TEXT, ooData BLOB);",
        )
        .unwrap();
        for id in 1..=clips {
            conn.execute(
                "INSERT INTO Main (lID, lDate, mText, bIsGroup, lParentID) VALUES (?1, 0, ?2, 0, -1)",
                rusqlite::params![i64::try_from(id).unwrap(), text],
            )
            .unwrap();
        }
        conn
    }

    /// Security review 2, finding 3: rows are counted against the memory budget as they are
    /// read, not after they are all held.
    #[test]
    fn reading_descriptions_stops_at_the_memory_budget() {
        let conn = database(10, &"x".repeat(1000));
        let mut report = ImportReport::default();
        let clips = read_clips(&conn, 100, 3500, &mut report).unwrap();
        assert_eq!(clips.len(), 3);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::Truncated { .. })),
            "{:?}",
            report.warnings
        );
        let mut report = ImportReport::default();
        let clips = read_clips(&conn, 100, 10_000, &mut report).unwrap();
        assert_eq!((clips.len(), report.warnings.len()), (10, 0));
    }

    #[test]
    fn reading_full_texts_stops_at_the_memory_budget() {
        let conn = database(10, "short");
        for id in 1..=10_i64 {
            conn.execute(
                "INSERT INTO Data (lParentID, strClipBoardFormat, ooData) VALUES (?1, 'CF_TEXT', ?2)",
                rusqlite::params![id, vec![b'y'; 1000]],
            )
            .unwrap();
        }
        let mut report = ImportReport::default();
        let mut clips = read_clips(&conn, 100, u64::MAX, &mut report).unwrap();
        fill_texts(&conn, &mut clips, 3500, &mut report).unwrap();
        let full = clips
            .iter()
            .filter(|c| c.text.as_deref().is_some_and(|t| t.len() == 1000))
            .count();
        assert!((1..=3).contains(&full), "{full} clips got their full text");
        assert!(
            report
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::Truncated { .. })),
            "{:?}",
            report.warnings
        );
    }
}
