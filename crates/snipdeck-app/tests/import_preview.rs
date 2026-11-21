//! Settings "Import...": the preview is a dry run that writes nothing, the real import adds
//! exactly what was previewed, duplicates are counted and skipped,
//! and a file that is not what was chosen gives a problem instead of a panic. Frozen clock, no
//! window.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use std::{fs, path::Path};

use snipdeck_app::{
    controller::{Controller, ImportKind, ImportProblem, ImportSource, Services},
    demo,
    host::{FakeTargets, FixedClock},
    settings::Settings,
    toast::ToastMessage,
};
use snipdeck_core::{Board, List, Name, Placement, Timestamp};
use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};
use snipdeck_portable::{ExportOptions, HistoryEntry, export_to_path};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, Store};

#[path = "support/mod.rs"]
mod support;
use support::ArchiveSteps as _;

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

fn now() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

fn rig_on(store: Store) -> Ctl {
    Controller::new(
        store,
        Services {
            clipboard: FakeClipboard::default(),
            keyboard: FakeKeyboard::default(),
            timing: FakeTiming::default(),
        },
        Box::new(FakeTargets::new(Some(demo::target()))),
        Box::new(FixedClock::new(now())),
        Settings::default(),
    )
    .expect("controller")
}

fn counts(ctl: &Ctl) -> (usize, usize) {
    (
        ctl.store().boards().unwrap().len(),
        ctl.store().all_items().unwrap().len(),
    )
}

/// A folder of `n` distinct text files.
fn folder_of(dir: &Path, n: usize) -> ImportSource {
    let folder = dir.join("notes");
    fs::create_dir_all(&folder).unwrap();
    for i in 0..n {
        fs::write(
            folder.join(format!("note{i}.txt")),
            format!("text number {i}"),
        )
        .unwrap();
    }
    ImportSource::Folder(folder)
}

/// An export of the demo library, in `dir`.
fn snipdeck_file(dir: &Path) -> ImportSource {
    let path = dir.join("demo.snipdeck");
    let store = demo::store(now()).unwrap();
    export_to_path(&store, now(), &ExportOptions::default(), &path).unwrap();
    ImportSource::Snipdeck(path)
}

#[test]
fn the_preview_writes_nothing_and_the_import_adds_exactly_what_it_showed() {
    let dir = tempfile::tempdir().unwrap();
    let source = folder_of(dir.path(), 4);
    let mut ctl = rig_on(Store::open_in_memory().unwrap());
    let before = counts(&ctl);

    let preview = ctl.preview_import(&source).unwrap();
    assert_eq!((preview.boards, preview.lists, preview.items), (1, 1, 4));
    assert_eq!(preview.duplicates, 0);
    assert_eq!(counts(&ctl), before, "a preview changes nothing");
    assert_eq!(
        ctl.preview_import(&source).unwrap(),
        preview,
        "and is repeatable"
    );

    assert!(ctl.apply_import(&source));
    let after = counts(&ctl);
    assert_eq!(after.0, before.0 + preview.boards);
    assert_eq!(after.1, before.1 + preview.items);
}

#[test]
fn what_is_already_in_the_library_is_counted_and_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let source = folder_of(dir.path(), 3);
    let mut ctl = rig_on(Store::open_in_memory().unwrap());
    assert!(ctl.apply_import(&source));
    let after_first = counts(&ctl);

    let again = ctl.preview_import(&source).unwrap();
    assert_eq!((again.items, again.duplicates), (0, 3));
    assert!(again.is_empty());

    // One new file among the old ones.
    fs::write(dir.path().join("notes").join("new.txt"), "brand new").unwrap();
    let mixed = ctl.preview_import(&source).unwrap();
    assert_eq!((mixed.items, mixed.duplicates), (1, 3));

    assert!(ctl.apply_import(&source));
    assert_eq!(counts(&ctl).1, after_first.1 + 1);
}

/// More boards than any one person is likely to keep: nothing caps the count.
const MANY_BOARDS: usize = 12;

/// Adds boards (each with one list) until the store holds `total`.
fn add_boards_up_to(store: &mut Store, total: usize) {
    let mut n = 0;
    while store.boards().unwrap().len() < total {
        n += 1;
        let board = Board::new(Name::new(&format!("Extra {n}")).unwrap(), now());
        store.insert_board(&board, Placement::Last).unwrap();
        let list = List::new(board.id, Name::new("L").unwrap(), now());
        store.insert_list(&list, Placement::Last).unwrap();
    }
}

