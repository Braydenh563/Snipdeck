//! The folder, Ditto and `CopyQ` importers.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]
#![allow(
    unknown_lints,
    clippy::assert_is_empty,
    reason = "`assert!(x.is_empty())` reads fine; the lint is newer than our pinned toolchain"
)]

mod common;

use std::fs;
use std::path::Path;

use common::*;
use rusqlite::Connection;
use snipdeck_portable::{
    Error, HistoryEntry, ImportOptions, MergeStrategy, Warning, import_copyq_json, import_ditto,
    import_entries, import_folder,
};
use snipdeck_store::{BlobStore, BoardRepo, ItemRepo, ListRepo, Store, TagRepo};

fn write(dir: &Path, name: &str, bytes: impl AsRef<[u8]>) {
    let path = dir.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

/// (title, text) of one item.
type Entry = (String, String);

/// (board, list, items) for everything in the library.
type Contents = Vec<(String, String, Vec<Entry>)>;

fn contents(store: &Store) -> Contents {
    let mut out = Vec::new();
    for board in store.boards().unwrap() {
        for list in store.lists(board.id).unwrap() {
            let items = store
                .items(list.id)
                .unwrap()
                .into_iter()
                .map(|item| {
                    let formats = store.formats(item.id).unwrap();
                    let snipdeck_core::Format::Text(c) = formats.primary() else {
                        panic!("not text")
                    };
                    (
                        item.title,
                        String::from_utf8(store.read(c).unwrap().into_owned()).unwrap(),
                    )
                })
                .collect();
            out.push((board.name.to_string(), list.name.to_string(), items));
        }
    }
    out
}

// ---------------------------------------------------------------- folder

fn import_dir(dir: &Path) -> (Store, snipdeck_portable::ImportReport) {
    let mut s = store();
    let report = import_folder(&mut s, dir, at(9), &ImportOptions::default()).unwrap();
    assert_healthy(&s);
    (s, report)
}

#[test]
fn folder_imports_txt_and_md_one_item_each() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("My Notes");
    write(&root, "beta.md", "# Beta\nbody");
    write(&root, "Alpha.txt", "alpha text\r\nsecond line");
    write(&root, "gamma.TXT", "upper-case extension");
    write(&root, "ignored.rs", "fn main() {}");
    write(&root, "noext", "no extension");
    write(&root, ".hidden.txt", "hidden");
    write(&root, "sub/inner.md", "inner note");
    write(&root, "sub/deep/too-deep.txt", "not visited");
    write(&root, "empty-dir/readme.rs", "nothing importable");

    let (s, report) = import_dir(&root);
    assert_eq!(
        contents(&s),
        [
            (
                "My Notes".to_owned(),
                "My Notes".to_owned(),
                vec![
                    ("Alpha".to_owned(), "alpha text\r\nsecond line".to_owned()),
                    ("beta".to_owned(), "# Beta\nbody".to_owned()),
                    ("gamma".to_owned(), "upper-case extension".to_owned()),
                ]
            ),
            (
                "My Notes".to_owned(),
                "sub".to_owned(),
                vec![("inner".to_owned(), "inner note".to_owned())]
            ),
        ]
    );
    assert_eq!(
        (report.boards_added, report.lists_added, report.items_added),
        (1, 2, 4)
    );
    assert_eq!(report.items_skipped, 0);
    assert!(!report.has_warnings(), "{:?}", report.warnings);
}

#[test]
fn folder_decodes_boms_and_replaces_bad_bytes_with_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "bom.txt", b"\xEF\xBB\xBFwith bom");
    let utf16: Vec<u8> = [0xFF, 0xFE]
        .into_iter()
        .chain("wide é😀".encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    write(dir.path(), "utf16.txt", utf16);
    write(dir.path(), "latin1.txt", b"caf\xE9 au lait");
    let (s, report) = import_dir(dir.path());
    let items = &contents(&s)[0].2;
    assert_eq!(items[0], ("bom".to_owned(), "with bom".to_owned()));
    assert_eq!(
        items[1],
        ("latin1".to_owned(), "caf\u{fffd} au lait".to_owned())
    );
    assert_eq!(items[2], ("utf16".to_owned(), "wide é😀".to_owned()));
    assert_eq!(
        report.warnings,
        [Warning::LossyText {
            name: "latin1.txt".to_owned()
        }]
    );
}

#[test]
fn folder_skips_empty_binary_and_oversized_files() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "empty.txt", "");
    write(dir.path(), "blank.txt", " \n\t\n");
    write(dir.path(), "binary.txt", b"abc\0def");
    let limit = 5 * 1024 * 1024;
    write(dir.path(), "exactly.txt", vec![b'a'; limit]);
    write(dir.path(), "toobig.txt", vec![b'a'; limit + 1]);
    write(dir.path(), "fine.txt", "fine");
    let (s, report) = import_dir(dir.path());
    let titles: Vec<_> = contents(&s)[0].2.iter().map(|i| i.0.clone()).collect();
    assert_eq!(titles, ["exactly", "fine"]);
    assert_eq!(report.items_added, 2);
    assert_eq!(report.items_skipped, 4);
    let kinds: Vec<&str> = report
        .warnings
        .iter()
        .map(|w| match w {
            Warning::Empty { .. } => "empty",
            Warning::Unreadable { .. } => "unreadable",
            Warning::TooLarge { .. } => "too large",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds.iter().filter(|k| **k == "empty").count(), 2);
    assert!(kinds.contains(&"unreadable") && kinds.contains(&"too large"));
}

