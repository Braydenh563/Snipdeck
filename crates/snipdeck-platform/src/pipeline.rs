//! The paste sequence of SPEC §5.1a, written against traits so it runs under test.
//!
//! Paste = snapshot the clipboard, put the item on it with delayed rendering, inject the paste
//! key, wait until the Target has asked for the data, restore the snapshot. A fixed delay is not
//! used as the signal because slow Targets (Remote Desktop) would otherwise paste the restored,
//! old content (M0 S1). The wait is capped; a Target that never asks is reported as a failed paste
//! and the item stays on the clipboard.

use std::time::Duration;

use snipdeck_core::{PasteFailure, UseMode, UseOutcome};

use crate::keys::{KeyEvent, Modifiers, PasteKey, paste_sequence};
use crate::payload::Payload;
use crate::route::{CopyReason, Route};

/// How long to wait for the Target to ask for the clipboard data.
pub const RENDER_WAIT_CAP: Duration = Duration::from_secs(2);
/// How often to check whether the Target has asked.
pub const POLL_INTERVAL: Duration = Duration::from_millis(10);
/// After the first request, time for the Target to read its remaining formats before restore.
pub const SETTLE: Duration = Duration::from_millis(50);

/// The clipboard operations the pipeline needs.
pub trait Clipboard {
    /// A copy of what was on the clipboard before the paste.
    type Snapshot;
    /// Identifies the clipboard contents the pipeline wrote, so a restore can tell whether the
    /// user has copied something newer in the meantime.
    type Written;

    /// Captures the current contents. A failure only means the paste cannot be undone.
    ///
    /// # Errors
    ///
    /// [`Error::Clipboard`] if the clipboard cannot be opened or read.
    fn snapshot(&mut self) -> Result<Self::Snapshot, Error>;
    /// Writes `payload` with delayed rendering and the markers that keep the content out of
    /// clipboard history, clipboard managers and cloud sync.
    ///
    /// # Errors
    ///
    /// [`Error::Clipboard`] if the clipboard cannot be opened or written.
    fn write_transient(&mut self, payload: &Payload) -> Result<Self::Written, Error>;
    /// Writes `payload` for the user to keep: it may appear in clipboard history.
    ///
    /// # Errors
    ///
    /// [`Error::Clipboard`] if the clipboard cannot be opened or written.
    fn write_persistent(&mut self, payload: &Payload) -> Result<(), Error>;
    /// Whether the Target has asked for the data written as `written`.
    fn render_requested(&mut self, written: &Self::Written) -> bool;
    /// Puts `snapshot` back unless the clipboard changed since `written`; `false` if it changed.
    ///
    /// # Errors
    ///
    /// [`Error::Clipboard`] if the clipboard cannot be opened or written.
    fn restore(&mut self, snapshot: Self::Snapshot, written: Self::Written) -> Result<bool, Error>;
}

/// Keyboard state and injection.
pub trait Keyboard {
    /// The modifiers physically held now.
    fn held_modifiers(&mut self) -> Modifiers;
    /// Injects `events` in order.
    ///
    /// # Errors
    ///
    /// [`Error::Input`] if Windows accepted fewer events than were sent.
    fn send(&mut self, events: &[KeyEvent]) -> Result<(), Error>;
}

/// Time, injected so tests do not depend on the real clock.
pub trait Timing {
    /// A monotonic reading.
    fn now(&mut self) -> Duration;
    /// Blocks for `duration`.
    fn sleep(&mut self, duration: Duration);
}

/// [`Timing`] on the system clock.
#[derive(Debug)]
pub struct SystemTiming(std::time::Instant);

impl SystemTiming {
    /// A clock that reads zero now.
    #[must_use]
    pub fn new() -> Self {
        Self(std::time::Instant::now())
    }
}

impl Default for SystemTiming {
    fn default() -> Self {
        Self::new()
    }
}

