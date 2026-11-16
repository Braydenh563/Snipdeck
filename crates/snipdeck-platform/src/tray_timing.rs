//! The timing decisions behind the notification-area icon, portable so they are tested with fixed
//! millisecond counts (no real clock). `windows::tray` reads a tick count and arms a timer.
//!
//! - [`AddRetry`]: how often to try `NIM_ADD` again while the icon is wanted but the shell has
//!   not taken it (Explorer still starting at sign-in, or slow after a restart). One state
//!   machine serves the first add and the one after `TaskbarCreated`.
//! - [`KeySelectDebounce`]: one Enter press on the focused icon can arrive as `NIN_KEYSELECT`
//!   twice; a toggle would open and close the Panel.
//!
//! **Behaviour on Windows unverified** (`tools/windows-test/`, the tray steps).

/// The waits between attempts, in milliseconds; the last one repeats until [`MAX_ATTEMPTS`].
const BACKOFF_MS: [u64; 6] = [250, 500, 1_000, 2_000, 4_000, 5_000];

/// Attempts (the first included) before the retries stop. With [`BACKOFF_MS`] that covers about
/// two and a half minutes, longer than a cold sign-in; after that only the next `TaskbarCreated`
/// or [`crate::tray::Tray::show`] tries again.
pub const MAX_ATTEMPTS: u32 = 30;

/// Retries `NIM_ADD` with a back-off while the icon is wanted and not added.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AddRetry {
    /// Attempts that failed since the last [`reset`](Self::reset).
    failures: u32,
    /// When the next attempt is due, if one is waiting.
    due_at: Option<u64>,
}

impl AddRetry {
    /// A new attempt is due now: the icon was just asked for, or the shell announced itself
    /// (`TaskbarCreated`). Forgets earlier failures.
    pub fn restart(&mut self, now_ms: u64) {
        self.failures = 0;
        self.due_at = Some(now_ms);
    }

    /// Nothing more to try: the icon is in place or no longer wanted.
    pub fn reset(&mut self) {
        self.failures = 0;
        self.due_at = None;
    }

    /// Whether an attempt should be made at `now_ms`. True until [`failed`](Self::failed) or
    /// [`reset`](Self::reset) says otherwise, so a caller that attempts and does neither would
    /// attempt again at once.
    #[must_use]
    pub fn is_due(&self, now_ms: u64) -> bool {
        self.due_at.is_some_and(|due| now_ms >= due)
    }

    /// The attempt made at `now_ms` failed: schedules the next one, or gives up after
    /// [`MAX_ATTEMPTS`].
    pub fn failed(&mut self, now_ms: u64) {
        self.failures += 1;
        self.due_at = if self.failures >= MAX_ATTEMPTS {
            None
        } else {
            let step = usize::try_from(self.failures - 1).unwrap_or(usize::MAX);
            let wait = BACKOFF_MS[step.min(BACKOFF_MS.len() - 1)];
            Some(now_ms.saturating_add(wait))
        };
    }

    /// Milliseconds until the next attempt (0 if overdue), for arming a timer; `None` when no
    /// attempt is waiting.
    #[must_use]
    pub fn wait_ms(&self, now_ms: u64) -> Option<u64> {
        self.due_at.map(|due| due.saturating_sub(now_ms))
    }
}

/// How long after an accepted keyboard select another one is taken for its echo.
pub const KEY_SELECT_QUIET_MS: u64 = 400;

/// Drops the repeat of a keyboard select (`NIN_KEYSELECT`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeySelectDebounce {
    last: Option<u64>,
}

impl KeySelectDebounce {
    /// A keyboard select at `now_ms`: true if it should act, false if it repeats one that did
    /// within [`KEY_SELECT_QUIET_MS`]. A dropped repeat does not extend the quiet time.
    pub fn accept(&mut self, now_ms: u64) -> bool {
        if let Some(last) = self.last
            && now_ms.saturating_sub(last) < KEY_SELECT_QUIET_MS
        {
            return false;
        }
        self.last = Some(now_ms);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_due_until_asked() {
        let r = AddRetry::default();
        assert!(!r.is_due(1_000));
        assert_eq!(r.wait_ms(1_000), None);
    }

    #[test]
    fn a_restart_is_due_at_once_and_stays_due_until_it_is_answered() {
        let mut r = AddRetry::default();
        r.restart(1_000);
        assert!(r.is_due(1_000));
        assert_eq!(r.wait_ms(1_000), Some(0));
        assert!(r.is_due(1_000), "asking does not consume the attempt");
    }

    #[test]
    fn failures_back_off_then_hold_at_the_last_wait() {
        let mut r = AddRetry::default();
        r.restart(0);
        let mut now = 0;
        let mut waits = Vec::new();
        for _ in 0..8 {
            r.failed(now);
            let wait = r.wait_ms(now).unwrap();
            assert!(!r.is_due(now + wait - 1));
            assert!(r.is_due(now + wait));
            waits.push(wait);
            now += wait;
        }
        assert_eq!(waits, [250, 500, 1_000, 2_000, 4_000, 5_000, 5_000, 5_000]);
    }

    #[test]
    fn it_gives_up_after_the_attempt_limit_and_a_restart_begins_again() {
        let mut r = AddRetry::default();
        r.restart(0);
        for _ in 0..MAX_ATTEMPTS {
            assert!(r.wait_ms(0).is_some());
            r.failed(0);
        }
        assert_eq!(r.wait_ms(0), None, "stopped");
        assert!(!r.is_due(u64::MAX));
        r.restart(9_000);
        assert!(r.is_due(9_000));
        r.failed(9_000);
        assert_eq!(r.wait_ms(9_000), Some(250), "the back-off starts over");
    }

    #[test]
    fn success_or_hiding_resets() {
        let mut r = AddRetry::default();
        r.restart(0);
        r.failed(0);
        r.reset();
        assert_eq!(r.wait_ms(0), None);
        assert!(!r.is_due(10_000));
    }

    #[test]
    fn a_repeated_key_select_is_dropped_and_a_later_one_acts() {
        let mut d = KeySelectDebounce::default();
        assert!(d.accept(10_000));
        assert!(!d.accept(10_010), "the echo of the same press");
        assert!(
            !d.accept(10_399),
            "still inside the quiet time of the first"
        );
        assert!(d.accept(10_400), "a new press");
        assert!(!d.accept(10_450));
    }

    #[test]
    fn the_first_key_select_always_acts_even_at_time_zero() {
        let mut d = KeySelectDebounce::default();
        assert!(d.accept(0));
    }
}
