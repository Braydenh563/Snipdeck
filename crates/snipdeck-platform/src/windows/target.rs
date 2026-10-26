//! `TargetTracker`: which window the user was in before they clicked Snipdeck, and whether it can
//! take a paste.
//!
//! # Threading
//!
//! A tracker belongs to the UI thread, the one that owns the panel window and runs the message
//! loop. The foreground hook is `WINEVENT_OUTOFCONTEXT`, so Windows delivers its events as
//! messages to that thread: **they arrive only while the thread pumps messages**, which the Slint
//! event loop does. COM is initialised on that thread (apartment-threaded) for UI Automation.
//! Everything here must be called on that thread; the type is not `Send`.
//!
//! # Privacy
//!
//! Window titles can be sensitive. Nothing in this module logs them; the only place a title goes
//! is the returned [`Target`].
//!
//! # UI Automation cost
//!
//! `GetGUIThreadInfo` is tried first and is cheap. Only when it finds no focused window
//! (`OneNote`, sometimes Windows Terminal) does [`TargetTracker::current`] ask UI Automation,
//! which makes a cross-process call into the Target on the UI thread. That call is bounded: the
//! connection timeout is [`UIA_CONNECTION_TIMEOUT_MS`] and the per-call timeout is
//! [`UIA_TRANSACTION_TIMEOUT_MS`] (the system defaults are 2 s and 20 s), and one request fetches
//! every property the decision needs, so a hung Target freezes the panel for at most about the
//! transaction timeout, once per `current()` call. An elevated Target is never probed with UI
//! Automation: it is copy-only anyway.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use windows::Win32::Foundation::{HANDLE, HWND, POINT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL,
    TOKEN_QUERY, TokenIntegrityLevel,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation8, HWINEVENTHOOK, IUIAutomation2, IUIAutomationCacheRequest,
    IUIAutomationTextPattern, IUIAutomationValuePattern, SetWinEventHook,
    UIA_ControlTypePropertyId, UIA_IsEnabledPropertyId, UIA_IsKeyboardFocusablePropertyId,
    UIA_ProcessIdPropertyId, UIA_TextPatternId, UIA_ValueIsReadOnlyPropertyId, UIA_ValuePatternId,
    UnhookWinEvent,
};
use windows::Win32::UI::Shell::{
    QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_SYSTEM_FOREGROUND, GUITHREADINFO, GetClassNameW, GetDesktopWindow, GetForegroundWindow,
    GetGUIThreadInfo, GetShellWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindow,
    SetForegroundWindow, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};
use windows::core::{BOOL, Owned, PWSTR};

use crate::route::{Focus, ScreenRect, Target};
use crate::win_logic::{self, ElementFacts, ForegroundKind};

/// How long UI Automation may take to connect to the Target's process, in milliseconds.
pub const UIA_CONNECTION_TIMEOUT_MS: u32 = 250;
/// How long one UI Automation call into the Target may take, in milliseconds.
pub const UIA_TRANSACTION_TIMEOUT_MS: u32 = 500;

/// Name shown when the Target's executable cannot be read.
const UNKNOWN_APP: &str = "unknown app";
/// Longest window title kept, in UTF-16 units.
const TITLE_UNITS: usize = 512;

thread_local! {
    /// The tracker the foreground hook reports to. The hook callback has no user-data pointer,
    /// so it finds its state here. A `Cell` (taken and put back) rather than a `RefCell`, so the
    /// callback has no borrow that could panic.
    static CURRENT: Cell<Option<Rc<Inner>>> = const { Cell::new(None) };
}

/// Whether the panel is activated for text entry, and what the Target was before that.
enum Entry {
    Inactive,
    /// `None` inside: there was no Target.
    Active(Option<Target>),
}

/// What the hook callback and the tracker share.
struct Inner {
    /// Last foreign foreground window (`HWND` value; 0 for none).
    target: Cell<isize>,
    /// The panel window (0 until [`TargetTracker::set_panel`]).
    panel: Cell<isize>,
    /// The Picker, the fill form and the toast window (`HWND` values): never a Target either.
    popups: RefCell<Vec<isize>>,
    /// Target captured when the panel activated itself for text entry.
    entry: RefCell<Entry>,
    /// A UI Automation probe is running; foreground events are queued until it ends.
    probing: Cell<bool>,
    /// A foreground change arrived during a probe.
    change_pending: Cell<bool>,
    /// Integrity level of this process, read once.
    own_integrity: Option<u32>,
    on_change: Box<dyn Fn()>,
}

