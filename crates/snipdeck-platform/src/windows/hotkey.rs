//! Global hotkeys on a dedicated thread.
//!
//! `RegisterHotKey` with no window ties the hotkey to the *calling thread's* message queue, and
//! `WM_HOTKEY` arrives there. So [`HotkeyService`] owns one thread that does nothing but run a
//! message loop, and [`register`](HotkeyService::register) / [`unregister`](HotkeyService::unregister)
//! hand it a command (a channel send plus a thread message to wake the loop) and wait for the
//! answer. The UI thread is never blocked on the OS beyond that round trip.
//!
//! **Delivery.** A hotkey press calls the `on_hotkey(id)` callback given to
//! [`HotkeyService::start`], *on the hotkey thread*. Chosen over a channel receiver so the app can
//! forward straight into the UI event loop (`slint::invoke_from_event_loop`) without a polling
//! thread. The callback must return quickly (it blocks further hotkeys and commands); a panic in
//! it is caught and ignored. A press queued just before [`unregister`](HotkeyService::unregister)
//! can still be delivered once after it returns.
//!
//! Every registration uses `MOD_NOREPEAT` (see [`Chord::to_win32`]). Registering an id that is
//! already registered *replaces* it atomically from the caller's point of view: if the new chord
//! cannot be registered, the old one is put back and the error returned (a failed rebind in the
//! settings dialog leaves the working hotkey in place). The rules (own-action conflicts, the
//! rollback, probing) are the portable [`HotkeyTable`], tested with a fake OS; this file only
//! owns the thread and the two Win32 calls.
//!
//! **A call that times out does not run later.** Each command carries a [`Ticket`]; a caller that
//! gave up cancels it and the thread skips the command, so the app's view ("stopped") and the
//! thread's table cannot differ. Dropping the service waits a bounded time for the thread.
//!
//! **Unverified on Windows.** Covered by the M2 wizard step in `tools/windows-test/`.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use windows::Win32::Foundation::{
    ERROR_HOTKEY_ALREADY_REGISTERED, ERROR_INVALID_PARAMETER, LPARAM, WPARAM,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayout, HOT_KEY_MODIFIERS, MAPVK_VK_TO_CHAR, MapVirtualKeyExW, RegisterHotKey,
    UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_APP, WM_HOTKEY, WM_QUIT,
    WM_USER,
};

use crate::chord::Chord;
use crate::error::{Error, win32_code};
pub use crate::hotkey_table::{HotkeyError, MAX_ID};
use crate::hotkey_table::{HotkeyTable, RawHotkeys, Ticket, await_reply, wait_until};

/// Wakes the loop to drain the command channel.
const WM_COMMANDS: u32 = WM_APP + 1;

/// How long a caller waits for the hotkey thread to answer before reporting it unresponsive.
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// How much longer a caller waits for a command the thread had already started when the timeout
/// ran out (its answer is on the way, and the caller must not report a command that ran as
/// failed).
const STARTED_GRACE: Duration = Duration::from_secs(2);

/// How long dropping the service waits for the thread to end: 300 polls, 10 ms apart.
const JOIN_POLLS: u32 = 300;

/// The Win32 error behind a failed call, as the service reports it. Windows' own words for a
/// code are not used: the code goes into the warning so it can be looked up.
fn from_windows(error: &windows::core::Error) -> HotkeyError {
    let code = win32_code(error.code().0);
    if code == ERROR_HOTKEY_ALREADY_REGISTERED.0 {
        HotkeyError::InUse
    } else if code == ERROR_INVALID_PARAMETER.0 {
        HotkeyError::Invalid
    } else {
        HotkeyError::Os(code)
    }
}

/// What a caller asks of the thread.
enum Request {
    Register { id: u32, chord: Chord },
    Unregister { id: u32 },
    Probe { chord: Chord },
}

/// A request, its claim ticket and where the answer goes.
struct Command {
    request: Request,
    ticket: Arc<Ticket>,
    reply: Sender<Result<(), HotkeyError>>,
}

/// Owns the hotkey thread. Dropping it unregisters every hotkey, stops the thread and joins it.
pub struct HotkeyService {
    thread_id: u32,
    commands: Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for HotkeyService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HotkeyService").finish_non_exhaustive()
    }
}

