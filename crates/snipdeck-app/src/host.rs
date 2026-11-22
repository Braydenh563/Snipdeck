//! The seams between the app and the machine it runs on, beyond the clipboard, keyboard and
//! timing traits that `snipdeck-platform` already owns: the wall clock, the Target tracker and
//! the global hotkeys. Each has a real implementation (the Windows backend, in `windows_host`)
//! and a fake here, so the controller runs under test with no desktop.

use std::{cell::Cell, cell::RefCell, rc::Rc, time::Duration, time::Instant};

use snipdeck_core::Timestamp;
use snipdeck_platform::win_formats::ClipFiles;
use snipdeck_platform::{
    chord::Chord,
    route::{ScreenRect, Target},
};

use crate::settings::HotkeyAction;

/// The wall clock, injected so a test never reads the real one.
pub trait Clock {
    /// The current moment.
    fn now(&self) -> Timestamp;

    /// Time since an arbitrary fixed start, from a monotonic source: it never goes back when the
    /// wall clock is set back. For measuring spans (the idle lock), not for dates.
    fn elapsed(&self) -> Duration;
}

/// [`Clock`] on the system clock.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }

    fn elapsed(&self) -> Duration {
        static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        START.get_or_init(Instant::now).elapsed()
    }
}

/// A clock that only moves when the test sets it. Cloning shares the same time.
#[derive(Debug, Clone)]
pub struct FixedClock(Rc<Cell<Timestamp>>, Rc<Cell<Duration>>);

impl FixedClock {
    /// A clock stopped at `at`.
    #[must_use]
    pub fn new(at: Timestamp) -> Self {
        Self(Rc::new(Cell::new(at)), Rc::new(Cell::new(Duration::ZERO)))
    }

    /// Moves the clock. The monotonic reading follows a move forward and ignores one back, like
    /// a real wall clock that is set back.
    pub fn set(&self, at: Timestamp) {
        let before = self.0.replace(at);
        if let Ok(by) = Duration::try_from(at.duration_since(before)) {
            self.1.set(self.1.get() + by);
        }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0.get()
    }

    fn elapsed(&self) -> Duration {
        self.1.get()
    }
}

/// Reading the text on the clipboard (quick capture). The pipeline's [`Clipboard`] only
/// writes, snapshots and restores; this is the one read the app needs.
///
/// [`Clipboard`]: snipdeck_platform::pipeline::Clipboard
pub trait ClipboardText {
    /// The clipboard's text, or `None` when it holds none or stays busy. Writes nothing.
    fn read_text(&mut self) -> Option<String>;
    /// The clipboard's text with its HTML and RTF flavours (only next to text; over the size cap
    /// they are dropped). A copy that carries a "keep me out" marker is not read: `excluded`.
    /// Writes nothing.
    fn read_rich(&mut self) -> snipdeck_platform::clipboard_watch::RichClip;
    /// Whether the clipboard holds a picture (a screenshot, say).
    fn has_image(&mut self) -> bool;
    /// The clipboard's picture as PNG bytes (a screenshot, a copied image), or `None`. Writes
    /// nothing.
    fn read_png(&mut self) -> Option<Vec<u8>>;
    /// The files copied in Explorer (no paths when there are none; too many is its own answer).
    /// Writes nothing.
    fn read_files(&mut self) -> ClipFiles;
    /// Empties the clipboard if it still holds exactly `text` (so something the user copied
    /// since is never lost). Whether it was cleared.
    fn clear_if_text(&mut self, text: &str) -> bool;
    /// The clipboard's sequence number, which changes whenever its contents do; `None` when it
    /// cannot be read. Covers content that is not text (HTML, images, files).
    fn sequence_number(&mut self) -> Option<u32>;
    /// The sequence number read right after this clipboard's latest write (not now): the
    /// reference for "nothing has touched the clipboard since our write". `None` before a write.
    fn written_sequence(&mut self) -> Option<u32>;
    /// Empties the clipboard, whatever formats it holds, if its sequence number is still
    /// `sequence`. Whether it did.
    fn clear_if_sequence(&mut self, sequence: u32) -> bool;
    /// Deletes the decrypted files that earlier file pastes left in the temp folder (a file
    /// another app still has open stays). Nothing for a clipboard that makes none.
    fn forget_pasted_files(&mut self) {}
    /// [`forget_pasted_files`](Self::forget_pasted_files), but returns only when it is done (the
    /// quit, whose process would end under a background deletion).
    fn forget_pasted_files_waiting(&mut self) {
        self.forget_pasted_files();
    }
    /// The text entries of the clipboard history besides the one on the clipboard, newest first
    /// (`{clipboard:2}` is the first). Empty when there is none, the history is off, or this
    /// platform has none. Writes nothing.
    fn read_history(&mut self) -> Vec<String> {
        Vec::new()
    }
}

