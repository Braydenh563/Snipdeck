//! "Rebuild from backup": the damaged database is set aside, the newest backup is restored into
//! a fresh one and what the damaged one could still be read for is merged back. Over a temporary
//! data folder and a frozen clock.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code: a failure is a failed test"
)]

use std::{fs, path::Path};

mod support;
use support::ArchiveSteps as _;

use snipdeck_app::{
    controller::{Controller, MainAction, Services},
    demo,
    host::{FakeTargets, FixedClock},
    item_editor::{EditorInput, Origin},
    settings::Settings,
    toast::ToastMessage,
};
use snipdeck_core::{Placement, Timestamp};
use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};
use snipdeck_store::{BlobStore, BoardRepo, ItemRepo, ListRepo, Store};

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

fn now() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

fn start(dir: &Path) -> Ctl {
    let mut store = Store::open(dir).expect("store");
    demo::seed(&mut store, now()).expect("seed");
    start_with(store)
}

fn start_with(store: Store) -> Ctl {
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

/// Inserts a text item into `list`; `big` bytes of content go to a blob file when over the limit.
fn add_text(store: &mut Store, list: snipdeck_core::ListId, title: &str, big: Option<usize>) {
    let content = match big {
        Some(len) => store.put(vec![b'x'; len]).expect("blob"),
        None => snipdeck_core::Content::Inline(
            snipdeck_core::Inline::new(title.as_bytes().to_vec()).expect("inline"),
        ),
    };
    let item =
        snipdeck_core::Item::new(list, title.to_owned(), snipdeck_core::ItemKind::Text, now());
    let formats =
        snipdeck_core::Formats::new(vec![snipdeck_core::Format::Text(content)]).expect("formats");
    store
        .insert_item(&item, &formats, Placement::Last)
        .expect("item");
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
        .filter(|n| n.starts_with("snipdeck.damaged-"))
        .collect()
}

/// No rebuild marker is left in the data folder.
fn assert_no_marker(dir: &Path) {
    assert!(
        !dir.join("snipdeck.rebuild").exists(),
        "the rebuild marker is gone"
    );
    assert!(!dir.join("snipdeck.rebuild.tmp").exists());
}

fn newest_backup(dir: &Path) -> std::path::PathBuf {
    snipdeck_portable::backup::list_backups(&snipdeck_portable::backup::backups_dir(dir))
        .expect("list")
        .first()
        .expect("a backup")
        .path
        .clone()
}

#[test]
fn rebuilding_restores_the_backup_recovers_newer_items_and_keeps_the_damaged_folder() {
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
    let kept_name = kept.clone();
    assert_eq!(
        damaged_files(dir.path()),
        vec![kept],
        "the toast names the kept folder"
    );
    assert!(!unreadable);
    assert_eq!(
        missing_sensitive, 1,
        "the demo's plain-text sensitive item is in no backup, and the toast says so"
    );
    assert!(dir.path().join("snipdeck.db").exists(), "a fresh database");
    assert_no_marker(dir.path());
    // The damaged library is a folder holding its database and blobs, and it opens.
    let aside = dir.path().join(&kept_name);
    assert!(aside.join("snipdeck.db").is_file());
    let old = Store::open(&aside).expect("the set-aside library opens");
    let old_titles: Vec<String> = old
        .all_items()
        .expect("items")
        .into_iter()
        .map(|i| i.title)
        .collect();
    assert!(old_titles.iter().any(|t| t == "Written after the backup"));
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
    assert_no_marker(dir.path());
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

fn blob_files(dir: &Path) -> usize {
    fn walk(dir: &Path) -> usize {
        fs::read_dir(dir).map_or(0, |entries| {
            entries
                .flatten()
                .map(|e| {
                    if e.path().is_dir() {
                        walk(&e.path())
                    } else {
                        1
                    }
                })
                .sum()
        })
    }
    walk(&dir.join("blobs"))
}

#[test]
fn after_a_rebuild_deleting_and_sweeping_keeps_the_damaged_copys_blobs() {
    let dir = tempfile::tempdir().expect("dir");
    let mut store = Store::open(dir.path()).expect("store");
    demo::seed(&mut store, now()).expect("seed");
    let list = store
        .lists(store.boards().expect("boards")[0].id)
        .expect("lists")[0]
        .id;
    add_text(
        &mut store,
        list,
        "A big one",
        Some(snipdeck_core::INLINE_LIMIT + 1),
    );
    let mut ctl = start_with(store);
    assert!(ctl.backup_now(dir.path()));
    let backup = newest_backup(dir.path());
    assert!(ctl.rebuild_from_backup(dir.path(), &backup));
    let kept = damaged_files(dir.path()).pop().expect("a kept folder");
    let aside = dir.path().join(kept);
    let aside_blobs = blob_files(&aside);
    assert!(
        aside_blobs > 0,
        "the damaged copy holds the big item's blob"
    );
    assert!(aside.join("snipdeck.db").is_file());

    // Delete the recovered item for good in the fresh library, then sweep its blobs.
    let screen = ctl.main_screen();
    let at = screen
        .rows
        .iter()
        .position(|r| r.title == "A big one")
        .expect("the recovered item is listed");
    ctl.main_action(MainAction::Delete, at);
    ctl.main_empty_trash();
    ctl.sweep_blobs(std::time::Duration::ZERO).expect("sweep");

    assert_eq!(
        blob_files(&aside),
        aside_blobs,
        "the aside blobs are untouched"
    );
    let old = Store::open(&aside).expect("the set-aside library opens");
    let big = old
        .all_items()
        .expect("items")
        .into_iter()
        .find(|i| i.title == "A big one")
        .expect("item");
    let formats = old.formats(big.id).expect("formats");
    let snipdeck_core::Format::Text(content) = formats.primary() else {
        panic!("a text item");
    };
    let bytes = old.read(content).expect("blob reads");
    assert_eq!(bytes.len(), snipdeck_core::INLINE_LIMIT + 1);
}

#[test]
fn an_item_written_while_the_salvage_is_copied_is_recovered_too() {
    use snipdeck_app::controller::{ArchiveEnd, Begun};
    let dir = tempfile::tempdir().expect("dir");
    let mut ctl = start(dir.path());
    assert!(ctl.backup_now(dir.path()));
    let backup = newest_backup(dir.path());

    let Begun::Job(job) = ctl.begin_rebuild_from_backup(dir.path(), &backup) else {
        panic!("an on-disk library hands the salvage to a worker");
    };
    // The worker has cut its copy; then the user keeps typing before the swap.
    let done = job.run();
    add_item(&mut ctl, "Typed during the salvage");
    // The copy is stale: the controller asks for it to be cut again on a worker.
    let ArchiveEnd::Again(again) = ctl.finish_archive(done) else {
        panic!("a changed library asks for another pass");
    };
    assert!(ctl.archive_busy(), "busy through the second pass too");
    assert!(matches!(
        ctl.finish_archive(again.run()),
        ArchiveEnd::Rebuilt(true)
    ));
    assert!(
        titles(&ctl).iter().any(|t| t == "Typed during the salvage"),
        "the swap keeps what was written after the copy was cut"
    );
}

#[test]
fn an_unreadable_backup_ends_the_rebuild_before_anything_is_touched() {
    use snipdeck_app::controller::{ArchiveEnd, Begun};
    let dir = tempfile::tempdir().expect("dir");
    let mut ctl = start(dir.path());
    let before = titles(&ctl);
    let broken = dir.path().join("broken.snipdeck");
    std::fs::write(&broken, b"not an archive").expect("file");

    let Begun::Job(job) = ctl.begin_rebuild_from_backup(dir.path(), &broken) else {
        panic!("the backup is read on a worker, not on this thread");
    };
    assert!(ctl.archive_busy());
    assert!(matches!(
        ctl.finish_archive(job.run()),
        ArchiveEnd::Rebuilt(false)
    ));
    assert!(!ctl.archive_busy());
    assert_eq!(titles(&ctl), before, "the library is as it was");
    let damaged = std::fs::read_dir(dir.path())
        .expect("dir")
        .flatten()
        .any(|e| e.file_name().to_string_lossy().contains("damaged"));
    assert!(!damaged, "nothing was set aside");
    assert!(!dir.path().join("snipdeck.salvage.snipdeck").exists());
}

#[test]
fn quitting_while_the_rebuild_reads_leaves_the_library_as_it_was() {
    use snipdeck_app::controller::Begun;
    let dir = tempfile::tempdir().expect("dir");
    let mut ctl = start(dir.path());
    assert!(ctl.backup_now(dir.path()));
    let backup = newest_backup(dir.path());
    let before = titles(&ctl);

    let Begun::Job(job) = ctl.begin_rebuild_from_backup(dir.path(), &backup) else {
        panic!("a worker job");
    };
    ctl.abandon_unfinished_archive();
    drop(ctl);
    let _done = job.run();
    let reopened = start_with(Store::open(dir.path()).expect("reopen"));
    assert_eq!(titles(&reopened), before);
    assert!(
        !std::fs::read_dir(dir.path())
            .expect("dir")
            .flatten()
            .any(|e| e.file_name().to_string_lossy().contains("damaged")),
        "nothing was set aside"
    );
}

#[test]
fn a_library_that_cannot_be_opened_is_set_aside_and_rebuild_finds_the_backups() {
    let dir = tempfile::tempdir().expect("dir");
    let mut ctl = start(dir.path());
    assert!(ctl.backup_now(dir.path()));
    let backed_up = titles(&ctl).len();
    drop(ctl);
    // Damage the header: the next start cannot open it.
    let db = dir.path().join("snipdeck.db");
    let mut bytes = fs::read(&db).expect("db");
    bytes[..16].fill(0x42);
    fs::write(&db, &bytes).expect("damage");
    assert!(Store::open(dir.path()).is_err());

    let (store, toast) = snipdeck_app::paths::open_library_folder(dir.path(), now())
        .expect("the app opens a fresh library");
    let Some(ToastMessage::LibrarySetAside { folder, has_backup }) = toast.map(|t| t.message)
    else {
        panic!("the set-aside notice");
    };
    assert!(has_backup, "the backup stays where Rebuild looks");
    assert_eq!(damaged_files(dir.path()), vec![folder.clone()]);
    assert_eq!(
        fs::read(dir.path().join(&folder).join("snipdeck.db")).expect("kept"),
        bytes
    );
    assert_no_marker(dir.path());

    // The real proof: Rebuild from backup restores the newest backup into the fresh library.
    let mut ctl = start_with(store);
    assert!(ctl.rebuild_from_backup(dir.path(), &newest_backup(dir.path())));
    assert!(!titles(&ctl).is_empty() && titles(&ctl).len() <= backed_up);
    assert_eq!(
        damaged_files(dir.path()).len(),
        2,
        "the first set-aside stays beside the new one"
    );
    assert_no_marker(dir.path());
}