/// Wave 3c review: duplicates are counted apart from the other skips.
#[test]
fn duplicates_are_counted_apart_from_other_skips() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", "same");
    write(dir.path(), "empty.txt", "");
    let mut s = store();
    let opts = ImportOptions {
        merge: MergeStrategy::SkipDuplicates,
        ..ImportOptions::default()
    };
    let first = import_folder(&mut s, dir.path(), at(1), &opts).unwrap();
    assert_eq!((first.duplicates_skipped, first.items_skipped), (0, 1));
    let second = import_folder(&mut s, dir.path(), at(2), &opts).unwrap();
    assert_eq!((second.duplicates_skipped, second.items_skipped), (1, 2));
}

/// Security review 2, finding 6: the 5 MB limit is on the text that gets stored, not on the
/// bytes in the file. Bad bytes become three-byte U+FFFD, and UTF-16 can grow by half.
#[test]
fn folder_checks_the_size_of_the_decoded_text() {
    let dir = tempfile::tempdir().unwrap();
    // 2 MB of bytes that are not UTF-8 decode to 6 MB of replacement characters.
    write(dir.path(), "bad-bytes.txt", vec![0x80_u8; 2_000_000]);
    // 4 MB of UTF-16 (2 M characters of CJK) is 6 MB as UTF-8.
    let mut wide = vec![0xFF, 0xFE];
    wide.extend(std::iter::repeat_n([0x2d_u8, 0x4e], 2_000_000).flatten());
    write(dir.path(), "wide.txt", wide);
    write(dir.path(), "fine.txt", "just a note");

    let mut s = store();
    let report = import_folder(&mut s, dir.path(), at(1), &ImportOptions::default()).unwrap();
    assert_eq!(report.items_added, 1, "{report:?}");
    let mut too_large: Vec<_> = report
        .warnings
        .iter()
        .filter_map(|w| match w {
            Warning::TooLarge { name, size } => Some((name.as_str(), *size)),
            _ => None,
        })
        .collect();
    too_large.sort_unstable();
    assert_eq!(
        too_large,
        [("bad-bytes.txt", 6_000_000), ("wide.txt", 6_000_000)]
    );
}

#[test]
fn folder_titles_are_cleaned_file_names() {
    let dir = tempfile::tempdir().unwrap();
    if fs::write(dir.path().join("two\nlines  here.txt"), "x").is_err() {
        return; // the file system refuses control characters in names
    }
    let (s, _) = import_dir(dir.path());
    assert_eq!(contents(&s)[0].2[0].0, "two lines here");
}

#[cfg(unix)]
#[test]
fn folder_does_not_follow_symbolic_links_or_open_special_files() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let secret = dir.path().join("outside-secret.txt");
    fs::write(&secret, "TOP SECRET").unwrap();
    let root = dir.path().join("root");
    write(&root, "real.txt", "real");
    symlink(&secret, root.join("link.txt")).unwrap();
    symlink(dir.path(), root.join("linked-dir")).unwrap();
    // A named pipe would block forever if opened for reading.
    let fifo = root.join("pipe.txt");
    let made = std::process::Command::new("mkfifo").arg(&fifo).status();

    let (s, report) = import_dir(&root);
    let all = contents(&s);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].2, [("real".to_owned(), "real".to_owned())]);
    assert!(!format!("{all:?}").contains("TOP SECRET"));
    assert!(report.warnings.contains(&Warning::SymlinkSkipped {
        name: "link.txt".to_owned()
    }));
    if made.is_ok_and(|m| m.success()) {
        assert_eq!(
            report.items_skipped, 1,
            "only the link counts; the pipe is not a file"
        );
    }
}

#[test]
fn folder_errors_and_empty_results() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = store();
    let missing = import_folder(
        &mut s,
        &dir.path().join("nope"),
        at(1),
        &ImportOptions::default(),
    );
    assert!(matches!(missing, Err(Error::File { .. })));
    write(dir.path(), "file.txt", "x");
    let not_dir = import_folder(
        &mut s,
        &dir.path().join("file.txt"),
        at(1),
        &ImportOptions::default(),
    );
    assert!(matches!(not_dir, Err(Error::File { .. })));

    let empty = tempfile::tempdir().unwrap();
    let report = import_folder(&mut s, empty.path(), at(1), &ImportOptions::default()).unwrap();
    assert_eq!((report.boards_added, report.items_added), (0, 0));
    assert_eq!(counts(&s), (0, 0, 0, 0));
}

#[test]
fn folder_skip_duplicates_and_board_name_and_limit() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", "same");
    write(dir.path(), "b.txt", "other");
    let mut s = store();
    let opts = ImportOptions {
        merge: MergeStrategy::SkipDuplicates,
        board_name: Some("Chosen".to_owned()),
        free_board_limit: Some(0),
        ..ImportOptions::default()
    };
    let first = import_folder(&mut s, dir.path(), at(1), &opts).unwrap();
    assert_eq!(first.items_added, 2);
    assert_eq!(s.boards().unwrap()[0].name.as_str(), "Chosen");
    assert!(
        first
            .warnings
            .contains(&Warning::BoardLimit { limit: 0, total: 1 })
    );
    let second = import_folder(&mut s, dir.path(), at(2), &opts).unwrap();
    assert_eq!(
        (
            second.items_added,
            second.items_skipped,
            second.boards_added
        ),
        (0, 2, 0)
    );
    assert!(
        !second
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::BoardLimit { .. }))
    );
}