#[test]
fn a_snipdeck_file_previews_its_boards_and_imports_them() {
    let dir = tempfile::tempdir().unwrap();
    let source = snipdeck_file(dir.path());
    let mut ctl = rig_on(Store::open_in_memory().unwrap());
    let preview = ctl.preview_import(&source).unwrap();
    assert_eq!(preview.boards, 1, "an import is one new board");
    assert!(preview.lists >= 2 && preview.items > 0);
    assert_eq!(counts(&ctl), (0, 0));
    assert!(ctl.apply_import(&source));
    assert_eq!(counts(&ctl), (1, preview.items));
}

#[test]
fn a_many_board_archive_imports_as_one_board_and_a_big_library_is_never_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("many.snipdeck");
    let mut many = demo::store(now()).unwrap();
    add_boards_up_to(&mut many, MANY_BOARDS + 1);
    export_to_path(&many, now(), &ExportOptions::default(), &path).unwrap();
    let source = ImportSource::Snipdeck(path.clone());

    // A library that already holds many boards (none of the archive's items): nothing caps it.
    let mut mine = Store::open_in_memory().unwrap();
    add_boards_up_to(&mut mine, MANY_BOARDS);
    let mut ctl = rig_on(mine);
    let before = ctl.store().boards().unwrap().len();
    assert_eq!(before, MANY_BOARDS);
    let preview = ctl.preview_import(&source).unwrap();
    assert_eq!(preview.boards, 1, "an import is one new board");
    assert!(ctl.apply_import(&source));
    assert_eq!(ctl.store().boards().unwrap().len(), before + 1);
    // A restore keeps every board of the backup.
    assert!(ctl.preview_restore(&path).is_ok());
    assert!(ctl.restore_backup(&path));
}

#[test]
fn a_history_import_adds_a_board_to_a_library_of_many() {
    let mut store = Store::open_in_memory().unwrap();
    add_boards_up_to(&mut store, MANY_BOARDS);
    let mut ctl = rig_on(store);
    let before = counts(&ctl);
    assert!(ctl.apply_import(&history_of(&["one"])));
    assert_eq!(counts(&ctl).0, before.0 + 1, "one more board, no limit");
}

#[test]
fn only_real_duplicates_are_counted_as_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let ImportSource::Folder(folder) = folder_of(dir.path(), 1) else {
        unreachable!()
    };
    fs::write(folder.join("empty.txt"), "").unwrap();
    let source = ImportSource::Folder(folder);
    let mut ctl = rig_on(Store::open_in_memory().unwrap());
    assert!(ctl.apply_import(&source));
    let again = ctl.preview_import(&source).unwrap();
    assert_eq!((again.items, again.duplicates), (0, 1));
}

#[test]
fn each_import_kind_round_trips_through_of_and_kind() {
    for kind in [
        ImportKind::Snipdeck,
        ImportKind::Ditto,
        ImportKind::CopyQ,
        ImportKind::Folder,
        ImportKind::Espanso,
        ImportKind::Beeftext,
        ImportKind::TextExpander,
        ImportKind::PhraseExpress,
    ] {
        let source = ImportSource::of(kind, "x".into());
        assert_eq!(source.kind(), Some(kind));
        assert_eq!(source.path(), Some(Path::new("x")));
    }
}

#[test]
fn a_malformed_beeftext_file_is_not_this_kind() {
    let dir = tempfile::tempdir().unwrap();
    let junk = dir.path().join("junk.json");
    fs::write(&junk, b"{ not json").unwrap();
    let mut ctl = rig_on(Store::open_in_memory().unwrap());
    for (source, kind) in [
        (ImportSource::Beeftext(junk.clone()), ImportKind::Beeftext),
        (
            ImportSource::TextExpander(dir.path().join("none.csv")),
            ImportKind::TextExpander,
        ),
    ] {
        let got = ctl.preview_import(&source);
        if kind == ImportKind::Beeftext {
            assert_eq!(got, Err(ImportProblem::NotThisKind(kind)));
        } else {
            assert_eq!(got, Err(ImportProblem::Unreadable));
        }
    }
}

