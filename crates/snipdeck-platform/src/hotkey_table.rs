//! The decisions behind the global-hotkey thread that need no Windows API.
//!
//! `windows::hotkey` owns the thread and the `RegisterHotKey` calls; everything it decides lives
//! here, behind the [`RawHotkeys`] seam, so it is tested on every OS with a fake:
//!
//! - [`HotkeyTable`]: which id holds which chord, replacing an id atomically from the caller's
//!   point of view (a failed rebind puts the old chord back), telling "Snipdeck's other action
//!   has this chord" from "another program has it" (the service's own table is checked first),
//!   and probing a chord (register, then unregister) without disturbing anything.
//! - [`Ticket`]: one per command sent to the thread. A caller that gives up waiting cancels it,
//!   and the thread then skips the command, so a command the caller was told failed never runs
//!   later (the app's view and the thread's table cannot differ). A command the thread has
//!   already started cannot be cancelled: the caller is told so and waits for its answer.
//! - [`wait_until`]: the bounded wait the thread joins use, so `Drop` cannot hang the UI thread.
//!
//! **Unverified on Windows** for the parts that call the OS; the logic here is unit-tested.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::chord::Chord;

/// Hotkey ids not tied to a window must be in `0x0000..=0xBFFF` (`RegisterHotKey` docs). The
/// highest one, [`PROBE_ID`], is the table's own, for [`HotkeyTable::probe`].
pub const MAX_ID: u32 = 0xBFFE;

/// The id [`HotkeyTable::probe`] registers a candidate under for a moment. Callers cannot use it.
pub const PROBE_ID: u32 = 0xBFFF;

/// Why a hotkey could not be registered or removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HotkeyError {
    /// Another application (or Windows itself) already owns the chord
    /// (`ERROR_HOTKEY_ALREADY_REGISTERED`). Show it as a conflict.
    #[error("another application already uses this shortcut")]
    InUse,
    /// The service's own table already holds the chord under another id (the payload): two
    /// Snipdeck actions were given the same shortcut.
    #[error("another Snipdeck shortcut already uses this chord")]
    OwnAction(u32),
    /// The chord or id is not registrable (failed [`Chord::validate`], id above [`MAX_ID`], or
    /// refused by Windows as invalid).
    #[error("this shortcut cannot be used")]
    Invalid,
    /// Windows refused for another reason; the Win32 error code.
    #[error("Windows refused the shortcut (Windows error {0})")]
    Os(u32),
    /// The hotkey thread has stopped or did not answer in time.
    #[error("the hotkey service is not running")]
    Stopped,
}

/// The two OS calls the table makes, for the hotkey thread (`RegisterHotKey` and
/// `UnregisterHotKey`) or a test.
pub trait RawHotkeys {
    /// Registers `chord` under `id` for the calling thread.
    ///
    /// # Errors
    ///
    /// [`HotkeyError::InUse`] when the OS says another program holds it; others as the OS says.
    fn register(&mut self, id: u32, chord: Chord) -> Result<(), HotkeyError>;

    /// Removes the hotkey `id`.
    ///
    /// # Errors
    ///
    /// The OS refused.
    fn unregister(&mut self, id: u32) -> Result<(), HotkeyError>;
}

/// What the hotkey thread holds, and the rules for changing it.
#[derive(Debug)]
pub struct HotkeyTable<R> {
    raw: R,
    registered: HashMap<u32, Chord>,
}

impl<R: RawHotkeys> HotkeyTable<R> {
    /// An empty table over `raw`.
    pub fn new(raw: R) -> Self {
        Self {
            raw,
            registered: HashMap::new(),
        }
    }

    /// The ids registered now, in no order.
    #[must_use]
    pub fn ids(&self) -> Vec<u32> {
        self.registered.keys().copied().collect()
    }

    /// The id (other than `id`) that holds `chord`, if any.
    fn held_by_other(&self, id: u32, chord: Chord) -> Option<u32> {
        self.registered
            .iter()
            .filter(|(other, held)| **other != id && **held == chord)
            .map(|(other, _)| *other)
            .min()
    }

