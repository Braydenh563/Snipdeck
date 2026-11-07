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

// ---------------------------------------------------------------- CSV: TextExpander, PhraseExpress

type CsvImport = fn(
    &mut Store,
    &std::path::Path,
    snipdeck_core::Timestamp,
    &ImportOptions<'_>,
) -> snipdeck_portable::Result<ImportReport>;

fn csv_with(
    import: CsvImport,
    bytes: &[u8],
    opts: &ImportOptions<'_>,
) -> snipdeck_portable::Result<(Store, ImportReport)> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.csv");
    fs::write(&path, bytes).unwrap();
    let mut s = store();
    let report = import(&mut s, &path, at(9), opts)?;
    assert_healthy(&s);
    Ok((s, report))
}

fn te(bytes: &[u8]) -> (Store, ImportReport) {
    csv_with(
        snipdeck_portable::import_textexpander_csv::<Store>,
        bytes,
        &ImportOptions::default(),
    )
    .unwrap()
}

fn pe(bytes: &[u8]) -> (Store, ImportReport) {
    csv_with(
        snipdeck_portable::import_phraseexpress_csv::<Store>,
        bytes,
        &ImportOptions::default(),
    )
    .unwrap()
}

const TEXTEXPANDER: &str = "Abbreviation,Content,Label\r\n\
;hi,\"Hello,\r\nthere \"\"friend\"\"\",Greeting\r\n\
;cb,A %clipboard B %| C {x},Paste\r\n\
;fill,Hi %fillpart:name=who%,Fill-in\r\n\
;nolabel,plain text,\r\n\
,no abbreviation,Just a label\r\n\
;empty,,Empty\r\n\
\r\n";

#[test]
fn textexpander_csv_maps_abbreviation_content_and_label() {
    let (s, report) = te(TEXTEXPANDER.as_bytes());
    assert_eq!(
        contents(&s),
        [(
            "Imported from TextExpander".to_owned(),
            "TextExpander".to_owned(),
            vec![
                e(
                    "Greeting (;hi)",
                    ItemKind::Text,
                    "Hello,\r\nthere \"friend\""
                ),
                e(
                    "Paste (;cb)",
                    ItemKind::Template,
                    "A {clipboard} B {cursor} C {{x}}"
                ),
                e("Fill-in (;fill)", ItemKind::Text, "Hi %fillpart:name=who%"),
                e(";nolabel", ItemKind::Text, "plain text"),
                e("Just a label", ItemKind::Text, "no abbreviation"),
            ]
        )]
    );
    assert_eq!(report.items_added, 5);
    assert_eq!(report.items_skipped, 1);
    assert!(
        notes(&report).iter().any(|n| n.contains("fill")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn textexpander_csv_without_a_header_is_abbreviation_content_label_group() {
    let (s, _) = te(b";a,first,One,Mail\n;b,second,Two,Mail\n;c,third,Three,\n");
    let c = contents(&s);
    assert_eq!(c.len(), 2);
    assert_eq!(c[0].1, "Mail");
    assert_eq!(c[0].2[1], e("Two (;b)", ItemKind::Text, "second"));
    assert_eq!(c[1].1, "TextExpander");
}

#[test]
fn textexpander_csv_groups_come_from_a_group_column() {
    let (s, _) = te(b"Group,Abbreviation,Content\nWork,;w,work\nwork ,;x,more\nHome,;h,home\n");
    let c = contents(&s);
    let names: Vec<_> = c.iter().map(|(_, l, i)| (l.as_str(), i.len())).collect();
    assert_eq!(names, [("Work", 2), ("Home", 1)]);
}

#[test]
fn textexpander_csv_reads_utf16_with_a_bom_and_tabs() {
    let text = "Abbreviation\tContent\n;t\tcafé\n";
    let bytes: Vec<u8> = [0xFF, 0xFE]
        .into_iter()
        .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    let (s, report) = te(&bytes);
    assert_eq!(contents(&s)[0].2, [e(";t", ItemKind::Text, "café")]);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn csv_garbage_does_not_panic_and_imports_nothing() {
    for bad in [
        &b""[..],
        b"\n\n\n",
        b"Abbreviation,Content\n",
        b"\"unterminated,quote\nrest",
        b"\x00\x01\x02\xff\xfe",
        b",,,,,,\n,,,\n",
    ] {
        let (s, _) = te(bad);
        let (s2, _) = pe(bad);
        for st in [s, s2] {
            for (_, _, items) in contents(&st) {
                for (title, _, text) in items {
                    assert!(!title.is_empty() && !text.trim().is_empty());
                }
            }
        }
    }
    // An unterminated quote keeps what was read.
    let (s, _) = te(b"Abbreviation,Content\n;q,\"never closed\nstill text");
    assert_eq!(
        contents(&s)[0].2,
        [e(";q", ItemKind::Text, "never closed\nstill text")]
    );
}

#[test]
fn csv_limits_rows_cell_size_and_file_size() {
    let mut csv = String::from("Abbreviation,Content\n");
    for i in 0..100 {
        writeln!(csv, ";a{i},body {i}").unwrap();
    }
    let (s, report) = csv_with(
        snipdeck_portable::import_textexpander_csv::<Store>,
        csv.as_bytes(),
        &small(10),
    )
    .unwrap();
    assert_eq!(contents(&s)[0].2.len(), 10);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::Truncated { .. }))
    );

    let big = "x".repeat(5 * 1024 * 1024 + 1);
    let csv = format!("Abbreviation,Content\n;big,{big}\n;ok,fine\n");
    let (s, report) = te(csv.as_bytes());
    assert_eq!(contents(&s)[0].2, [e(";ok", ItemKind::Text, "fine")]);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::TooLarge { .. }))
    );

    let opts = ImportOptions {
        limits: Limits {
            max_json_bytes: 50,
            ..Limits::default()
        },
        ..ImportOptions::default()
    };
    assert!(matches!(
        csv_with(
            snipdeck_portable::import_textexpander_csv::<Store>,
            TEXTEXPANDER.as_bytes(),
            &opts
        ),
        Err(Error::LimitExceeded { .. })
    ));
}

