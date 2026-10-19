//! Test doubles for the platform traits, shared with the app crate's tests (`testing` feature).

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crate::{
    error::Error as ServiceError,
    keys::{KeyEvent, Modifiers},
    payload::Payload,
    pipeline::{Clipboard, Error, Keyboard, Timing},
    system_look::{SystemLook, SystemLookSource},
    tray::{Gesture, MenuLine, Reaction, Tray, TrayEntry, TrayEvent, TrayLabels, TrayModel},
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

/// Locks, ignoring poisoning (a panicking test thread must not hide the real failure).
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct TrayState {
    model: TrayModel,
    /// Whether the icon is in the notification area, as the shell would have it.
    on_screen: bool,
    fail_show: bool,
    menus: Vec<Vec<MenuLine>>,
    log: Vec<String>,
}

type EventSink = Box<dyn Fn(TrayEvent) + Send>;

/// A tray icon that runs the real [`TrayModel`] and records what the backend would do. Cloning
/// shares the icon, so a test keeps a handle after handing the tray to the app.
#[derive(Clone)]
pub struct FakeTray {
    state: Arc<Mutex<TrayState>>,
    on_event: Arc<Mutex<EventSink>>,
}

impl std::fmt::Debug for FakeTray {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeTray").finish_non_exhaustive()
    }
}

impl FakeTray {
    /// A hidden icon; `on_event` hears what the user does to it.
    pub fn new(tooltip: &str, on_event: impl Fn(TrayEvent) + Send + 'static) -> Self {
        Self {
            state: Arc::new(Mutex::new(TrayState {
                model: TrayModel::new(tooltip),
                on_screen: false,
                fail_show: false,
                menus: Vec::new(),
                log: Vec::new(),
            })),
            on_event: Arc::new(Mutex::new(Box::new(on_event))),
        }
    }

    /// Makes adding the icon fail ([`Tray::show`] and the re-add in [`FakeTray::taskbar_created`]),
    /// as when Explorer is not running yet. The icon is still wanted: a later
    /// [`FakeTray::taskbar_created`] with this off adds it.
    pub fn set_fail_show(&self, fail: bool) {
        lock(&self.state).fail_show = fail;
    }

    /// Whether the icon is in the notification area now.
    #[must_use]
    pub fn visible(&self) -> bool {
        lock(&self.state).on_screen
    }

    /// The hover text.
    #[must_use]
    pub fn tooltip(&self) -> String {
        lock(&self.state).model.tooltip().to_owned()
    }

    /// Whether "Pause hotkeys" is checked.
    #[must_use]
    pub fn hotkeys_paused(&self) -> bool {
        lock(&self.state).model.hotkeys_paused()
    }

    /// The menus shown so far, oldest first.
    #[must_use]
    pub fn menus(&self) -> Vec<Vec<MenuLine>> {
        lock(&self.state).menus.clone()
    }

    /// What the backend did, in order (`show`, `hide`, `tooltip:<text>`, `paused:<bool>`,
    /// `labels:<open panel text>`, `readd`).
    #[must_use]
    pub fn log(&self) -> Vec<String> {
        lock(&self.state).log.clone()
    }

    fn emit(&self, event: TrayEvent) {
        (lock(&self.on_event))(event);
    }

    /// Records whether the icon is on screen and tells the app, once per change. Call without
    /// holding the state lock.
    fn set_on_screen(&self, on_screen: bool) {
        let changed = std::mem::replace(&mut lock(&self.state).on_screen, on_screen) != on_screen;
        if changed {
            self.emit(TrayEvent::IconPresent(on_screen));
        }
    }

    /// The user left-clicks the icon.
    pub fn left_click(&self) {
        let _ = self.gesture(Gesture::Select);
    }

    /// The user right-clicks the icon and the menu opens; returns it. Nothing is chosen until
    /// [`FakeTray::pick`].
    #[must_use]
    pub fn right_click(&self) -> Option<Vec<MenuLine>> {
        self.gesture(Gesture::ContextMenu)
    }

    fn gesture(&self, gesture: Gesture) -> Option<Vec<MenuLine>> {
        let reaction = {
            let mut state = lock(&self.state);
            if !state.on_screen {
                return None;
            }
            let reaction = state.model.gesture(gesture);
            if let Some(Reaction::PopupMenu(lines)) = &reaction {
                state.menus.push(lines.clone());
            }
            reaction
        };
        match reaction {
            Some(Reaction::Event(event)) => {
                self.emit(event);
                None
            }
            Some(Reaction::PopupMenu(lines)) => Some(lines),
            None => None,
        }
    }

    /// The user chooses `entry` in the open menu.
    pub fn pick(&self, entry: TrayEntry) {
        let event = lock(&self.state).model.pick(entry);
        self.emit(event);
    }

    /// Explorer restarted: the shell dropped every icon, and the backend adds this one again if
    /// it is wanted.
    ///
    /// The app hears `IconPresent(false)` for the dropped icon and `IconPresent(true)` once it is
    /// back (not when the re-add fails: see [`FakeTray::set_fail_show`]).
    pub fn taskbar_created(&self) {
        self.set_on_screen(false);
        let readd = {
            let mut state = lock(&self.state);
            let readd = state.model.on_taskbar_created() && !state.fail_show;
            if readd {
                state.log.push("readd".into());
            }
            readd
        };
        if readd {
            self.set_on_screen(true);
        }
    }
}