#[test]
fn warnings_are_capped_but_counted() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..(snipdeck_portable::MAX_WARNINGS + 50) {
        write(dir.path(), &format!("e{i}.txt"), "");
    }
    let (_, report) = import_dir(dir.path());
    assert_eq!(report.warnings.len(), snipdeck_portable::MAX_WARNINGS);
    assert_eq!(report.warnings_dropped, 50);
    assert_eq!(report.items_skipped, snipdeck_portable::MAX_WARNINGS + 50);
}

// ---------------------------------------------------------------- Ditto

/// Ditto's `CreateDB` statements (DatabaseUtilities.cpp), reproduced for fixtures.
const DITTO_DDL: &str = "
    CREATE TABLE Main(lID INTEGER PRIMARY KEY AUTOINCREMENT, lDate INTEGER, mText TEXT,
        lShortCut INTEGER, lDontAutoDelete INTEGER, CRC INTEGER, bIsGroup INTEGER,
        lParentID INTEGER, QuickPasteText TEXT, clipOrder REAL, clipGroupOrder REAL,
        globalShortCut INTEGER, lastPasteDate INTEGER, stickyClipOrder REAL,
        stickyClipGroupOrder REAL, MoveToGroupShortCut INTEGER, GlobalMoveToGroupShortCut INTEGER);
    CREATE TABLE Data(lID INTEGER PRIMARY KEY AUTOINCREMENT, lParentID INTEGER,
        strClipBoardFormat TEXT, ooData BLOB);
    CREATE TABLE Types(lID INTEGER PRIMARY KEY AUTOINCREMENT, TypeText TEXT);";

fn utf16z(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

struct Ditto(Connection);

impl Ditto {
    fn new(path: &Path) -> Self {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(DITTO_DDL).unwrap();
        Self(conn)
    }

    fn group(&self, id: i64, name: &str, parent: i64) {
        self.0
            .execute(
                "INSERT INTO Main (lID, lDate, mText, bIsGroup, lParentID) VALUES (?1, 0, ?2, 1, ?3)",
                rusqlite::params![id, name, parent],
            )
            .unwrap();
    }

    fn clip(&self, id: i64, date: i64, m_text: Option<&str>, starred: bool, parent: i64) {
        self.0
            .execute(
                "INSERT INTO Main (lID, lDate, mText, lDontAutoDelete, bIsGroup, lParentID, lastPasteDate)
                 VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
                rusqlite::params![id, date, m_text, i64::from(starred), parent, date.saturating_add(60)],
            )
            .unwrap();
    }

    fn data(&self, parent: i64, format: &str, bytes: &[u8]) {
        self.0
            .execute(
                "INSERT INTO Data (lParentID, strClipBoardFormat, ooData) VALUES (?1, ?2, ?3)",
                rusqlite::params![parent, format, bytes],
            )
            .unwrap();
    }
}

fn ditto_fixture(path: &Path) {
    let d = Ditto::new(path);
    d.group(10, "Work", -1);
    d.group(11, "Sub", 10);
    // Ungrouped, starred; the full text is longer than the stored description.
    d.clip(1, 1_700_000_000, Some("hello wor"), true, -1);
    d.data(1, "CF_UNICODETEXT", &utf16z("hello world, the full text"));
    d.data(1, "CF_TEXT", b"hello world, the full text\0");
    d.data(1, "HTML Format", b"<b>ignored</b>");
    // In a group, only the description exists.
    d.clip(2, 1_700_000_100, Some("from description"), false, 10);
    // Nested group, ANSI text only.
    d.clip(3, 1_700_000_200, Some("ansi"), false, 11);
    d.data(3, "CF_TEXT", b"ansi text\0");
    // An image: not text.
    d.clip(4, 1_700_000_300, Some(""), false, -1);
    d.data(4, "CF_DIB", &[1, 2, 3, 4]);
    // Empty text.
    d.clip(5, 1_700_000_400, Some("   "), false, -1);
    // Invalid UTF-16 (a lone surrogate).
    d.clip(6, 1_700_000_500, None, false, -1);
    d.data(
        6,
        "CF_UNICODETEXT",
        &[0x41, 0x00, 0x00, 0xD8, 0x42, 0x00, 0, 0],
    );
    // A group that does not exist: lands in the history list.
    d.clip(7, 1_700_000_600, Some("orphan"), false, 999);
    // Absurd date: falls back to the import time.
    d.clip(8, i64::MAX, Some("far future"), false, -1);
}

#[test]
fn ditto_imports_text_clips_groups_pins_and_dates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Data.db");
    ditto_fixture(&path);
    let before = fs::read(&path).unwrap();

    let mut s = store();
    let report = import_ditto(&mut s, &path, at(9), &ImportOptions::default()).unwrap();
    assert_healthy(&s);

    let all = contents(&s);
    let names: Vec<_> = all
        .iter()
        .map(|(b, l, _)| (b.as_str(), l.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("Imported from Ditto", "Ditto history"),
            ("Imported from Ditto", "Work"),
            ("Imported from Ditto", "Work / Sub"),
        ]
    );
    // Newest first; the full CF_UNICODETEXT beats the truncated description.
    let history: Vec<_> = all[0].2.iter().map(|(_, t)| t.as_str()).collect();
    assert_eq!(
        history,
        [
            "far future",
            "orphan",
            "A\u{fffd}B",
            "hello world, the full text"
        ]
    );
    assert_eq!(
        all[1].2,
        [("from description".to_owned(), "from description".to_owned())]
    );
    assert_eq!(all[2].2, [("ansi text".to_owned(), "ansi text".to_owned())]);

    let board = &s.boards().unwrap()[0];
    let list = &s.lists(board.id).unwrap()[0];
    let items = s.items(list.id).unwrap();
    let hello = items.iter().find(|i| i.title.starts_with("hello")).unwrap();
    assert!(hello.pinned);
    assert_eq!(hello.created_at.as_second(), 1_700_000_000);
    assert_eq!(hello.last_used_at.unwrap().as_second(), 1_700_000_060);
    let far = items.iter().find(|i| i.title == "far future").unwrap();
    assert_eq!(
        far.created_at,
        at(9),
        "an impossible date falls back to the import time"
    );

    assert_eq!(report.items_added, 6);
    assert_eq!(report.items_skipped, 2, "the image and the blank clip");
    assert!(
        report
            .warnings
            .contains(&Warning::NonTextSkipped { count: 1 })
    );
    assert!(report.warnings.contains(&Warning::LossyText {
        name: "Ditto clip 6".to_owned()
    }));

    assert_eq!(
        fs::read(&path).unwrap(),
        before,
        "the database is never modified"
    );
}

