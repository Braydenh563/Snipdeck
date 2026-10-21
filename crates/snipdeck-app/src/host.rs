//! The seams between the app and the machine it runs on, beyond the clipboard, keyboard and
//! timing traits that `snipdeck-platform` already owns: the wall clock, the Target tracker and
//! the global hotkeys. Each has a real implementation (the Windows backend, in `windows_host`)
//! and a fake here, so the controller runs under test with no desktop.

use std::{cell::Cell, cell::RefCell, rc::Rc};

use snipdeck_core::Timestamp;
use snipdeck_platform::{
    chord::Chord,
    route::{ScreenRect, Target},
};

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

/// Reading the text on the clipboard (quick capture). The pipeline's [`Clipboard`] only
/// writes, snapshots and restores; this is the one read the app needs.
///
/// [`Clipboard`]: snipdeck_platform::pipeline::Clipboard
pub trait ClipboardText {
    /// The clipboard's text, or `None` when it holds none or stays busy. Writes nothing.
    fn read_text(&mut self) -> Option<String>;
    /// Whether the clipboard holds a picture (a screenshot, say).
    fn has_image(&mut self) -> bool;
    /// Empties the clipboard if it still holds exactly `text` (so something the user copied
    /// since is never lost). Whether it was cleared.
    fn clear_if_text(&mut self, text: &str) -> bool;
    /// The clipboard's sequence number, which changes whenever its contents do; `None` when it
    /// cannot be read. Covers content that is not text (HTML, images, files).
    fn sequence_number(&mut self) -> Option<u32>;
    /// The sequence number read right after this clipboard's latest write (not now): the
    /// reference for "nothing has touched the clipboard since our write". `None` before a write.
    fn written_sequence(&mut self) -> Option<u32>;
    /// Empties the clipboard, whatever formats it holds, if its sequence number is still
    /// `sequence`. Whether it did.
    fn clear_if_sequence(&mut self, sequence: u32) -> bool;
}

#[cfg(windows)]
impl ClipboardText for snipdeck_platform::windows::clipboard::WinClipboard {
    fn read_text(&mut self) -> Option<String> {
        Self::read_text(self)
    }
    fn has_image(&mut self) -> bool {
        Self::has_image(self)
    }
    fn clear_if_text(&mut self, text: &str) -> bool {
        Self::clear_if_text(self, text)
    }
    fn sequence_number(&mut self) -> Option<u32> {
        Self::sequence_number(self)
    }
    fn written_sequence(&mut self) -> Option<u32> {
        Self::written_sequence(self)
    }
    fn clear_if_sequence(&mut self, sequence: u32) -> bool {
        Self::clear_if_sequence(self, sequence)
    }
}

/// The fake clipboard "holds" what was last written to it (as a real one holds what the user
/// last copied), so a test seeds it by pushing a payload onto `written`.
impl ClipboardText for snipdeck_platform::testing::FakeClipboard {
    fn read_text(&mut self) -> Option<String> {
        self.text_reads += 1;
        self.written.last()?.data().iter().find_map(|d| match d {
            snipdeck_platform::payload::Data::Text(text) => Some(text.clone()),
            _ => None,
        })
    }
    fn has_image(&mut self) -> bool {
        self.written.last().is_some_and(|p| {
            p.data()
                .iter()
                .any(|d| matches!(d, snipdeck_platform::payload::Data::Png(_)))
        })
    }
    fn clear_if_text(&mut self, text: &str) -> bool {
        if self.read_text().as_deref() != Some(text) || self.busy_for_clear() {
            return false;
        }
        self.written.clear();
        self.sequence += 1;
        self.log.push("clear".into());
        true
    }
    fn sequence_number(&mut self) -> Option<u32> {
        Some(self.sequence)
    }
    fn written_sequence(&mut self) -> Option<u32> {
        self.write_sequence
    }
    fn clear_if_sequence(&mut self, sequence: u32) -> bool {
        if self.sequence != sequence || self.busy_for_clear() {
            return false;
        }
        self.written.clear();
        self.sequence += 1;
        self.log.push("clear".into());
        true
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
    /// Where the Target's text caret is on the screen, in physical pixels, if it has a system
    /// caret. Read before a popup takes the foreground. `None` (the default) means "unknown":
    /// the Picker then opens at the mouse.
    fn caret(&self) -> Option<ScreenRect> {
        None
    }
}

/// A scripted Target. Cloning shares the state, so a test keeps a handle after handing the
/// source to the controller.
#[derive(Debug, Clone, Default)]
pub struct FakeTargets {
    current: Rc<RefCell<Option<Target>>>,
    refocused: Rc<Cell<u32>>,
    caret: Rc<Cell<Option<ScreenRect>>>,
}

impl FakeTargets {
    /// A source whose Target is `target`.
    #[must_use]
    pub fn new(target: Option<Target>) -> Self {
        Self {
            current: Rc::new(RefCell::new(target)),
            refocused: Rc::default(),
            caret: Rc::default(),
        }
    }