impl Inner {
    fn panel_hwnd(&self) -> HWND {
        hwnd_from(self.panel.get())
    }

    fn target_hwnd(&self) -> HWND {
        hwnd_from(self.target.get())
    }

    /// Whether `hwnd` belongs to this process or is the panel: never a Target.
    fn is_own(&self, hwnd: HWND) -> bool {
        if hwnd.0 as isize == self.panel.get() || self.is_popup(hwnd.0 as isize) {
            return true;
        }
        let mut pid = 0;
        // SAFETY: `pid` outlives the call; a stale `hwnd` makes the call return 0, not crash.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut pid)) };
        // SAFETY: no arguments, no preconditions.
        pid == unsafe { GetCurrentProcessId() }
    }

    /// Records `hwnd` as the Target if it is a foreign window ([`win_logic::next_target`]). Our
    /// own windows and the Alt+Tab switcher leave the Target alone, so a Panel reached with
    /// Alt+Tab still pastes into the app the user came from; a desktop or taskbar surface clears
    /// it (injected keys would go to it). Returns whether the Target changed.
    fn note_foreground(&self, hwnd: HWND) -> bool {
        if hwnd.is_invalid() {
            return false;
        }
        let kind = if self.is_own(hwnd) {
            ForegroundKind::Own
        } else if is_shell_surface(hwnd) {
            ForegroundKind::Shell
        } else if is_task_switcher(hwnd) {
            ForegroundKind::Switcher
        } else {
            ForegroundKind::Foreign
        };
        let new = win_logic::next_target(self.target.get(), hwnd.0 as isize, kind);
        let changed = self.target.replace(new) != new;
        if changed && let Ok(mut entry) = self.entry.try_borrow_mut() {
            *entry = Entry::Inactive;
        }
        changed
    }

    fn is_popup(&self, hwnd: isize) -> bool {
        self.popups.try_borrow().is_ok_and(|p| p.contains(&hwnd))
    }

    /// The panel or one of the popups holds the foreground.
    fn panel_is_foreground(&self) -> bool {
        // SAFETY: no arguments, no preconditions.
        let foreground = unsafe { GetForegroundWindow() };
        let panel = self.panel_hwnd();
        (!panel.is_invalid() && foreground == panel)
            || (!foreground.is_invalid() && self.is_popup(foreground.0 as isize))
    }

    fn notify(&self) {
        // A panic must not unwind out of the hook callback (it would abort the process).
        let _ = catch_unwind(AssertUnwindSafe(|| (self.on_change)()));
    }
}

/// Whether `hwnd` is the Alt+Tab switcher or Task View (see [`win_logic::is_task_switcher_class`]).
fn is_task_switcher(hwnd: HWND) -> bool {
    let mut class = [0u16; 256];
    // SAFETY: the slice is writable for its whole length; a stale handle makes the call return 0.
    let length = unsafe { GetClassNameW(hwnd, &mut class) };
    let length = usize::try_from(length).unwrap_or(0);
    class
        .get(..length)
        .is_some_and(|name| win_logic::is_task_switcher_class(&String::from_utf16_lossy(name)))
}

/// Whether `hwnd` is the desktop, the shell window or a taskbar / tray surface (see
/// [`win_logic::is_shell_surface_class`]).
fn is_shell_surface(hwnd: HWND) -> bool {
    // SAFETY: no arguments, no preconditions.
    let (shell, desktop) = unsafe { (GetShellWindow(), GetDesktopWindow()) };
    if hwnd == shell || hwnd == desktop {
        return true;
    }
    let mut class = [0u16; 256];
    // SAFETY: the slice is writable for its whole length; a stale handle makes the call return 0.
    let length = unsafe { GetClassNameW(hwnd, &mut class) };
    let length = usize::try_from(length).unwrap_or(0);
    class
        .get(..length)
        .is_some_and(|name| win_logic::is_shell_surface_class(&String::from_utf16_lossy(name)))
}

