//! The master password through the controller, without a window (SPEC 5.7, V1-PLAN C3): who may
//! set it, the checks on a new one, the library starting locked, the unlock prompt, change and
//! remove, and the idle lock on an injected clock. Over a library on disk, so a second
//! controller on the same folder is a restart. The real clock is never read: the controller's
//! clock is a `FixedClock` the test moves.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use std::{path::Path, time::Duration};

use snipdeck_app::{
    controller::{Controller, Effect, ImportSource, MasterProblem, MasterState, Services},
    demo,
    drafts::Drafts,
    host::{FakeTargets, FixedClock},
    item_editor::{EditorInput, Origin},
    master_job::{MasterDone, MasterOp},
    master_password::IDLE_LOCK,
    settings::Settings,
    toast::ToastMessage,
};
use snipdeck_core::{Entitlements, ItemId, Tier, Timestamp, UseMode};
use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};
use snipdeck_store::{ItemRepo, Store, crypto::KdfParams};

#[path = "support/mod.rs"]
mod support;
use support::{MasterSteps as _, SharedProvider, assert_no_plaintext};

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

const SECRET: &str = "ZQ9-correct-horse-battery";
fn password() -> &'static str {
    support::pw("main")
}

/// `by` short of the idle time.
fn just_before_idle(by: Duration) -> Duration {
    IDLE_LOCK
        .checked_sub(by)
        .expect("shorter than the idle time")
}

fn t0() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

/// A controller on the demo library kept in `data`, on a clock the caller keeps.
fn start(tier: Tier, provider: &SharedProvider, data: &Path, clock: &FixedClock) -> Ctl {
    let mut store = Store::open(data).expect("store");
    if snipdeck_store::BoardRepo::boards(&store)
        .expect("boards")
        .is_empty()
    {
        demo::seed(&mut store, t0()).expect("seed");
    }
    let mut ctl = Controller::new(
        store,
        Services {
            clipboard: FakeClipboard::default(),
            keyboard: FakeKeyboard::default(),
            timing: FakeTiming::default(),
        },
        Box::new(FakeTargets::new(Some(demo::target()))),
        Box::new(clock.clone()),
        Settings::default(),
        Entitlements::new(tier),
    )
    .expect("controller");
    ctl.set_drafts(Drafts::in_data_dir(data));
    ctl.set_key_provider(Box::new(provider.clone()));
    // Argon2id at the production cost would make every unlock here take a second.
    ctl.set_kdf_params(KdfParams::insecure_for_tests(64, 1, 1).expect("cheap params"));
    ctl.unlock_library();
    ctl
}

struct Rig {
    ctl: Ctl,
    clock: FixedClock,
    provider: SharedProvider,
    data: tempfile::TempDir,
    id: ItemId,
}

impl Rig {
    /// Restarts: a new controller on the same folder and key protection, at the same moment.
    fn restart(self, tier: Tier) -> Self {
        let Self {
            ctl,
            clock,
            provider,
            data,
            id,
        } = self;
        drop(ctl);
        let ctl = start(tier, &provider, data.path(), &clock);
        Self {
            ctl,
            clock,
            provider,
            data,
            id,
        }
    }

    fn advance(&self, by: Duration) {
        self.clock
            .set(self.clock_now().checked_add(by).expect("later"));
    }

    fn clock_now(&self) -> Timestamp {
        snipdeck_app::host::Clock::now(&self.clock)
    }

    fn secret(&self) -> Option<String> {
        snipdeck_app::usecase::item_text(self.ctl.store(), self.id)
            .ok()
            .flatten()
    }

    fn toast(&self) -> Option<ToastMessage> {
        self.ctl.view().toast.map(|t| t.message)
    }
}

/// A library with one sensitive item, on `tier`, key made and open, no password yet.
fn rig(tier: Tier) -> Rig {
    let data = tempfile::tempdir().expect("dir");
    let provider = SharedProvider::new();
    let clock = FixedClock::new(t0());
    let mut ctl = start(tier, &provider, data.path(), &clock);
    ctl.open_new_item(Origin::Main);
    let v = ctl.item_editor_view().expect("editor");
    ctl.item_editor_save(&EditorInput {
        sensitive: true,
        title: "Vault code".into(),
        content: SECRET.into(),
        ..v.fields
    });
    assert!(!ctl.item_editor_open(), "saved");
    let id = ctl
        .store()
        .all_items()
        .expect("items")
        .into_iter()
        .find(|i| i.title == "Vault code")
        .expect("the item")
        .id;
    Rig {
        ctl,
        clock,
        provider,
        data,
        id,
    }
}

fn with_password(tier: Tier) -> Rig {
    let mut r = rig(tier);
    r.ctl
        .set_master_password(password(), password())
        .expect("set the password");
    r
}

// ---- Who may set it, and the checks ------------------------------------------------------------

#[test]
fn a_free_library_cannot_set_a_master_password() {
    let mut r = rig(Tier::Free);
    assert!(!r.ctl.master_password_allowed());
    assert_eq!(
        r.ctl.set_master_password(password(), password()),
        Err(MasterProblem::SupporterOnly)
    );
    assert_eq!(r.ctl.master_password_state(), MasterState::NotSet);
    assert!(!r.ctl.library_locked());
    // Nothing changed: after a restart the key still opens without a password.
    let r = r.restart(Tier::Free);
    assert_eq!(r.secret().as_deref(), Some(SECRET));
}