    /// Sets where the Target's caret is (`None`: it has none).
    pub fn set_caret(&self, caret: Option<ScreenRect>) {
        self.caret.set(caret);
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

    fn caret(&self) -> Option<ScreenRect> {
        self.caret.get()
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

    /// Removes the chord under `id`. Removing an id that is not registered succeeds.
    ///
    /// # Errors
    ///
    /// [`HotkeyFailure::Unavailable`] if the service is not running or Windows refuses.
    fn unregister(&mut self, id: u32) -> Result<(), HotkeyFailure>;
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
    backend: &mut (impl HotkeyBackend + ?Sized),
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

/// Switches every global hotkey off ("Pause hotkeys"); returns how many could not be removed.
/// The settings are untouched: [`register_all`] switches them back on.
pub fn unregister_all(backend: &mut (impl HotkeyBackend + ?Sized)) -> usize {
    HotkeyAction::ALL
        .into_iter()
        .filter(|action| backend.unregister(id_of(*action)).is_err())
        .count()
}

/// What switching the hotkeys off or on did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PauseReport {
    /// Hotkeys that could not be registered when resuming.
    pub problems: Vec<HotkeyProblem>,
    /// Hotkeys that could not be removed when pausing.
    pub not_removed: usize,
}

/// "Pause hotkeys" changed: switches every hotkey off, or registers the chords in `current`
/// again. `current` must be the chords in force *now* (the settings may have changed since
/// start-up), not the ones read at launch.
pub fn set_paused(
    backend: &mut (impl HotkeyBackend + ?Sized),
    paused: bool,
    current: &crate::settings::Hotkeys,
) -> PauseReport {
    if paused {
        PauseReport {
            not_removed: unregister_all(backend),
            ..PauseReport::default()
        }
    } else {
        PauseReport {
            problems: register_all(backend, current),
            ..PauseReport::default()
        }
    }
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

    fn unregister(&mut self, id: u32) -> Result<(), HotkeyFailure> {
        self.registered.retain(|(existing, _)| *existing != id);
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

    /// Every action has a chord (Capture is off by default, see `HotkeyAction::default_chord`).
    fn all_on() -> Hotkeys {
        Hotkeys {
            capture: "Ctrl+Shift+2".into(),
            ..Hotkeys::default()
        }
    }

    #[test]
    fn every_default_hotkey_is_registered_and_capture_stays_off_until_set() {
        let mut backend = FakeHotkeys::default();
        let problems = register_all(&mut backend, &Hotkeys::default());
        assert_eq!(problems, Vec::new());
        assert_eq!(backend.registered.len(), 3);
        assert!(
            backend
                .registered
                .iter()
                .all(|(id, _)| *id != id_of(HotkeyAction::Capture))
        );
        let mut backend = FakeHotkeys::default();
        assert_eq!(register_all(&mut backend, &all_on()), Vec::new());
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
        let problems = register_all(&mut backend, &all_on());
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
    fn pausing_removes_every_hotkey_and_resuming_registers_them_again() {
        let mut backend = FakeHotkeys::default();
        let hotkeys = all_on();
        assert_eq!(register_all(&mut backend, &hotkeys), Vec::new());
        assert_eq!(unregister_all(&mut backend), 0);
        assert_eq!(backend.registered, Vec::new());
        assert_eq!(unregister_all(&mut backend), 0, "pausing twice is fine");
        assert_eq!(register_all(&mut backend, &hotkeys), Vec::new());
        assert_eq!(backend.registered.len(), 4);
    }

    #[test]
    fn resuming_registers_the_chords_in_force_now_not_the_ones_from_start_up() {
        let mut backend = FakeHotkeys::default();
        let at_start = all_on();
        assert_eq!(register_all(&mut backend, &at_start), Vec::new());
        assert_eq!(
            set_paused(&mut backend, true, &at_start),
            PauseReport::default()
        );
        assert_eq!(backend.registered, Vec::new());

        // While paused the settings changed: the Picker moved, Capture was switched off.
        let now = Hotkeys {
            picker: "Ctrl+Alt+P".into(),
            capture: String::new(),
            ..Hotkeys::default()
        };
        assert_eq!(
            set_paused(&mut backend, false, &now),
            PauseReport::default()
        );
        let picker = backend
            .registered
            .iter()
            .find(|(id, _)| *id == id_of(HotkeyAction::Picker))
            .expect("picker");
        assert_eq!(picker.1.to_string(), "Ctrl+Alt+P");
        assert_eq!(backend.registered.len(), 3, "capture stays off");
    }

    #[test]
    fn resuming_reports_a_chord_another_app_took_while_paused() {
        let mut backend = FakeHotkeys::default();
        let hotkeys = Hotkeys::default();
        let _ = set_paused(&mut backend, true, &hotkeys);
        backend.taken.push("Ctrl+Alt+Space".into());
        let report = set_paused(&mut backend, false, &hotkeys);
        assert_eq!(report.problems.len(), 1);
        assert_eq!(report.problems[0].action, HotkeyAction::TogglePanel);
        assert_eq!(report.not_removed, 0);
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