#[test]
fn ditto_works_on_a_read_only_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Data.db");
    ditto_fixture(&path);
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&path, perms).unwrap();
    let mut s = store();
    import_ditto(&mut s, &path, at(9), &ImportOptions::default()).unwrap();
    assert_eq!(counts(&s).2, 6);
}

#[test]
fn ditto_accepts_an_older_schema_without_paste_dates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Data.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE Main(lID INTEGER PRIMARY KEY, lDate INTEGER, mText TEXT,
            lDontAutoDelete INTEGER, bIsGroup INTEGER, lParentID INTEGER);
         CREATE TABLE Data(lID INTEGER PRIMARY KEY, lParentID INTEGER,
            strClipBoardFormat TEXT, ooData BLOB);
         INSERT INTO Main VALUES (1, 1700000000, 'old clip', 0, 0, -1);",
    )
    .unwrap();
    drop(conn);
    let mut s = store();
    let report = import_ditto(&mut s, &path, at(9), &ImportOptions::default()).unwrap();
    assert_eq!(report.items_added, 1);
    let item = s.all_items().unwrap().remove(0);
    assert_eq!(item.last_used_at, None);
}

#[test]
fn ditto_tolerates_odd_column_types() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Data.db");
    let d = Ditto::new(&path);
    d.0.execute_batch(
        "INSERT INTO Main (lID, lDate, mText, lDontAutoDelete, bIsGroup, lParentID)
           VALUES (1, 'not a number', x'68656c6c6f', 2.5, NULL, 'x');
         INSERT INTO Main (lID, lDate, mText, bIsGroup, lParentID) VALUES (2, 5.7, 12345, 0, NULL);
         INSERT INTO Data (lParentID, strClipBoardFormat, ooData) VALUES (1, 'CF_UNICODETEXT', 'text where a blob belongs');
         INSERT INTO Data (lParentID, strClipBoardFormat, ooData) VALUES (NULL, NULL, NULL);",
    )
    .unwrap();
    drop(d);
    let mut s = store();
    let report = import_ditto(&mut s, &path, at(9), &ImportOptions::default()).unwrap();
    // Clip 1's unicode row is odd-length text; clip 2 has a numeric description (ignored).
    assert!(report.items_added >= 1, "{report:?}");
    assert_healthy(&s);
}

// ---- security review 2, finding 3: a hostile Ditto file cannot make the import huge ----

/// A database whose `Main` and `Data` are views that compute their rows: a few KiB of file
/// that would expand to hundreds of MiB.
#[test]
fn ditto_refuses_views_standing_in_for_its_tables() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("views.db");
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE VIEW Main AS
               WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM c WHERE n < 12)
               SELECT n AS lID, 0 AS lDate, CAST(zeroblob(5000000) AS TEXT) AS mText,
                      0 AS lDontAutoDelete, 0 AS bIsGroup, -1 AS lParentID FROM c;
             CREATE VIEW Data AS
               SELECT 1 AS lParentID, 'CF_TEXT' AS strClipBoardFormat, zeroblob(10) AS ooData;",
        )
        .unwrap();
    let mut s = store();
    let e = import_ditto(&mut s, &path, at(1), &ImportOptions::default()).unwrap_err();
    assert!(
        matches!(&e, Error::NotDitto { reason, .. } if reason.contains("not a table")),
        "{e}"
    );
    assert_eq!(counts(&s), (0, 0, 0, 0));
}

#[test]
fn ditto_refuses_virtual_tables_and_computed_columns() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = store();

    let virtual_table = dir.path().join("virtual.db");
    Connection::open(&virtual_table)
        .unwrap()
        .execute_batch(
            "CREATE VIRTUAL TABLE Main USING fts5(lID, lDate, mText, lDontAutoDelete, bIsGroup, lParentID);
             CREATE TABLE Data(lID INTEGER PRIMARY KEY, lParentID INTEGER, strClipBoardFormat TEXT, ooData BLOB);",
        )
        .unwrap();
    let e = import_ditto(&mut s, &virtual_table, at(1), &ImportOptions::default()).unwrap_err();
    assert!(matches!(e, Error::NotDitto { .. }), "{e}");

    // A generated column computes its value for every row on every read.
    let generated = dir.path().join("generated.db");
    let conn = Connection::open(&generated).unwrap();
    conn.execute_batch(
        "CREATE TABLE Main(lID INTEGER PRIMARY KEY, lDate INTEGER,
            mText TEXT GENERATED ALWAYS AS (CAST(zeroblob(4000000) AS TEXT)) VIRTUAL,
            lDontAutoDelete INTEGER, bIsGroup INTEGER, lParentID INTEGER);
         CREATE TABLE Data(lID INTEGER PRIMARY KEY, lParentID INTEGER, strClipBoardFormat TEXT, ooData BLOB);
         WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM c WHERE n < 12)
           INSERT INTO Main (lID, lDate, lDontAutoDelete, bIsGroup, lParentID)
           SELECT n, 0, 0, 0, -1 FROM c;",
    )
    .unwrap();
    drop(conn);
    let e = import_ditto(&mut s, &generated, at(1), &ImportOptions::default()).unwrap_err();
    assert!(matches!(e, Error::NotDitto { .. }), "{e}");
    assert_eq!(counts(&s), (0, 0, 0, 0));
}