#[test]
fn a_supporter_library_sets_it_and_stays_open_for_the_session() {
    let mut r = rig(Tier::Pro);
    assert!(r.ctl.master_password_allowed());
    assert_eq!(r.ctl.master_password_state(), MasterState::NotSet);
    r.ctl
        .set_master_password(password(), password())
        .expect("set");
    assert_eq!(r.ctl.master_password_state(), MasterState::Set);
    assert!(!r.ctl.library_locked(), "the user just typed it");
    assert_eq!(r.secret().as_deref(), Some(SECRET));
    assert_eq!(r.toast(), Some(ToastMessage::MasterPasswordSet));
    assert_no_plaintext(r.data.path(), &[SECRET.as_bytes(), password().as_bytes()]);
}

#[test]
fn a_new_password_must_be_long_enough_and_typed_twice() {
    let mut r = rig(Tier::Pro);
    assert_eq!(
        r.ctl.set_master_password("short", "short"),
        Err(MasterProblem::TooShort)
    );
    assert_eq!(
        r.ctl.set_master_password("", ""),
        Err(MasterProblem::TooShort)
    );
    assert_eq!(
        r.ctl.set_master_password(password(), support::pw("second")),
        Err(MasterProblem::Mismatch)
    );
    assert_eq!(r.ctl.master_password_state(), MasterState::NotSet);
    let r = r.restart(Tier::Pro);
    assert!(!r.ctl.library_locked(), "no password was set by a refusal");
}

#[test]
fn a_machine_without_key_protection_cannot_set_one() {
    let data = tempfile::tempdir().expect("dir");
    let clock = FixedClock::new(t0());
    let mut ctl = Controller::new(
        demo::store(t0()).expect("demo"),
        Services {
            clipboard: FakeClipboard::default(),
            keyboard: FakeKeyboard::default(),
            timing: FakeTiming::default(),
        },
        Box::new(FakeTargets::new(Some(demo::target()))),
        Box::new(clock),
        Settings::default(),
        Entitlements::new(Tier::Pro),
    )
    .expect("controller");
    ctl.set_drafts(Drafts::in_data_dir(data.path()));
    assert_eq!(ctl.master_password_state(), MasterState::Unavailable);
    assert_eq!(
        ctl.set_master_password(password(), password()),
        Err(MasterProblem::Unavailable)
    );
}

// ---- The slow half runs off the controller -----------------------------------------------------

/// Runs a job on another thread, the way the app does, and waits for its outcome.
fn run_elsewhere(job: snipdeck_app::master_job::MasterJob) -> snipdeck_app::master_job::MasterDone {
    std::thread::spawn(move || job.run())
        .join()
        .expect("the job finishes")
}

#[test]
fn setting_a_password_changes_nothing_until_the_outcome_is_applied() {
    let mut r = rig(Tier::Pro);
    let job = r
        .ctl
        .begin_set_master_password(password(), password())
        .expect("checked");
    assert_eq!(job.op(), MasterOp::Set { exists: true });
    assert_eq!(r.ctl.master_busy(), Some(MasterOp::Set { exists: true }));
    assert_eq!(
        r.ctl.master_password_state(),
        MasterState::NotSet,
        "still the old state while the key is derived"
    );
    assert_eq!(
        r.ctl
            .begin_set_master_password(password(), password())
            .err(),
        Some(MasterProblem::Failed),
        "one step at a time"
    );
    let done = run_elsewhere(job);
    assert_eq!(r.ctl.finish_master(done), Ok(()));
    assert_eq!(r.ctl.master_busy(), None);
    assert_eq!(r.ctl.master_password_state(), MasterState::Set);
    assert_eq!(r.toast(), Some(ToastMessage::MasterPasswordSet));
    let r = r.restart(Tier::Pro);
    assert!(r.ctl.library_locked(), "the password was really stored");
}

#[test]
fn a_first_password_on_a_library_without_a_key_makes_the_key_off_the_controller() {
    let data = tempfile::tempdir().expect("dir");
    let provider = SharedProvider::new();
    let clock = FixedClock::new(t0());
    let store = Store::open(data.path()).expect("store");
    let mut ctl = Controller::new(
        store,
        Services {
            clipboard: FakeClipboard::default(),
            keyboard: FakeKeyboard::default(),
            timing: FakeTiming::default(),
        },
        Box::new(FakeTargets::new(Some(demo::target()))),
        Box::new(clock),
        Settings::default(),
        Entitlements::new(Tier::Pro),
    )
    .expect("controller");
    ctl.set_key_provider(Box::new(provider));
    ctl.set_kdf_params(KdfParams::insecure_for_tests(64, 1, 1).expect("cheap params"));
    let job = ctl
        .begin_set_master_password(password(), password())
        .expect("checked");
    assert_eq!(job.op(), MasterOp::Set { exists: false });
    assert_eq!(ctl.finish_master(run_elsewhere(job)), Ok(()));
    assert_eq!(ctl.master_password_state(), MasterState::Set);
    assert!(!ctl.library_locked());
}

