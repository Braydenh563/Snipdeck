//! Nothing is gated. A fresh install, with no licence file and no setting about one, can use every
//! feature that used to be Supporter-only: more than five boards, templates that ask for values, the
//! paste stack, Paste next and a master password. The controller half, over the demo library, the
//! platform fakes and a frozen clock.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use snipdeck_app::{
    controller::{BoardMenuEntry, Controller, MasterState, Services},
    demo,
    drafts::Drafts,
    host::{FakeTargets, FixedClock},
    item_editor::{EditorInput, Origin},
    settings::{Hotkeys, Settings},
    toast::ToastKind,
};
use snipdeck_core::{ItemKind, ListSort, Timestamp};
use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};
use snipdeck_store::{BoardRepo, ItemRepo, Store, crypto::KdfParams};

#[path = "support/mod.rs"]
mod support;
use support::{MasterSteps as _, SharedProvider};

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

fn now() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

/// A first run: new-install settings, the demo library, an empty data folder (so no licence file).
fn fresh(data: &std::path::Path) -> Ctl {
    let mut ctl = Controller::new(
        demo::store(now()).expect("demo store"),
        Services {
            clipboard: FakeClipboard::default(),
            keyboard: FakeKeyboard::default(),
            timing: FakeTiming::default(),
        },
        Box::new(FakeTargets::new(Some(demo::target()))),
        Box::new(FixedClock::new(now())),
        Settings::new_install(),
    )
    .expect("controller");
    ctl.set_drafts(Drafts::in_data_dir(data));
    ctl
}

fn no_danger_toast(ctl: &Ctl) {
    let toast = ctl.view().toast;
    assert!(
        toast.as_ref().is_none_or(|t| t.kind != ToastKind::Danger),
        "a refusal toast: {toast:?}"
    );
}

#[test]
fn a_fresh_install_has_no_board_limit() {
    let data = tempfile::tempdir().expect("dir");
    let mut ctl = fresh(data.path());
    let before = ctl.store().boards().expect("boards").len();
    for n in 0..10 {
        ctl.main_board_menu_open();
        let menu = ctl.main_screen().board_menu.expect("the menu");
        ctl.main_board_menu_close();
        let new = menu
            .iter()
            .position(|e| *e == BoardMenuEntry::New)
            .expect("New board is always offered");
        ctl.main_board_menu_activate(new);
        assert!(
            ctl.main_screen().editor.is_some(),
            "board {n} opens its editor"
        );
        ctl.main_editor_save(&format!("Board {n}"), None, None, ListSort::Manual);
    }
    assert_eq!(ctl.store().boards().expect("boards").len(), before + 10);
    assert!(
        ctl.main_screen()
            .toast
            .is_none_or(|t| t.kind != ToastKind::Danger)
    );
}

#[test]
fn a_fresh_install_saves_templates_that_ask_for_values() {
    let data = tempfile::tempdir().expect("dir");
    let mut ctl = fresh(data.path());
    for content in [
        "Dear {field:Name}",
        "Size {choice:Size|S|M|L}",
        "Notes {multiline:Notes|none}",
        "Due {pickdate:When|YYYY-MM-DD}",
        "Hi{optional:Greet} {field:Who}{/optional}",
    ] {
        ctl.open_new_item(Origin::Main);
        let view = ctl.item_editor_view().expect("editor");
        ctl.item_editor_save(&EditorInput {
            kind: ItemKind::Template,
            title: format!("T {content}"),
            content: content.to_owned(),
            ..view.fields
        });
        assert!(!ctl.item_editor_open(), "{content:?} saved");
        no_danger_toast(&ctl);
    }
}

#[test]
fn a_fresh_install_starts_a_paste_stack_and_registers_paste_next() {
    let data = tempfile::tempdir().expect("dir");
    let mut ctl = fresh(data.path());
    assert!(
        !Hotkeys::new_install().paste_next.is_empty(),
        "Paste next has a chord from the start"
    );
    let ids: Vec<_> = ctl
        .store()
        .all_items()
        .expect("items")
        .into_iter()
        .take(2)
        .map(|i| i.id)
        .collect();
    for id in &ids {
        ctl.toggle_stack_mark(*id);
    }
    ctl.start_stack();
    let stack = ctl.view().stack.expect("a stack");
    assert!(stack.running);
    assert_eq!(stack.count, 2);
    no_danger_toast(&ctl);
}

#[test]
fn a_fresh_install_sets_a_master_password() {
    let data = tempfile::tempdir().expect("dir");
    let provider = SharedProvider::new();
    let mut ctl = fresh(data.path());
    ctl.set_key_provider(Box::new(provider));
    ctl.set_kdf_params(KdfParams::insecure_for_tests(64, 1, 1).expect("cheap params"));
    ctl.unlock_library();
    assert_eq!(ctl.master_password_state(), MasterState::NotSet);
    let password = support::pw("everything free");
    ctl.set_master_password(password, password)
        .expect("a master password on a fresh install");
    assert_eq!(ctl.master_password_state(), MasterState::Set);
}