#[cfg(windows)]
impl ClipboardText for snipdeck_platform::windows::clipboard::WinClipboard {
    fn read_text(&mut self) -> Option<String> {
        Self::read_text(self)
    }
    fn read_rich(&mut self) -> snipdeck_platform::clipboard_watch::RichClip {
        Self::read_rich(self)
    }
    fn has_image(&mut self) -> bool {
        Self::has_image(self)
    }
    fn read_png(&mut self) -> Option<Vec<u8>> {
        Self::read_png(self)
    }
    fn read_files(&mut self) -> ClipFiles {
        Self::read_files(self)
    }
    fn clear_if_text(&mut self, text: &str) -> bool {
        Self::clear_if_text(self, text)
    }
    fn sequence_number(&mut self) -> Option<u32> {
        Self::sequence_number(self)
    }
    fn written_sequence(&mut self) -> Option<u32> {
        Self::written_sequence(self)
    }
    fn clear_if_sequence(&mut self, sequence: u32) -> bool {
        Self::clear_if_sequence(self, sequence)
    }
    fn forget_pasted_files(&mut self) {
        Self::forget_pasted_files(self);
    }
    fn forget_pasted_files_waiting(&mut self) {
        Self::forget_pasted_files_waiting(self);
    }
    fn read_history(&mut self) -> Vec<String> {
        // Unverified on Windows (see `snipdeck_platform::clipboard_history`). Blocks until Windows
        // answers; the entry the clipboard holds now is dropped, as `{clipboard}` is that one.
        let current = Self::read_text(self);
        let Ok(snipdeck_platform::clipboard_history::HistoryRead::Entries(entries)) =
            snipdeck_platform::clipboard_history::read_current_session()
        else {
            return Vec::new();
        };
        let mut texts: Vec<String> = entries.into_iter().map(|e| e.text).collect();
        if texts.first().is_some() && texts.first() == current.as_ref() {
            texts.remove(0);
        }
        texts
    }
}

/// The fake clipboard "holds" what was last written to it (as a real one holds what the user
/// last copied), so a test seeds it by pushing a payload onto `written`.
impl ClipboardText for snipdeck_platform::testing::FakeClipboard {
    fn read_text(&mut self) -> Option<String> {
        self.text_reads += 1;
        // A Target that was sent Ctrl+C answers after a few reads, by replacing the clipboard.
        if let Some((after, _)) = &mut self.copy_arrives {
            if *after == 0 {
                if let Some((_, text)) = self.copy_arrives.take() {
                    use snipdeck_platform::payload::{Data, Payload};
                    self.written.extend(Payload::new(vec![Data::Text(text)]));
                    self.sequence += 1;
                }
            } else {
                *after -= 1;
            }
        }
        self.written.last()?.data().iter().find_map(|d| match d {
            snipdeck_platform::payload::Data::Text(text) => Some(text.clone()),
            _ => None,
        })
    }
    fn read_history(&mut self) -> Vec<String> {
        self.held_history.clone()
    }
    fn read_rich(&mut self) -> snipdeck_platform::clipboard_watch::RichClip {
        if self.held_excluded
            || self
                .written
                .last()
                .is_some_and(snipdeck_platform::payload::Payload::is_private)
        {
            return snipdeck_platform::clipboard_watch::RichClip {
                excluded: true,
                ..Default::default()
            };
        }
        let text = ClipboardText::read_text(self);
        let (html, rtf) = if text.is_some() {
            (self.held_html.clone(), self.held_rtf.clone())
        } else {
            (None, None)
        };
        snipdeck_platform::clipboard_watch::RichClip {
            text,
            html,
            rtf,
            excluded: false,
        }
    }
    fn has_image(&mut self) -> bool {
        self.written.last().is_some_and(|p| {
            p.data()
                .iter()
                .any(|d| matches!(d, snipdeck_platform::payload::Data::Png(_)))
        })
    }
    fn read_png(&mut self) -> Option<Vec<u8>> {
        let written = self.written.last().and_then(|p| {
            p.data().iter().find_map(|d| match d {
                snipdeck_platform::payload::Data::Png(png) => Some(png.clone()),
                _ => None,
            })
        });
        written.or_else(|| self.held_png.clone())
    }
    fn read_files(&mut self) -> ClipFiles {
        if self.files_too_many {
            ClipFiles::TooMany
        } else {
            ClipFiles::Files(self.held_files.clone())
        }
    }
    fn clear_if_text(&mut self, text: &str) -> bool {
        if self.read_text().as_deref() != Some(text) || self.busy_for_clear() {
            return false;
        }
        self.written.clear();
        self.sequence += 1;
        self.log.push("clear".into());
        true
    }
    fn sequence_number(&mut self) -> Option<u32> {
        Some(self.sequence)
    }
    fn written_sequence(&mut self) -> Option<u32> {
        self.write_sequence
    }
    fn forget_pasted_files(&mut self) {
        self.pasted_files_forgotten += 1;
    }
    fn forget_pasted_files_waiting(&mut self) {
        self.pasted_files_forgotten += 1;
        self.pasted_files_forgotten_waited += 1;
    }
    fn clear_if_sequence(&mut self, sequence: u32) -> bool {
        if self.sequence != sequence || self.busy_for_clear() {
            return false;
        }
        self.written.clear();
        self.sequence += 1;
        self.log.push("clear".into());
        true
    }
}