#[test]
fn a_copy_while_the_first_password_is_derived_does_not_make_a_key_and_the_set_succeeds() {
    use snipdeck_app::recent::ClipboardObservation;
    let data = tempfile::tempdir().expect("dir");
    let provider = SharedProvider::new();
    let clock = FixedClock::new(t0());
    let store = Store::open(data.path()).expect("store");
    let mut ctl = Controller::new(
        store,
        Services {
            clipboard: FakeClipboard::default(),
            keyboard: FakeKeyboard::default(),
            timing: FakeTiming::default(),
        },
        Box::new(FakeTargets::new(Some(demo::target()))),
        Box::new(clock),
        Settings {
            recent_history: true,
            ..Settings::default()
        },
        Entitlements::new(Tier::Pro),
    )
    .expect("controller");
    ctl.set_key_provider(Box::new(provider));
    ctl.set_kdf_params(KdfParams::insecure_for_tests(64, 1, 1).expect("cheap params"));
    let job = ctl
        .begin_set_master_password(password(), password())
        .expect("checked");
    assert_eq!(job.op(), MasterOp::Set { exists: false });
    ctl.on_clipboard_change(&ClipboardObservation {
        text: Some("copied while the key is derived".to_owned()),
        source_app: Some("notepad.exe".to_owned()),
        ..ClipboardObservation::default()
    });
    assert_eq!(ctl.finish_master(run_elsewhere(job)), Ok(()));
    assert_eq!(ctl.master_password_state(), MasterState::Set);
    assert!(!ctl.library_locked());
    assert!(ctl.recent_entries().is_empty(), "that copy was dropped");
}

#[test]
fn a_wrong_password_comes_back_from_the_job_and_frees_the_controller() {
    let mut r = with_password(Tier::Pro).restart(Tier::Pro);
    let job = r
        .ctl
        .begin_unlock_with_password(support::pw("wrong"))
        .expect("checked")
        .expect("locked");
    assert_eq!(r.ctl.master_busy(), Some(MasterOp::Unlock));
    assert_eq!(
        r.ctl.finish_master(run_elsewhere(job)),
        Err(MasterProblem::WrongPassword)
    );
    assert_eq!(r.ctl.master_busy(), None);
    assert!(r.ctl.library_locked());
    let job = r
        .ctl
        .begin_unlock_with_password(password())
        .expect("checked")
        .expect("locked");
    assert_eq!(r.ctl.finish_master(run_elsewhere(job)), Ok(()));
    assert!(!r.ctl.library_locked());
    assert_eq!(r.secret().as_deref(), Some(SECRET));
}

#[test]
fn a_refused_step_never_becomes_busy_and_an_open_library_has_nothing_to_derive() {
    let mut r = rig(Tier::Pro);
    assert_eq!(
        r.ctl.begin_set_master_password("short", "short").err(),
        Some(MasterProblem::TooShort)
    );
    assert_eq!(r.ctl.master_busy(), None);
    assert!(
        r.ctl
            .begin_unlock_with_password(support::pw("anything"))
            .expect("open already")
            .is_none()
    );
    assert_eq!(r.ctl.master_busy(), None);
}

#[test]
fn changing_and_removing_run_the_same_way() {
    let mut r = with_password(Tier::Pro);
    let job = r
        .ctl
        .begin_change_master_password(password(), support::pw("another"), support::pw("another"))
        .expect("checked");
    assert_eq!(job.op(), MasterOp::Change);
    assert_eq!(r.ctl.finish_master(run_elsewhere(job)), Ok(()));
    assert_eq!(r.toast(), Some(ToastMessage::MasterPasswordChanged));
    let job = r
        .ctl
        .begin_remove_master_password(support::pw("another"))
        .expect("checked");
    assert_eq!(job.op(), MasterOp::Remove);
    assert_eq!(r.ctl.finish_master(run_elsewhere(job)), Ok(()));
    assert_eq!(r.ctl.master_password_state(), MasterState::NotSet);
}

#[test]
fn a_step_whose_worker_died_frees_the_controller_and_changes_nothing() {
    // The worker panicked or could not start: the app hands back a failed outcome.
    let mut r = rig(Tier::Pro);
    let job = r
        .ctl
        .begin_set_master_password(password(), password())
        .expect("checked");
    assert!(r.ctl.master_busy().is_some());
    let failed = MasterDone::failed(&job);
    drop(job);
    assert!(r.ctl.master_current(&failed));
    assert_eq!(r.ctl.finish_master(failed), Err(MasterProblem::Failed));
    assert_eq!(r.ctl.master_busy(), None, "free for another try");
    assert_eq!(r.ctl.master_password_state(), MasterState::NotSet);

    // An unlock that dies shows the generic problem, not support::pw("wrong"), and stays locked.
    let mut r = with_password(Tier::Pro).restart(Tier::Pro);
    let job = r
        .ctl
        .begin_unlock_with_password(password())
        .expect("checked")
        .expect("locked");
    let failed = MasterDone::failed(&job);
    assert_eq!(r.ctl.finish_master(failed), Err(MasterProblem::Failed));
    assert_eq!(r.ctl.master_busy(), None);
    assert!(r.ctl.library_locked());
}

#[test]
fn locking_while_a_change_is_being_derived_drops_the_outcome() {
    let mut r = with_password(Tier::Pro);
    let job = r
        .ctl
        .begin_change_master_password(password(), support::pw("another"), support::pw("another"))
        .expect("checked");
    let done = run_elsewhere(job);
    r.ctl.lock_now();
    assert_eq!(r.ctl.master_busy(), None, "locking ends the step");
    assert!(!r.ctl.master_current(&done));
    assert_eq!(r.ctl.finish_master(done), Err(MasterProblem::Failed));
    assert!(
        r.ctl.library_locked(),
        "the old key did not reopen the library"
    );
    assert_eq!(r.secret(), None);
    // The password did not change: the old one still opens it, the new one does not.
    assert_eq!(
        r.ctl.unlock_with_password(support::pw("another")),
        Err(MasterProblem::WrongPassword)
    );
    r.ctl
        .unlock_with_password(password())
        .expect("old password");
    assert_eq!(r.secret().as_deref(), Some(SECRET));
}

