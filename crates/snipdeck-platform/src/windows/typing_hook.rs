//! `TypingHook`: a low-level keyboard hook (`WH_KEYBOARD_LL`) for typed-trigger expansion.
//!
//! The hook is installed only while the feature is on, on its own thread with its own message
//! loop, so a slow callback can never delay the hotkey thread ([`super::hotkey`]). The callback
//! does the minimum: skip injected input (our own Backspaces and paste must not feed the
//! matcher), translate the key with `ToUnicodeEx` on the foreground layout with flag `0x4` (the
//! keyboard state, dead keys included, is left untouched), classify it
//! ([`crate::typing::classify`]) and post the [`TypedEvent`] to a channel. Windows removes hooks
//! that take too long, so nothing else happens there: no matching, no UI Automation, no locks.
//!
//! A change of foreground window posts a [`TypedEvent::Reset`] first. There is no mouse hook.
//!
//! Typed characters pass through the channel and are never logged or printed ([`TypedEvent`]'s
//! `Debug` hides them). `stop` (or drop) unhooks and joins the thread.

use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::thread::JoinHandle;
use std::time::Duration;

use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, GetKeyboardLayout, ToUnicodeEx, VK_CAPITAL, VK_CONTROL, VK_MENU,
    VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, HHOOK,
    KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN,
    WM_USER,
};

use crate::error::Error;
use crate::typing::{self, Classifier, Stroke, Translation, TypedEvent};
use zeroize::Zeroize;

// The portable table uses plain numbers; make sure they are Windows' numbers.
const _: () = {
    assert!(typing::VK_SHIFT == VK_SHIFT.0);
    assert!(typing::VK_CONTROL == VK_CONTROL.0);
    assert!(typing::VK_MENU == VK_MENU.0);
    assert!(typing::VK_CAPITAL == VK_CAPITAL.0);
};

/// `ToUnicodeEx` flag: do not change the keyboard state (dead keys stay pending). Windows 10 1607+.
const KEEP_KEYBOARD_STATE: u32 = 0x4;
/// Events the channel holds before the hook starts dropping (and resetting the matcher).
const CHANNEL_SLOTS: usize = 256;
const STOP_RETRY: Duration = Duration::from_millis(10);

/// What the hook thread keeps between callbacks (the callback has no user-data pointer).
struct Hooked {
    events: SyncSender<TypedEvent>,
    foreground: isize,
    classifier: Classifier,
    /// An event was dropped because the consumer is behind: send a Reset before the next one.
    dropped: bool,
}

thread_local! {
    static HOOKED: RefCell<Option<Hooked>> = const { RefCell::new(None) };
}

/// A running hook. Dropping it stops it.
#[derive(Debug)]
pub struct TypingHook {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl TypingHook {
    /// Installs the hook on a new thread. The receiver gets every [`TypedEvent`] in order.
    ///
    /// # Errors
    ///
    /// [`Error::Os`] if the hook could not be installed, [`Error::Unavailable`] if the thread
    /// could not start.
    pub fn start() -> Result<(Self, Receiver<TypedEvent>), Error> {
        let (events, receiver) = mpsc::sync_channel(CHANNEL_SLOTS);
        let (ready_tx, ready) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("snipdeck-typing-hook".into())
            .spawn(move || run(events, &ready_tx))
            .map_err(|_| Error::Unavailable("could not start the typing hook thread"))?;
        match ready.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(thread_id)) => Ok((
                Self {
                    thread_id,
                    thread: Some(thread),
                },
                receiver,
            )),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => Err(Error::Unavailable("the typing hook thread did not start")),
        }
    }

    /// Unhooks and joins the thread.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        // The queue exists before `start` returns, so the post only fails if the thread is gone.
        // Never detach with the hook installed: retry until it is posted or the thread ended.
        loop {
            // SAFETY: plain thread-message post; no pointers.
            let posted =
                unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) }
                    .is_ok();
            if posted || thread.is_finished() {
                break;
            }
            std::thread::sleep(STOP_RETRY);
        }
        let _ = thread.join();
    }
}

