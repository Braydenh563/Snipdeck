//! The machine the app runs on: the real Windows services on Windows, the platform fakes
//! everywhere else (so `cargo run -p snipdeck-app -- --demo` works on a Linux dev box and the
//! crate compiles and is checked on both).
//!
//! The Windows half only wires `snipdeck_platform::windows` into the traits the controller
//! uses; it contains no Win32 calls and no `unsafe` of its own. Nothing in it has run on
//! Windows yet (see `docs/HANDOVER.md`).

use crate::{
    controller::Services,
    host::{HotkeyBackend, TargetSource},
    settings::HotkeyAction,
};

#[cfg(windows)]
mod imp {
    use std::rc::Rc;

    use snipdeck_platform::{
        pipeline::SystemTiming,
        route::Target,
        windows::{
            clipboard::WinClipboard,
            hotkey::{HotkeyError, HotkeyService},
            keyboard::WinKeyboard,
            single_instance::{Acquired, SingleInstance},
            target::TargetTracker,
            window_style,
        },
    };

    use super::{Callbacks, Host, Start};
    use crate::{
        controller::Services,
        host::{HotkeyBackend, HotkeyFailure, TargetSource, action_of},
    };
    use snipdeck_platform::chord::Chord;

    /// The real clipboard.
    pub type Clipboard = WinClipboard;
    /// The real keyboard.
    pub type Keyboard = WinKeyboard;
    /// The system clock for the paste sequence.
    pub type Timing = SystemTiming;

    /// The Target tracker as a [`TargetSource`].
    struct Tracker(Rc<TargetTracker>);

    impl TargetSource for Tracker {
        fn current(&self) -> Option<Target> {
            self.0.current()
        }

        fn refocus(&self) -> bool {
            self.0.refocus_target()
        }
    }

    struct Hotkeys(HotkeyService);

    impl HotkeyBackend for Hotkeys {
        fn register(&mut self, id: u32, chord: Chord) -> Result<(), HotkeyFailure> {
            self.0.register(id, chord).map_err(|e| match e {
                HotkeyError::InUse => HotkeyFailure::InUse,
                HotkeyError::Invalid => HotkeyFailure::Invalid,
                HotkeyError::Os(_) | HotkeyError::Stopped => HotkeyFailure::Unavailable,
            })
        }
    }

    pub(super) fn connect(callbacks: Callbacks, single_instance: bool) -> Result<Start, String> {
        let Callbacks {
            on_hotkey,
            on_second,
            on_target_changed,
        } = callbacks;
        let guard = if single_instance {
            match SingleInstance::acquire(super::APP_ID, on_second) {
                Ok(Acquired::First(guard)) => Some(Box::new(guard) as Box<dyn std::any::Any>),
                Ok(Acquired::Second) => return Ok(Start::AlreadyRunning),
                Err(e) => return Err(format!("single-instance guard: {e}")),
            }
        } else {
            None
        };
        let clipboard = WinClipboard::new().map_err(|e| format!("clipboard service: {e}"))?;
        let hotkeys = HotkeyService::start(move |id| {
            if let Some(action) = action_of(id) {
                on_hotkey(action);
            }
        })
        .map_err(|e| format!("hotkey service: {e}"))?;
        let tracker = Rc::new(TargetTracker::install(on_target_changed));
        let panel_tracker = Rc::clone(&tracker);
        Ok(Start::Run(Box::new(Host {
            services: Services {
                clipboard,
                keyboard: WinKeyboard::new(),
                timing: SystemTiming::new(),
            },
            target: Box::new(Tracker(tracker)),
            hotkeys: Box::new(Hotkeys(hotkeys)),
            attach_panel: Box::new(move |hwnd| {
                panel_tracker.set_panel(hwnd);
                // False means Windows refused the subclass; the panel then activates on click.
                let _ = window_style::install_panel_style(hwnd);
            }),
            guard,
        })))
    }
}

