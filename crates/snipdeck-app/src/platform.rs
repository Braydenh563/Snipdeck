//! The machine the app runs on: the real Windows services on Windows, the platform fakes
//! everywhere else (so `cargo run -p snipdeck-app -- --demo` works on a Linux dev box and the
//! crate compiles and is checked on both).
//!
//! The Windows half only wires `snipdeck_platform::windows` into the traits the controller
//! uses; it contains no Win32 calls and no `unsafe` of its own. Nothing in it has run on
//! Windows yet (see `docs/HANDOVER.md`).

use std::sync::Arc;

use snipdeck_platform::{
    system_look::SystemLookSource,
    tray::{Tray, TrayEvent},
};

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
            screen,
            single_instance::{Acquired, SingleInstance},
            system_look::{SystemLookWatcher, WinSystemLook},
            target::TargetTracker,
            tray::WinTray,
            window_style,
        },
    };

    use std::sync::Arc;

    use super::{Callbacks, Host, Start};
    use crate::{
        controller::Services,
        frame::{Monitor, Rect},
        host::{HotkeyBackend, HotkeyFailure, TargetSource, action_of},
        placement::PopupHost,
    };
    use snipdeck_platform::{chord::Chord, system_look::SystemLookSource};

    pub(super) fn system_look() -> Arc<dyn SystemLookSource> {
        Arc::new(WinSystemLook)
    }

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

        fn caret(&self) -> Option<snipdeck_platform::route::ScreenRect> {
            self.0.caret()
        }
    }

    /// The Picker, the fill form and the toast window on the real machine.
    struct Popups(Rc<TargetTracker>);

    impl PopupHost for Popups {
        fn attach(&self, window: &slint::Window) {
            let Some(hwnd) = crate::native_window::handle_of(window) else {
                return;
            };
            self.0.register_popup(hwnd);
            // False means Windows refused the subclass; the window then activates on click.
            let _ = window_style::install_panel_style(hwnd);
        }

        fn activate(&self, window: &slint::Window) -> bool {
            crate::native_window::handle_of(window)
                .is_some_and(|hwnd| self.0.begin_text_entry_in(hwnd))
        }

        fn cursor(&self) -> Option<(i32, i32)> {
            screen::cursor_position()
        }

        fn monitor_at(&self, x: i32, y: i32) -> Option<Monitor> {
            let facts = screen::monitor_at(x, y)?;
            let area = |r: snipdeck_platform::route::ScreenRect| Rect {
                x: r.x,
                y: r.y,
                w: r.w,
                h: r.h,
            };
            Some(Monitor {
                id: Monitor::id_of("monitor", facts.bounds.w, facts.bounds.h),
                work_area: area(facts.work_area),
                scale: facts.scale,
            })
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

        fn unregister(&mut self, id: u32) -> Result<(), HotkeyFailure> {
            self.0
                .unregister(id)
                .map_err(|_| HotkeyFailure::Unavailable)
        }
    }

    pub(super) fn connect(callbacks: Callbacks, single_instance: bool) -> Result<Start, String> {
        let Callbacks {
            on_hotkey,
            on_second,
            on_target_changed,
            on_tray,
            on_look_changed,
        } = callbacks;
        // Before any window exists: the tray's window, menu and icon are made ahead of the UI
        // toolkit, which would otherwise be too late to make the process DPI aware.
        let _ = snipdeck_platform::windows::dpi::enable_per_monitor_awareness();
        let mut keep_alive: Vec<Box<dyn std::any::Any>> = Vec::new();
        if single_instance {
            match SingleInstance::acquire(super::APP_ID, on_second) {
                Ok(Acquired::First(guard)) => keep_alive.push(Box::new(guard)),
                Ok(Acquired::Second) => return Ok(Start::AlreadyRunning),
                Err(e) => return Err(format!("single-instance guard: {e}")),
            }
        }
        let clipboard = WinClipboard::new().map_err(|e| format!("clipboard service: {e}"))?;
        let hotkeys = HotkeyService::start(move |id| {
            if let Some(action) = action_of(id) {
                on_hotkey(action);
            }
        })
        .map_err(|e| format!("hotkey service: {e}"))?;
        let tracker = Rc::new(TargetTracker::install(on_target_changed));
        let panel_tracker = Rc::clone(&tracker);
        let popup_tracker = Rc::clone(&tracker);

        // The tray and the colour-mode watcher are conveniences: without them the app still
        // runs (closing the Panel then quits, and the theme is read once), so a failure to
        // start one is a warning, not an error.
        let mut warnings = Vec::new();
        let tray = match WinTray::start("Snipdeck", on_tray) {
            Ok(tray) => Some(Box::new(tray) as Box<dyn snipdeck_platform::tray::Tray>),
            Err(e) => {
                warnings.push(format!("tray icon unavailable: {e}"));
                None
            }
        };
        match SystemLookWatcher::start(on_look_changed) {
            Ok(watcher) => keep_alive.push(Box::new(watcher)),
            Err(e) => warnings.push(format!("not following the Windows colour mode: {e}")),
        }
        Ok(Start::Run(Box::new(Host {
            services: Services {
                clipboard,
                keyboard: WinKeyboard::new(),
                timing: SystemTiming::new(),
            },
            target: Box::new(Tracker(tracker)),
            hotkeys: Box::new(Hotkeys(hotkeys)),
            attach_panel: Box::new(move |hwnd, in_taskbar| {
                panel_tracker.set_panel(hwnd);
                // False means Windows refused the subclass; the panel then activates on click.
                let _ = window_style::install_panel_style(hwnd, in_taskbar);
            }),
            popups: Rc::new(Popups(popup_tracker)),
            tray,
            warnings,
            keep_alive,
        })))
    }
}

