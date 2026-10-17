//! The seams between the app and the machine it runs on, beyond the clipboard, keyboard and
//! timing traits that `snipdeck-platform` already owns: the wall clock, the Target tracker and
//! the global hotkeys. Each has a real implementation (the Windows backend, in `windows_host`)
//! and a fake here, so the controller runs under test with no desktop.

use std::{cell::Cell, cell::RefCell, rc::Rc};

use snipdeck_core::Timestamp;
use snipdeck_platform::{chord::Chord, route::Target};

use crate::settings::HotkeyAction;

/// The wall clock, injected so a test never reads the real one.
pub trait Clock {
    /// The current moment.
    fn now(&self) -> Timestamp;
}

/// [`Clock`] on the system clock.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }
}

/// A clock that only moves when the test sets it. Cloning shares the same time.
#[derive(Debug, Clone)]
pub struct FixedClock(Rc<Cell<Timestamp>>);

impl FixedClock {
    /// A clock stopped at `at`.
    #[must_use]
    pub fn new(at: Timestamp) -> Self {
        Self(Rc::new(Cell::new(at)))
    }

    /// Moves the clock.
    pub fn set(&self, at: Timestamp) {
        self.0.set(at);
    }
}

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0.get()
    }
}

/// Which window a paste would go into, and bringing it back to the front.
pub trait TargetSource {
    /// The window the user was in before they clicked Snipdeck, if it can still be pasted
    /// into. May probe the other app, so call it when needed, not per frame.
    fn current(&self) -> Option<Target>;
    /// Makes the Target the foreground window again so injected keys reach it (the panel may
    /// have activated itself for text entry). `false` if there is no Target or Windows refused.
    fn refocus(&self) -> bool;
}

/// A scripted Target. Cloning shares the state, so a test keeps a handle after handing the
/// source to the controller.
#[derive(Debug, Clone, Default)]
pub struct FakeTargets {
    current: Rc<RefCell<Option<Target>>>,
    refocused: Rc<Cell<u32>>,
}

impl FakeTargets {
    /// A source whose Target is `target`.
    #[must_use]
    pub fn new(target: Option<Target>) -> Self {
        Self {
            current: Rc::new(RefCell::new(target)),
            refocused: Rc::default(),
        }
    }

    /// Changes the Target, as the user switching windows would.
    pub fn set(&self, target: Option<Target>) {
        *self.current.borrow_mut() = target;
    }

    /// How many times the controller brought the Target back to the front.
    #[must_use]
    pub fn refocus_count(&self) -> u32 {
        self.refocused.get()
    }
}

impl TargetSource for FakeTargets {
    fn current(&self) -> Option<Target> {
        self.current.borrow().clone()
    }

    fn refocus(&self) -> bool {
        self.refocused.set(self.refocused.get() + 1);
        self.current.borrow().is_some()
    }
}

/// Why a global hotkey could not be registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HotkeyFailure {
    /// Another application already owns the chord.
    #[error("another application already uses this shortcut")]
    InUse,
    /// The chord cannot be registered.
    #[error("this shortcut cannot be used")]
    Invalid,
    /// The hotkey service is not running or Windows refused.
    #[error("the hotkey service is unavailable")]
    Unavailable,
}

/// Registers global hotkeys under numeric ids (see [`id_of`]).
pub trait HotkeyBackend {
    /// Registers `chord` under `id`, replacing any chord already under `id`.
    ///
    /// # Errors
    ///
    /// See [`HotkeyFailure`].
    fn register(&mut self, id: u32, chord: Chord) -> Result<(), HotkeyFailure>;
}

/// The id a hotkey action is registered under.
#[must_use]
pub const fn id_of(action: HotkeyAction) -> u32 {
    match action {
        HotkeyAction::TogglePanel => 1,
        HotkeyAction::Picker => 2,
        HotkeyAction::Capture => 3,
        HotkeyAction::PasteNext => 4,
    }
}

/// The action registered under `id`, if any.
#[must_use]
pub fn action_of(id: u32) -> Option<HotkeyAction> {
    HotkeyAction::ALL.into_iter().find(|a| id_of(*a) == id)
}

