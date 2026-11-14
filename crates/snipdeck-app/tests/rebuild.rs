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
    controller::{Controller, MainAction, Services},
    demo,
    host::{FakeTargets, FixedClock},
    item_editor::{EditorInput, Origin},
    settings::Settings,
    toast::ToastMessage,
};
use snipdeck_core::{Board, Entitlements, List, Name, Placement, Tier, Timestamp};
use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};
use snipdeck_store::{BlobStore, BoardRepo, ItemRepo, ListRepo, Store};

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

fn now() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

fn start(dir: &Path) -> Ctl {
    let mut store = Store::open(dir).expect("store");
    demo::seed(&mut store, now()).expect("seed");
    start_with(store, Tier::Pro)
}

fn start_with(store: Store, tier: Tier) -> Ctl {
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
    snipdeck_portable::backup::list_backups(dir)
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
    let mut ctl = start_with(store, Tier::Pro);
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
fn a_failing_import_rolls_back_to_the_original_library() {
    let dir = tempfile::tempdir().expect("dir");
    // A backup with more boards than the Free plan allows: the preview only reports the limit,
    // the real import after the swap is refused.
    let mut store = Store::open(dir.path()).expect("store");
    demo::seed(&mut store, now()).expect("seed");
    for n in 0..=Entitlements::FREE_BOARD_LIMIT {
        let name = Name::new(&format!("Extra {n}")).expect("name");
        let board = Board::new(name, now());
        store.insert_board(&board, Placement::Last).expect("board");
        let list = List::new(board.id, Name::new("Tab").expect("name"), now());
        store.insert_list(&list, Placement::Last).expect("list");
        add_text(&mut store, list.id, &format!("Extra item {n}"), None);
    }
    let mut pro = start_with(store, Tier::Pro);
    assert!(pro.backup_now(dir.path()));
    let backup = newest_backup(dir.path());
    drop(pro);
    let mut ctl = start_with(Store::open(dir.path()).expect("reopen"), Tier::Free);
    let before = titles(&ctl);

    assert!(!ctl.rebuild_from_backup(dir.path(), &backup));

    assert_eq!(titles(&ctl), before, "the original library is back");
    assert_eq!(damaged_files(dir.path()), Vec::<String>::new());
    assert_no_marker(dir.path());
    assert!(dir.path().join("snipdeck.db").exists());
    assert!(matches!(
        ctl.view().toast.expect("an error toast").message,
        ToastMessage::RestoreFailed(_)
    ));
    // And the controller still works on the original.
    add_item(&mut ctl, "Still writable");
    assert!(titles(&ctl).iter().any(|t| t == "Still writable"));
}