fn hwnd_from(value: isize) -> HWND {
    HWND(value as *mut c_void)
}

/// Tracks the Target window and probes it. See the module documentation for the threading rules.
pub struct TargetTracker {
    inner: Rc<Inner>,
    hook: HWINEVENTHOOK,
    // Dropped before `com`: the COM objects must be released before COM is uninitialised.
    uia: RefCell<UiaState>,
    com: ComGuard,
}

impl TargetTracker {
    /// Starts tracking on the current (UI) thread.
    ///
    /// `on_change` runs on this thread, from the message loop, each time a foreign window becomes
    /// the foreground window; use it to refresh "Pastes into App · title". It runs for every
    /// such window, including transient ones (Alt+Tab switcher, Start menu), so debounce any
    /// expensive work, and note that [`current`](Self::current) can call UI Automation. It must
    /// not call `install`. A panic in it is caught and discarded.
    ///
    /// The hook skips events caused by this process, so activating the panel never makes the
    /// panel the Target. If the hook cannot be installed, [`current`](Self::current) still works
    /// by reading the foreground window on demand; only `on_change` is lost.
    ///
    /// At most one tracker is active per thread: installing a second one replaces the first.
    #[must_use]
    pub fn install(on_change: impl Fn() + 'static) -> Self {
        let com = ComGuard::init();
        // SAFETY: the pseudo handle of the current process is always valid.
        let own_integrity = integrity_level(unsafe { GetCurrentProcess() });
        let inner = Rc::new(Inner {
            target: Cell::new(0),
            panel: Cell::new(0),
            popups: RefCell::new(Vec::new()),
            entry: RefCell::new(Entry::Inactive),
            probing: Cell::new(false),
            change_pending: Cell::new(false),
            own_integrity,
            on_change: Box::new(on_change),
        });
        // SAFETY: no arguments, no preconditions.
        inner.note_foreground(unsafe { GetForegroundWindow() });
        CURRENT.set(Some(Rc::clone(&inner)));
        // SAFETY: `foreground_hook` matches WINEVENTPROC and lives for the whole process.
        // Out of context: the callback runs on this thread, from its message loop.
        let hook = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(foreground_hook),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            )
        };
        Self {
            inner,
            hook,
            uia: RefCell::new(UiaState::Untried),
            com,
        }
    }

    /// Tells the tracker the panel's window (`HWND` value), so it is never the Target. Call it as
    /// soon as the window exists.
    pub fn set_panel(&self, hwnd: isize) {
        self.inner.panel.set(hwnd);
        if self.inner.target.get() == hwnd {
            self.inner.target.set(0);
        }
    }

    /// Tells the tracker the window `hwnd` (`HWND` value) is one of the app's popups (the
    /// Picker, the fill form, the toast window), so it is never the Target. Registering a
    /// window twice is harmless.
    pub fn register_popup(&self, hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        if let Ok(mut popups) = self.inner.popups.try_borrow_mut()
            && !popups.contains(&hwnd)
        {
            popups.push(hwnd);
        }
        if self.inner.target.get() == hwnd {
            self.inner.target.set(0);
        }
    }

    /// The Target now: the last foreign foreground window, probed.
    ///
    /// `None` when there is no such window (none yet, or it was closed). While the panel has
    /// activated itself for text entry ([`begin_text_entry`](Self::begin_text_entry)) this
    /// returns what was captured before the activation, because the Target has lost the keyboard
    /// focus and could no longer be probed.
    ///
    /// Reads the foreground window first, so it is right even if a foreground event is still
    /// queued. The `focus` probe may call UI Automation; see the module documentation.
    #[must_use]
    pub fn current(&self) -> Option<Target> {
        let inner = &self.inner;
        if inner.panel_is_foreground()
            && let Ok(entry) = inner.entry.try_borrow()
            && let Entry::Active(captured) = &*entry
        {
            return captured.clone();
        }
        // SAFETY: no arguments, no preconditions.
        inner.note_foreground(unsafe { GetForegroundWindow() });
        let hwnd = inner.target_hwnd();
        // SAFETY: `IsWindow` accepts any handle value.
        if hwnd.is_invalid() || !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            return None;
        }
        self.probe(hwnd)
    }

    /// The panel activates itself so its search box receives keystrokes.
    ///
    /// The Target is captured first (while it still has focus) and returned by
    /// [`current`](Self::current) until [`end_text_entry`](Self::end_text_entry) or until the
    /// user moves to another window. Windows allows the activation because the click that
    /// focused the search box was input to this process. Returns `false` if Windows refused.
    pub fn begin_text_entry(&self) -> bool {
        self.begin_text_entry_in(self.inner.panel.get())
    }

    /// [`begin_text_entry`](Self::begin_text_entry) for the window `hwnd` (the panel, or a popup
    /// registered with [`register_popup`](Self::register_popup)): the Picker takes the
    /// foreground the moment it opens so the user can type.
    pub fn begin_text_entry_in(&self, hwnd: isize) -> bool {
        let panel = hwnd_from(hwnd);
        if panel.is_invalid() {
            return false;
        }
        let captured = self.current();
        if let Ok(mut entry) = self.inner.entry.try_borrow_mut() {
            *entry = Entry::Active(captured);
        }
        // SAFETY: `panel` is a window handle; a stale one makes the call fail.
        let activated = unsafe { SetForegroundWindow(panel) }.as_bool();
        if !activated && let Ok(mut entry) = self.inner.entry.try_borrow_mut() {
            *entry = Entry::Inactive;
        }
        activated
    }

    /// The search box lost focus: hands the foreground back to the Target, but only if the panel
    /// still holds it. If the user clicked into another app meanwhile, that app stays in front.
    pub fn end_text_entry(&self) {
        let still_ours = self.inner.panel_is_foreground();
        if let Ok(mut entry) = self.inner.entry.try_borrow_mut() {
            *entry = Entry::Inactive;
        }
        if still_ours {
            self.refocus_target();
        }
    }

    /// Makes the Target the foreground window again so injected keys reach it. Returns `false`
    /// when there is no Target or Windows refused.
    ///
    /// Windows restores the Target's focused control asynchronously: wait about 50 ms before
    /// injecting (the spike used 50 ms).
    pub fn refocus_target(&self) -> bool {
        let target = self.inner.target_hwnd();
        // SAFETY: `IsWindow` accepts any handle value.
        if target.is_invalid() || !unsafe { IsWindow(Some(target)) }.as_bool() {
            return false;
        }
        // SAFETY: no arguments, no preconditions.
        if unsafe { GetForegroundWindow() } == target {
            return true;
        }
        // SAFETY: `target` is a window handle; a stale one makes the call fail.
        unsafe { SetForegroundWindow(target) }.as_bool()
    }

    /// The text caret of the Target, in screen pixels, if it has a system caret. Call it while
    /// the Target still has the focus (before a popup takes the foreground); `None` for apps
    /// that draw their own caret, which the caller treats as "use the mouse".
    #[must_use]
    pub fn caret(&self) -> Option<ScreenRect> {
        let hwnd = self.inner.target_hwnd();
        // SAFETY: `IsWindow` accepts any handle value.
        if hwnd.is_invalid() || !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            return None;
        }
        // SAFETY: a dead window makes the call return 0.
        let thread = unsafe { GetWindowThreadProcessId(hwnd, None) };
        if thread == 0 {
            return None;
        }
        gui_thread_caret(thread)
    }

    /// Builds the [`Target`] for `hwnd`.
    fn probe(&self, hwnd: HWND) -> Option<Target> {
        let mut pid = 0;
        // SAFETY: `pid` outlives the call; a dead window makes the call return 0.
        let thread = unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut pid)) };
        if thread == 0 || pid == 0 {
            return None;
        }
        let elevated = !win_logic::can_inject(self.inner.own_integrity, process_integrity(pid));
        let focus = match gui_thread_focus(thread) {
            Some(focus) => focus,
            None if elevated => Focus::None,
            None => self.automation_focus(),
        };
        Some(Target {
            app: process_exe_name(pid).unwrap_or_else(|| UNKNOWN_APP.to_owned()),
            title: window_title(hwnd),
            elevated,
            focus,
        })
    }

    /// The UI Automation fallback: a focused, editable element makes [`Focus::Automation`].
    fn automation_focus(&self) -> Focus {
        let Some(uia) = self.uia() else {
            return Focus::None;
        };
        let inner = &self.inner;
        inner.probing.set(true);
        let _probing = ProbeGuard(inner);
        // SAFETY: no arguments, no preconditions.
        let own_pid = unsafe { GetCurrentProcessId() };
        match uia.focused_element(own_pid) {
            Some(facts) if win_logic::accepts_text(facts) => Focus::Automation,
            _ => Focus::None,
        }
    }

    /// The UI Automation objects, created on first use.
    fn uia(&self) -> Option<Uia> {
        let mut state = self.uia.try_borrow_mut().ok()?;
        if matches!(*state, UiaState::Untried) {
            *state = Uia::create().map_or(UiaState::Unavailable, UiaState::Ready);
        }
        match &*state {
            UiaState::Ready(uia) => Some(uia.clone()),
            _ => None,
        }
    }
}

