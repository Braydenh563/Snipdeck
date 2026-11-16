//! Backup and export run as jobs a worker can carry: the job opens its own read connection to the
//! open library, a second request is refused while one is going, and the daily backup reaches the
//! scheduler through `Started` and `backup_finished`. Over temporary folders and a frozen clock.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code: a failure is a failed test"
)]

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use snipdeck_app::{
    archive_job::{self, Inline, Threads},
    controller::{ArchiveEnd, Begun, Controller, MainAction, Services, spawn_archive},
    demo,
    failure::Cause,
    host::{FakeTargets, FixedClock},
    item_editor::{EditorInput, Origin},
    maintenance::{BackupResult, Maintenance},
    settings::Settings,
    toast::ToastMessage,
};
use snipdeck_core::{Placement, Timestamp};
use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};
use snipdeck_store::{BlobStore, BoardRepo, ItemRepo, ListRepo, Store};

#[path = "support/mod.rs"]
mod support;
use support::on_ui_thread;

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

fn now() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

fn start(dir: &std::path::Path) -> Ctl {
    let mut store = Store::open(dir).expect("store");
    demo::seed(&mut store, now()).expect("seed");
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

fn job(begun: Begun) -> snipdeck_app::archive_job::ArchiveJob {
    match begun {
        Begun::Job(job) => job,
        other => panic!("an on-disk library hands out a job, got {other:?}"),
    }
}

#[test]
fn an_open_library_hands_the_backup_to_a_worker_and_a_second_request_waits() {
    let dir = tempfile::tempdir().expect("dir");
    let mut ctl = start(dir.path());
    assert!(!ctl.archive_busy());

    let backup = job(ctl.begin_backup_now(dir.path()));
    assert!(ctl.archive_busy());
    assert!(matches!(ctl.begin_backup_now(dir.path()), Begun::Refused));
    let out = tempfile::tempdir().expect("out");
    assert!(matches!(ctl.begin_export(out.path()), Begun::Refused));

    // The worker's half: no borrow of the controller, its own connection.
    let done = backup.run();
    assert!(matches!(
        ctl.finish_archive(done),
        ArchiveEnd::BackupNow(true)
    ));
    assert!(!ctl.archive_busy());
    assert_eq!(
        snipdeck_portable::backup::list_backups(dir.path())
            .expect("list")
            .len(),
        1
    );
}

fn add_item(ctl: &mut Ctl, title: &str) {
    ctl.open_new_item(Origin::Panel);
    let v = ctl.item_editor_view().expect("editor");
    ctl.item_editor_save(&EditorInput {
        title: title.to_owned(),
        content: "written after the export began".to_owned(),
        ..v.fields.clone()
    });
    assert!(!ctl.item_editor_open());
}

fn titles(path: &std::path::Path) -> Vec<String> {
    let other = tempfile::tempdir().expect("other");
    let mut store = Store::open(other.path()).expect("store");
    snipdeck_portable::import_archive_path(
        &mut store,
        path,
        now(),
        &snipdeck_portable::ImportOptions::default(),
    )
    .expect("import");
    store
        .all_items()
        .expect("items")
        .into_iter()
        .map(|i| i.title)
        .collect()
}

#[test]
fn an_export_cut_on_a_worker_holds_the_library_as_it_was_when_it_began() {
    let dir = tempfile::tempdir().expect("dir");
    let mut ctl = start(dir.path());
    // An export leaves the sensitive items out.
    let before = ctl
        .store()
        .all_items()
        .expect("items")
        .iter()
        .filter(|i| !i.sensitive)
        .count();
    assert!(before > 0);
    let out = tempfile::tempdir().expect("out");

    let export = job(ctl.begin_export(out.path()));
    // The library changes between the begin and the worker's run.
    add_item(&mut ctl, "Written after the export began");
    let ArchiveEnd::Exported(Some(file)) = ctl.finish_archive(export.run()) else {
        panic!("the export is written");
    };
    assert!(file.starts_with(out.path()));
    assert!(file.metadata().expect("file").len() > 0);

    let written = titles(&file);
    assert_eq!(written.len(), before, "the library as it was at the begin");
    assert!(
        !written
            .iter()
            .any(|t| t == "Written after the export began")
    );
    assert!(
        ctl.store()
            .all_items()
            .expect("items")
            .iter()
            .any(|i| i.title == "Written after the export began"),
        "the live library kept the new item"
    );
}

#[test]
fn replacing_an_items_big_content_while_a_backup_runs_does_not_break_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut store = Store::open(dir.path()).expect("store");
    demo::seed(&mut store, now()).expect("seed");
    let list = store
        .lists(store.boards().expect("boards")[0].id)
        .expect("lists")[0]
        .id;
    let content = store
        .put(vec![b'x'; snipdeck_core::INLINE_LIMIT + 1])
        .expect("blob");
    let item =
        snipdeck_core::Item::new(list, "Big".to_owned(), snipdeck_core::ItemKind::Text, now());
    let formats =
        snipdeck_core::Formats::new(vec![snipdeck_core::Format::Text(content)]).expect("formats");
    store
        .insert_item(&item, &formats, Placement::Last)
        .expect("item");
    let mut ctl = Controller::new(
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
    .expect("controller");

    let out = tempfile::tempdir().expect("out");
    let export = job(ctl.begin_export(out.path()));
    // The item is deleted for good: its blob file is orphaned, and used to be deleted at once,
    // under the worker's snapshot.
    let screen = ctl.main_screen();
    let at = screen
        .rows
        .iter()
        .position(|r| r.title == "Big")
        .expect("the item is listed");
    ctl.main_action(MainAction::Delete, at);
    ctl.main_empty_trash();
    assert_eq!(
        walk_files(&dir.path().join("blobs")),
        1,
        "held for the worker"
    );
    let ArchiveEnd::Exported(Some(file)) = ctl.finish_archive(export.run()) else {
        panic!("the export reads the blob it began with");
    };
    assert!(titles(&file).iter().any(|t| t == "Big"));
    // The hold is over: the orphaned file is gone.
    assert_eq!(
        walk_files(&dir.path().join("blobs")),
        0,
        "no blob file is left"
    );
}

fn walk_files(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir).map_or(0, |d| {
        d.flatten()
            .map(|e| {
                if e.path().is_dir() {
                    walk_files(&e.path())
                } else {
                    1
                }
            })
            .sum()
    })
}