impl HotkeyService {
    /// Starts the hotkey thread. `on_hotkey` is called with the registered id on every press.
    ///
    /// # Errors
    ///
    /// [`Error::Unavailable`] if the thread cannot be started.
    pub fn start(on_hotkey: impl Fn(u32) + Send + 'static) -> Result<Self, Error> {
        let (command_tx, command_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("snipdeck-hotkeys".into())
            .spawn(move || run(&command_rx, &ready_tx, &on_hotkey))
            .map_err(|_| Error::Unavailable("could not start the hotkey thread"))?;
        let Ok(thread_id) = ready_rx.recv() else {
            let _ = thread.join();
            return Err(Error::Unavailable(
                "the hotkey thread stopped while starting",
            ));
        };
        Ok(Self {
            thread_id,
            commands: command_tx,
            thread: Some(thread),
        })
    }

    /// Registers `chord` under `id` (replacing the chord already registered under `id`, if any).
    ///
    /// # Errors
    ///
    /// [`HotkeyError::InUse`] when another application owns the chord; [`HotkeyError::Invalid`]
    /// for an invalid chord or an `id` above [`MAX_ID`]; [`HotkeyError::Os`] for any other
    /// Windows refusal; [`HotkeyError::Stopped`] if the thread is gone. On error the previous
    /// registration of `id`, if any, is still in place.
    pub fn register(&self, id: u32, chord: Chord) -> Result<(), HotkeyError> {
        self.call(Request::Register { id, chord })
    }

    /// Removes the hotkey `id`. Removing an id that is not registered succeeds.
    ///
    /// # Errors
    ///
    /// [`HotkeyError::Os`] if Windows refuses; [`HotkeyError::Stopped`] if the thread is gone.
    pub fn unregister(&self, id: u32) -> Result<(), HotkeyError> {
        self.call(Request::Unregister { id })
    }

    /// Whether `chord` could be registered right now: registers it for a moment on the hotkey
    /// thread and takes it away again, leaving every real hotkey as it is. Used to offer only
    /// shortcuts that are free.
    ///
    /// # Errors
    ///
    /// As [`register`](Self::register): [`HotkeyError::InUse`] when another program holds the
    /// chord, [`HotkeyError::OwnAction`] when another Snipdeck action does.
    pub fn probe(&self, chord: Chord) -> Result<(), HotkeyError> {
        self.call(Request::Probe { chord })
    }

    fn call(&self, request: Request) -> Result<(), HotkeyError> {
        let (reply_tx, reply_rx): (_, Receiver<Result<(), HotkeyError>>) = mpsc::channel();
        let ticket = Arc::new(Ticket::default());
        let command = Command {
            request,
            ticket: Arc::clone(&ticket),
            reply: reply_tx,
        };
        if self.commands.send(command).is_err() {
            return Err(HotkeyError::Stopped);
        }
        // SAFETY: plain thread-message post to the hotkey thread, whose queue was created before
        // `start` returned; the message carries no pointers.
        let posted =
            unsafe { PostThreadMessageW(self.thread_id, WM_COMMANDS, WPARAM(0), LPARAM(0)) };
        if posted.is_err() {
            // The command is queued but nobody will wake the thread for it: it must not run
            // later, when some other command does.
            let _ = ticket.cancel();
            return Err(HotkeyError::Stopped);
        }
        await_reply(&reply_rx, &ticket, REPLY_TIMEOUT, STARTED_GRACE)
            .unwrap_or(Err(HotkeyError::Stopped))
    }
}

impl Drop for HotkeyService {
    fn drop(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        // A thread message posted with `PostThreadMessageW` is first in, first out like any other,
        // so commands sent before the drop are answered first. Retry briefly in case the queue is
        // full.
        let mut posted = false;
        for _ in 0..100 {
            // SAFETY: plain thread-message post; no pointers.
            posted = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) }
                .is_ok();
            if posted || thread.is_finished() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        // Wait a bounded time (a callback stuck in the app must not hang the UI thread at exit);
        // a thread that does not end is detached.
        if (posted || thread.is_finished())
            && wait_until(
                || thread.is_finished(),
                || std::thread::sleep(Duration::from_millis(10)),
                JOIN_POLLS,
            )
        {
            let _ = thread.join();
        }
        // Otherwise the thread could not be told to quit, or did not end in time; detach rather
        // than hang the app.
    }
}