#[test]
fn locking_while_a_removal_is_being_derived_keeps_the_password() {
    let mut r = with_password(Tier::Pro);
    let job = r
        .ctl
        .begin_remove_master_password(password())
        .expect("checked");
    let done = run_elsewhere(job);
    r.ctl.lock_now();
    assert_eq!(r.ctl.finish_master(done), Err(MasterProblem::Failed));
    assert!(r.ctl.library_locked());
    assert_eq!(r.ctl.master_password_state(), MasterState::Set);
}

#[test]
fn closing_the_prompt_while_the_password_is_checked_does_not_unlock() {
    let mut r = with_password(Tier::Pro).restart(Tier::Pro);
    r.ctl.request_unlock();
    let job = r
        .ctl
        .begin_unlock_with_password(password())
        .expect("checked")
        .expect("locked");
    let done = run_elsewhere(job);
    r.ctl.cancel_unlock();
    assert_eq!(r.ctl.master_busy(), None);
    assert_eq!(r.ctl.finish_master(done), Err(MasterProblem::Failed));
    assert!(r.ctl.library_locked(), "the user said no");
    assert_eq!(r.toast(), None, "no \"unlocked\" toast");

    // A new try after the cancelled one is not mistaken for it.
    r.ctl.request_unlock();
    let late = run_elsewhere(
        r.ctl
            .begin_unlock_with_password(password())
            .expect("checked")
            .expect("locked"),
    );
    assert!(r.ctl.master_current(&late));
    assert_eq!(r.ctl.finish_master(late), Ok(()));
    assert!(!r.ctl.library_locked());
}

#[test]
fn an_old_outcome_never_frees_a_newer_step() {
    let mut r = with_password(Tier::Pro).restart(Tier::Pro);
    r.ctl.request_unlock();
    let old = run_elsewhere(
        r.ctl
            .begin_unlock_with_password(password())
            .expect("checked")
            .expect("locked"),
    );
    r.ctl.cancel_unlock();
    r.ctl.request_unlock();
    let newer = r
        .ctl
        .begin_unlock_with_password(password())
        .expect("checked")
        .expect("locked");
    assert_eq!(r.ctl.finish_master(old), Err(MasterProblem::Failed));
    assert!(
        r.ctl.master_busy().is_some(),
        "the newer step is still running"
    );
    assert_eq!(r.ctl.finish_master(run_elsewhere(newer)), Ok(()));
}

// ---- Starting locked and the unlock prompt -----------------------------------------------------

#[test]
fn a_library_with_a_password_starts_locked_and_says_nothing() {
    let r = with_password(Tier::Pro).restart(Tier::Pro);
    assert_eq!(r.ctl.master_password_state(), MasterState::Set);
    assert!(r.ctl.library_locked());
    assert_eq!(r.secret(), None, "the text is out of reach");
    assert_eq!(r.toast(), None, "no toast at start-up");
    assert!(!r.ctl.unlock_prompt_open(), "the prompt waits for a need");
}

#[test]
fn needing_a_locked_item_opens_the_prompt_and_the_right_password_unlocks() {
    let mut r = with_password(Tier::Pro).restart(Tier::Pro);
    let used = r.ctl.use_item(r.id, UseMode::Copy);
    assert!(matches!(used, snipdeck_app::controller::UseResult::Failed));
    assert_eq!(r.toast(), Some(ToastMessage::Locked));
    assert!(r.ctl.unlock_prompt_open());
    assert!(r.ctl.take_pending().contains(&Effect::OpenUnlock));

    assert_eq!(
        r.ctl.unlock_with_password(support::pw("wrong")),
        Err(MasterProblem::WrongPassword)
    );
    assert_eq!(
        r.ctl.unlock_with_password(""),
        Err(MasterProblem::CurrentEmpty)
    );
    assert!(r.ctl.library_locked(), "still locked after the wrong tries");
    assert!(r.ctl.unlock_prompt_open(), "the prompt stays for a retry");

    r.ctl.unlock_with_password(password()).expect("unlock");
    assert!(!r.ctl.library_locked());
    assert!(!r.ctl.unlock_prompt_open());
    assert_eq!(r.secret().as_deref(), Some(SECRET));
    assert_eq!(r.toast(), Some(ToastMessage::Unlocked));
}

#[test]
fn closing_the_prompt_leaves_the_library_locked_and_asking_again_reopens_it() {
    let mut r = with_password(Tier::Pro).restart(Tier::Pro);
    r.ctl.request_unlock();
    assert!(r.ctl.unlock_prompt_open());
    r.ctl.cancel_unlock();
    assert!(!r.ctl.unlock_prompt_open());
    assert!(r.ctl.library_locked());
    r.ctl.take_pending();
    r.ctl.request_unlock();
    assert_eq!(r.ctl.take_pending(), vec![Effect::OpenUnlock]);
}