impl Timing for SystemTiming {
    fn now(&mut self) -> Duration {
        self.0.elapsed()
    }
    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// A platform call failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The clipboard could not be opened or written (another app holds it).
    #[error("the clipboard is unavailable: {0}")]
    Clipboard(String),
    /// Windows refused the injected input.
    #[error("key injection failed: {0}")]
    Input(String),
}

/// The platform services a use needs.
pub struct Env<'a, C, K, T> {
    /// The clipboard.
    pub clipboard: &'a mut C,
    /// The keyboard.
    pub keyboard: &'a mut K,
    /// The clock.
    pub timing: &'a mut T,
}

/// Uses an item: pastes it into the Target, or copies it, as `mode` and `route` allow.
///
/// Returns the outcome for the toast.
///
/// # Errors
///
/// [`Error::Clipboard`] if not even a copy was possible.
pub fn use_item<C: Clipboard, K: Keyboard, T: Timing>(
    env: &mut Env<'_, C, K, T>,
    payload: &Payload,
    mode: UseMode,
    route: &Route,
) -> Result<UseOutcome, Error> {
    let payload = if mode == UseMode::PastePlain {
        payload.plain_text()
    } else {
        payload.clone()
    };
    match (mode, route) {
        (UseMode::Copy, _) => {
            env.clipboard.write_persistent(&payload)?;
            Ok(UseOutcome::Copied)
        }
        (_, Route::Copy(reason)) => {
            env.clipboard.write_persistent(&payload)?;
            Ok(copy_outcome(reason))
        }
        (_, Route::Paste { key }) => paste(env, &payload, *key),
    }
}

fn copy_outcome(reason: &CopyReason) -> UseOutcome {
    reason
        .failure()
        .map_or(UseOutcome::Copied, UseOutcome::CopiedInstead)
}