/// The hotkey thread: creates its queue, reports its id, then serves commands and `WM_HOTKEY`.
/// Contains no `unwrap`, `expect` or indexing, and survives a panicking callback.
fn run(commands: &Receiver<Command>, ready: &Sender<u32>, on_hotkey: &dyn Fn(u32)) {
    let mut msg = MSG::default();
    // SAFETY: `msg` is a valid out-pointer. Peeking (without removing) a message in a range nobody
    // posts forces the system to create this thread's message queue, so that
    // `PostThreadMessageW` from other threads cannot fail with "no queue".
    let _ = unsafe { PeekMessageW(&raw mut msg, None, WM_USER, WM_USER, PM_NOREMOVE) };
    // SAFETY: no preconditions.
    let thread_id = unsafe { GetCurrentThreadId() };
    if ready.send(thread_id).is_err() {
        return;
    }

    let mut table = HotkeyTable::new(Win32Hotkeys);
    loop {
        // SAFETY: `msg` is a valid out-pointer; no window filter, so thread messages (including
        // `WM_HOTKEY` for hotkeys registered without a window) are returned.
        let got = unsafe { GetMessageW(&raw mut msg, None, 0, 0) };
        // 0 is WM_QUIT, -1 is an error; neither can be recovered from.
        if got.0 <= 0 {
            break;
        }
        match msg.message {
            WM_HOTKEY => {
                if let Ok(id) = u32::try_from(msg.wParam.0) {
                    let _ = catch_unwind(AssertUnwindSafe(|| on_hotkey(id)));
                }
            }
            WM_COMMANDS => {
                while let Ok(command) = commands.try_recv() {
                    serve(&mut table, &command);
                }
            }
            _ => {}
        }
    }
    // A failure at shutdown (the thread ends right after) is not actionable.
    table.clear();
}

fn serve(table: &mut HotkeyTable<Win32Hotkeys>, command: &Command) {
    // A caller that gave up has cancelled the ticket: the command must not run now.
    if !command.ticket.start() {
        return;
    }
    let result = match command.request {
        Request::Register { id, chord } => table.register(id, chord),
        Request::Unregister { id } => table.unregister(id),
        Request::Probe { chord } => table.probe(chord),
    };
    // A dropped reply receiver is not an error here.
    let _ = command.reply.send(result);
}

/// `id` as the `i32` the API takes; callers have checked `id <= MAX_ID` (the table's own probe
/// id is just above it, still in range).
fn hotkey_id(id: u32) -> i32 {
    i32::try_from(id).unwrap_or(i32::MAX)
}

/// The two Win32 calls, for the table.
struct Win32Hotkeys;

impl RawHotkeys for Win32Hotkeys {
    fn register(&mut self, id: u32, chord: Chord) -> Result<(), HotkeyError> {
        let (modifiers, vk) = chord.to_win32();
        if vk == 0 {
            return Err(HotkeyError::Invalid);
        }
        // SAFETY: registers a hotkey for the calling (hotkey) thread with no window; all
        // arguments are plain values. `modifiers` includes `MOD_NOREPEAT`.
        unsafe { RegisterHotKey(None, hotkey_id(id), HOT_KEY_MODIFIERS(modifiers), vk) }
            .map_err(|e| from_windows(&e))
    }

    fn unregister(&mut self, id: u32) -> Result<(), HotkeyError> {
        // SAFETY: unregisters a hotkey registered by this thread with no window; plain values.
        unsafe { UnregisterHotKey(None, hotkey_id(id)) }.map_err(|e| from_windows(&e))
    }
}

/// The character the current keyboard layout prints on the key behind `vk`
/// (`MapVirtualKeyExW(MAPVK_VK_TO_CHAR)` for the calling thread's layout), for labelling
/// punctuation keys as the user's keyboard shows them (PLATFORM-9). Call it on the UI thread, at
/// display time: the layout can change while the app runs. A dead key (an accent that waits for
/// the next letter) comes back with the top bit set; the bit is dropped, the character kept.
/// **Unverified on Windows.**
#[must_use]
pub fn layout_char(vk: u32) -> Option<char> {
    // SAFETY: `GetKeyboardLayout(0)` is the calling thread's layout and has no preconditions;
    // `MapVirtualKeyExW` takes plain values and that layout handle.
    let mapped = unsafe {
        let layout = GetKeyboardLayout(0);
        MapVirtualKeyExW(vk, MAPVK_VK_TO_CHAR, Some(layout))
    };
    char::from_u32(mapped & 0x7FFF_FFFF).filter(|c| *c != '\0')
}

