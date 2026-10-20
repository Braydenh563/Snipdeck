//! The Recent inbox is encrypted at rest with the library key (SPEC 5.6, `docs/privacy.md`):
//! sealed on disk with or without a master password, hidden and paused while locked, and old
//! plain rows sealed at the first start with a key. Over a library on disk; the clock is a
//! `FixedClock`, and no password or marker is written in the source.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code: a failure is a failed test"
)]

use std::path::Path;

use snipdeck_app::{
    controller::{Controller, Services},
    demo,
    drafts::Drafts,
    host::{FakeTargets, FixedClock},
    recent::ClipboardObservation,
    settings::Settings,
};
use snipdeck_core::{Entitlements, Tier, Timestamp};
use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};
use snipdeck_store::{ItemRepo, RecentRepo, Store, crypto::KdfParams};

#[path = "support/mod.rs"]
mod support;
use support::{SharedProvider, assert_no_plaintext};

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

fn t0() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

/// A marker that appears nowhere else, built at run time.
fn marker(label: &str) -> String {
    let key = blake3::derive_key("snipdeck recent at rest marker", label.as_bytes());
    format!(
        "MK-{}",
        &blake3::Hash::from_bytes(key).to_hex().as_str()[..20]
    )
}

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
        Settings {
            recent_history: true,
            ..Settings::default()
        },
        Entitlements::new(tier),
    )
    .expect("controller");
    ctl.set_drafts(Drafts::in_data_dir(data));
    ctl.set_key_provider(Box::new(provider.clone()));
    ctl.set_kdf_params(KdfParams::insecure_for_tests(64, 1, 1).expect("cheap params"));
    ctl.unlock_library();
    ctl
}

fn copied(text: &str) -> ClipboardObservation {
    ClipboardObservation {
        text: Some(text.to_owned()),
        source_app: Some(marker("app")),
        ..ClipboardObservation::default()
    }
}

fn texts(ctl: &Ctl) -> Vec<String> {
    ctl.recent_entries().into_iter().map(|e| e.text).collect()
}

#[test]
fn without_a_master_password_a_copy_is_sealed_on_disk_and_reads_back() {
    let data = tempfile::tempdir().expect("dir");
    let provider = SharedProvider::new();
    let clock = FixedClock::new(t0());
    let (m, app) = (marker("free"), marker("app"));
    let mut ctl = start(Tier::Free, &provider, data.path(), &clock);
    ctl.on_clipboard_change(&copied(&m));
    assert_eq!(texts(&ctl), std::slice::from_ref(&m));
    drop(ctl);
    assert_no_plaintext(data.path(), &[m.as_bytes(), app.as_bytes()]);
    // A restart opens the key by itself and the entry is still there.
    let ctl = start(Tier::Free, &provider, data.path(), &clock);
    assert_eq!(texts(&ctl), [m]);
}

#[test]
fn promote_prune_and_clear_work_on_sealed_entries() {
    let data = tempfile::tempdir().expect("dir");
    let provider = SharedProvider::new();
    let clock = FixedClock::new(t0());
    let m = marker("promote");
    let mut ctl = start(Tier::Free, &provider, data.path(), &clock);
    ctl.on_clipboard_change(&copied(&m));
    let id = ctl.recent_entries()[0].id;
    let list = snipdeck_store::ListRepo::lists(
        ctl.store(),
        snipdeck_store::BoardRepo::boards(ctl.store()).expect("boards")[0].id,
    )
    .expect("lists")[0]
        .id;
    let before = ctl.store().items(list).expect("items").len();
    ctl.recent_promote(id, list);
    assert_eq!(ctl.store().items(list).expect("items").len(), before + 1);
    assert_eq!(texts(&ctl), [m]);
    ctl.prune_recent();
    assert_eq!(texts(&ctl).len(), 1);
    assert_eq!(ctl.recent_clear(), 1);
    assert_eq!(texts(&ctl), Vec::<String>::new());
}

#[test]
fn with_a_master_password_the_inbox_is_sealed_locked_and_paused_until_unlock() {
    let data = tempfile::tempdir().expect("dir");
    let provider = SharedProvider::new();
    let clock = FixedClock::new(t0());
    let (before, during) = (marker("before"), marker("during"));
    let pw = support::pw("recent");
    let mut ctl = start(Tier::Pro, &provider, data.path(), &clock);
    ctl.on_clipboard_change(&copied(&before));
    ctl.set_master_password(pw, pw).expect("set the password");
    ctl.lock_now();
    assert!(ctl.recent_locked());
    assert!(texts(&ctl).is_empty(), "nothing is listed while locked");
    ctl.on_clipboard_change(&copied(&during));
    ctl.unlock_with_password(pw).expect("unlock");
    assert_eq!(
        texts(&ctl),
        std::slice::from_ref(&before),
        "nothing was recorded while locked"
    );
    drop(ctl);
    assert_no_plaintext(
        data.path(),
        &[
            before.as_bytes(),
            during.as_bytes(),
            marker("app").as_bytes(),
        ],
    );
    // A restart starts locked: the store itself refuses to read.
    let mut ctl = start(Tier::Pro, &provider, data.path(), &clock);
    assert!(ctl.recent_locked());
    assert_eq!(texts(&ctl), Vec::<String>::new());
    assert!(ctl.store().recent().is_err(), "the store is locked too");
    ctl.unlock_with_password(pw).expect("unlock");
    assert_eq!(texts(&ctl), [before]);
}

#[test]
fn plain_rows_from_before_encryption_are_sealed_at_the_first_start_with_a_key() {
    let data = tempfile::tempdir().expect("dir");
    let provider = SharedProvider::new();
    let clock = FixedClock::new(t0());
    let m = marker("legacy");
    {
        // No key ring yet: the row is stored plain, as version 4 stored every row.
        let mut store = Store::open(data.path()).expect("store");
        store.add_recent(&m, None, t0()).expect("add");
    }
    let file = std::fs::read(data.path().join("snipdeck.db")).expect("db");
    assert!(
        file.windows(m.len()).any(|w| w == m.as_bytes()),
        "the probe must see the plain row"
    );
    let ctl = start(Tier::Free, &provider, data.path(), &clock);
    assert_eq!(texts(&ctl), std::slice::from_ref(&m), "nothing lost");
    drop(ctl);
    assert_no_plaintext(data.path(), &[m.as_bytes()]);
}