fn paste<C: Clipboard, K: Keyboard, T: Timing>(
    env: &mut Env<'_, C, K, T>,
    payload: &Payload,
    key: PasteKey,
) -> Result<UseOutcome, Error> {
    // A failed snapshot only costs the restore; the paste itself still works.
    let snapshot = env.clipboard.snapshot().ok();
    let written = env.clipboard.write_transient(payload)?;

    let events = paste_sequence(env.keyboard.held_modifiers(), key);
    if env.keyboard.send(&events).is_err() {
        // The item is still on the clipboard from the transient write. Writing it again would
        // overwrite anything the user copied meanwhile and drop the privacy markers.
        return Ok(UseOutcome::CopiedInstead(PasteFailure::TargetUnavailable));
    }

    let started = env.timing.now();
    let mut requested = false;
    while env.timing.now().saturating_sub(started) < RENDER_WAIT_CAP {
        if env.clipboard.render_requested(&written) {
            requested = true;
            break;
        }
        env.timing.sleep(POLL_INTERVAL);
    }
    if !requested {
        // The Target never asked: nothing was pasted. The item stays on the clipboard from the
        // transient write for the user to paste by hand (see above for why it is not rewritten).
        return Ok(UseOutcome::CopiedInstead(PasteFailure::TargetUnavailable));
    }
    env.timing.sleep(SETTLE);
    if let Some(snapshot) = snapshot {
        // A restore that fails or finds newer content is not worth failing the paste for.
        let _ = env.clipboard.restore(snapshot, written);
    }
    Ok(UseOutcome::Pasted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{Key, Modifier};
    use crate::payload::Data;
    use crate::route::CopyReason;

    #[derive(Default)]
    struct FakeClipboard {
        log: Vec<String>,
        /// Poll count at which the target "asks"; `None` = never.
        asks_after_polls: Option<u32>,
        polls: u32,
        fail_snapshot: bool,
        fail_write: bool,
        /// What `restore` returns.
        restore_result: Option<Result<bool, Error>>,
    }

    impl Clipboard for FakeClipboard {
        type Snapshot = ();
        type Written = ();
        fn snapshot(&mut self) -> Result<(), Error> {
            self.log.push("snapshot".into());
            if self.fail_snapshot {
                Err(Error::Clipboard("busy".into()))
            } else {
                Ok(())
            }
        }
        fn write_transient(&mut self, payload: &Payload) -> Result<(), Error> {
            if self.fail_write {
                return Err(Error::Clipboard("busy".into()));
            }
            self.log.push(format!("transient:{}", payload.data().len()));
            Ok(())
        }
        fn write_persistent(&mut self, payload: &Payload) -> Result<(), Error> {
            if self.fail_write {
                return Err(Error::Clipboard("busy".into()));
            }
            self.log
                .push(format!("persistent:{}", payload.data().len()));
            Ok(())
        }
        fn render_requested(&mut self, (): &()) -> bool {
            self.polls += 1;
            self.asks_after_polls.is_some_and(|n| self.polls > n)
        }
        fn restore(&mut self, (): (), (): ()) -> Result<bool, Error> {
            self.log.push("restore".into());
            self.restore_result.clone().unwrap_or(Ok(true))
        }
    }

    #[derive(Default)]
    struct FakeKeyboard {
        held: Modifiers,
        sent: Vec<Vec<KeyEvent>>,
        fail: bool,
    }

    impl Keyboard for FakeKeyboard {
        fn held_modifiers(&mut self) -> Modifiers {
            self.held
        }
        fn send(&mut self, events: &[KeyEvent]) -> Result<(), Error> {
            if self.fail {
                return Err(Error::Input("blocked".into()));
            }
            self.sent.push(events.to_vec());
            Ok(())
        }
    }

    /// A clock that only moves when the pipeline sleeps.
    #[derive(Default)]
    struct FakeTiming {
        now: Duration,
        slept: Vec<Duration>,
    }

    impl Timing for FakeTiming {
        fn now(&mut self) -> Duration {
            self.now
        }
        fn sleep(&mut self, duration: Duration) {
            self.now += duration;
            self.slept.push(duration);
        }
    }

    fn text_and_html() -> Payload {
        Payload::new(vec![
            Data::Html("<b>hello</b>".into()),
            Data::Text("hello".into()),
        ])
        .unwrap()
    }

    struct World {
        clipboard: FakeClipboard,
        keyboard: FakeKeyboard,
        timing: FakeTiming,
    }

    impl World {
        fn new() -> Self {
            Self {
                clipboard: FakeClipboard {
                    asks_after_polls: Some(3),
                    ..FakeClipboard::default()
                },
                keyboard: FakeKeyboard::default(),
                timing: FakeTiming::default(),
            }
        }
        fn run(&mut self, mode: UseMode, route: &Route) -> Result<UseOutcome, Error> {
            let mut env = Env {
                clipboard: &mut self.clipboard,
                keyboard: &mut self.keyboard,
                timing: &mut self.timing,
            };
            use_item(&mut env, &text_and_html(), mode, route)
        }
    }

    const PASTE: Route = Route::Paste {
        key: PasteKey::CtrlV,
    };

    #[test]
    fn paste_snapshots_writes_sends_waits_and_restores_in_order() {
        let mut w = World::new();
        assert_eq!(w.run(UseMode::Paste, &PASTE), Ok(UseOutcome::Pasted));
        assert_eq!(w.clipboard.log, ["snapshot", "transient:2", "restore"]);
        assert_eq!(w.keyboard.sent.len(), 1);
        assert_eq!(w.timing.slept.last(), Some(&SETTLE));
    }

    #[test]
    fn restore_waits_for_the_target_not_for_a_fixed_delay() {
        let mut slow = World::new();
        slow.clipboard.asks_after_polls = Some(100);
        slow.run(UseMode::Paste, &PASTE).unwrap();
        let mut fast = World::new();
        fast.clipboard.asks_after_polls = Some(0);
        fast.run(UseMode::Paste, &PASTE).unwrap();
        assert!(slow.timing.now > fast.timing.now);
    }

    #[test]
    fn a_target_that_never_asks_gets_the_item_kept_and_a_failure() {
        let mut w = World::new();
        w.clipboard.asks_after_polls = None;
        assert_eq!(
            w.run(UseMode::Paste, &PASTE),
            Ok(UseOutcome::CopiedInstead(PasteFailure::TargetUnavailable))
        );
        // The transient write stays: rewriting could clobber newer content and drop the markers.
        assert_eq!(w.clipboard.log, ["snapshot", "transient:2"]);
        assert!(w.timing.now >= RENDER_WAIT_CAP && w.timing.now < RENDER_WAIT_CAP + SETTLE);
    }

    #[test]
    fn a_failed_snapshot_still_pastes_but_does_not_restore() {
        let mut w = World::new();
        w.clipboard.fail_snapshot = true;
        assert_eq!(w.run(UseMode::Paste, &PASTE), Ok(UseOutcome::Pasted));
        assert!(!w.clipboard.log.contains(&"restore".to_string()));
    }

    #[test]
    fn blocked_input_copies_instead() {
        let mut w = World::new();
        w.keyboard.fail = true;
        assert_eq!(
            w.run(UseMode::Paste, &PASTE),
            Ok(UseOutcome::CopiedInstead(PasteFailure::TargetUnavailable))
        );
        assert_eq!(w.clipboard.log, ["snapshot", "transient:2"]);
    }

    #[test]
    fn a_refused_or_failed_restore_does_not_fail_the_paste() {
        for result in [Ok(false), Err(Error::Clipboard("busy".into()))] {
            let mut w = World::new();
            w.clipboard.restore_result = Some(result);
            assert_eq!(w.run(UseMode::Paste, &PASTE), Ok(UseOutcome::Pasted));
            assert_eq!(w.clipboard.log, ["snapshot", "transient:2", "restore"]);
        }
    }

    #[test]
    fn a_failed_transient_write_is_an_error_and_sends_no_keys() {
        let mut w = World::new();
        w.clipboard.fail_write = true;
        assert!(w.run(UseMode::Paste, &PASTE).is_err());
        assert_eq!(w.keyboard.sent, Vec::<Vec<KeyEvent>>::new());
    }

    #[test]
    fn copy_mode_never_injects_keys() {
        let mut w = World::new();
        assert_eq!(w.run(UseMode::Copy, &PASTE), Ok(UseOutcome::Copied));
        assert_eq!(w.keyboard.sent, Vec::<Vec<KeyEvent>>::new());
        assert_eq!(w.clipboard.log, ["persistent:2"]);
    }

    #[test]
    fn copy_routes_report_why() {
        let mut w = World::new();
        let elevated = Route::Copy(CopyReason::Elevated {
            app: "taskmgr.exe".into(),
        });
        assert_eq!(
            w.run(UseMode::Paste, &elevated),
            Ok(UseOutcome::CopiedInstead(PasteFailure::TargetElevated))
        );
        let none = Route::Copy(CopyReason::NoTarget);
        assert_eq!(w.run(UseMode::Paste, &none), Ok(UseOutcome::Copied));
        assert_eq!(w.keyboard.sent, Vec::<Vec<KeyEvent>>::new());
    }

    #[test]
    fn plain_paste_writes_only_the_text_format() {
        let mut w = World::new();
        w.run(UseMode::PastePlain, &PASTE).unwrap();
        assert_eq!(w.clipboard.log[1], "transient:1");
    }

    #[test]
    fn held_alt_is_masked_in_the_injected_sequence() {
        let mut w = World::new();
        w.keyboard.held = Modifiers {
            alt: true,
            ..Modifiers::default()
        };
        w.run(UseMode::PastePlain, &PASTE).unwrap();
        let events = &w.keyboard.sent[0];
        assert!(events.contains(&KeyEvent {
            key: Key::Mask,
            down: true
        }));
        assert!(events.contains(&KeyEvent {
            key: Key::Modifier(Modifier::Alt),
            down: false
        }));
    }

    #[test]
    fn an_unwritable_clipboard_is_an_error() {
        let mut w = World::new();
        w.clipboard.fail_write = true;
        assert!(w.run(UseMode::Paste, &PASTE).is_err());
        assert!(w.run(UseMode::Copy, &PASTE).is_err());
    }
}