#[cfg(not(windows))]
mod imp {
    use std::sync::Arc;

    use snipdeck_platform::{
        system_look::SystemLookSource,
        testing::{FakeClipboard, FakeKeyboard, FakeSystemLook, FakeTiming},
    };

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

    pub(super) fn system_look() -> Arc<dyn SystemLookSource> {
        Arc::new(FakeSystemLook::default())
    }

    #[allow(
        clippy::unnecessary_wraps,
        reason = "same signature as the Windows backend, whose services can fail to start"
    )]
    pub(super) fn connect(callbacks: Callbacks, _single_instance: bool) -> Result<Start, String> {
        let _ = (
            callbacks.on_hotkey,
            callbacks.on_second,
            callbacks.on_target_changed,
            callbacks.on_tray,
            callbacks.on_look_changed,
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
            attach_panel: Box::new(|_, _| {}),
            popups: std::rc::Rc::new(crate::placement::NoPopups),
            // No tray off Windows: closing the Panel quits the dev build.
            tray: None,
            warnings: Vec::new(),
            keep_alive: Vec::new(),
        })))
    }
}

pub use imp::{Clipboard, Keyboard, Timing};

/// Windows' light / dark and high-contrast state (a fake that says "light" off Windows). Safe to
/// read from any thread; read it on the UI thread when [`Callbacks::on_look_changed`] fires.
#[must_use]
pub fn system_look() -> Arc<dyn SystemLookSource> {
    imp::system_look()
}

/// The file dialog of this machine: the Windows Open dialog; none elsewhere (the item editor then
/// offers only its path field).
#[must_use]
pub fn file_picker() -> Option<Box<dyn snipdeck_platform::file_dialog::FilePicker>> {
    #[cfg(windows)]
    {
        Some(Box::new(
            snipdeck_platform::windows::file_dialog::WinFilePicker,
        ))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

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
    /// The user used the tray icon (tray thread).
    pub on_tray: Box<dyn Fn(TrayEvent) + Send>,
    /// Windows' light / dark or high-contrast state may have changed (watcher thread); read
    /// [`system_look`] on the UI thread.
    pub on_look_changed: Box<dyn Fn() + Send>,
}

/// The platform services the app runs on.
pub struct Host {
    /// Clipboard, keyboard and clock for the paste sequence.
    pub services: Services<Clipboard, Keyboard, Timing>,
    /// Which window a paste would go into.
    pub target: Box<dyn TargetSource>,
    /// Registers the global hotkeys.
    pub hotkeys: Box<dyn HotkeyBackend>,
    /// Call with the Panel's native window handle once the window exists, and whether it has a
    /// taskbar button (the `panel_in_taskbar` setting): the Target tracker then ignores it and
    /// it becomes a non-activating window.
    pub attach_panel: Box<dyn Fn(isize, bool)>,
    /// What the Picker, the fill form and the toast window ask of the machine: their handles
    /// registered, the keyboard taken, the mouse and the monitors read.
    pub popups: std::rc::Rc<dyn crate::placement::PopupHost>,
    /// The notification-area icon, if one could be created (never off Windows). Not yet shown.
    pub tray: Option<Box<dyn Tray>>,
    /// Services that failed to start but are not essential: print them.
    pub warnings: Vec<String>,
    /// Kept alive for the life of the process (the single-instance claim, the colour-mode
    /// watcher).
    pub keep_alive: Vec<Box<dyn std::any::Any>>,
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
            on_tray: Box::new(|_| {}),
            on_look_changed: Box::new(|| {}),
        };
        let Ok(Start::Run(host)) = connect(callbacks, true) else {
            panic!("the dev host always starts")
        };
        assert_eq!(host.target.current(), Some(crate::demo::target()));
        assert!(host.keep_alive.is_empty());
        assert!(host.tray.is_none(), "no tray off Windows");
        assert_eq!(host.warnings, Vec::<String>::new());
        assert!(!system_look().read().dark);
        (host.attach_panel)(0, false);
    }
}