#[test]
fn a_second_request_while_one_runs_says_so_and_the_daily_backup_defers_as_busy() {
    let dir = tempfile::tempdir().expect("dir");
    let mut ctl = start(dir.path());
    let running = job(ctl.begin_backup_now(dir.path()));
    assert!(ctl.archive_busy());

    assert!(matches!(ctl.begin_backup_now(dir.path()), Begun::Refused));
    assert_eq!(
        ctl.view().toast.map(|t| t.message),
        Some(ToastMessage::ArchiveBusy)
    );
    let out = tempfile::tempdir().expect("out");
    assert!(matches!(ctl.begin_export(out.path()), Begun::Refused));

    let now_zoned = now().to_zoned(jiff::tz::TimeZone::UTC);
    let err = ctl
        .begin_daily_backup(dir.path(), &now_zoned)
        .expect_err("deferred behind the running backup");
    assert_eq!(err.cause, Cause::Busy);
    assert!(err.detail.contains("another backup or export"), "{err}");
    assert_ne!(err, snipdeck_app::maintenance::JobError::deferred());

    ctl.finish_archive(running.run());
    assert!(!ctl.archive_busy());
}

#[test]
fn the_daily_backup_through_spawn_backup_updates_the_scheduler() {
    let dir = tempfile::tempdir().expect("dir");
    let ctl = Rc::new(RefCell::new(start(dir.path())));
    let maintenance = Rc::new(RefCell::new(Maintenance::new(
        dir.path(),
        Box::new(FixedClock::new(now())),
    )));
    let job = {
        let mut c = ctl.borrow_mut();
        let mut m = maintenance.borrow_mut();
        snipdeck_app::maintenance::run(&mut m, &mut c)
            .backup
            .expect("a job")
    };
    snipdeck_app::maintenance::spawn_backup(job, &maintenance, &ctl);
    // The inline offload has finished it: the scheduler noted the backup and the controller is free.
    assert!(!ctl.borrow().archive_busy());
    assert!(dir.path().join("maintenance.json").exists());
    assert_eq!(
        snipdeck_portable::backup::list_backups(dir.path())
            .expect("list")
            .len(),
        1
    );
}

