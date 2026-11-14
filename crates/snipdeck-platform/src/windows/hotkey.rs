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
//! settings dialog leaves the working hotkey in place).
//!
//! **Unverified on Windows.** Covered by the M2 wizard step in `tools/windows-test/`.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use windows::Win32::Foundation::{
    ERROR_HOTKEY_ALREADY_REGISTERED, ERROR_INVALID_PARAMETER, LPARAM, WPARAM,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_APP, WM_HOTKEY, WM_QUIT,
    WM_USER,
};

use crate::chord::Chord;
use crate::error::{Error, win32_code};

/// Wakes the loop to drain the command channel.
const WM_COMMANDS: u32 = WM_APP + 1;

/// Hotkey ids not tied to a window must be in `0x0000..=0xBFFF` (`RegisterHotKey` docs).
pub const MAX_ID: u32 = 0xBFFF;

/// How long a caller waits for the hotkey thread to answer before reporting it unresponsive.
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// Why a hotkey could not be registered or removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HotkeyError {
    /// Another application (or Windows itself) already owns the chord
    /// (`ERROR_HOTKEY_ALREADY_REGISTERED`). Show it as a conflict.
    #[error("another application already uses this shortcut")]
    InUse,
    /// The chord or id is not registrable (failed [`Chord::validate`], id above [`MAX_ID`], or
    /// refused by Windows as invalid).
    #[error("this shortcut cannot be used")]
    Invalid,
    /// Windows refused for another reason; the Win32 error code.
    #[error("Windows refused the shortcut (OS error {0:#010x})")]
    Os(u32),
    /// The hotkey thread has stopped or did not answer in time.
    #[error("the hotkey service is not running")]
    Stopped,
}

impl HotkeyError {
    fn from_windows(error: &windows::core::Error) -> Self {
        let code = win32_code(error.code().0);
        if code == ERROR_HOTKEY_ALREADY_REGISTERED.0 {
            Self::InUse
        } else if code == ERROR_INVALID_PARAMETER.0 {
            Self::Invalid
        } else {
            Self::Os(code)
        }
    }
}

enum Command {
    Register {
        id: u32,
        chord: Chord,
        reply: Sender<Result<(), HotkeyError>>,
    },
    Unregister {
        id: u32,
        reply: Sender<Result<(), HotkeyError>>,
    },
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
        self.call(|reply| Command::Register { id, chord, reply })
    }

    /// Removes the hotkey `id`. Removing an id that is not registered succeeds.
    ///
    /// # Errors
    ///
    /// [`HotkeyError::Os`] if Windows refuses; [`HotkeyError::Stopped`] if the thread is gone.
    pub fn unregister(&self, id: u32) -> Result<(), HotkeyError> {
        self.call(|reply| Command::Unregister { id, reply })
    }

    fn call(
        &self,
        make: impl FnOnce(Sender<Result<(), HotkeyError>>) -> Command,
    ) -> Result<(), HotkeyError> {
        let (reply_tx, reply_rx): (_, Receiver<Result<(), HotkeyError>>) = mpsc::channel();
        self.commands
            .send(make(reply_tx))
            .map_err(|_| HotkeyError::Stopped)?;
        // SAFETY: plain thread-message post to the hotkey thread, whose queue was created before
        // `start` returned; the message carries no pointers.
        unsafe { PostThreadMessageW(self.thread_id, WM_COMMANDS, WPARAM(0), LPARAM(0)) }
            .map_err(|_| HotkeyError::Stopped)?;
        reply_rx
            .recv_timeout(REPLY_TIMEOUT)
            .map_err(|_| HotkeyError::Stopped)?
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
        if posted || thread.is_finished() {
            let _ = thread.join();
        }
        // Otherwise the thread could not be told to quit; detach rather than hang the app.
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

    let mut registered: HashMap<u32, Chord> = HashMap::new();
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
                    serve(&mut registered, command);
                }
            }
            _ => {}
        }
    }
    for id in registered.into_keys() {
        // SAFETY: unregisters a hotkey this thread registered with a null window, from the same
        // thread. A failure at shutdown (the thread ends right after) is not actionable.
        let _ = unsafe { UnregisterHotKey(None, hotkey_id(id)) };
    }
}

fn serve(registered: &mut HashMap<u32, Chord>, command: Command) {
    // A dropped reply receiver (the caller timed out) is not an error here.
    match command {
        Command::Register { id, chord, reply } => {
            let _ = reply.send(do_register(registered, id, chord));
        }
        Command::Unregister { id, reply } => {
            let _ = reply.send(do_unregister(registered, id));
        }
    }
}

/// `id` as the `i32` the API takes; callers have checked `id <= MAX_ID`.
fn hotkey_id(id: u32) -> i32 {
    i32::try_from(id).unwrap_or(i32::MAX)
}

fn register_raw(id: u32, chord: Chord) -> Result<(), HotkeyError> {
    let (modifiers, vk) = chord.to_win32();
    if vk == 0 {
        return Err(HotkeyError::Invalid);
    }
    // SAFETY: registers a hotkey for the calling (hotkey) thread with no window; all arguments are
    // plain values. `modifiers` includes `MOD_NOREPEAT`.
    unsafe { RegisterHotKey(None, hotkey_id(id), HOT_KEY_MODIFIERS(modifiers), vk) }
        .map_err(|e| HotkeyError::from_windows(&e))
}

fn unregister_raw(id: u32) -> Result<(), HotkeyError> {
    // SAFETY: unregisters a hotkey registered by this thread with no window; plain values.
    unsafe { UnregisterHotKey(None, hotkey_id(id)) }.map_err(|e| HotkeyError::from_windows(&e))
}

fn do_register(
    registered: &mut HashMap<u32, Chord>,
    id: u32,
    chord: Chord,
) -> Result<(), HotkeyError> {
    if id > MAX_ID || chord.validate().is_err() {
        return Err(HotkeyError::Invalid);
    }
    // Settings changes re-apply every chord: one already in place stays untouched instead of
    // being freed and registered again (a failure in between lost the hotkey).
    if registered.get(&id) == Some(&chord) {
        return Ok(());
    }
    let previous = registered.remove(&id);
    if previous.is_some() {
        // Free the id (and the old chord) first: the same chord again must succeed.
        // Unverified on Windows (Fable v1 sweep A17): if freeing fails the hotkey is still
        // registered, so it stays in the table.
        if let Err(e) = unregister_raw(id) {
            if let Some(old) = previous {
                registered.insert(id, old);
            }
            return Err(e);
        }
    }
    match register_raw(id, chord) {
        Ok(()) => {
            registered.insert(id, chord);
            Ok(())
        }
        Err(error) => {
            if let Some(old) = previous {
                // Put the working hotkey back; if even that fails it is simply no longer registered.
                if register_raw(id, old).is_ok() {
                    registered.insert(id, old);
                }
            }
            Err(error)
        }
    }
}

fn do_unregister(registered: &mut HashMap<u32, Chord>, id: u32) -> Result<(), HotkeyError> {
    if registered.remove(&id).is_none() {
        return Ok(());
    }
    unregister_raw(id)
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
