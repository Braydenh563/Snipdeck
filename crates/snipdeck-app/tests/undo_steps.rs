//! The undo history beyond item edits (LIBRARY-35, 36, 38, 39, 40): tag, list and board edits as
//! steps, a restore from the Trash, a purge that forgets only the steps it broke, the "Delete
//! forever" offer after undoing an add, and a toast Undo that is withdrawn once a later step
//! touches the same item. Over the demo library, the platform fakes and a frozen clock.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use snipdeck_app::{
    controller::{BoardMenuEntry, Controller, MainAction, MainScreen, Services},
    demo,
    history::Action,
    host::{FakeTargets, FixedClock},
    item_editor::{EditorInput, Origin},
    main_window::Section,
    settings::Settings,
    toast::{ToastKind, ToastMessage},
};
use snipdeck_core::{ItemId, ItemKind, Name, Sort, Tag, Timestamp};
use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, Store, TagRepo};

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

fn now() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

fn rig() -> Ctl {
    rig_with_tags(&["invoices", "billing"])
}

fn rig_with_tags(names: &[&str]) -> Ctl {
    let mut store = demo::store(now()).expect("demo store");
    for name in names {
        store
            .insert_tag(&Tag::new(Name::new(name).unwrap()))
            .unwrap();
    }
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

fn id_of(ctl: &Ctl, title: &str) -> ItemId {
    ctl.store()
        .all_items()
        .expect("items")
        .into_iter()
        .find(|i| i.title == title)
        .unwrap_or_else(|| panic!("no item {title}"))
        .id
}

fn row_of(screen: &MainScreen, title: &str) -> usize {
    screen
        .rows
        .iter()
        .position(|r| r.title == title)
        .unwrap_or_else(|| panic!("no row {title}"))
}

fn tag_names(ctl: &mut Ctl) -> Vec<String> {
    ctl.main_screen()
        .tags
        .iter()
        .map(|t| t.name.clone())
        .collect()
}

/// Ticks tag 0 on "Invoice reminder".
fn tag_invoice_reminder(ctl: &mut Ctl) -> ItemId {
    let at = row_of(&ctl.main_screen(), "Invoice reminder");
    ctl.main_action(MainAction::Tag, at);
    ctl.main_toggle_tag(0, true);
    id_of(ctl, "Invoice reminder")
}

fn save_new(ctl: &mut Ctl, title: &str, content: &str) -> ItemId {
    ctl.open_new_item(Origin::Main);
    let v = ctl.item_editor_view().expect("editor");
    let input = EditorInput {
        kind: ItemKind::Text,
        title: title.to_owned(),
        content: content.to_owned(),
        ..v.fields.clone()
    };
    ctl.item_editor_save(&input);
    id_of(ctl, title)
}

// ---- LIBRARY-35: a tag delete is a step ----------------------------------------------------

#[test]
fn deleting_a_tag_is_a_step_that_undo_brings_back_with_its_items() {
    let mut ctl = rig();
    let id = tag_invoice_reminder(&mut ctl);
    let tag = ctl.main_screen().tags[0].id;
    ctl.main_delete_tag(0);
    assert_eq!(ctl.store().item(id).unwrap().tags.len(), 0);
    assert_eq!(ctl.history_view().undo, Some(Action::Tag));
    ctl.undo();
    assert!(ctl.store().item(id).unwrap().tags.contains(&tag));
    assert_eq!(tag_names(&mut ctl).len(), 2);
    ctl.redo();
    assert_eq!(ctl.store().item(id).unwrap().tags.len(), 0);
    assert_eq!(tag_names(&mut ctl).len(), 1);
}

#[test]
fn the_steps_before_a_tag_delete_still_undo_after_it_is_taken_back() {
    let mut ctl = rig();
    let id = tag_invoice_reminder(&mut ctl);
    let at = row_of(&ctl.main_screen(), "Invoice reminder");
    let was = ctl.store().item(id).unwrap().pinned;
    ctl.main_action(MainAction::Pin, at);
    ctl.main_delete_tag(0);
    ctl.undo(); // the tag and its link come back
    ctl.undo(); // the pin
    assert_eq!(ctl.store().item(id).unwrap().pinned, was);
    let toast = ctl.view().toast.expect("a toast");
    assert_ne!(toast.kind, ToastKind::Danger, "{toast:?}");
    assert_ne!(ctl.store().item(id).unwrap().tags.len(), 0);
}

#[test]
fn renaming_a_tag_round_trips() {
    let mut ctl = rig();
    let before = tag_names(&mut ctl);
    assert_eq!(ctl.main_rename_tag(0, "Receipts"), Ok(()));
    assert!(tag_names(&mut ctl).contains(&"Receipts".to_owned()));
    assert_eq!(ctl.history_view().undo, Some(Action::Tag));
    ctl.undo();
    assert_eq!(tag_names(&mut ctl), before);
    ctl.redo();
    assert!(tag_names(&mut ctl).contains(&"Receipts".to_owned()));
}

// ---- Nested tags ---------------------------------------------------------------------------

/// "(depth) path" of every tag in the sidebar's order.
fn shape(ctl: &mut Ctl) -> Vec<String> {
    ctl.main_screen()
        .tags
        .iter()
        .map(|t| format!("{} {}", t.depth, t.path))
        .collect()
}

#[test]
fn nesting_a_tag_is_a_step_that_round_trips_and_a_loop_is_refused() {
    let mut ctl = rig();
    assert_eq!(shape(&mut ctl), ["0 billing", "0 invoices"]);
    ctl.main_nest_tag(1, Some(0));
    assert_eq!(shape(&mut ctl), ["0 billing", "1 billing/invoices"]);
    assert_eq!(ctl.history_view().undo, Some(Action::Tag));
    ctl.main_nest_tag(0, Some(1));
    assert_eq!(
        shape(&mut ctl),
        ["0 billing", "1 billing/invoices"],
        "billing cannot go under its own nested tag"
    );
    ctl.undo();
    assert_eq!(shape(&mut ctl), ["0 billing", "0 invoices"]);
    ctl.redo();
    assert_eq!(shape(&mut ctl), ["0 billing", "1 billing/invoices"]);
    ctl.main_nest_tag(1, None);
    assert_eq!(shape(&mut ctl), ["0 billing", "0 invoices"]);
}

#[test]
fn renaming_a_nested_tag_keeps_its_place() {
    let mut ctl = rig();
    ctl.main_nest_tag(1, Some(0));
    assert_eq!(ctl.main_rename_tag(1, "bills"), Ok(()));
    assert_eq!(shape(&mut ctl), ["0 billing", "1 billing/bills"]);
}

#[test]
fn deleting_a_tag_lifts_the_tags_under_it_and_undo_puts_them_back() {
    let mut ctl = rig_with_tags(&["invoices", "billing", "archive"]);
    // archive, billing, invoices: billing under archive, invoices under billing.
    ctl.main_nest_tag(1, Some(0));
    ctl.main_nest_tag(2, Some(1));
    assert_eq!(
        shape(&mut ctl),
        [
            "0 archive",
            "1 archive/billing",
            "2 archive/billing/invoices"
        ]
    );
    ctl.main_delete_tag(1);
    assert_eq!(shape(&mut ctl), ["0 archive", "1 archive/invoices"]);
    ctl.undo();
    assert_eq!(
        shape(&mut ctl),
        [
            "0 archive",
            "1 archive/billing",
            "2 archive/billing/invoices"
        ]
    );
    ctl.redo();
    assert_eq!(shape(&mut ctl), ["0 archive", "1 archive/invoices"]);
}

#[test]
fn merging_a_tag_moves_the_tags_under_it_to_the_target_and_undo_restores_them() {
    let mut ctl = rig_with_tags(&["invoices", "billing", "archive"]);
    ctl.main_nest_tag(2, Some(1)); // invoices under billing
    ctl.main_merge_tag(1, "archive").expect("merged");
    assert_eq!(shape(&mut ctl), ["0 archive", "1 archive/invoices"]);
    ctl.undo();
    assert_eq!(
        shape(&mut ctl),
        ["0 archive", "0 billing", "1 billing/invoices"]
    );
    ctl.redo();
    assert_eq!(shape(&mut ctl), ["0 archive", "1 archive/invoices"]);
}

#[test]
fn merging_a_tag_into_one_nested_under_it_keeps_the_target_out_of_a_loop() {
    let mut ctl = rig();
    ctl.main_nest_tag(1, Some(0)); // invoices under billing
    ctl.main_merge_tag(0, "invoices").expect("merged");
    assert_eq!(shape(&mut ctl), ["0 invoices"]);
    ctl.undo();
    assert_eq!(shape(&mut ctl), ["0 billing", "1 billing/invoices"]);
}

// ---- LIBRARY-38: lists, boards, restore ----------------------------------------------------

#[test]
fn a_list_edit_round_trips() {
    let mut ctl = rig();
    let list = ctl.main_screen().lists[0].clone();
    ctl.main_edit_list(0);
    ctl.main_editor_save("Renamed", None, None, Sort::MOST_USED);
    let stored = ctl.store().list(list.id).unwrap();
    assert_eq!(stored.name.as_str(), "Renamed");
    assert_eq!(stored.sort, Sort::MOST_USED);
    assert_eq!(ctl.history_view().undo, Some(Action::List));
    ctl.undo();
    let back = ctl.store().list(list.id).unwrap();
    assert_eq!(back.name.as_str(), list.name);
    assert_eq!(back.sort, Sort::MANUAL);
    ctl.redo();
    assert_eq!(ctl.store().list(list.id).unwrap().name.as_str(), "Renamed");
}

#[test]
fn a_new_list_round_trips() {
    let mut ctl = rig();
    let before = ctl.store().boards().unwrap();
    let board = before[0].id;
    let lists = ctl.store().lists(board).unwrap().len();
    ctl.main_new_list();
    ctl.main_editor_save("Fresh", None, None, Sort::MANUAL);
    assert_eq!(ctl.store().lists(board).unwrap().len(), lists + 1);
    ctl.undo();
    assert_eq!(ctl.store().lists(board).unwrap().len(), lists);
    ctl.redo();
    assert_eq!(ctl.store().lists(board).unwrap().len(), lists + 1);
}

#[test]
fn moving_a_list_round_trips() {
    let mut ctl = rig();
    let first = ctl.main_screen().lists[0].id;
    ctl.main_reorder_list(0, 1);
    let order =
        |ctl: &mut Ctl| -> Vec<_> { ctl.main_screen().lists.iter().map(|l| l.id).collect() };
    assert_ne!(order(&mut ctl)[0], first);
    assert_eq!(ctl.history_view().undo, Some(Action::Reorder));
    ctl.undo();
    assert_eq!(order(&mut ctl)[0], first);
    ctl.redo();
    assert_ne!(order(&mut ctl)[0], first);
}

fn rename_board(ctl: &mut Ctl, name: &str) {
    ctl.main_board_menu_open();
    let menu = ctl.main_screen().board_menu.expect("the menu");
    ctl.main_board_menu_close();
    let at = menu
        .iter()
        .position(|e| *e == BoardMenuEntry::Rename)
        .expect("Rename");
    ctl.main_board_menu_activate(at);
    ctl.main_editor_save(name, None, None, Sort::MANUAL);
}

#[test]
fn a_board_rename_round_trips() {
    let mut ctl = rig();
    let board = ctl.store().boards().unwrap()[0].clone();
    rename_board(&mut ctl, "Studio");
    assert_eq!(ctl.store().board(board.id).unwrap().name.as_str(), "Studio");
    assert_eq!(ctl.history_view().undo, Some(Action::Board));
    ctl.undo();
    assert_eq!(ctl.store().board(board.id).unwrap().name, board.name);
    ctl.redo();
    assert_eq!(ctl.store().board(board.id).unwrap().name.as_str(), "Studio");
}

#[test]
fn restoring_from_the_trash_is_a_step() {
    let mut ctl = rig();
    let at = row_of(&ctl.main_screen(), "Out of office");
    ctl.main_action(MainAction::Delete, at);
    ctl.main_open(Section::Trash);
    ctl.main_action(MainAction::Restore, 0);
    assert_eq!(ctl.main_screen().counts.trash, 0);
    assert_eq!(ctl.history_view().undo, Some(Action::Restore));
    ctl.undo();
    assert_eq!(ctl.main_screen().counts.trash, 1);
    ctl.redo();
    assert_eq!(ctl.main_screen().counts.trash, 0);
}

// ---- LIBRARY-38: a purge forgets only what it broke ---------------------------------------

#[test]
fn deleting_for_good_keeps_the_steps_that_do_not_need_the_item() {
    let mut ctl = rig();
    let pinned = id_of(&ctl, "Out of office");
    let was = ctl.store().item(pinned).unwrap().pinned;
    let at = row_of(&ctl.main_screen(), "Out of office");
    ctl.main_action(MainAction::Pin, at);
    let at = row_of(&ctl.main_screen(), "Invoice reminder");
    ctl.main_action(MainAction::Delete, at);
    ctl.main_open(Section::Trash);
    ctl.main_action(MainAction::DeleteForever, 0);
    // The delete step needs the purged item; the pin does not.
    assert_eq!(ctl.history_view().undo, Some(Action::Pin));
    ctl.undo();
    assert_eq!(ctl.store().item(pinned).unwrap().pinned, was);
    assert_eq!(ctl.history_view().undo, None);
}

#[test]
fn emptying_the_trash_keeps_the_steps_that_do_not_need_it() {
    let mut ctl = rig();
    let at = row_of(&ctl.main_screen(), "Out of office");
    ctl.main_action(MainAction::Pin, at);
    let at = row_of(&ctl.main_screen(), "Invoice reminder");
    ctl.main_action(MainAction::Delete, at);
    ctl.main_empty_trash();
    assert_eq!(ctl.history_view().undo, Some(Action::Pin));
}

// ---- LIBRARY-39: undoing an add offers Delete forever -------------------------------------

#[test]
fn undoing_an_add_offers_to_delete_it_for_good() {
    let mut ctl = rig();
    let id = save_new(&mut ctl, "Secret pasted by mistake", "hunter2");
    ctl.undo();
    assert_eq!(ctl.main_screen().counts.trash, 1);
    let toast = ctl.view().toast.expect("a toast");
    assert!(toast.purge, "the toast offers Delete forever");
    assert!(!toast.undo);
    ctl.toast_action();
    assert_eq!(ctl.main_screen().counts.trash, 0);
    assert!(ctl.store().item(id).is_err());
    assert!(matches!(
        ctl.view().toast.expect("a toast").message,
        ToastMessage::DeletedForever { count: 1 }
    ));
    // The add is gone for good: Redo cannot bring it back.
    assert_eq!(ctl.history_view().redo, None);
}

#[test]
fn undoing_a_delete_offers_no_purge() {
    let mut ctl = rig();
    let at = row_of(&ctl.main_screen(), "Out of office");
    ctl.main_action(MainAction::Delete, at);
    ctl.undo();
    assert!(!ctl.view().toast.expect("a toast").purge);
}