#[cfg(not(windows))]
mod imp {
    use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};

    use super::{Callbacks, Host, Start};
    use crate::{
        controller::Services,
        host::{FakeHotkeys, FakeTargets},
    };

    /// A clipboard that records what it is given.
    pub type Clipboard = FakeClipboard;
    /// A keyboard that records what it is sent.
    pub type Keyboard = FakeKeyboard;
    /// A clock that only moves when the paste sequence sleeps.
    pub type Timing = FakeTiming;

    #[allow(
        clippy::unnecessary_wraps,
        reason = "same signature as the Windows backend, whose services can fail to start"
    )]
    pub(super) fn connect(callbacks: Callbacks, _single_instance: bool) -> Result<Start, String> {
        let _ = (
            callbacks.on_hotkey,
            callbacks.on_second,
            callbacks.on_target_changed,
        );
        Ok(Start::Run(Box::new(Host {
            services: Services {
                clipboard: FakeClipboard {
                    asks_after_polls: Some(1),
                    ..FakeClipboard::default()
                },
                keyboard: FakeKeyboard::default(),
                timing: FakeTiming::default(),
            },
            target: Box::new(FakeTargets::new(Some(crate::demo::target()))),
            hotkeys: Box::new(FakeHotkeys::default()),
            attach_panel: Box::new(|_| {}),
            guard: None,
        })))
    }
}

pub use imp::{Clipboard, Keyboard, Timing};

/// The single-instance name (`Local\Snipdeck-<id>`, see `snipdeck_platform::instance_names`).
pub const APP_ID: &str = "main";

/// What the platform layer calls back into. The two `Send` ones run on service threads: hand the
/// event to the UI thread (`slint::invoke_from_event_loop`).
pub struct Callbacks {
    /// A global hotkey fired (service thread).
    pub on_hotkey: Box<dyn Fn(HotkeyAction) + Send>,
    /// A second launch asked this instance to show itself (service thread).
    pub on_second: Box<dyn Fn() + Send>,
    /// The foreground window changed (UI thread).
    pub on_target_changed: Box<dyn Fn()>,
}

/// The platform services the app runs on.
pub struct Host {
    /// Clipboard, keyboard and clock for the paste sequence.
    pub services: Services<Clipboard, Keyboard, Timing>,
    /// Which window a paste would go into.
    pub target: Box<dyn TargetSource>,
    /// Registers the global hotkeys.
    pub hotkeys: Box<dyn HotkeyBackend>,
    /// Call with the Panel's native window handle once the window exists: the Target tracker
    /// then ignores it and it becomes a non-activating tool window.
    pub attach_panel: Box<dyn Fn(isize)>,
    /// Kept alive for the life of the process (the single-instance claim).
    pub guard: Option<Box<dyn std::any::Any>>,
}

impl std::fmt::Debug for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Host").finish_non_exhaustive()
    }
}

/// Whether this process is the first Snipdeck or has handed over to a running one.
#[derive(Debug)]
pub enum Start {
    /// Run the app on these services.
    Run(Box<Host>),
    /// Another instance is running and has been asked to show itself: exit.
    AlreadyRunning,
}

/// Connects to the machine.
///
/// # Errors
///
/// A readable message when a platform service cannot start.
pub fn connect(callbacks: Callbacks, single_instance: bool) -> Result<Start, String> {
    imp::connect(callbacks, single_instance)
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn off_windows_the_dev_host_pastes_into_the_demo_target() {
        let callbacks = Callbacks {
            on_hotkey: Box::new(|_| {}),
            on_second: Box::new(|| {}),
            on_target_changed: Box::new(|| {}),
        };
        let Ok(Start::Run(host)) = connect(callbacks, true) else {
            panic!("the dev host always starts")
        };
        assert_eq!(host.target.current(), Some(crate::demo::target()));
        assert!(host.guard.is_none());
        (host.attach_panel)(0);
    }
}