#[test]
fn the_threads_offload_hands_the_outcome_back_on_the_event_loop() {
    on_ui_thread(|| {
        archive_job::set_offload(Rc::new(Threads));
        let dir = tempfile::tempdir().expect("dir");
        let ctl = Rc::new(RefCell::new(start(dir.path())));
        let begun = ctl.borrow_mut().begin_backup_now(dir.path());
        let result = Rc::new(Cell::new(None));
        let seen = Rc::clone(&result);
        spawn_archive(&ctl, begun, move |end| {
            seen.set(Some(matches!(end, ArchiveEnd::BackupNow(true))));
            let _ = slint::quit_event_loop();
        });
        assert!(ctl.borrow().archive_busy(), "busy until the loop delivers");
        slint::run_event_loop().expect("loop");
        assert_eq!(result.get(), Some(true));
        assert!(!ctl.borrow().archive_busy());
        archive_job::set_offload(Rc::new(Inline));
    });
}

#[test]
fn the_daily_backup_goes_to_a_worker_and_the_scheduler_waits_for_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut ctl = start(dir.path());
    let clock = FixedClock::new(now());
    let mut m = Maintenance::new(dir.path(), Box::new(clock.clone()));

    let ran = snipdeck_app::maintenance::run(&mut m, &mut ctl);
    let backup = ran.backup.expect("the daily backup is a job");
    assert!(ctl.archive_busy());

    // A tick while it runs starts nothing else.
    let again = snipdeck_app::maintenance::run(&mut m, &mut ctl);
    assert!(again.backup.is_none());

    let ArchiveEnd::Daily(result) = ctl.finish_archive(backup.run()) else {
        panic!("a daily end");
    };
    let outcomes = m.backup_finished(result.map(BackupResult::from));
    assert_eq!(outcomes.len(), 1, "one BackupCreated");
    assert!(dir.path().join("maintenance.json").exists());
    assert_eq!(
        snipdeck_portable::backup::list_backups(dir.path())
            .expect("list")
            .len(),
        1
    );
}

#[test]
fn quitting_mid_export_removes_only_the_part_file_of_that_export() {
    let dir = tempfile::tempdir().expect("dir");
    let mut ctl = start(dir.path());
    let out = tempfile::tempdir().expect("out");
    let export = job(ctl.begin_export(out.path()));
    let target = export.path().expect("an export path").to_path_buf();
    let mut part = target.clone().into_os_string();
    part.push(".part");
    let part = std::path::PathBuf::from(part);
    std::fs::write(&part, b"half").expect("part");
    let other = out.path().join("someone-elses.snipdeck.part");
    std::fs::write(&other, b"not ours").expect("other");

    ctl.abandon_unfinished_archive();
    assert!(!part.exists(), "this export's leftover is removed");
    assert!(other.exists(), "another .part is left alone");
}