#[test]
fn csv_many_columns_and_many_rows_stay_bounded() {
    // A million empty rows must not become a million records.
    let csv = format!("Abbreviation,Content\n{}", ",\n".repeat(1_000_000));
    let (s, _) = csv_with(
        snipdeck_portable::import_textexpander_csv::<Store>,
        csv.as_bytes(),
        &small(10),
    )
    .unwrap();
    assert!(contents(&s).is_empty());
}

const PHRASEEXPRESS: &str = "Description;Content;Autotext;Folder\n\
Greeting;\"Hello;\nthere\";hi;Mail\\Work\n\
Paste;Pasted: {#clipboard} {#caret} end;pp;Mail\\Work\n\
Stamp;On {#date} at {#time};ts;Mail\n\
Odd;Run {#shell calc} {#system}.;;\n\
Loose;no folder;;\n";

#[test]
fn phraseexpress_csv_maps_folders_autotext_and_macros() {
    let (s, report) = pe(PHRASEEXPRESS.as_bytes());
    let b = "Imported from PhraseExpress";
    assert_eq!(
        contents(&s),
        [
            (
                b.to_owned(),
                "Mail / Work".to_owned(),
                vec![
                    e("Greeting (hi)", ItemKind::Text, "Hello;\nthere"),
                    e(
                        "Paste (pp)",
                        ItemKind::Template,
                        "Pasted: {clipboard} {cursor} end"
                    ),
                ]
            ),
            (
                b.to_owned(),
                "Mail".to_owned(),
                vec![e("Stamp (ts)", ItemKind::Template, "On {date} at {time}")]
            ),
            (
                b.to_owned(),
                "PhraseExpress".to_owned(),
                vec![
                    e("Odd", ItemKind::Text, "Run {#shell calc} {#system}."),
                    e("Loose", ItemKind::Text, "no folder"),
                ]
            ),
        ]
    );
    assert!(
        notes(&report).iter().any(|n| n.contains("shell")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn phraseexpress_csv_without_a_header_is_description_then_content() {
    let (s, _) = pe(b"Hello,world body\nBye,see you\n");
    assert_eq!(
        contents(&s)[0].2,
        [
            e("Hello", ItemKind::Text, "world body"),
            e("Bye", ItemKind::Text, "see you")
        ]
    );
}

// ---------------------------------------------------------------- Espanso

const ESPANSO_BASE: &str = r#"
global_vars:
  - name: today
    type: date
    params:
      format: "%d/%m/%Y"
matches:
  - trigger: ":hi"
    replace: "Hello there"
  - triggers: [":sig", ":sign"]
    label: Signature
    replace: "Regards,\n$|$"
  - trigger: ":date"
    replace: "Today is {{mydate}} / {{today}}"
    vars:
      - name: mydate
        type: date
        params:
          format: "%Y-%m-%d"
  - trigger: ":clip"
    replace: "Pasted: {{clip}} {braces}"
    vars:
      - name: clip
        type: clipboard
  - trigger: ":shell"
    replace: "Out: {{out}}"
    vars:
      - name: out
        type: shell
        params:
          cmd: "echo hi"
  - trigger: ":echo"
    replace: "A{{e}}B {{clip}}"
    vars:
      - name: e
        type: echo
        params:
          echo: " {x} "
      - name: clip
        type: clipboard
  - trigger: ":pick"
    replace: "Colour: {{c}}"
    vars:
      - name: c
        type: choice
        params:
          values: [red, green]
  - regex: "hello(?P<n>\\d)"
    replace: "never"
  - trigger: ":form"
    form: |
      Dear [[name]], pick [[kind]] [[plain]]
    form_fields:
      kind:
        type: choice
        values: [a, b]
      plain:
        default: "dflt"
  - trigger: ":md"
    markdown: "**bold**"
  - trigger: ":img"
    image_path: "x.png"
  - trigger: ":empty"
    replace: ""
  - trigger: ":badfmt"
    replace: "{{d}}"
    vars:
      - name: d
        type: date
        params:
          format: "%Q"
  - trigger: ":twocursors"
    replace: "a $|$ b $|$"
"#;

fn espanso_in(files: &[(&str, &str)], opts: &ImportOptions<'_>) -> (Store, ImportReport) {
    let dir = tempfile::tempdir().unwrap();
    for (name, text) in files {
        let path = dir.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let mut s = store();
    let report = snipdeck_portable::import_espanso(&mut s, dir.path(), at(9), opts).unwrap();
    assert_healthy(&s);
    (s, report)
}

#[test]
fn espanso_maps_triggers_vars_forms_and_reports_the_rest() {
    let (s, report) = espanso_in(
        &[("match/base.yml", ESPANSO_BASE)],
        &ImportOptions::default(),
    );
    let c = contents(&s);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].0, "Imported from Espanso");
    assert_eq!(c[0].1, "base");
    assert_eq!(
        c[0].2,
        [
            e(":hi", ItemKind::Text, "Hello there"),
            e(
                "Signature (:sig, :sign)",
                ItemKind::Template,
                "Regards,\n{cursor}"
            ),
            e(
                ":date",
                ItemKind::Template,
                "Today is {datetime:%Y-%m-%d} / {datetime:%d/%m/%Y}"
            ),
            e(
                ":clip",
                ItemKind::Template,
                "Pasted: {clipboard} {{braces}}"
            ),
            e(":shell", ItemKind::Text, "Out: {{out}}"),
            e(":echo", ItemKind::Template, "A {{x}} B {clipboard}"),
            e(":pick", ItemKind::Template, "Colour: {choice:c|red|green}"),
            e(
                ":form",
                ItemKind::Template,
                "Dear {field:name}, pick {choice:kind|a|b} {field:plain|dflt}\n"
            ),
            e(":md", ItemKind::Text, "**bold**"),
            e(":badfmt", ItemKind::Text, "{{d}}"),
            e(":twocursors", ItemKind::Text, "a $|$ b $|$"),
        ]
    );
    // regex, image, empty
    assert_eq!(report.items_skipped, 3);
    assert_eq!(report.items_added, 11);
    let all = notes(&report).join("\n");
    for needle in ["shell", "regex", "image", "markdown", "%Q"] {
        assert!(all.contains(needle), "{needle} not in {all}");
    }
}

#[test]
fn espanso_takes_a_single_file_and_uses_the_file_name_as_the_list() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("emails.yaml");
    fs::write(&path, "matches:\n  - trigger: ':a'\n    replace: 'b'\n").unwrap();
    let mut s = store();
    snipdeck_portable::import_espanso(&mut s, &path, at(9), &ImportOptions::default()).unwrap();
    assert_eq!(contents(&s)[0].1, "emails");
}

#[test]
fn espanso_walks_a_config_folder_match_dir_only_and_skips_disabled_files() {
    let files = [
        ("config/default.yml", "toggle_key: ALT\n"),
        (
            "match/base.yml",
            "matches:\n  - trigger: ':a'\n    replace: 'A'\n",
        ),
        (
            "match/work/mail.yml",
            "matches:\n  - trigger: ':m'\n    replace: 'M'\n",
        ),
        (
            "match/_off.yml",
            "matches:\n  - trigger: ':o'\n    replace: 'O'\n",
        ),
        (
            "match/readme.txt",
            "matches:\n  - trigger: ':t'\n    replace: 'T'\n",
        ),
    ];
    let (s, report) = espanso_in(&files, &ImportOptions::default());
    let lists: Vec<_> = contents(&s)
        .into_iter()
        .map(|(_, l, i)| (l, i.len()))
        .collect();
    assert_eq!(
        lists,
        [("base".to_owned(), 1), ("work / mail".to_owned(), 1)]
    );
    assert!(
        notes(&report).iter().any(|n| n.contains("_off")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn espanso_bad_files_do_not_stop_the_good_ones() {
    let files = [
        ("a.yml", "matches: [unclosed\n  - : :\n"),
        ("b.yml", "matches:\n  - trigger: ':ok'\n    replace: fine\n"),
        ("c.yml", "just a string\n"),
        ("d.yml", "matches: 12\n"),
        ("e.yml", "matches:\n  - 5\n  - [1]\n  - trigger: ':t'\n"),
    ];
    let (s, report) = espanso_in(&files, &ImportOptions::default());
    assert_eq!(contents(&s)[0].2, [e(":ok", ItemKind::Text, "fine")]);
    assert!(report.warnings.len() >= 3, "{:?}", report.warnings);
}

#[test]
fn espanso_alias_bombs_and_deep_nesting_are_refused_cheaply() {
    // The classic "billion laughs": each level refers to the one before it ten times.
    let mut bomb = String::from("a0: &a0 [x, x, x, x, x, x, x, x, x, x]\n");
    for i in 1..12 {
        let prev = format!("*a{}", i - 1);
        let items = vec![prev; 10].join(", ");
        writeln!(bomb, "a{i}: &a{i} [{items}]").unwrap();
    }
    bomb.push_str("matches:\n  - trigger: ':ok'\n    replace: fine\n");
    let deep = format!("matches: {}{}\n", "[".repeat(10_000), "]".repeat(10_000));
    let files = [("bomb.yml", bomb.as_str()), ("deep.yml", deep.as_str())];
    let started = std::time::Instant::now();
    let (s, report) = espanso_in(&files, &ImportOptions::default());
    assert!(started.elapsed().as_secs() < 20);
    assert!(contents(&s).is_empty(), "{:?}", contents(&s));
    assert!(report.warnings.len() >= 2, "{:?}", report.warnings);

    // A modest alias is fine.
    let ok = "base: &b {replace: shared}\nmatches:\n  - {trigger: ':x', <<: *b}\n  - {trigger: ':y', replace: *x}\n";
    let _ = espanso_in(&[("ok.yml", ok)], &ImportOptions::default());
}

#[test]
fn espanso_limits_items_and_size_and_decodes_bad_bytes() {
    let mut yaml = String::from("matches:\n");
    for i in 0..50 {
        writeln!(yaml, "  - trigger: ':t{i}'\n    replace: 'v{i}'").unwrap();
    }
    let (s, report) = espanso_in(&[("m.yml", &yaml)], &small(7));
    assert_eq!(contents(&s)[0].2.len(), 7);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::Truncated { .. }))
    );

    let big = "x".repeat(5 * 1024 * 1024 + 1);
    let yaml = format!(
        "matches:\n  - trigger: ':big'\n    replace: '{big}'\n  - trigger: ':ok'\n    replace: fine\n"
    );
    let (s, report) = espanso_in(&[("m.yml", &yaml)], &ImportOptions::default());
    assert_eq!(contents(&s)[0].2, [e(":ok", ItemKind::Text, "fine")]);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::TooLarge { .. }))
    );

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("m.yml");
    fs::write(
        &path,
        b"matches:\n  - trigger: ':b'\n    replace: 'a\xffb'\n",
    )
    .unwrap();
    let mut s = store();
    let report =
        snipdeck_portable::import_espanso(&mut s, &path, at(9), &ImportOptions::default()).unwrap();
    assert!(contents(&s)[0].2[0].2.contains('\u{fffd}'));
    assert!(
        report
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::LossyText { .. }))
    );

    let opts = ImportOptions {
        limits: Limits {
            max_json_bytes: 20,
            ..Limits::default()
        },
        ..ImportOptions::default()
    };
    fs::write(&path, ESPANSO_BASE).unwrap();
    let report = snipdeck_portable::import_espanso(&mut store(), &path, at(9), &opts).unwrap();
    assert_eq!(report.items_added, 0);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::TooLarge { .. }))
    );
}

#[test]
fn espanso_missing_path_and_empty_folder() {
    let mut s = store();
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        snipdeck_portable::import_espanso(
            &mut s,
            &dir.path().join("nope"),
            at(9),
            &ImportOptions::default()
        ),
        Err(Error::File { .. })
    ));
    let report =
        snipdeck_portable::import_espanso(&mut s, dir.path(), at(9), &ImportOptions::default())
            .unwrap();
    assert_eq!(report.items_added, 0);
}