#[test]
fn a_file_that_is_not_what_was_chosen_is_a_problem_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let junk = dir.path().join("junk.bin");
    fs::write(&junk, b"definitely not an archive").unwrap();
    let mut ctl = rig_on(Store::open_in_memory().unwrap());
    let before = counts(&ctl);

    for (source, problem) in [
        (
            ImportSource::Snipdeck(junk.clone()),
            ImportProblem::NotThisKind(ImportKind::Snipdeck),
        ),
        (
            ImportSource::Ditto(junk.clone()),
            ImportProblem::NotThisKind(ImportKind::Ditto),
        ),
        (
            ImportSource::CopyQ(junk.clone()),
            ImportProblem::NotThisKind(ImportKind::CopyQ),
        ),
        (
            ImportSource::Snipdeck(dir.path().join("missing.snipdeck")),
            ImportProblem::Unreadable,
        ),
        (
            ImportSource::Folder(dir.path().join("no-such-folder")),
            ImportProblem::Unreadable,
        ),
    ] {
        assert_eq!(ctl.preview_import(&source), Err(problem), "{source:?}");
        assert!(!ctl.apply_import(&source));
    }
    assert_eq!(counts(&ctl), before);
}

#[test]
fn restore_keeps_the_backups_boards() {
    let dir = tempfile::tempdir().unwrap();
    let ImportSource::Snipdeck(path) = snipdeck_file(dir.path()) else {
        unreachable!()
    };
    let mut ctl = rig_on(Store::open_in_memory().unwrap());
    let preview = ctl.preview_restore(&path).unwrap();
    assert!(ctl.restore_backup(&path));
    assert_eq!(counts(&ctl), (preview.boards, preview.items));
}

/// Clipboard-history entries as the platform layer hands them over, newest first.
fn history_of(texts: &[&str]) -> ImportSource {
    ImportSource::ClipboardHistory {
        entries: texts
            .iter()
            .map(|text| HistoryEntry {
                text: (*text).to_owned(),
                timestamp: None,
            })
            .collect(),
        board: "Zwischenablageverlauf".to_owned(),
    }
}

#[test]
fn the_clipboard_history_previews_and_imports_through_the_same_path_as_a_file() {
    let source = history_of(&["first copy", "second copy", "third copy"]);
    assert!(source.path().is_none() && source.kind().is_none());
    let mut ctl = rig_on(Store::open_in_memory().unwrap());
    let before = counts(&ctl);

    let preview = ctl.preview_import(&source).unwrap();
    assert_eq!((preview.boards, preview.lists, preview.items), (1, 1, 3));
    assert_eq!(counts(&ctl), before, "a preview changes nothing");

    assert!(ctl.apply_import(&source));
    assert_eq!(counts(&ctl), (before.0 + 1, before.1 + 3));
    assert_eq!(
        ctl.main_screen().toast.expect("a toast").message,
        ToastMessage::Imported { items: 3 }
    );
}

#[test]
fn a_second_history_import_skips_what_is_already_there_and_blank_entries_are_listed() {
    let mut ctl = rig_on(Store::open_in_memory().unwrap());
    assert!(ctl.apply_import(&history_of(&["alpha", "beta"])));
    let after_first = counts(&ctl);

    let again = history_of(&["alpha", "beta", "gamma", "   "]);
    let preview = ctl.preview_import(&again).unwrap();
    assert_eq!((preview.items, preview.duplicates), (1, 2));
    assert_eq!(
        preview.warnings.len(),
        1,
        "the blank entry is listed as left out"
    );
    assert!(ctl.apply_import(&again));
    assert_eq!(counts(&ctl).1, after_first.1 + 1);
}

#[test]
fn a_history_import_names_its_board_as_the_caller_said() {
    let mut ctl = rig_on(Store::open_in_memory().unwrap());
    assert!(ctl.apply_import(&history_of(&["plain text"])));
    let names: Vec<String> = ctl
        .store()
        .boards()
        .unwrap()
        .iter()
        .map(|b| b.name.to_string())
        .collect();
    assert!(
        names.iter().any(|n| n == "Zwischenablageverlauf"),
        "{names:?}"
    );
}

#[test]
fn a_history_of_only_blank_entries_previews_as_nothing_to_import() {
    let mut ctl = rig_on(Store::open_in_memory().unwrap());
    let preview = ctl.preview_import(&history_of(&["  ", "\n"])).unwrap();
    assert!(preview.is_empty());
    assert_eq!(preview.warnings.len(), 2);
}
