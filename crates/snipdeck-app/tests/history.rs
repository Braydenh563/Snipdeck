//! The app-wide undo and redo (Ctrl+Z, Ctrl+Y): a round trip for every kind of step, a new
//! action clearing redo, a bulk action being one step and the 50-step depth. Over the demo
//! library, the platform fakes and a frozen clock.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use snipdeck_app::{
    controller::{Controller, MainAction, MainScreen, Services},
    demo,
    history::Action,
    host::{FakeTargets, FixedClock},
    item_editor::{EditorInput, Origin},
    main_window::{Section, SelectMode},
    settings::Settings,
};
use snipdeck_core::{Entitlements, ItemId, ItemKind, ItemLabel, ListId, Tier, Timestamp};
use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};
use snipdeck_store::{ItemRepo, Store};

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

fn now() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

fn rig() -> Ctl {
    Controller::new(
        demo::store(now()).expect("demo store"),
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

fn list_id(ctl: &mut Ctl, name: &str) -> ListId {
    let screen = ctl.main_screen();
    screen
        .lists
        .iter()
        .find(|l| l.name == name)
        .unwrap_or_else(|| panic!("no list {name}"))
        .id
}

fn order(ctl: &Ctl, list: ListId) -> Vec<String> {
    ctl.store()
        .items(list)
        .expect("items")
        .into_iter()
        .map(|i| i.title)
        .collect()
}

fn text_of(ctl: &Ctl, id: ItemId) -> String {
    snipdeck_app::usecase::item_text(ctl.store(), id)
        .expect("text")
        .expect("some text")
}

#[test]
fn an_empty_history_has_nothing_to_undo_or_redo() {
    let mut ctl = rig();
    let view = ctl.history_view();
    assert_eq!((view.undo, view.redo), (None, None));
    ctl.undo();
    ctl.redo();
    assert_eq!(ctl.main_screen().counts.all, 11);
}

#[test]
fn delete_round_trips_and_names_itself() {
    let mut ctl = rig();
    let screen = ctl.main_screen();
    let at = row_of(&screen, "Out of office");
    ctl.main_action(MainAction::Delete, at);
    assert_eq!(ctl.main_screen().counts.all, 10);
    assert_eq!(ctl.history_view().undo, Some(Action::Delete));
    ctl.undo();
    assert_eq!(ctl.main_screen().counts.all, 11);
    assert_eq!(ctl.history_view().undo, None);
    assert_eq!(ctl.history_view().redo, Some(Action::Delete));
    ctl.redo();
    assert_eq!(ctl.main_screen().counts.all, 10);
    assert_eq!(ctl.history_view().redo, None);
}

#[test]
fn pin_round_trips() {
    let mut ctl = rig();
    let id = id_of(&ctl, "Out of office");
    let was = ctl.store().item(id).unwrap().pinned;
    let at = row_of(&ctl.main_screen(), "Out of office");
    ctl.main_action(MainAction::Pin, at);
    assert_eq!(ctl.store().item(id).unwrap().pinned, !was);
    ctl.undo();
    assert_eq!(ctl.store().item(id).unwrap().pinned, was);
    ctl.redo();
    assert_eq!(ctl.store().item(id).unwrap().pinned, !was);
}

#[test]
fn duplicate_round_trips() {
    let mut ctl = rig();
    let at = row_of(&ctl.main_screen(), "Out of office");
    ctl.main_action(MainAction::Duplicate, at);
    assert_eq!(ctl.main_screen().counts.all, 12);
    ctl.undo();
    assert_eq!(ctl.main_screen().counts.all, 11);
    ctl.redo();
    assert_eq!(ctl.main_screen().counts.all, 12);
}

#[test]
fn moving_to_another_list_round_trips_to_the_same_place() {
    let mut ctl = rig();
    let email = list_id(&mut ctl, "Email replies");
    let code = list_id(&mut ctl, "Code");
    let before = order(&ctl, email);
    let screen = ctl.main_screen();
    let at = row_of(&screen, &before[1]);
    ctl.main_action(MainAction::Move, at);
    let to = screen.lists.iter().position(|l| l.id == code).unwrap();
    ctl.main_move_to(to);
    assert_eq!(order(&ctl, email).len(), before.len() - 1);
    assert_eq!(ctl.history_view().undo, Some(Action::Move));
    ctl.undo();
    assert_eq!(order(&ctl, email), before, "back in its old place");
    ctl.redo();
    assert_eq!(order(&ctl, email).len(), before.len() - 1);
    assert!(order(&ctl, code).contains(&before[1]));
}

#[test]
fn reordering_round_trips_to_the_same_order() {
    let mut ctl = rig();
    let email = list_id(&mut ctl, "Email replies");
    let before = order(&ctl, email);
    ctl.main_open(Section::List(email));
    ctl.main_drop_row(0, 2);
    let after = order(&ctl, email);
    assert_ne!(after, before);
    assert_eq!(ctl.history_view().undo, Some(Action::Reorder));
    ctl.undo();
    assert_eq!(order(&ctl, email), before);
    ctl.redo();
    assert_eq!(order(&ctl, email), after);
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
    assert!(!ctl.item_editor_open());
    id_of(ctl, title)
}

#[test]
fn adding_an_item_round_trips() {
    let mut ctl = rig();
    let id = save_new(&mut ctl, "Fresh note", "hello");
    assert_eq!(ctl.main_screen().counts.all, 12);
    assert_eq!(ctl.history_view().undo, Some(Action::Add));
    ctl.undo();
    assert_eq!(ctl.main_screen().counts.all, 11);
    ctl.redo();
    assert_eq!(ctl.main_screen().counts.all, 12);
    assert_eq!(text_of(&ctl, id), "hello");
}

#[test]
fn editing_the_text_and_title_round_trips() {
    let mut ctl = rig();
    let id = save_new(&mut ctl, "Edit me", "first");
    ctl.open_item_editor(id);
    let v = ctl.item_editor_view().expect("editor");
    let input = EditorInput {
        title: "Edited".to_owned(),
        content: "second".to_owned(),
        ..v.fields.clone()
    };
    ctl.item_editor_save(&input);
    assert_eq!(ctl.history_view().undo, Some(Action::Edit));
    assert_eq!(text_of(&ctl, id), "second");
    ctl.undo();
    assert_eq!(text_of(&ctl, id), "first");
    assert_eq!(ctl.store().item(id).unwrap().title, "Edit me");
    ctl.redo();
    assert_eq!(text_of(&ctl, id), "second");
    assert_eq!(ctl.store().item(id).unwrap().title, "Edited");
}

#[test]
fn a_colour_change_round_trips() {
    let mut ctl = rig();
    let id = id_of(&ctl, "Out of office");
    let was = ctl.store().item(id).unwrap().label;
    ctl.open_item_editor(id);
    let v = ctl.item_editor_view().expect("editor");
    let label = if was == Some(ItemLabel::Blue) {
        ItemLabel::Green
    } else {
        ItemLabel::Blue
    };
    let input = EditorInput {
        label: Some(label),
        ..v.fields.clone()
    };
    ctl.item_editor_save(&input);
    assert_eq!(ctl.history_view().undo, Some(Action::Colour));
    assert_eq!(ctl.store().item(id).unwrap().label, Some(label));
    ctl.undo();
    assert_eq!(ctl.store().item(id).unwrap().label, was);
    ctl.redo();
    assert_eq!(ctl.store().item(id).unwrap().label, Some(label));
}

#[test]
fn a_new_action_clears_redo() {
    let mut ctl = rig();
    let at = row_of(&ctl.main_screen(), "Out of office");
    ctl.main_action(MainAction::Delete, at);
    ctl.undo();
    assert_eq!(ctl.history_view().redo, Some(Action::Delete));
    let at = row_of(&ctl.main_screen(), "Out of office");
    ctl.main_action(MainAction::Pin, at);
    assert_eq!(ctl.history_view().redo, None);
    ctl.redo();
    assert_eq!(
        ctl.main_screen().counts.trash,
        0,
        "redo has nothing to bring"
    );
}

#[test]
fn a_bulk_action_is_one_step() {
    let mut ctl = rig();
    ctl.main_select(0, SelectMode::Replace);
    ctl.main_select(2, SelectMode::Extend);
    ctl.main_action(MainAction::Delete, 0);
    assert_eq!(ctl.main_screen().counts.trash, 3);
    ctl.undo();
    assert_eq!(ctl.main_screen().counts.trash, 0, "one undo for all three");
    assert_eq!(ctl.history_view().undo, None);
    ctl.redo();
    assert_eq!(ctl.main_screen().counts.trash, 3);
}

#[test]
fn the_history_goes_fifty_steps_deep() {
    let mut ctl = rig();
    let id = id_of(&ctl, "Out of office");
    let was = ctl.store().item(id).unwrap().pinned;
    for _ in 0..60 {
        let at = row_of(&ctl.main_screen(), "Out of office");
        ctl.main_action(MainAction::Pin, at);
    }
    for _ in 0..60 {
        ctl.undo();
    }
    assert_eq!(ctl.history_view().undo, None, "all sixty were kept");
    assert_eq!(ctl.store().item(id).unwrap().pinned, was);
    for _ in 0..60 {
        ctl.redo();
    }
    assert_eq!(ctl.store().item(id).unwrap().pinned, was, "sixty toggles");
}

#[test]
fn the_toast_undo_takes_back_its_own_step_even_after_a_newer_one() {
    let mut ctl = rig();
    let at = row_of(&ctl.main_screen(), "Out of office");
    ctl.main_action(MainAction::Delete, at);
    ctl.main_undo();
    assert_eq!(ctl.main_screen().counts.trash, 0);
    assert_eq!(ctl.history_view().redo, Some(Action::Delete));
}

#[test]
fn deleting_for_good_forgets_what_it_could_not_bring_back() {
    let mut ctl = rig();
    let at = row_of(&ctl.main_screen(), "Out of office");
    ctl.main_action(MainAction::Delete, at);
    ctl.main_empty_trash();
    assert_eq!(ctl.history_view().undo, None);
}