/// Which window a paste would go into, and bringing it back to the front.
pub trait TargetSource {
    /// The window the user was in before they clicked Snipdeck, if it can still be pasted
    /// into. May probe the other app, so call it when needed, not per frame.
    fn current(&self) -> Option<Target>;
    /// Makes the Target the foreground window again so injected keys reach it (the panel may
    /// have activated itself for text entry). `false` if there is no Target or Windows refused.
    fn refocus(&self) -> bool;
    /// Where the Target's text caret is on the screen, in physical pixels, if it has a system
    /// caret. Read before a popup takes the foreground. `None` (the default) means "unknown":
    /// the Picker then opens at the mouse.
    fn caret(&self) -> Option<ScreenRect> {
        None
    }
    /// The mouse position in physical pixels, if it can be read. `None` (the default) leaves
    /// the Panel where it was.
    fn cursor(&self) -> Option<(i32, i32)> {
        None
    }
    /// Whether the primary mouse button is held down, wherever the pointer is. `None` (the
    /// default) means the host cannot tell: a drag then ends only on the window's own
    /// pointer-up.
    fn primary_down(&self) -> Option<bool> {
        None
    }
    /// Whether the system's own size loop runs on the Panel's window: it starts a moment
    /// after a resize is asked for and ends when the grip is let go. `None` (the default)
    /// means the host cannot tell.
    fn size_loop(&self) -> Option<bool> {
        None
    }
    /// The monitor under a physical point, if the host can tell.
    fn monitor_at(&self, _x: i32, _y: i32) -> Option<crate::frame::Monitor> {
        None
    }
    /// Whether a full-screen app, game or presentation covers `monitor`. Only that monitor: one
    /// on another screen is not in the Panel's way. `false` (the default) where the host cannot
    /// tell.
    fn fullscreen_on(&self, _monitor: &crate::frame::Monitor) -> bool {
        false
    }
}

/// The typed-trigger service: the keyboard hook that watches what is typed for abbreviations.
///
/// The controller tells it whether anyone wants it (`set_enabled`) and which triggers exist
/// (`set_triggers`); while it is disabled the implementation must hold no keyboard hook at all.
pub trait TypingControl {
    /// Installs the hook (`true`) or removes it (`false`). Idempotent. Returns `false` only when
    /// the hook was wanted and could not be installed.
    fn set_enabled(&mut self, on: bool) -> bool;
    /// The triggers to watch for, as of now, and the candidates a typed prefix is matched
    /// against for suggestions (empty when suggestions are off).
    fn set_triggers(
        &mut self,
        matcher: snipdeck_core::Matcher,
        candidates: Vec<snipdeck_core::Candidate>,
    );
    /// Hint number `id` is really on screen: the hook may take a plain `Tab` for it. Only the UI
    /// thread raises the flag, and only after showing the hint.
    fn hint_up(&mut self, id: u64);
    /// The generation of the running worker (see `typing_worker::next_generation`); a hint number
    /// from another generation is stale. 0 while none runs.
    fn generation(&self) -> u64;
    /// Hint number `id` was taken down: the hook stops taking a plain `Tab` for it. A newer
    /// hint's flag is left alone.
    fn hint_down(&mut self, id: u64);
    /// The expansion for `fired` was not sent (a Snipdeck window, an excluded app, a skipped
    /// route): the code stays in the app, so the next trigger needs a boundary of its own.
    fn not_expanded(&mut self, fired: snipdeck_core::Fired);
}

/// A [`TypingControl`] that records what it was told. Cloning shares the state.
#[derive(Debug, Clone, Default)]
pub struct FakeTyping {
    enabled: Rc<RefCell<Vec<bool>>>,
    candidates: Rc<RefCell<Vec<snipdeck_core::Candidate>>>,
    up: Rc<RefCell<Vec<u64>>>,
    generation: Rc<Cell<u64>>,
    hint: Rc<RefCell<Vec<u64>>>,
    skipped: Rc<RefCell<Vec<snipdeck_core::Fired>>>,
    fail_start: Rc<Cell<bool>>,
}

