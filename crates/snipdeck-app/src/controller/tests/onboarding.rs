//! The welcome tour through the controller: when it opens by itself, how Back and Next move
//! between the steps, and that finishing or skipping from any step records it. The clock is the
//! fixed one the other controller tests use.

use super::*;
use crate::tour::TourStep;

/// A controller over a library with a board and a list but no item (a first run), on the fixed
/// clock, with `settings`.
fn first_run(settings: Settings) -> Ctl {
    let now = at("2026-10-17T09:00:00Z");
    let mut store = Store::open_in_memory().expect("store");
    crate::starter::ensure_board(&mut store, now).expect("starter board");
    Controller::new(
        store,
        Services {
            clipboard: FakeClipboard::default(),
            keyboard: FakeKeyboard::default(),
            timing: FakeTiming::default(),
        },
        Box::new(FakeTargets::new(None)),
        Box::new(FixedClock::new(at("2026-10-17T10:30:00Z"))),
        settings,
        Entitlements::new(Tier::Free),
    )
    .expect("controller")
}

#[test]
fn the_tour_is_due_on_an_empty_library_that_has_not_seen_it() {
    assert!(first_run(Settings::default()).tour_due());
}

#[test]
fn the_tour_is_not_due_once_seen_or_when_the_library_has_items() {
    let seen = Settings {
        onboarded: true,
        ..Settings::default()
    };
    assert!(!first_run(seen).tour_due());
    // The ordinary test library holds items: an older settings file (no flag) is not onboarded,
    // and still the tour stays away.
    let w = world();
    assert!(!w.ctl.settings().onboarded);
    assert!(!w.ctl.tour_due());
}

#[test]
fn showing_the_tour_again_starts_at_the_welcome_and_asks_for_the_window() {
    let mut ctl = first_run(Settings {
        onboarded: true,
        ..Settings::default()
    });
    assert_eq!(ctl.tour_step(), None);
    assert_eq!(ctl.show_welcome_tour(), vec![Effect::OpenOnboarding]);
    assert_eq!(ctl.tour_step(), Some(TourStep::Welcome));
    ctl.tour_next();
    ctl.tour_next();
    assert_eq!(ctl.show_welcome_tour(), vec![Effect::OpenOnboarding]);
    assert_eq!(ctl.tour_step(), Some(TourStep::Welcome));
}

#[test]
fn next_and_back_walk_the_four_steps_and_stop_at_the_first() {
    let mut ctl = first_run(Settings::default());
    ctl.show_welcome_tour();
    ctl.tour_back();
    assert_eq!(ctl.tour_step(), Some(TourStep::Welcome));
    for expected in [TourStep::Shortcuts, TourStep::Panel, TourStep::Startup] {
        ctl.tour_next();
        assert_eq!(ctl.tour_step(), Some(expected));
        assert!(!ctl.settings().onboarded, "{expected:?} is not the end");
    }
    ctl.tour_back();
    assert_eq!(ctl.tour_step(), Some(TourStep::Panel));
}

#[test]
fn next_on_the_last_step_finishes_and_records_it() {
    let mut ctl = first_run(Settings::default());
    ctl.show_welcome_tour();
    for _ in 0..3 {
        ctl.tour_next();
    }
    assert_eq!(ctl.tour_step(), Some(TourStep::Startup));
    ctl.tour_next();
    assert_eq!(ctl.tour_step(), None);
    assert!(ctl.settings().onboarded);
    assert!(!ctl.tour_due());
}

#[test]
fn skip_on_every_step_ends_the_tour_and_keeps_the_earlier_choices() {
    for steps in 0..TourStep::ALL.len() {
        let mut ctl = first_run(Settings::default());
        ctl.show_welcome_tour();
        for _ in 0..steps {
            ctl.tour_next();
        }
        // A choice made on the way stays.
        ctl.update_settings(|s| s.close_to_tray = false);
        ctl.tour_skip();
        assert_eq!(ctl.tour_step(), None, "skipped after {steps} steps");
        assert!(ctl.settings().onboarded);
        assert!(!ctl.settings().close_to_tray);
        assert!(!ctl.tour_due());
    }
}

#[test]
fn next_and_skip_do_nothing_harmful_when_no_tour_runs() {
    let mut ctl = first_run(Settings::default());
    ctl.tour_next();
    ctl.tour_back();
    assert_eq!(ctl.tour_step(), None);
    assert!(!ctl.settings().onboarded);
}