#[test]
fn an_unlocked_library_or_one_without_a_password_never_prompts() {
    let mut r = rig(Tier::Pro);
    r.ctl.request_unlock();
    assert!(!r.ctl.unlock_prompt_open());
    assert_eq!(r.ctl.take_pending(), Vec::<Effect>::new());
    let mut r = with_password(Tier::Pro);
    r.ctl.request_unlock();
    assert!(!r.ctl.unlock_prompt_open(), "open already");
}

#[test]
fn duplicating_a_sensitive_row_while_locked_asks_for_the_password_and_copies_nothing() {
    let mut r = with_password(Tier::Pro).restart(Tier::Pro);
    let count = || r.ctl.store().all_items().expect("items").len();
    let before = count();
    let at = r
        .ctl
        .view()
        .rows
        .iter()
        .position(|row| row.id == r.id)
        .expect("the sensitive row is in the panel");
    r.ctl.duplicate_row(at);
    assert_eq!(r.toast(), Some(ToastMessage::UnlockFirst));
    assert!(r.ctl.unlock_prompt_open());
    assert!(r.ctl.take_pending().contains(&Effect::OpenUnlock));
    assert_eq!(r.ctl.store().all_items().expect("items").len(), before);

    r.ctl.unlock_with_password(password()).expect("unlock");
    let at = r
        .ctl
        .view()
        .rows
        .iter()
        .position(|row| row.id == r.id)
        .expect("the row");
    r.ctl.duplicate_row(at);
    assert_eq!(r.ctl.store().all_items().expect("items").len(), before + 1);
}

#[test]
fn revealing_a_sensitive_row_while_locked_asks_for_the_password_instead() {
    let mut r = with_password(Tier::Pro).restart(Tier::Pro);
    let at = r
        .ctl
        .view()
        .rows
        .iter()
        .position(|row| row.id == r.id)
        .expect("the sensitive row is in the panel");
    r.ctl.reveal_row(at, true);
    assert_eq!(r.ctl.view().revealed, None);
    assert!(r.ctl.unlock_prompt_open());
    r.ctl.unlock_with_password(password()).expect("unlock");
    r.ctl.reveal_row(at, true);
    let view = r.ctl.view();
    assert_eq!(view.revealed, Some(at));
    assert_eq!(view.rows[at].preview, SECRET);
}

#[test]
fn opening_a_locked_sensitive_item_in_the_editor_prompts() {
    let mut r = with_password(Tier::Pro).restart(Tier::Pro);
    r.ctl.open_item_editor(r.id);
    assert!(
        !r.ctl.item_editor_open(),
        "nothing decrypted without the key"
    );
    assert!(r.ctl.unlock_prompt_open());
    r.ctl.unlock_with_password(password()).expect("unlock");
    r.ctl.open_item_editor(r.id);
    assert_eq!(
        r.ctl.item_editor_view().expect("editor").fields.content,
        SECRET
    );
}

// ---- Change and remove -------------------------------------------------------------------------

#[test]
fn changing_the_password_needs_the_current_one_and_keeps_every_item() {
    let mut r = with_password(Tier::Pro);
    let new = support::pw("marble");
    assert_eq!(
        r.ctl.change_master_password(support::pw("wrong"), new, new),
        Err(MasterProblem::WrongPassword)
    );
    assert_eq!(
        r.ctl.change_master_password("", new, new),
        Err(MasterProblem::CurrentEmpty)
    );
    assert_eq!(
        r.ctl.change_master_password(password(), "short", "short"),
        Err(MasterProblem::TooShort)
    );
    assert_eq!(
        r.ctl
            .change_master_password(password(), new, "marble orchard 78"),
        Err(MasterProblem::Mismatch)
    );
    r.ctl
        .change_master_password(password(), new, new)
        .expect("change");
    assert_eq!(r.toast(), Some(ToastMessage::MasterPasswordChanged));
    let mut r = r.restart(Tier::Pro);
    assert_eq!(
        r.ctl.unlock_with_password(password()),
        Err(MasterProblem::WrongPassword),
        "the old password no longer opens it"
    );
    r.ctl.unlock_with_password(new).expect("the new one does");
    assert_eq!(r.secret().as_deref(), Some(SECRET));
}

#[test]
fn changing_the_password_from_a_locked_library_unlocks_it() {
    let mut r = with_password(Tier::Pro).restart(Tier::Pro);
    assert!(r.ctl.library_locked());
    let new = support::pw("marble");
    r.ctl
        .change_master_password(password(), new, new)
        .expect("change");
    assert!(!r.ctl.library_locked(), "the user proved the password");
    assert_eq!(r.secret().as_deref(), Some(SECRET));
}

#[test]
fn removing_the_password_needs_the_current_one_and_returns_to_the_os_key_alone() {
    let mut r = with_password(Tier::Pro);
    assert_eq!(
        r.ctl.remove_master_password(support::pw("wrong")),
        Err(MasterProblem::WrongPassword)
    );
    assert_eq!(r.ctl.master_password_state(), MasterState::Set, "unchanged");
    r.ctl.remove_master_password(password()).expect("remove");
    assert_eq!(r.ctl.master_password_state(), MasterState::NotSet);
    assert_eq!(r.toast(), Some(ToastMessage::MasterPasswordRemoved));
    let r = r.restart(Tier::Pro);
    assert!(!r.ctl.library_locked(), "opens at start-up again");
    assert_eq!(r.secret().as_deref(), Some(SECRET));
}

