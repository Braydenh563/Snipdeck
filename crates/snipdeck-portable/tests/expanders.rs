//! The text-expander importers: Beeftext, `TextExpander`, `PhraseExpress` and Espanso.
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

use std::fmt::Write as _;
use std::fs;

use common::*;
use snipdeck_core::ItemKind;
use snipdeck_portable::{Error, ImportOptions, ImportReport, Limits, MergeStrategy, Warning};
use snipdeck_store::{BlobStore, BoardRepo, ItemRepo, ListRepo, Store};

/// (title, kind key, text) of one item.
type Entry = (String, String, String);

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
                        item.kind.key().to_owned(),
                        String::from_utf8(store.read(c).unwrap().into_owned()).unwrap(),
                    )
                })
                .collect();
            out.push((board.name.to_string(), list.name.to_string(), items));
        }
    }
    out
}

fn e(title: &str, kind: ItemKind, text: &str) -> Entry {
    (title.to_owned(), kind.key().to_owned(), text.to_owned())
}

fn small(max_items: usize) -> ImportOptions<'static> {
    ImportOptions {
        limits: Limits {
            max_items,
            ..Limits::default()
        },
        ..ImportOptions::default()
    }
}

fn notes(report: &ImportReport) -> Vec<String> {
    report
        .warnings
        .iter()
        .filter_map(|w| match w {
            Warning::Note(n) => Some(n.clone()),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------- Beeftext

fn beeftext(json: &[u8], opts: &ImportOptions<'_>) -> (Store, ImportReport) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.json");
    fs::write(&path, json).unwrap();
    let mut s = store();
    let report = snipdeck_portable::import_beeftext(&mut s, &path, at(9), opts).unwrap();
    assert_healthy(&s);
    (s, report)
}

fn beeftext_err(json: &[u8]) -> Error {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.json");
    fs::write(&path, json).unwrap();
    let mut s = store();
    snipdeck_portable::import_beeftext(&mut s, &path, at(9), &ImportOptions::default()).unwrap_err()
}

const BEEFTEXT: &str = r#"{
  "version": 8,
  "groups": [
    {"uuid": "g-1", "name": "Email", "description": "", "enabled": true},
    {"uuid": "g-2", "name": "Work", "enabled": true}
  ],
  "combos": [
    {"uuid": "c-1", "name": "Greeting", "keyword": ";hi", "snippet": "Hello there,\nBest", "group": "g-1"},
    {"uuid": "c-2", "name": "", "keyword": ";sig", "snippet": "Regards, #{cursor}", "group": "g-1"},
    {"uuid": "c-3", "name": "Stamp", "keyword": ";ts", "snippet": "Today: #{date} at #{time}", "group": "g-2"},
    {"uuid": "c-4", "name": "Paste", "keyword": ";pp", "snippet": "Wrapped: #{clipboard} {braces}", "group": "g-2"},
    {"uuid": "c-5", "name": "Ask", "keyword": ";ask", "snippet": "Dear #{input:Customer name}", "group": "g-2"},
    {"uuid": "c-6", "name": "Odd", "keyword": ";odd", "snippet": "Run #{powershell:C:\\x.ps1} now {x}", "group": "g-2"},
    {"uuid": "c-7", "name": "Loose", "keyword": ";loose", "snippet": "no group", "group": "missing"},
    {"uuid": "c-8", "name": "Blank", "keyword": ";blank", "snippet": "  ", "group": "g-2"},
    "junk",
    {"uuid": "c-9", "name": "NoSnippet", "keyword": ";n"}
  ]
}"#;

#[test]
fn beeftext_maps_groups_keywords_and_variables() {
    let (s, report) = beeftext(BEEFTEXT.as_bytes(), &ImportOptions::default());
    let b = "Imported from Beeftext";
    assert_eq!(
        contents(&s),
        [
            (
                b.to_owned(),
                "Email".to_owned(),
                vec![
                    e("Greeting (;hi)", ItemKind::Text, "Hello there,\nBest"),
                    e(";sig", ItemKind::Template, "Regards, {cursor}"),
                ]
            ),
            (
                b.to_owned(),
                "Work".to_owned(),
                vec![
                    e("Stamp (;ts)", ItemKind::Template, "Today: {date} at {time}"),
                    e(
                        "Paste (;pp)",
                        ItemKind::Template,
                        "Wrapped: {clipboard} {{braces}}"
                    ),
                    e(
                        "Ask (;ask)",
                        ItemKind::Template,
                        "Dear {field:Customer name}"
                    ),
                    e(
                        "Odd (;odd)",
                        ItemKind::Text,
                        "Run #{powershell:C:\\x.ps1} now {x}"
                    ),
                ]
            ),
            (
                b.to_owned(),
                "Beeftext".to_owned(),
                vec![e("Loose (;loose)", ItemKind::Text, "no group")]
            ),
        ]
    );
    assert_eq!(report.items_added, 7);
    // The blank snippet, the string, and the combo without a snippet.
    assert_eq!(report.items_skipped, 3);
    assert!(
        notes(&report).iter().any(|n| n.contains("powershell")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn beeftext_accepts_a_bare_array_of_combos_and_a_bom() {
    let json = "\u{feff}[{\"keyword\":\"k\",\"snippet\":\"v\"}]";
    let (s, report) = beeftext(json.as_bytes(), &ImportOptions::default());
    assert_eq!(
        contents(&s)[0].2,
        [e("k", ItemKind::Text, "v")],
        "{report:?}"
    );
}

#[test]
fn beeftext_rejects_what_is_not_an_export() {
    for bad in [
        &b""[..],
        b"not json",
        b"42",
        b"{\"x\":1}",
        b"{\"combos\":3}",
    ] {
        assert!(
            matches!(beeftext_err(bad), Error::Malformed { .. }),
            "{}",
            String::from_utf8_lossy(bad)
        );
    }
}

#[test]
fn beeftext_deep_nesting_is_an_error_not_a_crash() {
    let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    assert!(matches!(
        beeftext_err(deep.as_bytes()),
        Error::Malformed { .. }
    ));
    let nested = format!(
        "{{\"combos\":[{{\"keyword\":\"a\",\"snippet\":\"b\",\"extra\":{}{}}}]}}",
        "[".repeat(100_000),
        "]".repeat(100_000)
    );
    assert!(matches!(
        beeftext_err(nested.as_bytes()),
        Error::Malformed { .. }
    ));
}

#[test]
fn beeftext_limits_items_size_and_bad_bytes() {
    let mut json = String::from("{\"combos\":[");
    for i in 0..20 {
        write!(json, "{{\"keyword\":\"k{i}\",\"snippet\":\"v{i}\"}},").unwrap();
    }
    json.push_str("{\"keyword\":\"last\",\"snippet\":\"v\"}]}");
    let (s, report) = beeftext(json.as_bytes(), &small(5));
    assert_eq!(contents(&s)[0].2.len(), 5);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::Truncated { .. }))
    );

    // One snippet over 5 MB is skipped, the rest imported.
    let big = "x".repeat(5 * 1024 * 1024 + 1);
    let json = format!(
        "[{{\"keyword\":\"big\",\"snippet\":\"{big}\"}},{{\"keyword\":\"ok\",\"snippet\":\"fine\"}}]"
    );
    let (s, report) = beeftext(json.as_bytes(), &ImportOptions::default());
    assert_eq!(contents(&s)[0].2, [e("ok", ItemKind::Text, "fine")]);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::TooLarge { .. }))
    );

    // A file over the size limit is refused whole.
    let opts = ImportOptions {
        limits: Limits {
            max_json_bytes: 100,
            ..Limits::default()
        },
        ..ImportOptions::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("e.json");
    fs::write(&path, BEEFTEXT).unwrap();
    let mut s = store();
    assert!(matches!(
        snipdeck_portable::import_beeftext(&mut s, &path, at(9), &opts),
        Err(Error::LimitExceeded { .. })
    ));

    // Bad UTF-8 inside the JSON text is replaced, with a warning.
    let mut bytes = b"[{\"keyword\":\"k\",\"snippet\":\"a".to_vec();
    bytes.extend_from_slice(b"\xff\xfe");
    bytes.extend_from_slice(b"b\"}]");
    let (s, report) = beeftext(&bytes, &ImportOptions::default());
    assert!(contents(&s)[0].2[0].2.contains('\u{fffd}'));
    assert!(
        report
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::LossyText { .. }))
    );
}

#[test]
fn beeftext_two_cursors_fall_back_to_plain_text_and_duplicates_are_skipped() {
    let json = r#"[{"keyword":"c","snippet":"a #{cursor} b #{cursor}"}]"#;
    let (s, report) = beeftext(json.as_bytes(), &ImportOptions::default());
    assert_eq!(
        contents(&s)[0].2,
        [e("c", ItemKind::Text, "a #{cursor} b #{cursor}")]
    );
    assert!(!notes(&report).is_empty());

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("e.json");
    fs::write(&path, BEEFTEXT).unwrap();
    let mut s = store();
    let opts = ImportOptions {
        merge: MergeStrategy::SkipDuplicates,
        ..ImportOptions::default()
    };
    snipdeck_portable::import_beeftext(&mut s, &path, at(9), &opts).unwrap();
    let again = snipdeck_portable::import_beeftext(&mut s, &path, at(10), &opts).unwrap();
    assert_eq!(again.items_added, 0);
    assert_eq!(again.duplicates_skipped, 7);
}