impl FakeTyping {
    /// Makes every later turn-on fail, as a hook Windows refuses to install does.
    pub fn fail_start(&self, fail: bool) {
        self.fail_start.set(fail);
    }

    /// The candidates last handed over.
    #[must_use]
    pub fn candidates(&self) -> Vec<snipdeck_core::Candidate> {
        self.candidates.borrow().clone()
    }

    /// Makes the fake report `generation` as the running worker's.
    pub fn set_generation(&self, generation: u64) {
        self.generation.set(generation);
    }

    /// Every `hint_up` call, in order.
    #[must_use]
    pub fn hint_up_history(&self) -> Vec<u64> {
        self.up.borrow().clone()
    }

    /// Every `hint_down` call, in order.
    #[must_use]
    pub fn hint_history(&self) -> Vec<u64> {
        self.hint.borrow().clone()
    }

    /// The triggers reported as not expanded, in order.
    #[must_use]
    pub fn not_expanded_history(&self) -> Vec<snipdeck_core::Fired> {
        self.skipped.borrow().clone()
    }

    /// Every `set_enabled` call, in order.
    #[must_use]
    pub fn enabled_history(&self) -> Vec<bool> {
        self.enabled.borrow().clone()
    }

    /// Whether the hook was ever asked to be installed.
    #[must_use]
    pub fn ever_enabled(&self) -> bool {
        self.enabled.borrow().contains(&true)
    }
}

impl TypingControl for FakeTyping {
    fn set_enabled(&mut self, on: bool) -> bool {
        self.enabled.borrow_mut().push(on);
        !(on && self.fail_start.get())
    }

    fn set_triggers(
        &mut self,
        _matcher: snipdeck_core::Matcher,
        candidates: Vec<snipdeck_core::Candidate>,
    ) {
        *self.candidates.borrow_mut() = candidates;
    }

    fn hint_up(&mut self, id: u64) {
        self.up.borrow_mut().push(id);
    }

    fn generation(&self) -> u64 {
        self.generation.get()
    }

    fn hint_down(&mut self, id: u64) {
        self.hint.borrow_mut().push(id);
    }

    fn not_expanded(&mut self, fired: snipdeck_core::Fired) {
        self.skipped.borrow_mut().push(fired);
    }
}

/// A Target change waiting for a flag: see [`FakeTargets::switch_when`].
type TargetSwitch = (
    std::sync::Arc<std::sync::atomic::AtomicBool>,
    Option<Target>,
);

/// A scripted Target. Cloning shares the state, so a test keeps a handle after handing the
/// source to the controller.
#[derive(Debug, Clone, Default)]
pub struct FakeTargets {
    current: Rc<RefCell<Option<Target>>>,
    refocused: Rc<Cell<u32>>,
    caret: Rc<Cell<Option<ScreenRect>>>,
    cursor: Rc<Cell<Option<(i32, i32)>>>,
    primary: Rc<Cell<Option<bool>>>,
    size_loop: Rc<Cell<Option<bool>>>,
    fullscreen: Rc<RefCell<Option<String>>>,
    monitors: Rc<RefCell<Vec<crate::frame::Monitor>>>,
    /// A Target change that happens once the flag is raised (from another thread, as a typed
    /// key would be).
    switch: Rc<RefCell<Option<TargetSwitch>>>,
}

impl FakeTargets {
    /// A source whose Target is `target`.
    #[must_use]
    pub fn new(target: Option<Target>) -> Self {
        Self {
            current: Rc::new(RefCell::new(target)),
            refocused: Rc::default(),
            caret: Rc::default(),
            cursor: Rc::default(),
            primary: Rc::default(),
            size_loop: Rc::default(),
            fullscreen: Rc::default(),
            monitors: Rc::default(),
            switch: Rc::default(),
        }
    }

    /// Changes the Target to `target` the first time it is read after `flag` is raised.
    pub fn switch_when(&self, flag: std::sync::Arc<std::sync::atomic::AtomicBool>, target: Target) {
        *self.switch.borrow_mut() = Some((flag, Some(target)));
    }

    /// Adds a monitor [`monitor_at`](TargetSource::monitor_at) can find (none: the host cannot
    /// tell, as before).
    pub fn add_monitor(&self, monitor: crate::frame::Monitor) {
        self.monitors.borrow_mut().push(monitor);
    }

    /// Sets where the Target's caret is (`None`: it has none).
    pub fn set_caret(&self, caret: Option<ScreenRect>) {
        self.caret.set(caret);
    }

