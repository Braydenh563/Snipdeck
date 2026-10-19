//! Settings "Import...": the preview is a dry run that writes nothing, the real import adds
//! exactly what was previewed, duplicates are counted and skipped, the Free board limit blocks,
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
use snipdeck_core::{Board, Entitlements, List, Name, Placement, Tier, Timestamp};
use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};
use snipdeck_portable::{ExportOptions, export_to_path};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, Store};

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

fn now() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

fn rig_on(store: Store, tier: Tier) -> Ctl {
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
        Entitlements::new(tier),
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
    let mut ctl = rig_on(Store::open_in_memory().unwrap(), Tier::Pro);
    let before = counts(&ctl);

    let preview = ctl.preview_import(&source).unwrap();
    assert_eq!((preview.boards, preview.lists, preview.items), (1, 1, 4));
    assert_eq!(preview.duplicates, 0);
    assert!(!preview.blocked_by_board_limit);
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
    let mut ctl = rig_on(Store::open_in_memory().unwrap(), Tier::Pro);
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
    let mut ctl = rig_on(Store::open_in_memory().unwrap(), Tier::Pro);
    let preview = ctl.preview_import(&source).unwrap();
    assert_eq!(preview.boards, 1, "an import is one new board");
    assert!(preview.lists >= 2 && preview.items > 0);
    assert_eq!(counts(&ctl), (0, 0));
    assert!(ctl.apply_import(&source));
    assert_eq!(counts(&ctl), (1, preview.items));
}

#[test]
fn a_free_library_imports_a_many_board_archive_as_one_board() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("many.snipdeck");
    // One board more than the Free plan allows.
    let mut many = demo::store(now()).unwrap();
    add_boards_up_to(&mut many, Entitlements::FREE_BOARD_LIMIT + 1);
    export_to_path(&many, now(), &ExportOptions::default(), &path).unwrap();
    let source = ImportSource::Snipdeck(path.clone());

    let mut mine = Store::open_in_memory().unwrap();
    let own = Board::new(Name::new("Mine").unwrap(), now());
    mine.insert_board(&own, Placement::Last).unwrap();
    let mut ctl = rig_on(mine, Tier::Free);
    let before = ctl.store().boards().unwrap().len();
    assert_eq!(before, 1);
    let preview = ctl.preview_import(&source).unwrap();
    assert!(!preview.blocked_by_board_limit);
    assert_eq!(preview.boards, 1);
    assert!(ctl.apply_import(&source));
    assert_eq!(ctl.store().boards().unwrap().len(), before + 1);
    // A restore keeps the boards (two with items), so this full Free library would pass the limit.
    let mut theirs = Store::open_in_memory().unwrap();
    add_boards_up_to(&mut theirs, Entitlements::FREE_BOARD_LIMIT);
    let mut other = rig_on(theirs, Tier::Free);
    assert!(other.preview_restore(&path).unwrap().blocked_by_board_limit);
    assert!(!other.restore_backup(&path));
}

#[test]
fn only_real_duplicates_are_counted_as_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let ImportSource::Folder(folder) = folder_of(dir.path(), 1) else {
        unreachable!()
    };
    fs::write(folder.join("empty.txt"), "").unwrap();
    let source = ImportSource::Folder(folder);
    let mut ctl = rig_on(Store::open_in_memory().unwrap(), Tier::Pro);
    assert!(ctl.apply_import(&source));
    let again = ctl.preview_import(&source).unwrap();
    assert_eq!((again.items, again.duplicates), (0, 1));
}

#[test]
fn the_free_board_limit_blocks_the_preview_and_the_import_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    // One board short of the Free plan's limit, so one more import fits and a second does not.
    let mut store = demo::store(now()).unwrap();
    add_boards_up_to(&mut store, Entitlements::FREE_BOARD_LIMIT - 1);
    let mut ctl = rig_on(store, Tier::Free);
    let first = folder_of(dir.path(), 2);
    assert!(!ctl.preview_import(&first).unwrap().blocked_by_board_limit);
    assert!(ctl.apply_import(&first));
    assert_eq!(
        ctl.store().boards().unwrap().len(),
        Entitlements::FREE_BOARD_LIMIT
    );

    // A fourth board is over the limit.
    let other = dir.path().join("more");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("a.txt"), "something else").unwrap();
    let second = ImportSource::Folder(other);
    let before = counts(&ctl);
    let preview = ctl.preview_import(&second).unwrap();
    assert!(preview.blocked_by_board_limit);
    assert_eq!(preview.items, 1, "the counts are still the file's");
    assert!(!ctl.apply_import(&second));
    assert_eq!(counts(&ctl), before, "a refused import writes nothing");
    let toast = ctl.main_screen().toast.expect("a toast").message;
    assert_eq!(toast, ToastMessage::BoardLimit);

    // The same import on Pro is allowed.
    let mut pro_store = demo::store(now()).unwrap();
    add_boards_up_to(&mut pro_store, Entitlements::FREE_BOARD_LIMIT - 1);
    let mut pro = rig_on(pro_store, Tier::Pro);
    assert!(pro.apply_import(&first));
    assert!(pro.apply_import(&second));
    assert_eq!(
        pro.store().boards().unwrap().len(),
        Entitlements::FREE_BOARD_LIMIT + 1
    );
}

#[test]
fn a_file_that_is_not_what_was_chosen_is_a_problem_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let junk = dir.path().join("junk.bin");
    fs::write(&junk, b"definitely not an archive").unwrap();
    let mut ctl = rig_on(Store::open_in_memory().unwrap(), Tier::Pro);
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
    let mut ctl = rig_on(Store::open_in_memory().unwrap(), Tier::Pro);
    let preview = ctl.preview_restore(&path).unwrap();
    assert!(ctl.restore_backup(&path));
    assert_eq!(counts(&ctl), (preview.boards, preview.items));
    assert!(!ctl.preview_restore(&path).unwrap().blocked_by_board_limit);
}