#[test]
fn changing_or_removing_when_none_is_set_is_refused() {
    let mut r = rig(Tier::Pro);
    assert_eq!(
        r.ctl
            .change_master_password(password(), password(), password()),
        Err(MasterProblem::Failed)
    );
    assert_eq!(
        r.ctl.remove_master_password(password()),
        Err(MasterProblem::Failed)
    );
}

#[test]
fn setting_again_while_one_is_set_is_refused() {
    let mut r = with_password(Tier::Pro);
    assert_eq!(
        r.ctl
            .set_master_password("another phrase 9", "another phrase 9"),
        Err(MasterProblem::Failed)
    );
}

#[test]
fn a_lapsed_licence_can_still_unlock_change_and_remove() {
    // Set as a Supporter, then the next run has no licence: the library must not be trapped.
    let mut r = with_password(Tier::Pro).restart(Tier::Free);
    assert!(r.ctl.library_locked());
    r.ctl
        .unlock_with_password(password())
        .expect("unlock on Free");
    let new = support::pw("marble");
    r.ctl
        .change_master_password(password(), new, new)
        .expect("change on Free");
    r.ctl.remove_master_password(new).expect("remove on Free");
    assert_eq!(r.ctl.master_password_state(), MasterState::NotSet);
    assert_eq!(
        r.ctl.set_master_password(password(), password()),
        Err(MasterProblem::SupporterOnly),
        "but setting one again is Supporter"
    );
}

// ---- The idle lock, on an injected clock -------------------------------------------------------

#[test]
fn the_library_locks_after_the_idle_time_and_asks_again() {
    let mut r = with_password(Tier::Pro);
    assert_eq!(r.ctl.idle_lock_wait(), Some(IDLE_LOCK));
    r.advance(just_before_idle(Duration::from_secs(1)));
    assert!(!r.ctl.lock_if_idle(), "a second early");
    assert_eq!(r.ctl.idle_lock_wait(), Some(Duration::from_secs(1)));
    assert_eq!(r.secret().as_deref(), Some(SECRET));
    r.advance(Duration::from_secs(1));
    assert!(r.ctl.lock_if_idle(), "due");
    assert!(r.ctl.library_locked());
    assert_eq!(r.secret(), None);
    assert_eq!(r.ctl.idle_lock_wait(), None, "nothing more to wait for");
    r.ctl
        .unlock_with_password(password())
        .expect("unlock again");
    assert_eq!(r.ctl.idle_lock_wait(), Some(IDLE_LOCK), "a fresh count");
    assert_eq!(r.secret().as_deref(), Some(SECRET));
}

#[test]
fn using_the_library_restarts_the_idle_count() {
    let mut r = with_password(Tier::Pro);
    r.advance(just_before_idle(Duration::from_mins(1)));
    let used = r.ctl.use_item(r.id, UseMode::Copy);
    assert!(matches!(used, snipdeck_app::controller::UseResult::Done(_)));
    assert_eq!(
        r.ctl.idle_lock_wait(),
        Some(IDLE_LOCK),
        "counted from the use"
    );
    r.advance(just_before_idle(Duration::from_secs(1)));
    assert!(!r.ctl.lock_if_idle());
    r.advance(Duration::from_secs(1));
    assert!(r.ctl.lock_if_idle());
}

#[test]
fn locking_hides_a_revealed_row() {
    let mut r = with_password(Tier::Pro);
    let at = r
        .ctl
        .view()
        .rows
        .iter()
        .position(|row| row.id == r.id)
        .expect("row");
    r.ctl.reveal_row(at, true);
    assert_eq!(r.ctl.view().revealed, Some(at));
    r.advance(IDLE_LOCK);
    assert!(r.ctl.lock_if_idle());
    let view = r.ctl.view();
    assert_eq!(view.revealed, None);
    assert_eq!(view.rows[at].preview, "", "the text is gone from the row");
}

#[test]
fn without_a_password_nothing_ever_locks() {
    let mut r = rig(Tier::Pro);
    assert_eq!(r.ctl.idle_lock_wait(), None);
    r.advance(IDLE_LOCK * 4);
    assert!(!r.ctl.lock_if_idle());
    r.ctl.lock_now();
    assert_eq!(
        r.secret().as_deref(),
        Some(SECRET),
        "OS-only keys stay open"
    );
}

// ---- A sensitive editor and the lock -----------------------------------------------------------

#[test]
fn locking_closes_a_sensitive_editor_so_no_plain_text_stays_on_screen() {
    let mut r = with_password(Tier::Pro);
    r.ctl.open_item_editor(r.id);
    assert!(r.ctl.item_editor_open());
    r.advance(IDLE_LOCK);
    assert!(r.ctl.lock_if_idle());
    assert!(!r.ctl.item_editor_open(), "the editor is closed");
    assert!(r.ctl.item_editor_view().is_none(), "and holds no text");
    assert!(r.ctl.library_locked());
}

#[test]
fn typing_in_a_sensitive_editor_counts_as_use() {
    let mut r = with_password(Tier::Pro);
    r.ctl.open_item_editor(r.id);
    r.advance(just_before_idle(Duration::from_secs(1)));
    r.ctl.item_editor_edited();
    assert_eq!(
        r.ctl.idle_lock_wait(),
        Some(IDLE_LOCK),
        "counted from the key"
    );
    r.advance(just_before_idle(Duration::from_secs(1)));
    assert!(
        !r.ctl.lock_if_idle(),
        "more than the idle time since opening"
    );
    assert!(r.ctl.item_editor_open(), "still open while typing");
    r.advance(Duration::from_secs(1));
    assert!(r.ctl.lock_if_idle(), "idle after the last key");
    assert!(!r.ctl.item_editor_open());
}