    /// Sets where the mouse is (`None`: it cannot be read).
    pub fn set_cursor(&self, cursor: Option<(i32, i32)>) {
        self.cursor.set(cursor);
    }

    /// Sets what the host reads for the primary mouse button (`None`: it cannot be read).
    pub fn set_primary_down(&self, down: Option<bool>) {
        self.primary.set(down);
    }

    /// Sets whether the system's size loop runs on the Panel (`None`: the host cannot tell).
    pub fn set_size_loop(&self, running: Option<bool>) {
        self.size_loop.set(running);
    }

    /// Sets the monitor a full-screen app covers (`None`: there is none).
    pub fn set_fullscreen(&self, on: Option<&crate::frame::Monitor>) {
        *self.fullscreen.borrow_mut() = on.map(|m| m.id.clone());
    }

    /// Replaces the monitors the host knows.
    pub fn set_monitors(&self, monitors: Vec<crate::frame::Monitor>) {
        *self.monitors.borrow_mut() = monitors;
    }

    /// Changes the Target, as the user switching windows would.
    pub fn set(&self, target: Option<Target>) {
        *self.current.borrow_mut() = target;
    }

    /// How many times the controller brought the Target back to the front.
    #[must_use]
    pub fn refocus_count(&self) -> u32 {
        self.refocused.get()
    }
}

impl TargetSource for FakeTargets {
    fn current(&self) -> Option<Target> {
        let raised = self
            .switch
            .borrow()
            .as_ref()
            .is_some_and(|(flag, _)| flag.load(std::sync::atomic::Ordering::SeqCst));
        if raised && let Some((_, target)) = self.switch.borrow_mut().take() {
            *self.current.borrow_mut() = target;
        }
        self.current.borrow().clone()
    }

    fn refocus(&self) -> bool {
        self.refocused.set(self.refocused.get() + 1);
        self.current.borrow().is_some()
    }

    fn caret(&self) -> Option<ScreenRect> {
        self.caret.get()
    }

    fn cursor(&self) -> Option<(i32, i32)> {
        self.cursor.get()
    }

    fn size_loop(&self) -> Option<bool> {
        self.size_loop.get()
    }

    fn primary_down(&self) -> Option<bool> {
        self.primary.get()
    }

    fn fullscreen_on(&self, monitor: &crate::frame::Monitor) -> bool {
        self.fullscreen.borrow().as_ref() == Some(&monitor.id)
    }

    fn monitor_at(&self, x: i32, y: i32) -> Option<crate::frame::Monitor> {
        self.monitors
            .borrow()
            .iter()
            .find(|m| m.holds(x, y))
            .cloned()
    }
}

/// Why a global hotkey could not be registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HotkeyFailure {
    /// Another application already owns the chord.
    #[error("another application already uses this shortcut")]
    InUse,
    /// Another Snipdeck shortcut already holds the chord (two actions given the same one).
    #[error("another Snipdeck shortcut already uses this chord")]
    OwnAction,
    /// The chord cannot be registered.
    #[error("this shortcut cannot be used")]
    Invalid,
    /// Windows refused for another reason; its error code.
    #[error("Windows refused this shortcut (Windows error {0})")]
    Refused(u32),
    /// The hotkey service is not running or Windows refused.
    #[error("the hotkey service is unavailable")]
    Unavailable,
}

/// Registers global hotkeys under numeric ids (see [`id_of`]).
pub trait HotkeyBackend {
    /// Registers `chord` under `id`, replacing any chord already under `id`.
    ///
    /// # Errors
    ///
    /// See [`HotkeyFailure`].
    fn register(&mut self, id: u32, chord: Chord) -> Result<(), HotkeyFailure>;

    /// Removes the chord under `id`. Removing an id that is not registered succeeds.
    ///
    /// # Errors
    ///
    /// [`HotkeyFailure::Unavailable`] if the service is not running or Windows refuses.
    fn unregister(&mut self, id: u32) -> Result<(), HotkeyFailure>;

    /// Whether `chord` could be registered now, without leaving anything registered: another
    /// program that holds it is [`HotkeyFailure::InUse`], another Snipdeck action
    /// [`HotkeyFailure::OwnAction`].
    ///
    /// # Errors
    ///
    /// Why it could not be registered.
    fn probe(&mut self, chord: Chord) -> Result<(), HotkeyFailure>;
}

/// The id a hotkey action is registered under.
#[must_use]
pub const fn id_of(action: HotkeyAction) -> u32 {
    match action {
        HotkeyAction::TogglePanel => 1,
        HotkeyAction::Picker => 2,
        HotkeyAction::Capture => 3,
        HotkeyAction::PasteNext => 4,
    }
}

