//! Files an older build wrote still open. Settings that carry licence, tier or supporter keys, and
//! a `licence.key` left in the data folder, are ignored: nothing is gated, nothing is rejected, and
//! nothing the user set is lost.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

mod support;
use support::ArchiveSteps as _;

use snipdeck_app::{
    controller::{Controller, Services},
    demo,
    host::{FakeTargets, FixedClock},
    paths::{LoadOutcome, read_settings},
    settings::Settings,
    toast::ToastKind,
};
use snipdeck_core::Timestamp;
use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};
use snipdeck_store::Store;

fn now() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

/// The settings an old build might have written: a changed value of our own and the keys that no
/// longer mean anything.
fn changed() -> Settings {
    let base = Settings::new_install();
    Settings {
        close_to_tray: !base.close_to_tray,
        ..base
    }
}

fn old_json() -> String {
    let mut value: serde_json::Value = serde_json::from_str(&changed().to_json()).expect("json");
    let map = value.as_object_mut().expect("an object");
    map.insert("licence".into(), "SNIP1.abc.def".into());
    map.insert("licence_key".into(), "SNIP1.abc.def".into());
    map.insert("tier".into(), "pro".into());
    map.insert("supporter".into(), true.into());
    map.insert(
        "entitlements".into(),
        serde_json::json!({ "tier": "pro", "boards": 5 }),
    );
    serde_json::to_string_pretty(&value).expect("text")
}

#[test]
fn settings_with_licence_tier_and_supporter_keys_load_and_keep_every_other_value() {
    let loaded = Settings::from_json(&old_json());
    assert_eq!(loaded, changed());
    // And the old keys are not written back.
    let written = loaded.to_json();
    for gone in ["licence", "tier", "supporter", "entitlements"] {
        assert!(!written.contains(gone), "{gone} is still written");
    }
}

#[test]
fn a_settings_file_with_old_keys_is_read_in_place_not_set_aside() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("settings.json"), old_json()).expect("write");
    let loaded = read_settings(dir.path());
    assert_eq!(loaded.outcome, LoadOutcome::Read);
    assert_eq!(loaded.settings, changed(), "the user's values are kept");
    assert!(!dir.path().join("settings.json.bad").exists());
}

#[test]
fn a_stale_licence_file_in_the_data_folder_is_ignored_and_the_library_works() {
    let dir = tempfile::tempdir().expect("dir");
    let stale = dir.path().join("licence.key");
    std::fs::write(&stale, "SNIP1.stale.stale").expect("write");
    // The real store opens beside it ...
    drop(Store::open(dir.path()).expect("the library opens"));
    // ... and a controller over a library works: nothing is refused, a backup is made.
    let mut ctl = Controller::new(
        demo::store(now()).expect("demo store"),
        Services {
            clipboard: FakeClipboard::default(),
            keyboard: FakeKeyboard::default(),
            timing: FakeTiming::default(),
        },
        Box::new(FakeTargets::new(Some(demo::target()))),
        Box::new(FixedClock::new(now())),
        read_settings(dir.path()).settings,
    )
    .expect("controller");
    assert!(ctl.backup_now(dir.path()), "a backup still works");
    assert!(
        ctl.view().toast.is_none_or(|t| t.kind != ToastKind::Danger),
        "no refusal"
    );
    assert_eq!(
        std::fs::read_to_string(&stale).expect("kept"),
        "SNIP1.stale.stale"
    );
}