/// Rows over the per-value limit are skipped with their real size; the rest still imports.
#[test]
fn ditto_skips_oversized_values_with_their_real_size_and_carries_on() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Data.db");
    let d = Ditto::new(&path);
    let big = 12_000_000_usize;
    // Oversized CF_UNICODETEXT, then an oversized description with no data rows.
    d.clip(1, 1_700_000_000, Some("big data row"), false, -1);
    d.data(1, "CF_UNICODETEXT", &vec![b'a'; big]);
    d.clip(2, 1_700_000_100, None, false, -1);
    d.0.execute(
        "UPDATE Main SET mText = ?1 WHERE lID = 2",
        rusqlite::params![vec![b'b'; big]],
    )
    .unwrap();
    d.clip(3, 1_700_000_200, Some("fine"), false, -1);
    drop(d);

    let mut s = store();
    let report = import_ditto(&mut s, &path, at(9), &ImportOptions::default()).unwrap();
    assert_eq!(report.items_added, 1, "{report:?}");
    assert_eq!(report.items_skipped, 2, "{report:?}");
    let mut sizes: Vec<_> = report
        .warnings
        .iter()
        .filter_map(|w| match w {
            Warning::TooLarge { name, size } => Some((name.as_str(), *size)),
            _ => None,
        })
        .collect();
    sizes.sort_unstable();
    assert_eq!(
        sizes,
        [("Ditto clip 1", big as u64), ("Ditto clip 2", big as u64)]
    );
    assert_healthy(&s);
}

#[test]
fn ditto_group_cycles_and_depth_cannot_loop() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Data.db");
    let d = Ditto::new(&path);
    d.group(1, "A", 2);
    d.group(2, "B", 1);
    d.group(3, "Self", 3);
    d.clip(10, 1_700_000_000, Some("in a"), false, 1);
    d.clip(11, 1_700_000_001, Some("in self"), false, 3);
    drop(d);
    let mut s = store();
    import_ditto(&mut s, &path, at(9), &ImportOptions::default()).unwrap();
    let lists: Vec<_> = contents(&s).into_iter().map(|c| c.1).collect();
    assert_eq!(lists, ["B / A", "Self"]);
}

#[test]
fn ditto_rejects_what_is_not_a_ditto_database() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = store();
    let opts = ImportOptions::default();

    let missing = import_ditto(&mut s, &dir.path().join("none.db"), at(1), &opts);
    assert!(matches!(missing, Err(Error::File { .. })));

    let garbage = dir.path().join("garbage.db");
    fs::write(&garbage, noise(4096, 3)).unwrap();
    let e = import_ditto(&mut s, &garbage, at(1), &opts).unwrap_err();
    assert!(
        matches!(e, Error::NotDitto { .. } | Error::Sqlite(_)),
        "{e}"
    );

    let other = dir.path().join("other.db");
    Connection::open(&other)
        .unwrap()
        .execute_batch("CREATE TABLE notes(id INTEGER, body TEXT);")
        .unwrap();
    let e = import_ditto(&mut s, &other, at(1), &opts).unwrap_err();
    assert!(matches!(e, Error::NotDitto { .. }), "{e}");

    let partial = dir.path().join("partial.db");
    Connection::open(&partial)
        .unwrap()
        .execute_batch(
            "CREATE TABLE Main(lID INTEGER, mText TEXT);
             CREATE TABLE Data(lID INTEGER, lParentID INTEGER, strClipBoardFormat TEXT, ooData BLOB);",
        )
        .unwrap();
    let e = import_ditto(&mut s, &partial, at(1), &opts).unwrap_err();
    assert!(
        matches!(&e, Error::NotDitto { reason, .. } if reason.contains("lDate")),
        "{e}"
    );

    // The file is a SQLite file but truncated mid-page.
    let good = dir.path().join("good.db");
    ditto_fixture(&good);
    let bytes = fs::read(&good).unwrap();
    let cut = dir.path().join("cut.db");
    fs::write(&cut, &bytes[..bytes.len() / 2]).unwrap();
    let before = counts(&s);
    if import_ditto(&mut s, &cut, at(1), &opts).is_err() {
        assert_eq!(counts(&s), before, "a damaged file adds nothing");
    }
    assert_healthy(&s);
}

// ---------------------------------------------------------------- CopyQ

fn copyq(json: &[u8], opts: &ImportOptions<'_>) -> (Store, snipdeck_portable::ImportReport) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("copyq.json");
    fs::write(&path, json).unwrap();
    let mut s = store();
    let report = import_copyq_json(&mut s, &path, at(9), opts).unwrap();
    assert_healthy(&s);
    (s, report)
}