/// The action registered under `id`, if any.
#[must_use]
pub fn action_of(id: u32) -> Option<HotkeyAction> {
    HotkeyAction::ALL.into_iter().find(|a| id_of(*a) == id)
}

/// A hotkey that could not be registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyProblem {
    /// What the hotkey would have done.
    pub action: HotkeyAction,
    /// The chord as configured, for the message.
    pub chord: String,
    /// Why.
    pub failure: HotkeyFailure,
}

/// Registers every configured hotkey. A hotkey that is off (empty) is skipped; one that fails
/// is reported and the rest are still registered.
pub fn register_all(
    backend: &mut (impl HotkeyBackend + ?Sized),
    hotkeys: &crate::settings::Hotkeys,
) -> Vec<HotkeyProblem> {
    let mut problems = Vec::new();
    for action in HotkeyAction::ALL {
        let Some(chord) = hotkeys.chord(action) else {
            continue;
        };
        let shown = chord.label();
        if let Err(failure) = backend.register(id_of(action), chord) {
            crate::warn!("hotkey {action:?} ({shown}) could not be registered: {failure}");
            problems.push(HotkeyProblem {
                action,
                chord: shown,
                failure,
            });
        }
    }
    problems
}

/// Registers again the hotkeys in `failed` (PLATFORM-3): the program that held a chord may have
/// ended, or the hotkey service restarted. Uses the chords in `hotkeys` now (one the user has
/// since turned off or changed is not the failed one). Returns the problems that remain, so a
/// success clears its warning.
pub fn retry_failed(
    backend: &mut (impl HotkeyBackend + ?Sized),
    hotkeys: &crate::settings::Hotkeys,
    failed: &[HotkeyProblem],
) -> Vec<HotkeyProblem> {
    let mut remaining = Vec::new();
    for problem in failed {
        let Some(chord) = hotkeys.chord(problem.action) else {
            continue;
        };
        let shown = chord.label();
        if let Err(failure) = backend.register(id_of(problem.action), chord) {
            remaining.push(HotkeyProblem {
                action: problem.action,
                chord: shown,
                failure,
            });
        }
    }
    remaining
}

/// Decides when failed hotkeys are tried again (PLATFORM-3, -4), with the time given by the
/// caller so a test needs no clock: on a slow interval, and at once after a wake, unlock,
/// remote connect or opening Settings ([`Self::now`]).
#[derive(Debug, Clone, Copy)]
pub struct RetryGate {
    interval: std::time::Duration,
    last: Option<std::time::Duration>,
}

impl RetryGate {
    /// A gate that allows a retry every `interval` (and one at once the first time).
    #[must_use]
    pub const fn new(interval: std::time::Duration) -> Self {
        Self {
            interval,
            last: None,
        }
    }

    /// Whether a retry is due at `now` (time since any fixed start); if so, it counts as made.
    pub fn due(&mut self, now: std::time::Duration) -> bool {
        let due = self
            .last
            .is_none_or(|last| now.saturating_sub(last) >= self.interval);
        if due {
            self.last = Some(now);
        }
        due
    }

    /// The user is back (wake, unlock, remote connect) or opened Settings: the next
    /// [`due`](Self::due) is true whatever the time.
    pub fn now(&mut self) {
        self.last = None;
    }
}

/// Switches every global hotkey off ("Pause hotkeys"); returns how many could not be removed.
/// The settings are untouched: [`register_all`] switches them back on.
pub fn unregister_all(backend: &mut (impl HotkeyBackend + ?Sized)) -> usize {
    HotkeyAction::ALL
        .into_iter()
        .filter(|action| backend.unregister(id_of(*action)).is_err())
        .count()
}

/// What switching the hotkeys off or on did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PauseReport {
    /// Hotkeys that could not be registered when resuming.
    pub problems: Vec<HotkeyProblem>,
    /// Hotkeys that could not be removed when pausing.
    pub not_removed: usize,
}

/// "Pause hotkeys" changed: switches every hotkey off, or registers the chords in `current`
/// again. `current` must be the chords in force *now* (the settings may have changed since
/// start-up), not the ones read at launch.
pub fn set_paused(
    backend: &mut (impl HotkeyBackend + ?Sized),
    paused: bool,
    current: &crate::settings::Hotkeys,
) -> PauseReport {
    if paused {
        PauseReport {
            not_removed: unregister_all(backend),
            ..PauseReport::default()
        }
    } else {
        PauseReport {
            problems: register_all(backend, current),
            ..PauseReport::default()
        }
    }
}

/// A hotkey backend that records registrations and can refuse chosen chords.
#[derive(Debug, Default)]
pub struct FakeHotkeys {
    /// Every successful registration, in order.
    pub registered: Vec<(u32, Chord)>,
    /// Chords (as text) another app "owns".
    pub taken: Vec<String>,
}

