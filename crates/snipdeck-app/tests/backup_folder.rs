//! The backups folder is the user's choice and "Backups to keep" takes effect when it is
//! changed (DATA-13, DATA-17). Fixed clock: nothing here reads the real date.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]

use snipdeck_app::{
    controller::{Controller, Services},
    demo,
    host::{FakeTargets, FixedClock},
    settings::Settings,
};
use snipdeck_core::Timestamp;
use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};
use snipdeck_store::Store;

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

fn now() -> Timestamp {
    "2026-10-17T12:00:00Z".parse().expect("timestamp")
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

fn plant(folder: &std::path::Path, days: std::ops::RangeInclusive<u32>) {
    std::fs::create_dir_all(folder).unwrap();
    for day in days {
        std::fs::write(
            folder.join(format!("snipdeck-backup-202601{day:02}.snipdeck")),
            b"x",
        )
        .unwrap();
    }
}

fn count(folder: &std::path::Path) -> usize {
    std::fs::read_dir(folder).unwrap().count()
}

#[test]
fn lowering_keep_removes_the_extra_copies_at_once_in_the_chosen_folder() {
    let data = tempfile::tempdir().unwrap();
    let chosen = tempfile::tempdir().unwrap().keep();
    let mut ctl = start(data.path());
    ctl.set_backup_folder(chosen.to_str().unwrap());
    plant(&chosen, 1..=10);
    assert_eq!(ctl.settings().backup_folder(data.path()), chosen);

    let retained = ctl.set_backups_keep(3, data.path());
    assert_eq!(retained.removed.len(), 7, "{retained:?}");
    assert_eq!(count(&chosen), 3);
    // The default folder was never touched.
    assert!(!data.path().join("backups").exists());
}

#[test]
fn an_empty_folder_setting_means_the_default_and_a_blank_is_not_a_folder() {
    let data = tempfile::tempdir().unwrap();
    let mut ctl = start(data.path());
    ctl.set_backup_folder("   ");
    assert_eq!(
        ctl.settings().backup_folder(data.path()),
        snipdeck_portable::backup::backups_dir(data.path())
    );
}