impl Drop for TargetTracker {
    fn drop(&mut self) {
        if !self.hook.is_invalid() {
            // SAFETY: `hook` came from `SetWinEventHook` on this thread and is unhooked once.
            let _ = unsafe { UnhookWinEvent(self.hook) };
        }
        // Stop the callback from reaching this tracker, unless a newer one replaced it.
        let current = CURRENT.take();
        if let Some(active) = current
            && !Rc::ptr_eq(&active, &self.inner)
        {
            CURRENT.set(Some(active));
        }
        // `uia` drops before `com` (field order), releasing the COM objects first.
        let _ = &self.com;
    }
}

/// Ends the probe flag; runs the change notification that arrived meanwhile.
struct ProbeGuard<'a>(&'a Inner);

impl Drop for ProbeGuard<'_> {
    fn drop(&mut self) {
        self.0.probing.set(false);
        if self.0.change_pending.replace(false) {
            self.0.notify();
        }
    }
}

/// `WINEVENTPROC` for `EVENT_SYSTEM_FOREGROUND`. Runs inside the message loop: nothing here may
/// panic or unwind.
unsafe extern "system" fn foreground_hook(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    let active = CURRENT.take();
    CURRENT.set(active.clone());
    let Some(inner) = active else {
        return;
    };
    if !inner.note_foreground(hwnd) {
        return;
    }
    if inner.probing.get() {
        inner.change_pending.set(true);
    } else {
        inner.notify();
    }
}