/// A folder of text files, one per entry of `texts`.
fn notes(dir: &std::path::Path, texts: &[&str]) -> snipdeck_app::controller::ImportSource {
    let folder = dir.join("notes");
    std::fs::create_dir_all(&folder).expect("folder");
    for (i, text) in texts.iter().enumerate() {
        std::fs::write(folder.join(format!("n{i}.txt")), text).expect("note");
    }
    snipdeck_app::controller::ImportSource::Folder(folder)
}

fn item_count(ctl: &Ctl) -> usize {
    ctl.store().all_items().expect("items").len()
}

#[test]
fn an_import_reads_on_a_worker_previews_the_same_as_it_adds_and_a_second_start_waits() {
    let dir = tempfile::tempdir().expect("dir");
    let mut ctl = start(dir.path());
    let src = tempfile::tempdir().expect("src");
    let source = notes(src.path(), &["alpha text", "beta text"]);
    let before = item_count(&ctl);

    let preview = job(ctl.begin_import_preview(&source));
    assert!(ctl.archive_busy());
    assert!(matches!(ctl.begin_apply_import(&source), Begun::Refused));
    let ArchiveEnd::ImportPreview(Ok(preview)) = ctl.finish_archive(preview.run()) else {
        panic!("a preview");
    };
    assert_eq!(preview.items, 2);
    assert_eq!(item_count(&ctl), before, "a preview writes nothing");

    let apply = job(ctl.begin_apply_import(&source));
    assert!(matches!(
        ctl.finish_archive(apply.run()),
        ArchiveEnd::Imported(true)
    ));
    assert_eq!(item_count(&ctl), before + preview.items);
    assert!(!ctl.archive_busy());
}

#[test]
fn an_item_made_between_the_read_and_the_apply_is_still_skipped_as_a_duplicate() {
    let dir = tempfile::tempdir().expect("dir");
    let mut ctl = start(dir.path());
    let src = tempfile::tempdir().expect("src");
    let source = notes(
        src.path(),
        &["written after the export began", "something else"],
    );
    let before = item_count(&ctl);

    let apply = job(ctl.begin_apply_import(&source));
    // Typed while the worker reads: the file's first text is now in the library.
    add_item(&mut ctl, "typed meanwhile");
    assert!(matches!(
        ctl.finish_archive(apply.run()),
        ArchiveEnd::Imported(true)
    ));
    assert_eq!(
        item_count(&ctl),
        before + 1 + 1,
        "the typed one and one new"
    );
}

#[test]
fn a_corrupt_source_fails_and_leaves_the_library_untouched() {
    let dir = tempfile::tempdir().expect("dir");
    let mut ctl = start(dir.path());
    let src = tempfile::tempdir().expect("src");
    let file = src.path().join("broken.snipdeck");
    std::fs::write(&file, b"this is not an archive").expect("file");
    let source = snipdeck_app::controller::ImportSource::Snipdeck(file.clone());
    let before = item_count(&ctl);

    let apply = job(ctl.begin_apply_import(&source));
    assert!(matches!(
        ctl.finish_archive(apply.run()),
        ArchiveEnd::Imported(false)
    ));
    assert_eq!(item_count(&ctl), before);
    let restore = job(ctl.begin_restore_backup(&file));
    assert!(matches!(
        ctl.finish_archive(restore.run()),
        ArchiveEnd::Imported(false)
    ));
    assert_eq!(item_count(&ctl), before);
    assert!(!ctl.archive_busy());
}

#[test]
fn quitting_while_a_source_is_being_read_writes_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut ctl = start(dir.path());
    let src = tempfile::tempdir().expect("src");
    let source = notes(src.path(), &["one", "two"]);
    let before = item_count(&ctl);

    let apply = job(ctl.begin_apply_import(&source));
    ctl.abandon_unfinished_archive();
    drop(ctl);
    // The worker finishes after the controller is gone: nobody applies it.
    let _done = apply.run();
    let store = Store::open(dir.path()).expect("reopen");
    assert_eq!(store.all_items().expect("items").len(), before);
}