impl Drop for TypingHook {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run(events: SyncSender<TypedEvent>, ready: &Sender<Result<u32, Error>>) {
    let mut msg = MSG::default();
    // SAFETY: `msg` is a valid out-pointer; peeking a message nobody posts creates the queue so
    // `PostThreadMessageW` from `stop` cannot fail with "no queue".
    let _ = unsafe { PeekMessageW(&raw mut msg, None, WM_USER, WM_USER, PM_NOREMOVE) };
    // SAFETY: no preconditions.
    let thread_id = unsafe { GetCurrentThreadId() };
    HOOKED.with(|state| {
        *state.borrow_mut() = Some(Hooked {
            events,
            foreground: 0,
            classifier: Classifier::new(),
            dropped: false,
        });
    });
    let hook = install();
    let hook = match hook {
        Ok(hook) => hook,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    if ready.send(Ok(thread_id)).is_err() {
        // SAFETY: `hook` was returned by `SetWindowsHookExW` on this thread.
        let _ = unsafe { UnhookWindowsHookEx(hook) };
        return;
    }
    loop {
        // SAFETY: valid out-pointer; the low-level hook is called from inside this call.
        let got = unsafe { GetMessageW(&raw mut msg, None, 0, 0) };
        if got.0 <= 0 {
            break;
        }
    }
    // SAFETY: `hook` was returned by `SetWindowsHookExW` on this thread.
    let _ = unsafe { UnhookWindowsHookEx(hook) };
    HOOKED.with(|state| *state.borrow_mut() = None);
}

fn install() -> Result<HHOOK, Error> {
    // SAFETY: no preconditions.
    let module = unsafe { GetModuleHandleW(None) }
        .map_err(|error| Error::from_windows("GetModuleHandleW", &error))?;
    // SAFETY: `keyboard_proc` has the `HOOKPROC` signature and lives for the whole program; a
    // low-level hook needs a module handle but no DLL.
    unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(keyboard_proc),
            Some(HINSTANCE(module.0)),
            0,
        )
    }
    .map_err(|error| Error::from_windows("SetWindowsHookExW", &error))
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // A panic must not unwind across the FFI boundary; the event is simply lost.
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if code >= 0 && matches!(u32::try_from(wparam.0), Ok(WM_KEYDOWN | WM_SYSKEYDOWN)) {
            // SAFETY: for a `WH_KEYBOARD_LL` call with `code >= 0`, `lparam` points to a
            // `KBDLLHOOKSTRUCT` that is valid for the duration of the call.
            let info = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };
            handle(&info);
        }
    }));
    // SAFETY: passes the unchanged arguments on; the hook handle argument is ignored by Windows.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn handle(info: &KBDLLHOOKSTRUCT) {
    if info.flags.0 & LLKHF_INJECTED.0 != 0 {
        return;
    }
    // SAFETY: no preconditions.
    let foreground = unsafe { GetForegroundWindow() };
    let stroke = stroke_of(info, foreground);
    HOOKED.with(|state| {
        let mut guard = state.borrow_mut();
        let Some(hooked) = guard.as_mut() else {
            return;
        };
        let mut pending = [None, None, None];
        if hooked.dropped {
            pending[0] = Some(TypedEvent::Reset);
        }
        if hooked.foreground != foreground.0 as isize {
            if hooked.foreground != 0 {
                pending[1] = Some(TypedEvent::Reset);
            }
            hooked.foreground = foreground.0 as isize;
        }
        pending[2] = hooked.classifier.classify(stroke);
        for event in pending.into_iter().flatten() {
            hooked.dropped = hooked.events.try_send(event).is_err();
            if hooked.dropped {
                break;
            }
        }
    });
}

fn async_down(vk: u16) -> bool {
    // SAFETY: `GetAsyncKeyState` takes a plain integer and has no preconditions.
    (unsafe { GetAsyncKeyState(i32::from(vk)) }) < 0
}

fn stroke_of(info: &KBDLLHOOKSTRUCT, foreground: windows::Win32::Foundation::HWND) -> Stroke {
    let vk = u16::try_from(info.vkCode).unwrap_or(0);
    let shift = async_down(VK_SHIFT.0);
    let ctrl = async_down(VK_CONTROL.0);
    let alt = async_down(VK_MENU.0);
    let win = async_down(typing::VK_LWIN) || async_down(typing::VK_RWIN);
    // The thread's keyboard state is not maintained for a low-level hook: build it.
    let mut state = [0u8; 256];
    if shift {
        state[usize::from(VK_SHIFT.0)] = 0x80;
    }
    if ctrl {
        state[usize::from(VK_CONTROL.0)] = 0x80;
    }
    if alt {
        state[usize::from(VK_MENU.0)] = 0x80;
    }
    // SAFETY: no preconditions; bit 0 is the toggle state.
    if unsafe { GetKeyState(i32::from(VK_CAPITAL.0)) } & 1 != 0 {
        state[usize::from(VK_CAPITAL.0)] = 0x01;
    }
    // Only a key that may type text is translated; the rest are classified by `vk` alone.
    let text = if vk == 0 || (ctrl != alt) || win {
        Translation::Nothing
    } else {
        translate(info, &state, foreground)
    };
    Stroke {
        vk,
        ctrl,
        alt,
        win,
        text,
    }
}

fn translate(
    info: &KBDLLHOOKSTRUCT,
    state: &[u8; 256],
    foreground: windows::Win32::Foundation::HWND,
) -> Translation {
    // SAFETY: a null or dead window gives thread 0, which selects the hook thread's own layout
    // (a fallback: it may differ from the user's).
    let thread = unsafe { GetWindowThreadProcessId(foreground, None) };
    // SAFETY: no preconditions.
    let layout = unsafe { GetKeyboardLayout(thread) };
    // Typed text: wiped after use.
    let mut out = [0u16; 8];
    // SAFETY: `state` is the 256-byte array the call requires and `out` is a live buffer.
    let written = unsafe {
        ToUnicodeEx(
            info.vkCode,
            info.scanCode,
            state,
            &mut out,
            KEEP_KEYBOARD_STATE,
            Some(layout),
        )
    };
    let translation = match written {
        n if n < 0 => Translation::Dead,
        0 => Translation::Nothing,
        n => {
            let units = &out[..usize::try_from(n).unwrap_or(0).min(out.len())];
            let mut chars = char::decode_utf16(units.iter().copied());
            match (chars.next(), chars.next()) {
                (Some(Ok(c)), None) => Translation::Text(c),
                _ => Translation::Nothing,
            }
        }
    };
    out.zeroize();
    translation
}