/// Initialises COM for UI Automation; uninitialises on drop if this call added a reference.
struct ComGuard {
    uninitialize: bool,
}

impl ComGuard {
    fn init() -> Self {
        // SAFETY: `None` reserved pointer is the only valid value. The call is balanced by
        // `CoUninitialize` in `drop` exactly when it succeeded.
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        // S_OK and S_FALSE both add a reference. RPC_E_CHANGED_MODE (the thread already runs as
        // MTA) leaves COM usable and nothing to balance. Other failures leave UI Automation
        // unavailable, which `Focus::None` covers.
        Self {
            uninitialize: result.is_ok(),
        }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.uninitialize {
            // SAFETY: balances the successful `CoInitializeEx` on this thread in `init`.
            unsafe { CoUninitialize() };
        }
    }
}

enum UiaState {
    Untried,
    Ready(Uia),
    /// Creation failed (no UI Automation); not retried.
    Unavailable,
}

/// The UI Automation client and the cache request that fetches everything in one call.
#[derive(Clone)]
struct Uia {
    automation: IUIAutomation2,
    request: IUIAutomationCacheRequest,
}

impl Uia {
    fn create() -> Option<Self> {
        // SAFETY: COM is initialised on this thread by `ComGuard`; the class is in-process.
        let automation: IUIAutomation2 =
            unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER) }.ok()?;
        // SAFETY: plain method calls on a live interface. Failing to set a timeout only leaves
        // the (long) default in place.
        unsafe {
            let _ = automation.SetConnectionTimeout(UIA_CONNECTION_TIMEOUT_MS);
            let _ = automation.SetTransactionTimeout(UIA_TRANSACTION_TIMEOUT_MS);
        }
        // SAFETY: as above.
        let request = unsafe { automation.CreateCacheRequest() }.ok()?;
        // SAFETY: `request` is live; the ids are constants from the UI Automation headers.
        unsafe {
            for property in [
                UIA_ProcessIdPropertyId,
                UIA_ControlTypePropertyId,
                UIA_IsKeyboardFocusablePropertyId,
                UIA_IsEnabledPropertyId,
                UIA_ValueIsReadOnlyPropertyId,
            ] {
                request.AddProperty(property).ok()?;
            }
            for pattern in [UIA_ValuePatternId, UIA_TextPatternId] {
                request.AddPattern(pattern).ok()?;
            }
        }
        Some(Self {
            automation,
            request,
        })
    }

    /// Facts about the globally focused element, or `None` when there is none, it cannot be read,
    /// or it lives in this process (the panel's own search box).
    ///
    /// One cross-process call (`GetFocusedElementBuildCache`) fetches everything; reading the
    /// cached values afterwards does not go back to the Target.
    fn focused_element(&self, own_pid: u32) -> Option<ElementFacts> {
        // SAFETY: live interfaces; the cache request is passed by reference.
        let element = unsafe { self.automation.GetFocusedElementBuildCache(&self.request) }.ok()?;
        // SAFETY: `element` is live; the properties were added to the request, so they are cached.
        unsafe {
            if u32::try_from(element.CachedProcessId().ok()?).ok()? == own_pid {
                return None;
            }
            let value_read_only = element
                .GetCachedPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .ok()
                .and_then(|pattern| pattern.CachedIsReadOnly().ok())
                .map(BOOL::as_bool);
            let has_text_pattern = element
                .GetCachedPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                .is_ok();
            Some(ElementFacts {
                control_type: element.CachedControlType().ok()?.0,
                keyboard_focusable: element.CachedIsKeyboardFocusable().ok()?.as_bool(),
                enabled: element.CachedIsEnabled().ok()?.as_bool(),
                value_read_only,
                has_text_pattern,
            })
        }
    }
}

