//! Saving everything when Windows ends the session (sign-out, shutdown, restart, an installer's
//! Restart Manager).
//!
//! Windows tells the hidden tray window (`WM_ENDSESSION`, on the tray thread) and ends the process
//! soon after, without the event loop ever returning. The state worth saving lives on the UI thread,
//! so the tray thread hands one "flush now" job to it and waits, with a limit: a UI thread that is
//! stuck must not hold the shutdown up. The job is the same one the normal exit runs (`main.rs`), so
//! there is one list of what a clean end saves.

use std::{sync::mpsc, time::Duration};

/// How a [`flush_on_ui_thread`] call ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flushed {
    /// The job ran to its end.
    Done,
    /// The job did not finish within the limit (the UI thread is busy or stuck).
    TimedOut,
    /// The job could not be handed over (the event loop has already ended: the normal exit has
    /// saved, or is saving, everything).
    NotPosted,
}

/// Runs `job` on the UI thread through `post` (which returns whether it queued the job) and waits up
/// to `limit` for it to finish. Call from any thread but the UI thread.
pub fn flush_on_ui_thread(
    post: impl FnOnce(Box<dyn FnOnce() + Send>) -> bool,
    job: impl FnOnce() + Send + 'static,
    limit: Duration,
) -> Flushed {
    let (done, finished) = mpsc::channel::<()>();
    let queued = post(Box::new(move || {
        job();
        // The waiter may have given up and gone: nobody to tell.
        let _ = done.send(());
    }));
    if !queued {
        return Flushed::NotPosted;
    }
    match finished.recv_timeout(limit) {
        Ok(()) => Flushed::Done,
        // A job that panicked drops `done` without sending: not finished either.
        Err(_) => Flushed::TimedOut,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use super::*;

    /// A stand-in UI thread: runs what it is given on its own thread.
    fn ui_thread(job: Box<dyn FnOnce() + Send>) -> bool {
        std::thread::spawn(job);
        true
    }

    #[test]
    fn the_flush_runs_and_is_waited_for() {
        let ran = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&ran);
        let outcome = flush_on_ui_thread(
            ui_thread,
            move || {
                std::thread::sleep(Duration::from_millis(50));
                flag.store(true, Ordering::SeqCst);
            },
            Duration::from_secs(5),
        );
        assert_eq!(outcome, Flushed::Done);
        assert!(ran.load(Ordering::SeqCst), "returned only after the job");
    }

    #[test]
    fn a_stuck_ui_thread_does_not_hold_the_session_end_up() {
        let (release, hold) = mpsc::channel::<()>();
        let outcome = flush_on_ui_thread(
            ui_thread,
            move || {
                let _ = hold.recv_timeout(Duration::from_secs(5));
            },
            Duration::from_millis(50),
        );
        assert_eq!(outcome, Flushed::TimedOut);
        drop(release);
    }

    #[test]
    fn an_ended_event_loop_is_reported_and_runs_nothing() {
        let ran = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&ran);
        let outcome = flush_on_ui_thread(
            |_job| false,
            move || flag.store(true, Ordering::SeqCst),
            Duration::from_secs(5),
        );
        assert_eq!(outcome, Flushed::NotPosted);
        assert!(!ran.load(Ordering::SeqCst));
    }

    #[test]
    fn a_job_that_panics_is_not_a_finished_flush() {
        let outcome =
            flush_on_ui_thread(ui_thread, || panic!("flush failed"), Duration::from_secs(5));
        assert_eq!(outcome, Flushed::TimedOut);
    }
}