impl HotkeyBackend for FakeHotkeys {
    fn register(&mut self, id: u32, chord: Chord) -> Result<(), HotkeyFailure> {
        if self.taken.contains(&chord.to_string()) {
            return Err(HotkeyFailure::InUse);
        }
        // Like the real table: a chord another Snipdeck id already holds is refused.
        if self
            .registered
            .iter()
            .any(|(other, c)| *other != id && *c == chord)
        {
            return Err(HotkeyFailure::OwnAction);
        }
        self.registered.retain(|(existing, _)| *existing != id);
        self.registered.push((id, chord));
        Ok(())
    }

    fn unregister(&mut self, id: u32) -> Result<(), HotkeyFailure> {
        self.registered.retain(|(existing, _)| *existing != id);
        Ok(())
    }

    fn probe(&mut self, chord: Chord) -> Result<(), HotkeyFailure> {
        if self.taken.contains(&chord.to_string()) {
            return Err(HotkeyFailure::InUse);
        }
        if self.registered.iter().any(|(_, c)| *c == chord) {
            return Err(HotkeyFailure::OwnAction);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Hotkeys;

    #[test]
    fn failed_hotkeys_are_retried_and_the_warning_clears_on_success() {
        let hotkeys = all_on();
        let mut backend = FakeHotkeys {
            taken: vec![hotkeys.toggle_panel.clone()],
            ..FakeHotkeys::default()
        };
        let problems = register_all(&mut backend, &hotkeys);
        assert_eq!(problems.len(), 1);
        // Still taken: still reported, with the same reason.
        assert_eq!(retry_failed(&mut backend, &hotkeys, &problems), problems);
        // The other program ended: the retry registers it and nothing remains.
        backend.taken.clear();
        assert_eq!(retry_failed(&mut backend, &hotkeys, &problems).len(), 0);
        assert!(
            backend
                .registered
                .iter()
                .any(|(id, _)| *id == id_of(HotkeyAction::TogglePanel))
        );
        // A hotkey the user has since turned off is not retried.
        let mut off = hotkeys.clone();
        off.toggle_panel = String::new();
        assert_eq!(retry_failed(&mut backend, &off, &problems).len(), 0);
    }

    #[test]
    fn the_retry_gate_waits_its_interval_and_opens_at_once_when_the_user_is_back() {
        use std::time::Duration;
        let mut gate = RetryGate::new(Duration::from_mins(1));
        assert!(gate.due(Duration::from_secs(5)), "the first time");
        assert!(!gate.due(Duration::from_secs(30)));
        assert!(!gate.due(Duration::from_secs(64)));
        assert!(gate.due(Duration::from_secs(65)));
        gate.now();
        assert!(gate.due(Duration::from_secs(66)), "after a wake or unlock");
        assert!(!gate.due(Duration::from_secs(67)));
    }

    #[test]
    fn probing_leaves_nothing_registered_and_names_who_holds_the_chord() {
        let hotkeys = all_on();
        let mut backend = FakeHotkeys {
            taken: vec!["Ctrl+Shift+G".into()],
            ..FakeHotkeys::default()
        };
        assert_eq!(register_all(&mut backend, &hotkeys).len(), 0);
        let before = backend.registered.clone();
        let chord = |t: &str| t.parse::<Chord>().unwrap();
        assert_eq!(backend.probe(chord("Ctrl+Shift+H")), Ok(()));
        assert_eq!(
            backend.probe(chord("Ctrl+Shift+G")),
            Err(HotkeyFailure::InUse)
        );
        let own = hotkeys.chord(HotkeyAction::TogglePanel).unwrap();
        assert_eq!(backend.probe(own), Err(HotkeyFailure::OwnAction));
        assert_eq!(backend.registered, before);
    }

    #[test]
    fn ids_round_trip_and_are_distinct() {
        for action in HotkeyAction::ALL {
            assert_eq!(action_of(id_of(action)), Some(action));
        }
        assert_eq!(action_of(0), None);
        assert_eq!(action_of(99), None);
    }

    /// Every action has a chord (Capture is off by default, see `HotkeyAction::default_chord`).
    fn all_on() -> Hotkeys {
        Hotkeys {
            capture: "Ctrl+Shift+2".into(),
            ..Hotkeys::new_install()
        }
    }

    #[test]
    fn every_default_hotkey_is_registered_and_capture_stays_off_until_set() {
        let mut backend = FakeHotkeys::default();
        let problems = register_all(&mut backend, &Hotkeys::new_install());
        assert_eq!(problems, Vec::new());
        assert_eq!(backend.registered.len(), 3);
        assert!(
            backend
                .registered
                .iter()
                .all(|(id, _)| *id != id_of(HotkeyAction::Capture))
        );
        let mut backend = FakeHotkeys::default();
        assert_eq!(register_all(&mut backend, &all_on()), Vec::new());
        assert_eq!(backend.registered.len(), 4);
        let toggle = backend
            .registered
            .iter()
            .find(|(id, _)| *id == id_of(HotkeyAction::TogglePanel))
            .expect("toggle registered");
        assert_eq!(toggle.1.to_string(), "Ctrl+Shift+Y");
    }

    #[test]
    fn a_chord_another_app_owns_is_reported_and_the_rest_still_register() {
        let mut backend = FakeHotkeys {
            taken: vec!["Ctrl+Shift+Space".into()],
            ..FakeHotkeys::default()
        };
        let problems = register_all(&mut backend, &all_on());
        assert_eq!(
            problems,
            [HotkeyProblem {
                action: HotkeyAction::Picker,
                chord: "Ctrl+Shift+Space".into(),
                failure: HotkeyFailure::InUse,
            }]
        );
        assert_eq!(backend.registered.len(), 3);
    }

    #[test]
    fn a_hotkey_that_is_off_is_skipped() {
        let mut backend = FakeHotkeys::default();
        let hotkeys = Hotkeys {
            capture: String::new(),
            ..Hotkeys::new_install()
        };
        assert_eq!(register_all(&mut backend, &hotkeys), Vec::new());
        assert_eq!(backend.registered.len(), 3);
        assert!(
            backend
                .registered
                .iter()
                .all(|(id, _)| *id != id_of(HotkeyAction::Capture))
        );
    }

    #[test]
    fn pausing_removes_every_hotkey_and_resuming_registers_them_again() {
        let mut backend = FakeHotkeys::default();
        let hotkeys = all_on();
        assert_eq!(register_all(&mut backend, &hotkeys), Vec::new());
        assert_eq!(unregister_all(&mut backend), 0);
        assert_eq!(backend.registered, Vec::new());
        assert_eq!(unregister_all(&mut backend), 0, "pausing twice is fine");
        assert_eq!(register_all(&mut backend, &hotkeys), Vec::new());
        assert_eq!(backend.registered.len(), 4);
    }

    #[test]
    fn resuming_registers_the_chords_in_force_now_not_the_ones_from_start_up() {
        let mut backend = FakeHotkeys::default();
        let at_start = all_on();
        assert_eq!(register_all(&mut backend, &at_start), Vec::new());
        assert_eq!(
            set_paused(&mut backend, true, &at_start),
            PauseReport::default()
        );
        assert_eq!(backend.registered, Vec::new());

        // While paused the settings changed: the Picker moved, Capture was switched off.
        let now = Hotkeys {
            picker: "Ctrl+Alt+P".into(),
            capture: String::new(),
            ..Hotkeys::new_install()
        };
        assert_eq!(
            set_paused(&mut backend, false, &now),
            PauseReport::default()
        );
        let picker = backend
            .registered
            .iter()
            .find(|(id, _)| *id == id_of(HotkeyAction::Picker))
            .expect("picker");
        assert_eq!(picker.1.to_string(), "Ctrl+Alt+P");
        assert_eq!(backend.registered.len(), 3, "capture stays off");
    }

    #[test]
    fn resuming_reports_a_chord_another_app_took_while_paused() {
        let mut backend = FakeHotkeys::default();
        let hotkeys = Hotkeys::new_install();
        let _ = set_paused(&mut backend, true, &hotkeys);
        backend.taken.push("Ctrl+Shift+Y".into());
        let report = set_paused(&mut backend, false, &hotkeys);
        assert_eq!(report.problems.len(), 1);
        assert_eq!(report.problems[0].action, HotkeyAction::TogglePanel);
        assert_eq!(report.not_removed, 0);
    }

    #[test]
    fn a_fake_target_source_counts_refocus_calls_through_clones() {
        let source = FakeTargets::new(None);
        let handle = source.clone();
        assert!(!source.refocus(), "nothing to refocus");
        assert_eq!(handle.refocus_count(), 1);
        assert_eq!(source.current(), None);
    }

    #[test]
    fn the_fixed_clock_moves_only_when_set() {
        let clock = FixedClock::new("2026-10-17T09:00:00Z".parse().expect("ts"));
        let other = clock.clone();
        assert_eq!(clock.now().to_string(), "2026-10-17T09:00:00Z");
        other.set("2026-10-18T09:00:00Z".parse().expect("ts"));
        assert_eq!(clock.now().to_string(), "2026-10-18T09:00:00Z");
    }
}