/// Makes [`Chord::label`] show punctuation keys as the current layout prints them.
pub fn show_layout_labels() {
    crate::chord::set_layout_lookup(Some(layout_char));
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    fn chord(s: &str) -> Chord {
        s.parse().unwrap()
    }

    fn service() -> HotkeyService {
        HotkeyService::start(|_| {}).unwrap()
    }

    #[test]
    fn the_layout_gives_a_printable_character_for_a_letter_key() {
        // Whatever the layout, VK_A is the letter key `a` (upper or lower case varies by
        // Windows version); this only proves the call works and the dead-key bit is dropped.
        let c = layout_char(0x41).unwrap();
        assert!(c.eq_ignore_ascii_case(&'a') || c.is_alphabetic(), "{c:?}");
        assert_eq!(layout_char(0xFF), None);
    }

    #[test]
    fn registers_unregisters_and_rebinds() {
        let svc = service();
        assert_eq!(svc.register(1, chord("Ctrl+Alt+Shift+F23")), Ok(()));
        // Same id again with another chord replaces it; the first chord is free again.
        assert_eq!(svc.register(1, chord("Ctrl+Alt+Shift+F22")), Ok(()));
        assert_eq!(svc.register(2, chord("Ctrl+Alt+Shift+F23")), Ok(()));
        assert_eq!(svc.unregister(1), Ok(()));
        assert_eq!(svc.unregister(1), Ok(()));
        assert_eq!(svc.unregister(2), Ok(()));
    }

    #[test]
    fn the_same_chord_twice_is_in_use_and_keeps_the_old_binding() {
        let a = service();
        let b = service();
        assert_eq!(a.register(1, chord("Ctrl+Alt+Shift+F21")), Ok(()));
        assert_eq!(
            b.register(1, chord("Ctrl+Alt+Shift+F21")),
            Err(HotkeyError::InUse)
        );
        // A failed rebind leaves the working hotkey registered.
        assert_eq!(a.register(2, chord("Ctrl+Alt+Shift+F20")), Ok(()));
        assert_eq!(
            a.register(1, chord("Ctrl+Alt+Shift+F20")),
            Err(HotkeyError::InUse)
        );
        assert_eq!(
            b.register(3, chord("Ctrl+Alt+Shift+F21")),
            Err(HotkeyError::InUse)
        );
    }

    #[test]
    fn a_probe_sees_a_taken_chord_and_leaves_nothing_behind() {
        let a = service();
        let b = service();
        assert_eq!(a.register(1, chord("Ctrl+Alt+Shift+F17")), Ok(()));
        assert_eq!(
            b.probe(chord("Ctrl+Alt+Shift+F17")),
            Err(HotkeyError::InUse)
        );
        assert_eq!(b.probe(chord("Ctrl+Alt+Shift+F16")), Ok(()));
        // Probed twice: the first probe did not keep the chord.
        assert_eq!(b.probe(chord("Ctrl+Alt+Shift+F16")), Ok(()));
        assert_eq!(b.register(1, chord("Ctrl+Alt+Shift+F16")), Ok(()));
        assert_eq!(
            b.register(2, chord("Ctrl+Alt+Shift+F16")),
            Err(HotkeyError::OwnAction(1))
        );
    }

    #[test]
    fn invalid_ids_and_chords_are_refused_without_calling_windows() {
        let svc = service();
        assert_eq!(
            svc.register(MAX_ID + 1, chord("Ctrl+Alt+Shift+F19")),
            Err(HotkeyError::Invalid)
        );
        let bad = Chord {
            modifiers: crate::keys::Modifiers::default(),
            key: crate::chord::ChordKey::Letter(b'V'),
        };
        assert_eq!(svc.register(1, bad), Err(HotkeyError::Invalid));
    }

    #[test]
    fn drop_stops_the_thread_and_frees_the_chord() {
        let counter = Arc::new(AtomicU32::new(0));
        let seen = Arc::clone(&counter);
        let svc = HotkeyService::start(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
        assert_eq!(svc.register(1, chord("Ctrl+Alt+Shift+F18")), Ok(()));
        drop(svc);
        let again = service();
        assert_eq!(again.register(1, chord("Ctrl+Alt+Shift+F18")), Ok(()));
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }
}
