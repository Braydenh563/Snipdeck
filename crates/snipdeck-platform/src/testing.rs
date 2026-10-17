//! Test doubles for the platform traits, shared with the app crate's tests (`testing` feature).

use std::time::Duration;

use crate::{
    keys::{KeyEvent, Modifiers},
    payload::Payload,
    pipeline::{Clipboard, Error, Keyboard, Timing},
};

/// A clipboard that records what the pipeline does to it.
#[derive(Default)]
pub struct FakeClipboard {
    /// What was done, in order (`snapshot`, `transient:N`, `persistent:N`, `restore`).
    pub log: Vec<String>,
    /// Every payload written, in order, transient or persistent.
    pub written: Vec<Payload>,
    /// Poll count at which the target "asks"; `None` = never.
    pub asks_after_polls: Option<u32>,
    /// How many times the pipeline asked whether the target rendered.
    pub polls: u32,
    /// Make `snapshot` fail.
    pub fail_snapshot: bool,
    /// Make writes fail.
    pub fail_write: bool,
    /// What `restore` returns.
    pub restore_result: Option<Result<bool, Error>>,
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
        self.written.push(payload.clone());
        self.log.push(format!("transient:{}", payload.data().len()));
        Ok(())
    }
    fn write_persistent(&mut self, payload: &Payload) -> Result<(), Error> {
        if self.fail_write {
            return Err(Error::Clipboard("busy".into()));
        }
        self.written.push(payload.clone());
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

/// A keyboard that records injected events.
#[derive(Default)]
pub struct FakeKeyboard {
    /// The modifiers reported as held.
    pub held: Modifiers,
    /// Each injected batch.
    pub sent: Vec<Vec<KeyEvent>>,
    /// Make `send` fail.
    pub fail: bool,
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
/// A clock that only moves when the pipeline sleeps.
#[derive(Default)]
pub struct FakeTiming {
    /// Elapsed fake time.
    pub now: Duration,
    /// Each requested sleep.
    pub slept: Vec<Duration>,
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
