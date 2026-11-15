//! Backup and export run as jobs a worker can carry: the job opens its own read connection to the
//! open library, a second request is refused while one is going, and the daily backup reaches the
//! scheduler through `Started` and `backup_finished`. Over temporary folders and a frozen clock.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code: a failure is a failed test"
)]

use snipdeck_app::{
    controller::{ArchiveEnd, Begun, Controller, Services},
    demo,
    host::{FakeTargets, FixedClock},
    maintenance::{BackupResult, Maintenance},
    settings::Settings,
};
use snipdeck_core::Timestamp;
use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};
use snipdeck_store::{ItemRepo, Store};

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
    let ArchiveEnd::Exported(Some(file)) = ctl.finish_archive(export.run()) else {
        panic!("the export is written");
    };
    assert!(file.starts_with(out.path()));
    assert!(file.metadata().expect("file").len() > 0);

    // Read back into an empty library it brings every item.
    let other = tempfile::tempdir().expect("other");
    let mut store = Store::open(other.path()).expect("store");
    snipdeck_portable::import_archive_path(
        &mut store,
        &file,
        now(),
        &snipdeck_portable::ImportOptions::default(),
    )
    .expect("import");
    assert_eq!(store.all_items().expect("items").len(), before);
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