#[test]
fn copyq_object_with_tab_items_tags_and_pins() {
    let json = serde_json::json!({
        "tab": "clipboard",
        "items": [
            "a plain string item",
            {"text": "second\nline", "title": "Custom title", "tags": ["Work", "work", "mail"], "pinned": true, "extra": 1},
            {"text": "third"},
        ],
        "other": "ignored"
    });
    let mut pre = store();
    tag(&mut pre, "MAIL");
    let _ = pre;
    let (s, report) = copyq(json.to_string().as_bytes(), &ImportOptions::default());
    let all = contents(&s);
    assert_eq!(all.len(), 1);
    assert_eq!(
        (all[0].0.as_str(), all[0].1.as_str()),
        ("Imported from CopyQ", "clipboard")
    );
    let titles: Vec<_> = all[0].2.iter().map(|i| i.0.as_str()).collect();
    assert_eq!(titles, ["a plain string item", "Custom title", "third"]);
    assert_eq!(all[0].2[1].1, "second\nline");
    assert_eq!(report.items_added, 3);
    assert!(!report.has_warnings());
    let tags: Vec<_> = s
        .tags()
        .unwrap()
        .iter()
        .map(|t| t.name.to_string())
        .collect();
    assert_eq!(tags, ["mail", "Work"], "tags merge ignoring case");
    let pinned: Vec<_> = s
        .all_items()
        .unwrap()
        .into_iter()
        .filter(|i| i.pinned)
        .collect();
    assert_eq!(pinned.len(), 1);
}

#[test]
fn copyq_reuses_existing_tags() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.json");
    fs::write(&path, r#"[{"text":"x","tags":["MAIL"]}]"#).unwrap();
    let mut s = store();
    tag(&mut s, "mail");
    let report = import_copyq_json(&mut s, &path, at(1), &ImportOptions::default()).unwrap();
    assert_eq!(report.tags_added, 0);
    assert_eq!(s.tags().unwrap().len(), 1);
}

/// Security review 2, finding 5: tags clash the way the store says they do (full case folding
/// and Unicode normalisation), or the import aborts with `TagExists`.
#[test]
fn copyq_tags_match_the_way_the_store_matches_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.json");
    fs::write(
        &path,
        r#"[{"text":"x","tags":["Straße","STRASSE"]},{"text":"y","tags":["e\u0301cole"]}]"#,
    )
    .unwrap();
    let mut s = store();
    tag(&mut s, "\u{c9}COLE");
    let report = import_copyq_json(&mut s, &path, at(1), &ImportOptions::default()).unwrap();
    assert_eq!(report.items_added, 2);
    assert_eq!(report.tags_added, 1, "Straße and STRASSE are one tag");
    assert_eq!(s.tags().unwrap().len(), 2, "the composed École was reused");
    assert_healthy(&s);
}

/// Security review 2, finding 7: tags are capped per item and in total, and looked up by key,
/// so a file with thousands of them neither stalls the import nor floods the library.
#[test]
fn copyq_caps_the_tags_per_item_and_in_total() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.json");

    // One item with 20 000 distinct tags: only the first few are used.
    let many: Vec<String> = (0..20_000).map(|n| format!("tag-{n}")).collect();
    let json = serde_json::json!([{ "text": "x", "tags": many }]);
    fs::write(&path, json.to_string()).unwrap();
    let mut s = store();
    let report = import_copyq_json(&mut s, &path, at(1), &ImportOptions::default()).unwrap();
    assert_eq!(report.items_added, 1);
    assert_eq!(report.tags_added, 32, "{report:?}");
    assert!(
        report
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::Truncated { reason } if reason.contains("tags"))),
        "{:?}",
        report.warnings
    );
    assert_healthy(&s);

    // Many items with distinct tags each: the total is capped.
    let items: Vec<_> = (0..200)
        .map(|i| {
            let tags: Vec<String> = (0..32).map(|t| format!("t-{i}-{t}")).collect();
            serde_json::json!({ "text": format!("item {i}"), "tags": tags })
        })
        .collect();
    fs::write(&path, serde_json::Value::Array(items).to_string()).unwrap();
    let mut s = store();
    let report = import_copyq_json(&mut s, &path, at(1), &ImportOptions::default()).unwrap();
    assert_eq!(report.items_added, 200);
    assert_eq!(report.tags_added, 5_000, "{report:?}");
    assert_eq!(s.tags().unwrap().len(), 5_000);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::Truncated { reason } if reason.contains("tags"))),
    );
    assert_healthy(&s);
}