/// `GetGUIThreadInfo` on the Target's UI thread: a caret, or a focus window without one.
fn gui_thread_focus(thread: u32) -> Option<Focus> {
    let mut info = GUITHREADINFO {
        cbSize: u32::try_from(size_of::<GUITHREADINFO>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: `info` is initialised with its size, as the call requires, and outlives the call.
    unsafe { GetGUIThreadInfo(thread, &raw mut info) }.ok()?;
    if !info.hwndCaret.is_invalid() {
        Some(Focus::Caret)
    } else if !info.hwndFocus.is_invalid() {
        Some(Focus::Control)
    } else {
        None
    }
}

/// The caret rectangle of the thread's caret window, in screen pixels.
fn gui_thread_caret(thread: u32) -> Option<ScreenRect> {
    let mut info = GUITHREADINFO {
        cbSize: u32::try_from(size_of::<GUITHREADINFO>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: `info` is initialised with its size, as the call requires, and outlives the call.
    unsafe { GetGUIThreadInfo(thread, &raw mut info) }.ok()?;
    if info.hwndCaret.is_invalid() {
        return None;
    }
    // The rectangle is in the caret window's client coordinates.
    let mut origin = POINT {
        x: info.rcCaret.left,
        y: info.rcCaret.top,
    };
    // SAFETY: `origin` outlives the call; a stale window makes it fail.
    if !unsafe { ClientToScreen(info.hwndCaret, &raw mut origin) }.as_bool() {
        return None;
    }
    Some(ScreenRect {
        x: origin.x,
        y: origin.y,
        w: u32::try_from(info.rcCaret.right.saturating_sub(info.rcCaret.left)).unwrap_or(0),
        h: u32::try_from(info.rcCaret.bottom.saturating_sub(info.rcCaret.top)).unwrap_or(0),
    })
}

/// The window title, truncated to [`TITLE_UNITS`]. Never logged.
fn window_title(hwnd: HWND) -> String {
    let mut buffer = [0u16; TITLE_UNITS];
    // SAFETY: the slice is writable for its whole length. For a window of another process this
    // reads the title the system keeps; it does not send a message to the (possibly hung) app.
    let length = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    let length = usize::try_from(length).unwrap_or(0);
    buffer
        .get(..length)
        .map(String::from_utf16_lossy)
        .unwrap_or_default()
}

/// The executable file name of process `pid`, for example `notepad.exe`.
fn process_exe_name(pid: u32) -> Option<String> {
    let process = open_process(pid)?;
    // Paths can reach 32 767 units; a Vec avoids a large stack array.
    let mut buffer = vec![0u16; 32_768];
    let mut length = u32::try_from(buffer.len()).ok()?;
    // SAFETY: `buffer` is writable for `length` units and both outlive the call; `process` is a
    // live handle with query rights.
    unsafe {
        QueryFullProcessImageNameW(
            *process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &raw mut length,
        )
    }
    .ok()?;
    let path = String::from_utf16_lossy(buffer.get(..usize::try_from(length).ok()?)?);
    win_logic::exe_file_name(&path).map(str::to_owned)
}

/// The executable file name (lower case) of the process that owns window `hwnd`, for the
/// Recent inbox's per-app exclusions.
pub(super) fn window_exe_name(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    // SAFETY: `pid` outlives the call; for a window of another process this reads the owner
    // the system keeps and sends no message.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut pid)) };
    if pid == 0 {
        return None;
    }
    process_exe_name(pid).map(|name| name.to_ascii_lowercase())
}

/// A handle to `pid` with query-only rights, closed on drop. `None` when Windows refuses.
fn open_process(pid: u32) -> Option<Owned<HANDLE>> {
    // SAFETY: plain call; the returned handle is owned and closed by `Owned`.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    // SAFETY: `handle` is a valid handle that nothing else closes.
    Some(unsafe { Owned::new(handle) })
}

/// The integrity level of process `pid`; `None` if it cannot be read (what an elevated process
/// looks like from a non-elevated one).
fn process_integrity(pid: u32) -> Option<u32> {
    let process = open_process(pid)?;
    integrity_level(*process)
}

/// The mandatory integrity RID of a process token (0x2000 medium, 0x3000 high, ...).
fn integrity_level(process: HANDLE) -> Option<u32> {
    let mut token = HANDLE::default();
    // SAFETY: `token` outlives the call; `process` is a live handle.
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &raw mut token) }.ok()?;
    // SAFETY: `token` was just opened and nothing else closes it.
    let token = unsafe { Owned::new(token) };
    // u64 storage keeps the pointer field of TOKEN_MANDATORY_LABEL aligned.
    let mut buffer = [0u64; 16];
    let mut needed = 0;
    // SAFETY: `buffer` is writable for the stated size and outlives the call.
    unsafe {
        GetTokenInformation(
            *token,
            TokenIntegrityLevel,
            Some(buffer.as_mut_ptr().cast()),
            u32::try_from(size_of_val(&buffer)).ok()?,
            &raw mut needed,
        )
    }
    .ok()?;
    // SAFETY: on success the buffer holds a TOKEN_MANDATORY_LABEL whose SID points into the
    // same buffer, which is alive and suitably aligned. The sub-authority pointers are checked
    // for null before they are read.
    unsafe {
        let label = &*buffer.as_ptr().cast::<TOKEN_MANDATORY_LABEL>();
        let sid = label.Label.Sid;
        if sid.0.is_null() {
            return None;
        }
        let count = GetSidSubAuthorityCount(sid).as_ref().copied()?;
        let last = u32::from(count).checked_sub(1)?;
        GetSidSubAuthority(sid, last).as_ref().copied()
    }
}

/// Whether a full-screen app, game or presentation is in front. Pasting from the deck into it
/// would be wrong, and the hotkey should not pop the panel over it.
#[must_use]
pub fn fullscreen_in_front() -> bool {
    // SAFETY: no arguments, no preconditions.
    let state = unsafe { SHQueryUserNotificationState() };
    matches!(
        state,
        Ok(QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE)
    )
}
