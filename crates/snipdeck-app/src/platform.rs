//! The machine the app runs on: the real Windows services on Windows, the platform fakes
//! everywhere else (so `cargo run -p snipdeck-app -- --demo` works on a Linux dev box and the
//! crate compiles and is checked on both).
//!
//! The Windows half only wires `snipdeck_platform::windows` into the traits the controller
//! uses; it contains no Win32 calls and no `unsafe` of its own. Nothing in it has run on
//! Windows yet (see `docs/HANDOVER.md`).

use std::sync::Arc;

use snipdeck_platform::{
    clipboard_watch::{ClipboardChange, WatchGate},
    handover::Request,
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
            single_instance::{Acquired, Delivery, SingleInstance},
            system_look::{SystemLookWatcher, WinSystemLook},
            target::{PasswordProbe, TargetTracker},
            tray::WinTray,
            typing_hook::TypingHook,
            window_style,
        },
    };

    use std::sync::Arc;

    use super::{Callbacks, Host, Instance, Start, WatchGate};
    use crate::typing_worker::{Env, Out, TypingList, Update};
    use crate::{
        controller::Services,
        frame::{Monitor, Rect},
        host::{HotkeyBackend, HotkeyFailure, TargetSource, TypingControl, action_of},
        placement::PopupHost,
    };
    use snipdeck_core::{Candidate, Matcher};
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

        fn cursor(&self) -> Option<(i32, i32)> {
            screen::cursor_position()
        }

        fn primary_down(&self) -> Option<bool> {
            Some(screen::primary_button_down())
        }

        fn size_loop(&self) -> Option<bool> {
            // Asked on the UI thread, the Panel's own.
            Some(screen::in_size_loop(self.0.panel()))
        }

        fn monitor_at(&self, x: i32, y: i32) -> Option<Monitor> {
            monitor_under(x, y)
        }

        fn fullscreen_on(&self, monitor: &Monitor) -> bool {
            // Any point of the monitor names it: the middle of its work area.
            let area = monitor.work_area;
            let middle = |start: i32, len: u32| start.saturating_add_unsigned(len / 2);
            snipdeck_platform::windows::target::fullscreen_covers(
                middle(area.x, area.w),
                middle(area.y, area.h),
            )
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
            // Popups never get a taskbar button. False means Windows refused the subclass; the
            // window then activates on click.
            let _ = window_style::install_panel_style(hwnd, false);
        }

        fn activate(&self, window: &slint::Window) -> bool {
            crate::native_window::handle_of(window)
                .is_some_and(|hwnd| self.0.begin_text_entry_in(hwnd))
        }

        fn release(&self) {
            self.0.end_text_entry();
        }

        fn cursor(&self) -> Option<(i32, i32)> {
            screen::cursor_position()
        }

        fn monitor_at(&self, x: i32, y: i32) -> Option<Monitor> {
            monitor_under(x, y)
        }
    }

    /// The monitor under a physical point (the nearest one outside them all).
    fn monitor_under(x: i32, y: i32) -> Option<Monitor> {
        let facts = screen::monitor_at(x, y)?;
        let area = |r: snipdeck_platform::route::ScreenRect| Rect {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
        };
        Some(Monitor::on(
            &facts.device,
            area(facts.bounds),
            area(facts.work_area),
            facts.scale,
        ))
    }

    struct Hotkeys(HotkeyService);

    fn failure_of(e: HotkeyError) -> HotkeyFailure {
        match e {
            HotkeyError::InUse => HotkeyFailure::InUse,
            HotkeyError::Invalid => HotkeyFailure::Invalid,
            HotkeyError::OwnAction(_) => HotkeyFailure::OwnAction,
            HotkeyError::Os(code) => HotkeyFailure::Refused(code),
            HotkeyError::Stopped => HotkeyFailure::Unavailable,
        }
    }

    impl HotkeyBackend for Hotkeys {
        fn register(&mut self, id: u32, chord: Chord) -> Result<(), HotkeyFailure> {
            let result = self.0.register(id, chord);
            // Every result, with Windows' own reason (`InUse` is ERROR_HOTKEY_ALREADY_REGISTERED,
            // 1409; `Os` carries the GetLastError code), for the SNIPDECK_LOG / DIAG log.
            crate::trace!(
                "hotkey register id={id} ({:?}) chord={chord}: {result:?}",
                crate::host::action_of(id)
            );
            result.map_err(failure_of)
        }

        fn probe(&mut self, chord: Chord) -> Result<(), HotkeyFailure> {
            let result = self.0.probe(chord);
            crate::trace!("hotkey probe chord={chord}: {result:?}");
            result.map_err(failure_of)
        }

        fn unregister(&mut self, id: u32) -> Result<(), HotkeyFailure> {
            let result = self.0.unregister(id);
            crate::trace!(
                "hotkey unregister id={id} ({:?}): {result:?}",
                crate::host::action_of(id)
            );
            result.map_err(|_| HotkeyFailure::Unavailable)
        }
    }

    /// The typing hook and the worker that turns its keys into completed triggers. Nothing runs
    /// until [`TypingControl::set_enabled`] turns it on (UNVERIFIED on Windows).
    struct WinTyping {
        on_typed: Arc<dyn Fn(Out) + Send + Sync>,
        list: Option<(Matcher, Vec<Candidate>)>,
        running: Option<Running>,
        /// The generation of the running worker (see `typing_worker::next_generation`).
        generation: u64,
    }

    struct Running {
        hook: TypingHook,
        tab: Arc<std::sync::atomic::AtomicU64>,
        updates: std::sync::mpsc::Sender<Update>,
        worker: std::thread::JoinHandle<()>,
    }

    impl WinTyping {
        fn new(on_typed: Arc<dyn Fn(Out) + Send + Sync>) -> Self {
            Self {
                on_typed,
                list: None,
                running: None,
                generation: 0,
            }
        }

        fn start(&mut self) -> Result<Running, String> {
            let (hook, events) = TypingHook::start().map_err(|e| format!("typing hook: {e}"))?;
            let (updates, rx) = std::sync::mpsc::channel();
            if let Some((matcher, candidates)) = &self.list {
                let _ = updates.send(Update::List(TypingList {
                    matcher: matcher.clone(),
                    candidates: candidates.clone(),
                }));
            }
            let on_typed = Arc::clone(&self.on_typed);
            let generation = crate::typing_worker::next_generation();
            self.generation = generation;
            let tab = hook.tab_flag();
            let env = Env::new(
                hook.sent(),
                hook.lost(),
                Arc::clone(&tab),
                Arc::new(snipdeck_platform::windows::typing_hook::foreground_window),
                Arc::new(|| {
                    use snipdeck_platform::{
                        keys::{Key, KeyEvent},
                        pipeline::Keyboard,
                    };
                    let tab = |down| KeyEvent {
                        key: Key::Tab,
                        down,
                    };
                    // The hook ignores injected keys, so this Tab is not swallowed again.
                    let _ = snipdeck_platform::windows::keyboard::WinKeyboard
                        .send(&[tab(true), tab(false)]);
                }),
                generation,
            );
            let worker = std::thread::Builder::new()
                .name("snipdeck-typing-worker".into())
                .spawn(move || {
                    // Made on this thread: the probe blocks and must stay off the UI thread.
                    let probe = PasswordProbe::new();
                    crate::typing_worker::run(
                        &events,
                        &rx,
                        &env,
                        || probe.focus_is_password(),
                        |typed| on_typed(typed),
                    );
                })
                .map_err(|e| format!("typing worker: {e}"))?;
            Ok(Running {
                hook,
                tab,
                updates,
                worker,
            })
        }
    }

    impl TypingControl for WinTyping {
        #[allow(
            clippy::print_stderr,
            reason = "a hook that cannot be installed has no other place to say so"
        )]
        fn set_enabled(&mut self, on: bool) -> bool {
            if on == self.running.is_some() {
                return true;
            }
            if on {
                match self.start() {
                    Ok(running) => self.running = Some(running),
                    Err(e) => {
                        eprintln!("snipdeck: {e}");
                        return false;
                    }
                }
            } else if let Some(running) = self.running.take() {
                // Unhooking drops the event sender, which ends the worker. It is not joined: it
                // may be inside the password probe (which can block on the other app), and this
                // runs on the UI thread. A late result it still delivers meets the settings gate.
                running.hook.stop();
                drop(running.updates);
                drop(running.worker);
            }
            true
        }

        fn set_triggers(&mut self, matcher: Matcher, candidates: Vec<Candidate>) {
            if let Some(running) = &self.running {
                let _ = running.updates.send(Update::List(TypingList {
                    matcher: matcher.clone(),
                    candidates: candidates.clone(),
                }));
            }
            self.list = Some((matcher, candidates));
        }

        fn generation(&self) -> u64 {
            if self.running.is_some() {
                self.generation
            } else {
                0
            }
        }

        fn hint_up(&mut self, id: u64) {
            // Hints of a worker from before the last switch-off never arm the new hook.
            if crate::typing_worker::generation_of(id) != self.generation {
                return;
            }
            if let Some(running) = &self.running {
                let _ = running.tab.compare_exchange(
                    0,
                    id,
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                );
            }
        }

        fn hint_down(&mut self, id: u64) {
            if let Some(running) = &self.running {
                // Only this hint's own flag: a newer hint's stays.
                let _ = running.tab.compare_exchange(
                    id,
                    0,
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                );
            }
        }

        fn not_expanded(&mut self, fired: snipdeck_core::Fired) {
            if let Some(running) = &self.running {
                let _ = running.updates.send(Update::NotExpanded(fired));
            }
        }
    }

    pub(super) fn connect(
        callbacks: Callbacks,
        instance: Option<&Instance<'_>>,
    ) -> Result<Start, String> {
        let Callbacks {
            on_hotkey,
            on_request,
            on_quit,
            on_target_changed,
            on_tray,
            on_look_changed,
            on_session_lock,
            on_session_resume,
            on_clipboard_change,
            on_typed,
        } = callbacks;
        // Shortcut labels follow the keyboard layout (unverified on Windows).
        snipdeck_platform::windows::hotkey::show_layout_labels();
        // Before any window exists: the tray's window, menu and icon are made ahead of the UI
        // toolkit, which would otherwise be too late to make the process DPI aware.
        let _ = snipdeck_platform::windows::dpi::enable_per_monitor_awareness();
        let mut keep_alive: Vec<Box<dyn std::any::Any>> = Vec::new();
        if let Some(instance) = instance {
            let on_request =
                move |request, ack: snipdeck_platform::windows::single_instance::Ack| {
                    on_request(request, Box::new(move || ack.send()));
                };
            match SingleInstance::acquire(instance.id, instance.request, on_request, on_quit) {
                Ok(Acquired::First(guard)) => {
                    if let Some(why) = guard.marker_error() {
                        crate::trace!("installer marker mutex: {why}");
                    }
                    keep_alive.push(Box::new(guard));
                }
                Ok(Acquired::Second(Delivery::Delivered)) => return Ok(Start::AlreadyRunning),
                Ok(Acquired::Second(Delivery::Unresponsive)) => return Ok(Start::NotAnswering),
                Err(e) => return Err(format!("single-instance guard: {e}")),
            }
        }
        let clipboard = WinClipboard::new().map_err(|e| format!("clipboard service: {e}"))?;
        // The Recent inbox's listener (UNVERIFIED on Windows). It reads nothing until the
        // controller opens the gate, which it does only for an opted-in, unpaused inbox.
        let recent_gate = WatchGate::new();
        clipboard.watch(recent_gate.clone(), on_clipboard_change);
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
        // Recent records nothing without the listener: the host tells the user once.
        let clipboard_watch_failed = !clipboard.is_listening();
        if clipboard_watch_failed {
            warnings.push("clipboard listener not registered; Recent cannot record".to_owned());
        }
        let tray = match WinTray::start("Snipdeck", on_tray) {
            Ok(tray) => Some(Box::new(tray) as Box<dyn snipdeck_platform::tray::Tray>),
            Err(e) => {
                warnings.push(format!("tray icon unavailable: {e}"));
                None
            }
        };
        match SystemLookWatcher::start(on_look_changed, on_session_lock, on_session_resume) {
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
            recent_gate,
            typing: Box::new(WinTyping::new(Arc::from(on_typed))),
            warnings,
            clipboard_watch_failed,
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

    use super::{Callbacks, Host, Instance, Start, WatchGate};
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
    pub(super) fn connect(
        callbacks: Callbacks,
        _instance: Option<&Instance<'_>>,
    ) -> Result<Start, String> {
        let _ = (
            callbacks.on_hotkey,
            callbacks.on_request,
            callbacks.on_quit,
            callbacks.on_target_changed,
            callbacks.on_tray,
            callbacks.on_look_changed,
            callbacks.on_session_lock,
            callbacks.on_session_resume,
            callbacks.on_clipboard_change,
            callbacks.on_typed,
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
            recent_gate: WatchGate::new(),
            typing: Box::new(crate::host::FakeTyping::default()),
            warnings: Vec::new(),
            clipboard_watch_failed: false,
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

/// Lets a GUI-subsystem build print to the terminal that started it (`--help`, `--check-db`,
/// usage errors). Nothing happens off Windows, where the binary has a console already.
/// Unverified on Windows (Fable v1 sweep A1).
pub fn attach_parent_console() {
    #[cfg(windows)]
    snipdeck_platform::windows::console::attach_parent_console();
}

/// Limits where Windows looks for DLLs that are loaded at run time (`LoadLibrary` and later
/// loads) to the application folder and System32, so a library planted beside the exe is not
/// loaded then. Call first thing in `main`. Nothing happens off Windows.
///
/// This does not cover the DLLs the exe links statically: the loader resolves those before
/// `main` runs. That part is the `/DEPENDENTLOADFLAG:0x800` link argument that `build.rs` passes
/// for the MSVC target. Both are unverified on Windows.
pub fn harden_dll_search() {
    #[cfg(windows)]
    let _ = snipdeck_platform::windows::dll_search::restrict_dll_search();
}

/// Shows a start-up failure in a native message box: the release Windows build has no console
/// and no window yet, so this is the only way to tell the user. Nothing happens off Windows,
/// where the failure is on stderr. Unverified on Windows.
pub fn show_startup_failure(title: &str, text: &str) {
    #[cfg(windows)]
    snipdeck_platform::windows::message_box::show_error(title, text);
    #[cfg(not(windows))]
    let _ = (title, text);
}

/// `--quit`: asks the Snipdeck running for `id` (see [`instance_id`]) to quit, as the tray's Quit
/// does, and returns at once. Nothing is opened or created but the existing quit event; whether or
/// not one is running (or can be asked), this prints nothing and the caller exits 0. Does nothing
/// off Windows. Unverified on Windows.
pub fn ask_to_quit(id: &str) {
    #[cfg(windows)]
    let _ = snipdeck_platform::windows::single_instance::ask_to_quit(id);
    #[cfg(not(windows))]
    let _ = id;
}

/// The id of the instance that uses `data_folder`: what every kernel object name is built from
/// (`snipdeck_platform::instance_names`). The same user and folder always give the same id, so
/// launches meet; another folder (`--data-dir`) or another Windows user gives another one.
///
/// # Errors
///
/// A readable message when the user's SID cannot be read (Windows only).
pub fn instance_id(data_folder: &std::path::Path) -> Result<String, String> {
    #[cfg(windows)]
    let sid = snipdeck_platform::windows::single_instance::user_sid()
        .map_err(|e| format!("reading the user account: {e}"))?;
    #[cfg(not(windows))]
    let sid = "dev".to_owned();
    let key = snipdeck_platform::instance_names::normalize_dir_key(&canonical_for_key(data_folder));
    Ok(snipdeck_platform::instance_names::instance_id(&sid, &key))
}

/// `folder` spelled the one way the file system knows it: symlinks and `..` resolved. A folder that
/// does not exist yet (first run) is resolved through its nearest existing parent.
fn canonical_for_key(folder: &std::path::Path) -> String {
    let absolute = std::path::absolute(folder).unwrap_or_else(|_| folder.to_path_buf());
    let mut tail = Vec::new();
    let mut probe = absolute.as_path();
    loop {
        if let Ok(mut canonical) = std::fs::canonicalize(probe) {
            canonical.extend(tail.iter().rev());
            return canonical.to_string_lossy().into_owned();
        }
        match (probe.parent(), probe.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name.to_owned());
                probe = parent;
            }
            _ => return absolute.to_string_lossy().into_owned(),
        }
    }
}

/// Whether the app runs from an installed MSIX / Store package; never off Windows.
#[must_use]
pub fn is_packaged() -> bool {
    #[cfg(windows)]
    {
        snipdeck_platform::windows::package::is_packaged()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Whether Windows started this process as the package's startup task, which cannot pass
/// `--hidden`. The first call asks Windows (the answer exists only once), so `main` calls it
/// before anything else; later calls return the kept answer. Unverified on Windows.
#[must_use]
pub fn launched_by_startup_task() -> bool {
    #[cfg(windows)]
    {
        snipdeck_platform::windows::package::launched_by_startup_task()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// At start-up with "Start with Windows" on: points a `Run` entry left by a moved or older
/// install at this executable (a portable folder that was moved), with the arguments the saved
/// "When Windows starts" choice calls for. An entry the user removed stays removed; the packaged
/// build has no such entry. Failures are only logged. Unverified on Windows.
pub fn sync_autostart(setting_on: bool, starts_visible: bool) {
    #[cfg(windows)]
    {
        if !setting_on || snipdeck_platform::windows::package::is_packaged() {
            return;
        }
        match std::env::current_exe() {
            Ok(exe) => {
                let args = if starts_visible { "" } else { "--hidden" };
                match snipdeck_platform::windows::autostart::rewrite_if_stale(&exe, args, true) {
                    Ok(true) => crate::trace!("autostart: rewrote a stale Run entry"),
                    Ok(false) => {}
                    Err(error) => crate::trace!("autostart rewrite: {error}"),
                }
            }
            Err(error) => crate::trace!("autostart rewrite: {error}"),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (setting_on, starts_visible);
    }
}

/// Windows Hello on this machine; none elsewhere.
#[must_use]
pub fn hello() -> Option<std::sync::Arc<dyn snipdeck_platform::hello::HelloAuthenticator>> {
    #[cfg(windows)]
    {
        Some(std::sync::Arc::new(
            snipdeck_platform::windows::hello::WindowsHello,
        ))
    }
    #[cfg(not(windows))]
    {
        None
    }
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

/// What a launch is, for the single-instance check.
#[derive(Debug, Clone, Copy)]
pub struct Instance<'a> {
    /// The instance id ([`instance_id`]).
    pub id: &'a str,
    /// What this launch asks for; handed to the running instance if there is one.
    pub request: &'a Request,
}

/// Tells a waiting second launch that its request has been acted on (call once, on the UI thread).
pub type Ack = Box<dyn FnOnce() + Send>;

/// What the platform layer calls back into. The two `Send` ones run on service threads: hand the
/// event to the UI thread (`slint::invoke_from_event_loop`).
pub struct Callbacks {
    /// A global hotkey fired (service thread).
    pub on_hotkey: Box<dyn Fn(HotkeyAction) + Send>,
    /// A later launch handed this instance a request (service thread). Act on it on the UI thread,
    /// then call the [`Ack`]: the launch waits for it, and tells the user if it never comes.
    pub on_request: Box<dyn Fn(Request, Ack) + Send>,
    /// `snipdeck.exe --quit` asked this instance to quit (service thread); do what the tray's
    /// Quit does.
    pub on_quit: Box<dyn Fn() + Send>,
    /// The foreground window changed (UI thread).
    pub on_target_changed: Box<dyn Fn()>,
    /// The user used the tray icon (tray thread).
    pub on_tray: Box<dyn Fn(TrayEvent) + Send>,
    /// Windows' light / dark or high-contrast state may have changed (watcher thread); read
    /// [`system_look`] on the UI thread.
    pub on_look_changed: Box<dyn Fn() + Send>,
    /// Windows locked the session (Win+L) or is going to sleep (watcher thread): a library with a
    /// master password locks.
    pub on_session_lock: Box<dyn Fn() + Send>,
    /// The computer woke up, the session was unlocked or a console / remote connection came back
    /// (watcher thread): failed global hotkeys are tried again.
    pub on_session_resume: Box<dyn Fn() + Send>,
    /// The clipboard changed and the Recent inbox is listening (clipboard thread).
    pub on_clipboard_change: Box<dyn Fn(ClipboardChange) + Send>,
    /// A typed trigger completed, and whether the focused element is a password box (typing
    /// worker thread).
    pub on_typed: Box<dyn Fn(crate::typing_worker::Out) + Send + Sync>,
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
    /// Switches the clipboard listener of the Recent inbox on and off.
    pub recent_gate: WatchGate,
    /// Installs and removes the typing hook (typed-trigger expansion and suggestions).
    pub typing: Box<dyn crate::host::TypingControl>,
    /// Services that failed to start but are not essential: print them.
    pub warnings: Vec<String>,
    /// The clipboard listener could not be registered (Windows only): Recent records nothing,
    /// and a toast says so once.
    pub clipboard_watch_failed: bool,
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
    /// Another instance is running and has taken this launch's request: exit.
    AlreadyRunning,
    /// Another instance is running but did not answer (hung, or run as administrator): tell the
    /// user, then exit.
    NotAnswering,
}

/// Connects to the machine.
///
/// # Errors
///
/// A readable message when a platform service cannot start.
pub fn connect(callbacks: Callbacks, instance: Option<&Instance<'_>>) -> Result<Start, String> {
    imp::connect(callbacks, instance)
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn the_instance_follows_the_data_folder_however_it_is_spelled() {
        let dir = tempfile::tempdir().expect("temp dir");
        let folder = dir.path().join("Library");
        std::fs::create_dir(&folder).expect("folder");
        let plain = instance_id(&folder).expect("id");
        // `..` and a not-yet-created tail resolve to the same folder as the plain spelling.
        let roundabout = folder.join("..").join("Library");
        assert_eq!(instance_id(&roundabout).expect("id"), plain);
        // Another folder (`--data-dir`) is another instance.
        let other = dir.path().join("Other");
        assert_ne!(instance_id(&other).expect("id"), plain);
        // A folder not made yet (first run) has a stable id of its own.
        assert_eq!(
            instance_id(&other).expect("id"),
            instance_id(&other).expect("id")
        );
    }

    #[test]
    fn off_windows_the_dev_host_pastes_into_the_demo_target() {
        let callbacks = Callbacks {
            on_hotkey: Box::new(|_| {}),
            on_request: Box::new(|_, _| {}),
            on_quit: Box::new(|| {}),
            on_target_changed: Box::new(|| {}),
            on_tray: Box::new(|_| {}),
            on_look_changed: Box::new(|| {}),
            on_session_lock: Box::new(|| {}),
            on_session_resume: Box::new(|| {}),
            on_clipboard_change: Box::new(|_| {}),
            on_typed: Box::new(|_| {}),
        };
        let Ok(Start::Run(host)) = connect(callbacks, None) else {
            panic!("the dev host always starts")
        };
        assert_eq!(host.target.current(), Some(crate::demo::target()));
        assert_eq!(host.keep_alive.len(), 0);
        assert!(host.tray.is_none(), "no tray off Windows");
        assert_eq!(host.warnings, Vec::<String>::new());
        assert!(!system_look().read().dark);
        (host.attach_panel)(0, false);
    }
}
