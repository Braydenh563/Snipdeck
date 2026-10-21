//! "Rebuild from backup": the damaged database is set aside, the newest backup is restored into
//! a fresh one and what the damaged one could still be read for is merged back. Over a temporary
//! data folder and a frozen clock.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code: a failure is a failed test"
)]

use std::{fs, path::Path};

use snipdeck_app::{
    controller::{Controller, Services},
    demo,
    host::{FakeTargets, FixedClock},
    item_editor::{EditorInput, Origin},
    settings::Settings,
    toast::ToastMessage,
};
use snipdeck_core::{Entitlements, Tier, Timestamp};
use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};
use snipdeck_store::{ItemRepo, Store};

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

fn now() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

fn start(dir: &Path) -> Ctl {
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
        Entitlements::new(Tier::Pro),
    )
    .expect("controller")
}

fn titles(ctl: &Ctl) -> Vec<String> {
    ctl.store()
        .all_items()
        .expect("items")
        .into_iter()
        .map(|i| i.title)
        .collect()
}

fn add_item(ctl: &mut Ctl, title: &str) {
    ctl.open_new_item(Origin::Panel);
    let v = ctl.item_editor_view().expect("editor");
    let input = EditorInput {
        title: title.to_owned(),
        content: "written after the backup".to_owned(),
        ..v.fields.clone()
    };
    ctl.item_editor_save(&input);
    assert!(!ctl.item_editor_open());
}

fn damaged_files(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .expect("dir")
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| {
            n.starts_with("snipdeck.damaged-")
                && Path::new(n).extension().is_some_and(|e| e == "db")
        })
        .collect()
}

fn newest_backup(dir: &Path) -> std::path::PathBuf {
    snipdeck_portable::backup::list_backups(dir)
        .expect("list")
        .first()
        .expect("a backup")
        .path
        .clone()
}

#[test]
fn rebuilding_restores_the_backup_recovers_newer_items_and_keeps_the_damaged_file() {
    let dir = tempfile::tempdir().expect("dir");
    let mut ctl = start(dir.path());
    assert!(ctl.backup_now(dir.path()));
    let backed_up = titles(&ctl).len();
    add_item(&mut ctl, "Written after the backup");
    let backup = newest_backup(dir.path());

    assert!(ctl.rebuild_from_backup(dir.path(), &backup));

    let after = titles(&ctl);
    assert!(after.iter().any(|t| t == "Written after the backup"));
    // A backup never holds an unencrypted sensitive item, so it restores a little less than it was cut from.
    let Some(ToastMessage::DatabaseRebuilt {
        restored,
        recovered,
        kept,
        unreadable,
        missing_sensitive,
    }) = ctl.view().toast.map(|t| t.message)
    else {
        panic!("a rebuilt toast");
    };
    assert!(restored > 0 && restored <= backed_up);
    assert_eq!(
        recovered, 1,
        "only the newer item came from the damaged file"
    );
    assert_eq!(after.len(), restored + recovered);
    assert_eq!(
        damaged_files(dir.path()),
        vec![kept],
        "the toast names the kept file"
    );
    assert!(!unreadable);
    assert_eq!(
        missing_sensitive, 1,
        "the demo's plain-text sensitive item is in no backup, and the toast says so"
    );
    assert!(dir.path().join("snipdeck.db").exists(), "a fresh database");
}

#[test]
fn a_failure_before_the_swap_leaves_the_original_in_place() {
    let dir = tempfile::tempdir().expect("dir");
    let mut ctl = start(dir.path());
    let before = titles(&ctl);
    let missing = dir.path().join("backups").join("nope.snipdeck");

    assert!(!ctl.rebuild_from_backup(dir.path(), &missing));

    assert_eq!(titles(&ctl), before, "nothing changed");
    assert_eq!(damaged_files(dir.path()), Vec::<String>::new());
    assert!(dir.path().join("snipdeck.db").exists());
    assert!(matches!(
        ctl.view().toast.expect("an error toast").message,
        ToastMessage::RestoreFailed(_)
    ));
}

#[test]
fn after_a_rebuild_search_and_the_picker_read_the_new_store_not_the_old_one() {
    let dir = tempfile::tempdir().expect("dir");
    let mut ctl = start(dir.path());
    assert!(ctl.backup_now(dir.path()));
    let backup = newest_backup(dir.path());
    // Warm the cache with the old library, which holds an item no backup can carry.
    ctl.open_picker();
    ctl.picker_query("Server password");
    let before = ctl.picker().expect("picker").rows().len();
    assert_eq!(before, 1, "the old library has it");
    ctl.close_picker();

    assert!(ctl.rebuild_from_backup(dir.path(), &backup));

    ctl.open_picker();
    ctl.picker_query("Server password");
    assert_eq!(
        ctl.picker().expect("picker").rows().len(),
        0,
        "a stale cache would still show the old library's item"
    );
}