#[test]
fn copyq_accepts_arrays_and_json_lines() {
    let (s, _) = copyq(br#"["one", "two"]"#, &ImportOptions::default());
    assert_eq!(contents(&s)[0].1, "CopyQ");
    assert_eq!(contents(&s)[0].2.len(), 2);

    let (s, report) = copyq(
        b"\"first\"\n{\"text\": \"second\"}\n\n\"third\"\n",
        &ImportOptions::default(),
    );
    assert_eq!(contents(&s)[0].2.len(), 3);
    assert_eq!(report.items_added, 3);
}

#[test]
fn copyq_reads_what_powershell_and_windows_tools_write() {
    // PowerShell's `>` writes UTF-16LE with a BOM; Notepad may write a UTF-8 BOM.
    let json = r#"{"tab":"t","items":["héllo 😀"]}"#;
    let utf16: Vec<u8> = [0xFF, 0xFE]
        .into_iter()
        .chain(json.encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    let (s, _) = copyq(&utf16, &ImportOptions::default());
    assert_eq!(contents(&s)[0].2[0].1, "héllo 😀");
    let bom: Vec<u8> = [0xEF, 0xBB, 0xBF].into_iter().chain(json.bytes()).collect();
    let (s, _) = copyq(&bom, &ImportOptions::default());
    assert_eq!(contents(&s)[0].2[0].1, "héllo 😀");
}

#[test]
fn copyq_skips_bad_items_with_warnings() {
    let big = "x".repeat(5 * 1024 * 1024 + 1);
    let json = serde_json::json!(["ok", "", "   ", 42, {"no_text": true}, null, {"text": big}, ["nested"]]);
    let (s, report) = copyq(json.to_string().as_bytes(), &ImportOptions::default());
    assert_eq!(report.items_added, 1);
    assert_eq!(report.items_skipped, 7);
    assert_eq!(report.warnings.len(), 7);
    assert_eq!(contents(&s)[0].2[0].1, "ok");
}

#[test]
fn copyq_rejects_files_that_are_not_json_or_the_wrong_shape() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = store();
    for (name, content) in [
        ("empty", &b""[..]),
        ("prose", b"just some words"),
        ("object", br#"{"tab":"x"}"#),
        ("number", b"42"),
        ("half", b"[1, 2"),
        ("binary", &[0x00, 0x01, 0x02][..]),
    ] {
        let path = dir.path().join(name);
        fs::write(&path, content).unwrap();
        let e = import_copyq_json(&mut s, &path, at(1), &ImportOptions::default()).unwrap_err();
        assert!(matches!(e, Error::Malformed { .. }), "{name}: {e}");
    }
    let missing = import_copyq_json(
        &mut s,
        &dir.path().join("none"),
        at(1),
        &ImportOptions::default(),
    );
    assert!(matches!(missing, Err(Error::File { .. })));
    assert_eq!(counts(&s), (0, 0, 0, 0));
}

#[test]
fn copyq_cpq_files_are_not_parsed() {
    // A real .cpq starts with a QDataStream QByteArray "CopyQ v4"/"CopyQ v5".
    let mut cpq = vec![0, 0, 0, 8];
    cpq.extend_from_slice(b"CopyQ v5");
    cpq.extend_from_slice(&noise(200, 4));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tab.cpq");
    fs::write(&path, cpq).unwrap();
    let mut s = store();
    let e = import_copyq_json(&mut s, &path, at(1), &ImportOptions::default()).unwrap_err();
    assert!(matches!(e, Error::Malformed { .. }), "{e}");
}

#[test]
fn copyq_skip_duplicates_applies() {
    let json = br#"["same", "same", "different"]"#;
    let opts = ImportOptions {
        merge: MergeStrategy::SkipDuplicates,
        ..ImportOptions::default()
    };
    let (_, report) = copyq(json, &opts);
    assert_eq!((report.items_added, report.items_skipped), (2, 1));
}

// ---------------------------------------------------------------- dry run

#[test]
fn dry_run_reports_exactly_what_a_real_import_adds_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", "alpha");
    write(dir.path(), "b.md", "beta");
    write(dir.path(), "sub/c.txt", "gamma");
    let mut s = store();
    let before = (s.boards().unwrap().len(), s.tags().unwrap().len());
    let dry = ImportOptions {
        dry_run: true,
        ..ImportOptions::default()
    };
    let preview = import_folder(&mut s, dir.path(), at(9), &dry).unwrap();
    assert_eq!((s.boards().unwrap().len(), s.tags().unwrap().len()), before);
    assert!(contents(&s).is_empty());

    let real = import_folder(&mut s, dir.path(), at(9), &ImportOptions::default()).unwrap();
    assert_eq!(preview.items_added, 3);
    assert_eq!(
        (
            preview.boards_added,
            preview.lists_added,
            preview.items_added
        ),
        (real.boards_added, real.lists_added, real.items_added)
    );
}

#[test]
fn dry_run_counts_duplicates_as_skipped() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", "alpha");
    let mut s = store();
    import_folder(&mut s, dir.path(), at(9), &ImportOptions::default()).unwrap();
    let dry = ImportOptions {
        dry_run: true,
        merge: MergeStrategy::SkipDuplicates,
        ..ImportOptions::default()
    };
    let preview = import_folder(&mut s, dir.path(), at(10), &dry).unwrap();
    assert_eq!((preview.items_added, preview.items_skipped), (0, 1));
    assert_eq!(contents(&s).len(), 1);
}

// ---------------------------------------------------------------- clipboard history

fn clip(text: &str, secs: Option<i64>) -> HistoryEntry {
    HistoryEntry {
        text: text.to_owned(),
        timestamp: secs.map(at),
    }
}

#[test]
fn history_keeps_order_skips_blank_and_titles_from_first_line() {
    let mut s = store();
    let entries = [
        clip("first\nsecond line", Some(5)),
        clip("  \r\n\t ", None),
        clip("", None),
        clip("third", None),
    ];
    let report = import_entries(&mut s, &entries, "", at(9), &ImportOptions::default()).unwrap();
    assert_healthy(&s);
    assert_eq!(
        (report.boards_added, report.lists_added, report.items_added),
        (1, 1, 2)
    );
    assert_eq!(report.items_skipped, 2);
    assert_eq!(
        contents(&s),
        [(
            "Windows clipboard history".to_owned(),
            "Windows clipboard history".to_owned(),
            vec![
                ("first".to_owned(), "first\nsecond line".to_owned()),
                ("third".to_owned(), "third".to_owned()),
            ]
        )]
    );
    let item = first_item(&s);
    assert_eq!(item.created_at, at(5));
}

#[test]
fn history_board_name_comes_from_options_then_argument() {
    let mut s = store();
    let e = [clip("a", None)];
    import_entries(
        &mut s,
        &e,
        "Zwischenablage",
        at(1),
        &ImportOptions::default(),
    )
    .unwrap();
    let opts = ImportOptions {
        board_name: Some("Chosen".to_owned()),
        ..ImportOptions::default()
    };
    import_entries(&mut s, &e, "Zwischenablage", at(2), &opts).unwrap();
    let names: Vec<_> = s
        .boards()
        .unwrap()
        .iter()
        .map(|b| b.name.to_string())
        .collect();
    assert!(names.contains(&"Zwischenablage".to_owned()) && names.contains(&"Chosen".to_owned()));
}