#[test]
fn a_plain_editor_stays_open_through_the_lock() {
    let mut r = with_password(Tier::Pro);
    r.ctl.open_new_item(Origin::Main);
    assert!(r.ctl.item_editor_open());
    r.advance(IDLE_LOCK);
    assert!(r.ctl.lock_if_idle());
    assert!(r.ctl.item_editor_open(), "nothing secret in it");
}

#[test]
fn erasing_the_library_closes_the_item_editor_and_forgets_the_old_index() {
    let mut r = rig(Tier::Pro);
    let before = r.ctl.view().rows.len();
    assert!(before > 0, "the demo library has rows");
    r.ctl.open_new_item(Origin::Main);
    assert!(r.ctl.item_editor_open());
    assert!(r.ctl.erase_library(r.data.path(), false));
    assert!(
        !r.ctl.item_editor_open(),
        "the editor of the old library is closed"
    );
    r.ctl.set_query("a");
    assert_eq!(
        r.ctl.view().rows.len(),
        0,
        "a search must not find items of the erased library"
    );
}

#[test]
fn the_recent_gate_is_closed_right_after_unlocking_a_library_with_a_password() {
    use snipdeck_platform::clipboard_watch::WatchGate;
    let r = with_password(Tier::Pro);
    let Rig {
        ctl,
        clock,
        provider,
        data,
        ..
    } = r;
    drop(ctl);
    let mut ctl = start(Tier::Pro, &provider, data.path(), &clock);
    assert!(
        ctl.library_locked(),
        "a password is set: locked after a restart"
    );
    ctl.update_settings(|s| s.recent_history = true);
    let gate = WatchGate::new();
    ctl.set_recent_gate(gate.clone());
    gate.set(true);
    ctl.unlock_library();
    assert!(ctl.library_locked());
    assert!(!gate.is_open(), "locked: the listener reads nothing");
}

#[test]
fn an_editor_switched_to_sensitive_is_closed_by_the_lock_too() {
    let mut r = with_password(Tier::Pro);
    r.ctl.open_new_item(Origin::Main);
    let v = r.ctl.item_editor_view().expect("editor");
    // The two-second tick hands over what the window holds, including the Sensitive switch.
    r.ctl.item_editor_autosave(&EditorInput {
        sensitive: true,
        content: SECRET.into(),
        ..v.fields
    });
    r.ctl.item_editor_edited();
    r.advance(IDLE_LOCK);
    assert!(r.ctl.lock_if_idle());
    assert!(!r.ctl.item_editor_open());
}

// ---- A key store that goes away right after a change ------------------------------------------

// The key is opened once, while the new record is made, and the library opens with that same key:
// a key store that fails after its first unwrap cannot leave the library locked behind a change
// the user just made.

#[test]
fn a_change_opens_the_library_with_the_key_it_already_derived() {
    let mut r = with_password(Tier::Pro).restart(Tier::Pro);
    let new = support::pw("marble");
    r.provider.fail_unwraps_after(Some(1));
    assert_eq!(r.ctl.change_master_password(password(), new, new), Ok(()));
    assert_eq!(r.toast(), Some(ToastMessage::MasterPasswordChanged));
    assert_eq!(r.ctl.master_password_state(), MasterState::Set);
    assert!(!r.ctl.library_locked(), "opened by the change itself");
    assert_eq!(r.secret().as_deref(), Some(SECRET));
    r.provider.fail_unwraps_after(None);
    let mut r = r.restart(Tier::Pro);
    assert_eq!(
        r.ctl.unlock_with_password(password()),
        Err(MasterProblem::WrongPassword),
        "the old password is gone"
    );
    r.ctl
        .unlock_with_password(new)
        .expect("the new one opens it");
    assert_eq!(r.secret().as_deref(), Some(SECRET));
}

#[test]
fn a_removal_opens_the_library_with_the_key_it_already_derived() {
    let mut r = with_password(Tier::Pro).restart(Tier::Pro);
    r.provider.fail_unwraps_after(Some(1));
    assert_eq!(r.ctl.remove_master_password(password()), Ok(()));
    assert_eq!(r.toast(), Some(ToastMessage::MasterPasswordRemoved));
    assert_eq!(r.ctl.master_password_state(), MasterState::NotSet);
    assert!(!r.ctl.library_locked());
    assert!(!r.ctl.unlock_prompt_open(), "no password left to ask for");
    r.provider.fail_unwraps_after(None);
    let r = r.restart(Tier::Pro);
    assert_eq!(r.secret().as_deref(), Some(SECRET), "opens by itself again");
}

// ---- A key record that cannot be read ----------------------------------------------------------

#[test]
fn a_key_record_that_cannot_be_read_is_not_offered_a_password() {
    let r = with_password(Tier::Pro);
    r.provider.fail_unwraps_after(Some(0));
    let mut r = r.restart(Tier::Pro);
    assert_eq!(r.toast(), Some(ToastMessage::KeyUnavailable));
    assert_eq!(r.ctl.master_password_state(), MasterState::KeyUnavailable);
    assert_eq!(
        r.ctl.set_master_password(password(), password()),
        Err(MasterProblem::Failed),
        "not a second record over one that may hold a password"
    );
    r.provider.fail_unwraps_after(None);
    let r = r.restart(Tier::Pro);
    assert_eq!(
        r.ctl.master_password_state(),
        MasterState::Set,
        "back to normal"
    );
}