    /// Registers `chord` under `id`, replacing the chord already under `id`. On failure the
    /// previous chord is back in place.
    ///
    /// # Errors
    ///
    /// [`HotkeyError::Invalid`] for an invalid chord or id, [`HotkeyError::OwnAction`] when
    /// another id of this table holds the chord (nothing is changed, Windows is not asked), and
    /// whatever the OS says otherwise.
    pub fn register(&mut self, id: u32, chord: Chord) -> Result<(), HotkeyError> {
        if id > MAX_ID || chord.validate().is_err() {
            return Err(HotkeyError::Invalid);
        }
        if let Some(other) = self.held_by_other(id, chord) {
            return Err(HotkeyError::OwnAction(other));
        }
        let previous = self.registered.remove(&id);
        if previous.is_some() {
            // Free the id (and the old chord) first: the same chord again must succeed.
            self.raw.unregister(id)?;
        }
        match self.raw.register(id, chord) {
            Ok(()) => {
                self.registered.insert(id, chord);
                Ok(())
            }
            Err(error) => {
                if let Some(old) = previous {
                    // Put the working hotkey back; if even that fails it is simply gone.
                    if self.raw.register(id, old).is_ok() {
                        self.registered.insert(id, old);
                    }
                }
                Err(error)
            }
        }
    }

    /// Removes `id`. An id that is not registered succeeds.
    ///
    /// # Errors
    ///
    /// The OS refused.
    pub fn unregister(&mut self, id: u32) -> Result<(), HotkeyError> {
        if self.registered.remove(&id).is_none() {
            return Ok(());
        }
        self.raw.unregister(id)
    }

    /// Whether `chord` could be registered right now: registers it under [`PROBE_ID`] and takes
    /// it away again. Nothing else is touched.
    ///
    /// # Errors
    ///
    /// As [`register`](Self::register).
    pub fn probe(&mut self, chord: Chord) -> Result<(), HotkeyError> {
        if chord.validate().is_err() {
            return Err(HotkeyError::Invalid);
        }
        if let Some(other) = self.held_by_other(PROBE_ID, chord) {
            return Err(HotkeyError::OwnAction(other));
        }
        self.raw.register(PROBE_ID, chord)?;
        self.raw.unregister(PROBE_ID)
    }

    /// Removes every hotkey (the thread is ending); failures are not actionable.
    pub fn clear(&mut self) {
        for id in std::mem::take(&mut self.registered).into_keys() {
            let _ = self.raw.unregister(id);
        }
    }
}

const PENDING: u8 = 0;
const STARTED: u8 = 1;
const CANCELLED: u8 = 2;

/// A command's claim ticket: the caller and the thread race for it.
#[derive(Debug, Default)]
pub struct Ticket(AtomicU8);