#[test]
fn history_skips_an_entry_over_the_text_limit_with_a_warning() {
    let mut s = store();
    let huge = "x".repeat(5 * 1024 * 1024 + 1);
    let entries = [clip(&huge, None), clip("ok", None)];
    let report = import_entries(&mut s, &entries, "", at(1), &ImportOptions::default()).unwrap();
    assert_eq!((report.items_added, report.items_skipped), (1, 1));
    assert!(report.warnings.iter().any(
        |w| matches!(w, Warning::TooLarge { name, size } if name == "entry 1" && *size == 5 * 1024 * 1024 + 1)
    ));
}

#[test]
fn history_dry_run_writes_nothing_and_duplicates_are_skipped() {
    let mut s = store();
    let entries = [clip("same", None), clip("other", None)];
    let dry = ImportOptions {
        dry_run: true,
        ..ImportOptions::default()
    };
    let preview = import_entries(&mut s, &entries, "", at(1), &dry).unwrap();
    assert_eq!(preview.items_added, 2);
    assert_eq!(counts(&s), (0, 0, 0, 0));

    import_entries(&mut s, &entries, "", at(2), &ImportOptions::default()).unwrap();
    let skip = ImportOptions {
        merge: MergeStrategy::SkipDuplicates,
        ..ImportOptions::default()
    };
    let again = import_entries(&mut s, &entries, "", at(3), &skip).unwrap();
    assert_eq!((again.items_added, again.items_skipped), (0, 2));
    assert_eq!(contents(&s).len(), 1);
}

#[test]
fn history_with_nothing_importable_adds_no_board() {
    let mut s = store();
    let report = import_entries(&mut s, &[], "", at(1), &ImportOptions::default()).unwrap();
    assert_eq!(report.boards_added, 0);
    assert_eq!(counts(&s), (0, 0, 0, 0));
}

#[test]
fn copyq_a_huge_array_of_junk_is_cut_off_at_the_scan_cap() {
    // 2 MB of `0,` would be a million values; the cap is on items looked at, so the rest is
    // never even visited, let alone kept.
    let mut json = String::from("[");
    json.push_str(&"0,".repeat(1_000_000));
    json.push_str("0]");
    let opts = ImportOptions {
        limits: snipdeck_portable::Limits {
            max_items: 10,
            ..snipdeck_portable::Limits::default()
        },
        ..ImportOptions::default()
    };
    let (s, report) = copyq(json.as_bytes(), &opts);
    assert_eq!(counts(&s), (0, 0, 0, 0));
    assert_eq!(report.items_skipped, 40, "scan cap is 4 x max_items");
    assert!(
        report
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::Truncated { .. })),
        "{:?}",
        report.warnings
    );
    assert!(report.warnings.len() <= snipdeck_portable::MAX_WARNINGS);
}

#[test]
fn copyq_stops_reading_at_max_items_without_a_tree() {
    let items: Vec<String> = (0..50).map(|i| format!("item {i}")).collect();
    let json = serde_json::to_string(&items).unwrap();
    let opts = ImportOptions {
        limits: snipdeck_portable::Limits {
            max_items: 5,
            ..snipdeck_portable::Limits::default()
        },
        ..ImportOptions::default()
    };
    let (_, report) = copyq(json.as_bytes(), &opts);
    assert_eq!(report.items_added, 5);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::Truncated { .. }))
    );
}

#[test]
fn copyq_a_deeply_nested_array_is_skipped_not_recursed_into() {
    let depth = 200_000;
    let json = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deep.json");
    fs::write(&path, json).unwrap();
    let mut s = store();
    // Nesting is skipped without being built or recursed into: one unreadable element, or a
    // refusal; never a stack overflow and never an item.
    match import_copyq_json(&mut s, &path, at(1), &ImportOptions::default()) {
        Ok(report) => assert_eq!((report.items_added, report.items_skipped), (0, 1)),
        Err(e) => assert!(matches!(e, Error::Malformed { .. }), "{e}"),
    }
    assert_eq!(counts(&s), (0, 0, 0, 0));
}

#[test]
fn copyq_a_huge_tags_array_in_one_item_keeps_only_the_cap() {
    let tags = format!("[{}]", r#""a","#.repeat(500_000) + r#""b""#);
    let json = format!(r#"{{"items":[{{"text":"hello","tags":{tags}}}]}}"#);
    let (s, report) = copyq(json.as_bytes(), &ImportOptions::default());
    assert_eq!(report.items_added, 1);
    assert!(report.tags_added <= 2, "{}", report.tags_added);
    assert_healthy(&s);
}

#[test]
fn ditto_reads_at_most_a_capped_number_of_groups() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Data.db");
    let d = Ditto::new(&path);
    // 10 001 group rows from one statement; the importer reads the first 10 000 and says so.
    d.0.execute_batch(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 10001)
         INSERT INTO Main (lID, lDate, mText, bIsGroup, lParentID)
         SELECT i, 0, 'g' || i, 1, -1 FROM n;",
    )
    .unwrap();
    d.clip(20_000, 1_700_000_000, Some("a clip"), false, 1);
    drop(d);
    let mut s = store();
    let report = import_ditto(&mut s, &path, at(9), &ImportOptions::default()).unwrap();
    assert_eq!(report.items_added, 1);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::Truncated { reason } if reason.contains("groups"))),
        "{:?}",
        report.warnings
    );
}