// ---- Copies of the library while it is locked --------------------------------------------------

fn backup_files(data: &Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    snipdeck_portable::backup::list_backups(data)
        .expect("list")
        .into_iter()
        .map(|b| {
            let bytes = std::fs::read(&b.path).expect("read");
            (b.path, bytes)
        })
        .collect()
}

#[test]
fn a_locked_library_does_not_export_or_back_up_a_copy_without_its_sensitive_items() {
    let mut r = with_password(Tier::Pro);
    assert!(r.ctl.backup_now(r.data.path()), "a complete one while open");
    let before = backup_files(r.data.path());
    assert_eq!(before.len(), 1);
    let mut r = r.restart(Tier::Pro);
    let out = tempfile::tempdir().expect("dir");

    assert_eq!(r.ctl.export_library(out.path()), None);
    assert_eq!(r.toast(), Some(ToastMessage::UnlockFirst));
    assert!(r.ctl.unlock_prompt_open(), "asks for the password");
    assert_eq!(
        std::fs::read_dir(out.path()).expect("dir").count(),
        0,
        "no partial file"
    );

    r.ctl.cancel_unlock();
    assert!(!r.ctl.backup_now(r.data.path()));
    assert_eq!(r.toast(), Some(ToastMessage::UnlockFirst));
    assert!(r.ctl.unlock_prompt_open());
    assert_eq!(
        backup_files(r.data.path()),
        before,
        "today's complete backup is not replaced"
    );

    r.ctl.unlock_with_password(password()).expect("unlock");
    assert!(r.ctl.export_library(out.path()).is_some());
    assert!(r.ctl.backup_now(r.data.path()));
}

#[test]
fn a_locked_library_does_not_restore_a_backup_that_would_drop_sensitive_items() {
    let mut r = with_password(Tier::Pro);
    assert!(r.ctl.backup_now(r.data.path()));
    let file = backup_files(r.data.path()).remove(0).0;
    let mut r = r.restart(Tier::Pro);

    assert!(!r.ctl.restore_backup(&file));
    assert_eq!(r.toast(), Some(ToastMessage::UnlockFirst));
    assert!(r.ctl.unlock_prompt_open());

    r.ctl.unlock_with_password(password()).expect("unlock");
    assert!(r.ctl.restore_backup(&file));
}

#[test]
fn copying_a_locked_item_puts_nothing_on_the_clipboard() {
    let mut r = with_password(Tier::Pro).restart(Tier::Pro);
    let used = r.ctl.use_item(r.id, UseMode::Copy);
    assert!(matches!(used, snipdeck_app::controller::UseResult::Failed));
    assert_eq!(r.toast(), Some(ToastMessage::Locked));
    assert!(r.ctl.unlock_prompt_open());
    assert!(
        r.ctl.services().clipboard.written.is_empty(),
        "no empty or partial copy"
    );
}

#[test]
fn a_locked_library_does_not_import_a_snipdeck_file_that_would_drop_sensitive_items() {
    let mut r = with_password(Tier::Pro);
    assert!(r.ctl.backup_now(r.data.path()));
    let file = backup_files(r.data.path()).remove(0).0;
    let mut r = r.restart(Tier::Pro);
    let source = ImportSource::Snipdeck(file);

    assert!(!r.ctl.apply_import(&source));
    assert_eq!(r.toast(), Some(ToastMessage::UnlockFirst));
    assert!(r.ctl.unlock_prompt_open());

    r.ctl.unlock_with_password(password()).expect("unlock");
    assert!(r.ctl.apply_import(&source));
}

// ---- Recent while locked -------------------------------------------------------------------------

#[test]
fn while_locked_recent_is_hidden_and_collects_nothing_until_unlocked() {
    use snipdeck_app::recent::ClipboardObservation;
    let copied = |text: &str| ClipboardObservation {
        text: Some(text.to_owned()),
        source_app: Some("notepad.exe".to_owned()),
        ..ClipboardObservation::default()
    };
    let mut r = with_password(Tier::Pro);
    r.ctl.update_settings(|s| s.recent_history = true);
    r.ctl.on_clipboard_change(&copied("before the lock"));
    assert_eq!(r.ctl.recent_entries().len(), 1);

    r.ctl.lock_now();
    assert!(r.ctl.library_locked());
    assert!(
        r.ctl.recent_entries().is_empty(),
        "nothing listed while locked"
    );
    let view = r.ctl.view().recent.expect("history is on");
    assert!(
        view.locked && view.rows.is_empty(),
        "the Panel asks for the unlock"
    );
    assert!(r.ctl.main_screen().recent_locked, "so does the Main window");
    r.ctl.on_clipboard_change(&copied("while locked"));

    r.ctl.unlock_with_password(password()).expect("unlock");
    let texts: Vec<_> = r.ctl.recent_entries().into_iter().map(|e| e.text).collect();
    assert_eq!(
        texts,
        ["before the lock"],
        "nothing was recorded while locked"
    );
    assert!(!r.ctl.view().recent.expect("on").locked);
}
