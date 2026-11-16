//! Saving everything when Windows ends the session (sign-out, shutdown, restart, an installer's
//! Restart Manager).
//!
//! Windows tells the hidden tray window (`WM_ENDSESSION`, on the tray thread) and ends the process
//! soon after, without the event loop ever returning. The state worth saving lives on the UI thread,
//! so the tray thread hands one "flush now" job to it and waits, with a limit: a UI thread that is
//! stuck must not hold the shutdown up. The job is the same one the normal exit runs (`main.rs`), so
//! there is one list of what a clean end saves.

use std::{
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

/// What the flush job did on the UI thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobResult {
    /// Everything was saved.
    Saved,
    /// Part of it could not run now (the controller was borrowed by the code that was running when
    /// the job came up). Posting the job again runs it after that code returns.
    Skipped,
}

/// How a [`flush_until_saved`] call ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flushed {
    /// The job ran to its end and saved everything.
    Done,
    /// The job did not finish within the limit (the UI thread is busy or stuck).
    TimedOut,
    /// The job kept skipping until the limit passed.
    Skipped,
    /// The job could not be handed over (the event loop has already ended: the normal exit has
    /// saved, or is saving, everything).
    NotPosted,
}

/// Time for the retry loop, so a test can use a fixed one.
pub trait Clock {
    /// Time since the flush began.
    fn elapsed(&self) -> Duration;
    /// Waits between two tries.
    fn pause(&self, how_long: Duration);
}

/// The real clock.
pub struct SystemClock(Instant);

impl SystemClock {
    /// Starts counting now.
    #[must_use]
    pub fn start() -> Self {
        Self(Instant::now())
    }
}

impl Clock for SystemClock {
    fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }

    fn pause(&self, how_long: Duration) {
        std::thread::sleep(how_long);
    }
}

/// The gap between two tries after a skip.
pub const RETRY_PAUSE: Duration = Duration::from_millis(50);

/// Runs `job` on the UI thread through `post` (which returns whether it queued the job) and waits
/// until `limit` for it to save. A job that reports [`JobResult::Skipped`] is posted again after a
/// short pause (the code that held the borrow has returned by then), until the limit passes. Call
/// from any thread but the UI thread.
pub fn flush_until_saved(
    post: impl Fn(Box<dyn FnOnce() + Send>) -> bool,
    job: impl Fn() -> JobResult + Send + Sync + 'static,
    limit: Duration,
    clock: &dyn Clock,
) -> Flushed {
    let job = Arc::new(job);
    let mut tries = 0_u32;
    loop {
        tries += 1;
        let left = limit.saturating_sub(clock.elapsed());
        let (done, finished) = mpsc::channel::<JobResult>();
        let this = Arc::clone(&job);
        let queued = post(Box::new(move || {
            let result = this();
            // The waiter may have given up and gone: nobody to tell.
            let _ = done.send(result);
        }));
        if !queued {
            return Flushed::NotPosted;
        }
        match finished.recv_timeout(left) {
            Ok(JobResult::Saved) => return Flushed::Done,
            // A job that panicked drops `done` without sending: not finished either.
            Err(_) => return Flushed::TimedOut,
            Ok(JobResult::Skipped) => {
                crate::trace!("session end flush skipped (try {tries}); trying again");
                if clock.elapsed() >= limit {
                    return Flushed::Skipped;
                }
                clock.pause(RETRY_PAUSE);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    };

    use super::*;

    /// A stand-in UI thread: runs what it is given on its own thread.
    fn ui_thread(job: Box<dyn FnOnce() + Send>) -> bool {
        std::thread::spawn(job);
        true
    }

    /// A UI thread that runs the job at once, on the caller's.
    fn inline(job: Box<dyn FnOnce() + Send>) -> bool {
        job();
        true
    }

    /// A clock that only moves when it is asked to wait.
    struct FixedClock(Mutex<Duration>);

    impl FixedClock {
        fn new() -> Self {
            Self(Mutex::new(Duration::ZERO))
        }
    }

    impl Clock for FixedClock {
        fn elapsed(&self) -> Duration {
            *self.0.lock().expect("clock")
        }

        fn pause(&self, how_long: Duration) {
            *self.0.lock().expect("clock") += how_long;
        }
    }

    #[test]
    fn the_flush_runs_and_is_waited_for() {
        let ran = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&ran);
        let outcome = flush_until_saved(
            ui_thread,
            move || {
                std::thread::sleep(Duration::from_millis(50));
                flag.store(true, Ordering::SeqCst);
                JobResult::Saved
            },
            Duration::from_secs(5),
            &SystemClock::start(),
        );
        assert_eq!(outcome, Flushed::Done);
        assert!(ran.load(Ordering::SeqCst), "returned only after the job");
    }

    #[test]
    fn a_stuck_ui_thread_does_not_hold_the_session_end_up() {
        let (release, hold) = mpsc::channel::<()>();
        let hold = Mutex::new(hold);
        let outcome = flush_until_saved(
            ui_thread,
            move || {
                let _ = hold
                    .lock()
                    .expect("hold")
                    .recv_timeout(Duration::from_secs(5));
                JobResult::Saved
            },
            Duration::from_millis(50),
            &SystemClock::start(),
        );
        assert_eq!(outcome, Flushed::TimedOut);
        drop(release);
    }

    #[test]
    fn an_ended_event_loop_is_reported_and_runs_nothing() {
        let ran = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&ran);
        let outcome = flush_until_saved(
            |_job| false,
            move || {
                flag.store(true, Ordering::SeqCst);
                JobResult::Saved
            },
            Duration::from_secs(5),
            &FixedClock::new(),
        );
        assert_eq!(outcome, Flushed::NotPosted);
        assert!(!ran.load(Ordering::SeqCst));
    }

    #[test]
    fn a_job_that_panics_is_not_a_finished_flush() {
        let outcome = flush_until_saved(
            ui_thread,
            || panic!("flush failed"),
            Duration::from_secs(5),
            &SystemClock::start(),
        );
        assert_eq!(outcome, Flushed::TimedOut);
    }

    #[test]
    fn a_skipped_flush_is_tried_again_until_it_saves() {
        let tries = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&tries);
        let clock = FixedClock::new();
        let outcome = flush_until_saved(
            inline,
            move || {
                // The controller is borrowed for the first two tries.
                if counter.fetch_add(1, Ordering::SeqCst) < 2 {
                    JobResult::Skipped
                } else {
                    JobResult::Saved
                }
            },
            Duration::from_secs(3),
            &clock,
        );
        assert_eq!(outcome, Flushed::Done);
        assert_eq!(tries.load(Ordering::SeqCst), 3);
        assert_eq!(clock.elapsed(), RETRY_PAUSE * 2);
    }

    #[test]
    fn a_flush_that_always_skips_gives_up_at_the_limit() {
        let tries = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&tries);
        let clock = FixedClock::new();
        let outcome = flush_until_saved(
            inline,
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
                JobResult::Skipped
            },
            Duration::from_millis(500),
            &clock,
        );
        assert_eq!(outcome, Flushed::Skipped);
        // One try at 0 ms, then every 50 ms up to and including the limit.
        assert_eq!(tries.load(Ordering::SeqCst), 11);
        assert_eq!(clock.elapsed(), Duration::from_millis(500));
    }
}