impl Tray for FakeTray {
    fn show(&mut self) -> Result<(), ServiceError> {
        {
            let mut state = lock(&self.state);
            state.model.show();
            state.log.push("show".into());
            if state.fail_show {
                return Err(ServiceError::Unavailable("the taskbar is not ready"));
            }
        }
        self.set_on_screen(true);
        Ok(())
    }

    fn hide(&mut self) -> Result<(), ServiceError> {
        {
            let mut state = lock(&self.state);
            state.model.hide();
            state.log.push("hide".into());
        }
        self.set_on_screen(false);
        Ok(())
    }

    fn set_tooltip(&mut self, text: &str) -> Result<(), ServiceError> {
        let mut state = lock(&self.state);
        state.model.set_tooltip(text);
        state.log.push(format!("tooltip:{text}"));
        Ok(())
    }

    fn set_hotkeys_paused(&mut self, paused: bool) -> Result<(), ServiceError> {
        let mut state = lock(&self.state);
        state.model.set_hotkeys_paused(paused);
        state.log.push(format!("paused:{paused}"));
        Ok(())
    }

    fn set_labels(&mut self, labels: TrayLabels) -> Result<(), ServiceError> {
        let mut state = lock(&self.state);
        state.log.push(format!("labels:{}", labels.open_panel));
        state.model.set_labels(labels);
        Ok(())
    }
}

/// A [`SystemLookSource`] a test sets. Cloning shares the value.
#[derive(Debug, Clone, Default)]
pub struct FakeSystemLook(Arc<Mutex<SystemLook>>);

impl FakeSystemLook {
    /// A source that reports `look`.
    #[must_use]
    pub fn new(look: SystemLook) -> Self {
        Self(Arc::new(Mutex::new(look)))
    }

    /// Changes what Windows "says".
    pub fn set(&self, look: SystemLook) {
        *lock(&self.0) = look;
    }
}

impl SystemLookSource for FakeSystemLook {
    fn read(&self) -> SystemLook {
        *lock(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    fn tray() -> (FakeTray, mpsc::Receiver<TrayEvent>) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let tray = FakeTray::new("Snipdeck", move |e| {
            let _ = lock(&tx).send(e);
        });
        (tray, rx)
    }

    #[test]
    fn a_hidden_tray_hears_nothing() {
        let (tray, rx) = tray();
        tray.left_click();
        assert!(tray.right_click().is_none());
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn clicks_and_menu_picks_reach_the_app_as_events() {
        let (mut tray, rx) = tray();
        tray.show().unwrap();
        assert!(tray.visible());
        assert_eq!(rx.try_recv(), Ok(TrayEvent::IconPresent(true)));
        tray.left_click();
        assert_eq!(rx.try_recv(), Ok(TrayEvent::TogglePanel));
        assert!(tray.right_click().is_some());
        assert_eq!(tray.menus().len(), 1);
        tray.pick(TrayEntry::PauseHotkeys);
        assert_eq!(rx.try_recv(), Ok(TrayEvent::HotkeysPaused(true)));
        assert!(tray.hotkeys_paused());
        tray.pick(TrayEntry::Quit);
        assert_eq!(rx.try_recv(), Ok(TrayEvent::Quit));
    }

    #[test]
    fn explorer_restarting_brings_back_a_wanted_icon_even_after_a_failed_show() {
        let (mut tray, _rx) = tray();
        tray.set_fail_show(true);
        assert!(tray.show().is_err());
        assert!(!tray.visible());
        tray.set_fail_show(false);
        tray.taskbar_created();
        assert!(tray.visible());
        tray.hide().unwrap();
        tray.taskbar_created();
        assert!(!tray.visible(), "a hidden icon stays hidden");
    }

    #[test]
    fn the_app_hears_when_the_icon_appears_and_goes_once_per_change() {
        let (mut tray, rx) = tray();
        tray.set_fail_show(true);
        assert!(tray.show().is_err());
        assert!(rx.try_recv().is_err(), "a failed add changes nothing");
        tray.set_fail_show(false);
        tray.taskbar_created();
        assert_eq!(rx.try_recv(), Ok(TrayEvent::IconPresent(true)));
        assert!(rx.try_recv().is_err());
        tray.show().unwrap();
        assert!(rx.try_recv().is_err(), "showing a shown icon is silent");
        tray.taskbar_created();
        assert_eq!(rx.try_recv(), Ok(TrayEvent::IconPresent(false)));
        assert_eq!(rx.try_recv(), Ok(TrayEvent::IconPresent(true)));
        tray.hide().unwrap();
        assert_eq!(rx.try_recv(), Ok(TrayEvent::IconPresent(false)));
        tray.hide().unwrap();
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn a_failed_re_add_leaves_the_icon_gone() {
        let (mut tray, rx) = tray();
        tray.show().unwrap();
        assert_eq!(rx.try_recv(), Ok(TrayEvent::IconPresent(true)));
        tray.set_fail_show(true);
        tray.taskbar_created();
        assert_eq!(rx.try_recv(), Ok(TrayEvent::IconPresent(false)));
        assert!(rx.try_recv().is_err());
        assert!(!tray.visible());
    }

    #[test]
    fn the_fake_look_follows_what_the_test_sets() {
        let look = FakeSystemLook::default();
        assert!(!look.read().dark);
        let handle = look.clone();
        handle.set(SystemLook {
            dark: true,
            high_contrast: None,
            reduce_motion: true,
        });
        assert!(look.read().dark);
        assert!(look.read().reduce_motion);
    }
}