impl Ticket {
    /// The thread is about to run the command. `false` means the caller gave up: skip it.
    pub fn start(&self) -> bool {
        self.0
            .compare_exchange(PENDING, STARTED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// The caller is giving up. `true` means the command will never run, so the caller may report
    /// failure; `false` means the thread already started it and its answer is still coming.
    pub fn cancel(&self) -> bool {
        self.0
            .compare_exchange(PENDING, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

/// Waits for the answer to a command the thread may not run: `timeout` for it to come; then the
/// ticket is cancelled. If that works the command will never run and `None` is returned (the
/// caller may report failure with a clear conscience); if the thread had already started it,
/// waits `grace` more for the answer that is on its way.
pub fn await_reply<T>(
    reply: &std::sync::mpsc::Receiver<T>,
    ticket: &Ticket,
    timeout: std::time::Duration,
    grace: std::time::Duration,
) -> Option<T> {
    if let Ok(answer) = reply.recv_timeout(timeout) {
        return Some(answer);
    }
    if ticket.cancel() {
        return None;
    }
    reply.recv_timeout(grace).ok()
}

/// Polls `done` up to `max_polls` times, calling `pause` between polls; `true` if it became true.
/// A thread join waits with this: `done` is `is_finished`, `pause` a short sleep.
pub fn wait_until(mut done: impl FnMut() -> bool, mut pause: impl FnMut(), max_polls: u32) -> bool {
    for _ in 0..max_polls {
        if done() {
            return true;
        }
        pause();
    }
    done()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Another program's hotkeys plus a log of what the table asked the OS.
    #[derive(Debug, Default)]
    struct FakeOs {
        /// Chords another program owns.
        foreign: Vec<Chord>,
        /// What is registered, by id.
        held: HashMap<u32, Chord>,
        log: Vec<String>,
        /// Refuse every registration with this code.
        refuse: Option<u32>,
    }

    impl RawHotkeys for FakeOs {
        fn register(&mut self, id: u32, chord: Chord) -> Result<(), HotkeyError> {
            self.log.push(format!("register {id} {chord}"));
            if let Some(code) = self.refuse {
                return Err(HotkeyError::Os(code));
            }
            if self.foreign.contains(&chord) || self.held.values().any(|c| *c == chord) {
                return Err(HotkeyError::InUse);
            }
            self.held.insert(id, chord);
            Ok(())
        }

        fn unregister(&mut self, id: u32) -> Result<(), HotkeyError> {
            self.log.push(format!("unregister {id}"));
            self.held.remove(&id);
            Ok(())
        }
    }

    fn chord(s: &str) -> Chord {
        s.parse().unwrap()
    }

    fn table() -> HotkeyTable<FakeOs> {
        HotkeyTable::new(FakeOs::default())
    }

    #[test]
    fn registers_replaces_and_unregisters() {
        let mut t = table();
        assert_eq!(t.register(1, chord("Ctrl+Alt+K")), Ok(()));
        assert_eq!(t.register(1, chord("Ctrl+Alt+J")), Ok(()));
        assert_eq!(
            t.register(1, chord("Ctrl+Alt+J")),
            Ok(()),
            "the same chord again"
        );
        assert_eq!(t.raw.held.get(&1), Some(&chord("Ctrl+Alt+J")));
        assert_eq!(t.unregister(1), Ok(()));
        assert_eq!(t.unregister(1), Ok(()), "twice is fine");
        assert!(t.raw.held.is_empty());
    }

    #[test]
    fn a_chord_another_program_owns_is_in_use_and_the_old_chord_stays() {
        let mut t = table();
        t.raw.foreign.push(chord("Ctrl+Alt+J"));
        assert_eq!(t.register(1, chord("Ctrl+Alt+K")), Ok(()));
        assert_eq!(t.register(1, chord("Ctrl+Alt+J")), Err(HotkeyError::InUse));
        assert_eq!(t.raw.held.get(&1), Some(&chord("Ctrl+Alt+K")));
        assert_eq!(t.ids(), [1]);
    }

    #[test]
    fn the_services_own_other_action_is_told_apart_and_windows_is_not_asked() {
        let mut t = table();
        assert_eq!(t.register(1, chord("Ctrl+Alt+K")), Ok(()));
        let asked = t.raw.log.len();
        assert_eq!(
            t.register(2, chord("Ctrl+Alt+K")),
            Err(HotkeyError::OwnAction(1))
        );
        assert_eq!(t.raw.log.len(), asked, "refused before any OS call");
        // The id that holds the chord may register it again.
        assert_eq!(t.register(1, chord("Ctrl+Alt+K")), Ok(()));
    }

    #[test]
    fn an_other_os_error_keeps_its_code() {
        let mut t = table();
        t.raw.refuse = Some(5);
        assert_eq!(t.register(1, chord("Ctrl+Alt+K")), Err(HotkeyError::Os(5)));
        assert_eq!(
            HotkeyError::Os(5).to_string(),
            "Windows refused the shortcut (Windows error 5)"
        );
    }

    #[test]
    fn invalid_ids_and_chords_are_refused_without_calling_windows() {
        let mut t = table();
        assert_eq!(
            t.register(MAX_ID + 1, chord("Ctrl+Alt+K")),
            Err(HotkeyError::Invalid)
        );
        assert_eq!(
            t.register(PROBE_ID, chord("Ctrl+Alt+K")),
            Err(HotkeyError::Invalid)
        );
        let paste = Chord {
            modifiers: crate::keys::Modifiers {
                ctrl: true,
                ..Default::default()
            },
            key: crate::chord::ChordKey::Letter(b'V'),
        };
        assert_eq!(t.register(1, paste), Err(HotkeyError::Invalid));
        assert_eq!(t.raw.log, Vec::<String>::new());
    }

    #[test]
    fn swapping_two_chords_works_when_the_changed_ids_are_freed_first() {
        let (a, b) = (chord("Ctrl+Alt+K"), chord("Ctrl+Alt+J"));
        let mut t = table();
        assert_eq!(t.register(1, a), Ok(()));
        assert_eq!(t.register(2, b), Ok(()));
        // Without freeing first, each id would meet the other's chord.
        assert_eq!(t.register(1, b), Err(HotkeyError::OwnAction(2)));
        assert_eq!(t.unregister(1), Ok(()));
        assert_eq!(t.unregister(2), Ok(()));
        assert_eq!(t.register(1, b), Ok(()));
        assert_eq!(t.register(2, a), Ok(()));
        assert_eq!(t.raw.held.get(&1), Some(&b));
        assert_eq!(t.raw.held.get(&2), Some(&a));
    }

    #[test]
    fn a_probe_registers_and_removes_without_touching_the_table() {
        let mut t = table();
        t.raw.foreign.push(chord("Ctrl+Alt+J"));
        assert_eq!(t.register(1, chord("Ctrl+Alt+K")), Ok(()));
        assert_eq!(t.probe(chord("Ctrl+Alt+H")), Ok(()));
        assert_eq!(t.probe(chord("Ctrl+Alt+J")), Err(HotkeyError::InUse));
        assert_eq!(t.probe(chord("Ctrl+Alt+K")), Err(HotkeyError::OwnAction(1)));
        assert_eq!(t.ids(), [1]);
        assert_eq!(t.raw.held.len(), 1, "the probe left nothing behind");
        assert!(
            t.raw
                .log
                .contains(&format!("register {PROBE_ID} Ctrl+Alt+H"))
        );
    }

    #[test]
    fn clear_removes_every_hotkey() {
        let mut t = table();
        assert_eq!(t.register(1, chord("Ctrl+Alt+K")), Ok(()));
        assert_eq!(t.register(2, chord("Ctrl+Alt+J")), Ok(()));
        t.clear();
        assert!(t.raw.held.is_empty());
        assert_eq!(t.ids().len(), 0);
    }

    #[test]
    fn a_cancelled_ticket_is_skipped_and_a_started_one_cannot_be_cancelled() {
        let late = Ticket::default();
        assert!(
            late.cancel(),
            "nothing started: the caller may report failure"
        );
        assert!(!late.start(), "the thread skips it");
        assert!(!late.cancel());

        let running = Ticket::default();
        assert!(running.start());
        assert!(!running.cancel(), "already running: the answer is coming");
    }

    #[test]
    fn a_caller_that_gives_up_before_the_thread_starts_cancels_the_command() {
        use std::sync::mpsc;
        use std::time::Duration;
        let (tx, rx) = mpsc::channel::<u8>();
        let ticket = Ticket::default();
        let none = Duration::ZERO;
        assert_eq!(await_reply(&rx, &ticket, none, none), None);
        assert!(!ticket.start(), "the late thread skips the command");
        drop(tx);
    }

    #[test]
    fn a_command_already_started_is_waited_for_not_abandoned() {
        use std::sync::mpsc;
        use std::time::Duration;
        let ticket = Ticket::default();
        assert!(ticket.start());
        let (tx, rx) = mpsc::channel::<u8>();
        tx.send(7).unwrap();
        let wait = Duration::from_millis(50);
        assert_eq!(await_reply(&rx, &ticket, Duration::ZERO, wait), Some(7));
        // Started but never answered: gives up after the grace.
        let (_tx, rx) = mpsc::channel::<u8>();
        assert_eq!(
            await_reply(&rx, &ticket, Duration::ZERO, Duration::ZERO),
            None
        );
    }

    #[test]
    fn a_bounded_wait_gives_up_after_its_polls() {
        let mut polls = 0;
        let mut pauses = 0;
        assert!(!wait_until(
            || {
                polls += 1;
                false
            },
            || pauses += 1,
            5
        ));
        assert_eq!(pauses, 5);
        let mut left = 3;
        assert!(wait_until(
            || {
                left -= 1;
                left == 0
            },
            || {},
            10
        ));
    }
}