/// A hotkey that could not be registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyProblem {
    /// What the hotkey would have done.
    pub action: HotkeyAction,
    /// The chord as configured, for the message.
    pub chord: String,
    /// Why.
    pub failure: HotkeyFailure,
}

/// Registers every configured hotkey. A hotkey that is off (empty) is skipped; one that fails
/// is reported and the rest are still registered.
pub fn register_all(
    backend: &mut impl HotkeyBackend,
    hotkeys: &crate::settings::Hotkeys,
) -> Vec<HotkeyProblem> {
    let mut problems = Vec::new();
    for action in HotkeyAction::ALL {
        let Some(chord) = hotkeys.chord(action) else {
            continue;
        };
        let shown = chord.to_string();
        if let Err(failure) = backend.register(id_of(action), chord) {
            problems.push(HotkeyProblem {
                action,
                chord: shown,
                failure,
            });
        }
    }
    problems
}

/// A hotkey backend that records registrations and can refuse chosen chords.
#[derive(Debug, Default)]
pub struct FakeHotkeys {
    /// Every successful registration, in order.
    pub registered: Vec<(u32, Chord)>,
    /// Chords (as text) another app "owns".
    pub taken: Vec<String>,
}

impl HotkeyBackend for FakeHotkeys {
    fn register(&mut self, id: u32, chord: Chord) -> Result<(), HotkeyFailure> {
        if self.taken.contains(&chord.to_string()) {
            return Err(HotkeyFailure::InUse);
        }
        self.registered.retain(|(existing, _)| *existing != id);
        self.registered.push((id, chord));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Hotkeys;

    #[test]
    fn ids_round_trip_and_are_distinct() {
        for action in HotkeyAction::ALL {
            assert_eq!(action_of(id_of(action)), Some(action));
        }
        assert_eq!(action_of(0), None);
        assert_eq!(action_of(99), None);
    }

    #[test]
    fn every_default_hotkey_is_registered() {
        let mut backend = FakeHotkeys::default();
        let problems = register_all(&mut backend, &Hotkeys::default());
        assert_eq!(problems, Vec::new());
        assert_eq!(backend.registered.len(), 4);
        let toggle = backend
            .registered
            .iter()
            .find(|(id, _)| *id == id_of(HotkeyAction::TogglePanel))
            .expect("toggle registered");
        assert_eq!(toggle.1.to_string(), "Ctrl+Alt+Space");
    }

    #[test]
    fn a_chord_another_app_owns_is_reported_and_the_rest_still_register() {
        let mut backend = FakeHotkeys {
            taken: vec!["Ctrl+Shift+Space".into()],
            ..FakeHotkeys::default()
        };
        let problems = register_all(&mut backend, &Hotkeys::default());
        assert_eq!(
            problems,
            [HotkeyProblem {
                action: HotkeyAction::Picker,
                chord: "Ctrl+Shift+Space".into(),
                failure: HotkeyFailure::InUse,
            }]
        );
        assert_eq!(backend.registered.len(), 3);
    }

    #[test]
    fn a_hotkey_that_is_off_is_skipped() {
        let mut backend = FakeHotkeys::default();
        let hotkeys = Hotkeys {
            capture: String::new(),
            ..Hotkeys::default()
        };
        assert_eq!(register_all(&mut backend, &hotkeys), Vec::new());
        assert_eq!(backend.registered.len(), 3);
        assert!(
            backend
                .registered
                .iter()
                .all(|(id, _)| *id != id_of(HotkeyAction::Capture))
        );
    }

    #[test]
    fn a_fake_target_source_counts_refocus_calls_through_clones() {
        let source = FakeTargets::new(None);
        let handle = source.clone();
        assert!(!source.refocus(), "nothing to refocus");
        assert_eq!(handle.refocus_count(), 1);
        assert_eq!(source.current(), None);
    }

    #[test]
    fn the_fixed_clock_moves_only_when_set() {
        let clock = FixedClock::new("2026-10-17T09:00:00Z".parse().expect("ts"));
        let other = clock.clone();
        assert_eq!(clock.now().to_string(), "2026-10-17T09:00:00Z");
        other.set("2026-10-18T09:00:00Z".parse().expect("ts"));
        assert_eq!(clock.now().to_string(), "2026-10-18T09:00:00Z");
    }
}
