//! A hidden top-level window on its own thread: the receiver for the tray icon's callbacks and
//! for the system-wide broadcasts (`TaskbarCreated`, `WM_SETTINGCHANGE`, `WM_THEMECHANGED`).
//!
//! **Why not a message-only window** (`HWND_MESSAGE`, as ADR-0008 first said). Message-only
//! windows are not top-level windows, and broadcasts (`HWND_BROADCAST`, which both
//! `TaskbarCreated` and `WM_SETTINGCHANGE` use) are delivered to top-level windows only. So the
//! window here is an ordinary `WS_POPUP` tool window that is never shown: it has no taskbar
//! button, no Alt+Tab entry and no visible effect, but it hears the broadcasts.
//!
//! The thread creates the window, runs a message loop, and on drop posts `WM_CLOSE` and joins.
//! A [`WindowHandler`] gets every message through `&self`: a modal loop (a popup menu) inside one
//! message dispatches further messages into the same handler, so handlers keep their mutable
//! state in `Cell` / `RefCell` and never hold a borrow across a call that can pump messages.
//!
//! **Unverified on Windows.**

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;
use std::thread::JoinHandle;

use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetMessageW,
    GetWindowLongPtrW, MSG, PostMessageW, PostQuitMessage, RegisterClassExW, SetWindowLongPtrW,
    TranslateMessage, WM_CLOSE, WM_DESTROY, WM_NCCREATE, WM_NCDESTROY, WNDCLASSEXW,
    WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::{HSTRING, PCWSTR, w};

use crate::error::Error;

/// What a hidden window does with its messages. See the module documentation for the rules.
pub(super) trait WindowHandler: 'static {
    /// Handles one message. `Some(result)` ends it; `None` passes it to `DefWindowProcW`.
    fn message(&self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT>;
}

/// The window and its thread. Dropping closes the window (the handler sees `WM_DESTROY` first)
/// and joins the thread.
pub(super) struct HiddenWindow {
    hwnd: isize,
    thread: Option<JoinHandle<()>>,
}

impl HiddenWindow {
    /// Starts the thread, registers `class_name` (once per process) and creates the window.
    /// `make` builds the handler on the new thread.
    ///
    /// # Errors
    ///
    /// [`Error::Unavailable`] if the thread cannot start; [`Error::Os`] if the class or window
    /// cannot be created.
    pub(super) fn spawn<H: WindowHandler>(
        thread_name: &str,
        class_name: &'static str,
        make: impl FnOnce() -> H + Send + 'static,
    ) -> Result<Self, Error> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<isize, Error>>();
        let thread = std::thread::Builder::new()
            .name(thread_name.into())
            .spawn(move || run(class_name, make, &ready_tx))
            .map_err(|_| Error::Unavailable("could not start the window thread"))?;
        match ready_rx.recv() {
            Ok(Ok(hwnd)) => Ok(Self {
                hwnd,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = thread.join();
                Err(Error::Unavailable(
                    "the window thread stopped while starting",
                ))
            }
        }
    }

    /// Posts `msg` (no parameters) to the window from any thread. `false` if the window is gone
    /// or its queue is full.
    pub(super) fn post(&self, msg: u32) -> bool {
        // SAFETY: `PostMessageW` is safe to call from any thread with any window handle; a
        // stale handle makes it fail rather than misbehave. The message carries no pointers.
        unsafe { PostMessageW(Some(HWND(self.hwnd as *mut _)), msg, WPARAM(0), LPARAM(0)) }.is_ok()
    }
}

impl Drop for HiddenWindow {
    fn drop(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        // Retry briefly in case the queue is full; a thread that has already ended needs nothing.
        let mut posted = false;
        for _ in 0..100 {
            posted = self.post(WM_CLOSE);
            if posted || thread.is_finished() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if posted || thread.is_finished() {
            let _ = thread.join();
        }
        // Otherwise the window could not be told to close; detach rather than hang the app.
    }
}

fn run<H: WindowHandler>(
    class_name: &'static str,
    make: impl FnOnce() -> H,
    ready: &mpsc::Sender<Result<isize, Error>>,
) {
    let handler: *mut H = Box::into_raw(Box::new(make()));
    match create::<H>(class_name, handler) {
        Ok(hwnd) => {
            if ready.send(Ok(hwnd.0 as isize)).is_err() {
                // Nobody is waiting any more; close the window again.
                // SAFETY: `hwnd` was just created by this thread.
                let _ = unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
            }
            let mut msg = MSG::default();
            loop {
                // SAFETY: `msg` is a valid out-pointer; no window filter, so this thread's
                // messages are all returned. 0 is `WM_QUIT`, -1 an error.
                let got = unsafe { GetMessageW(&raw mut msg, None, 0, 0) };
                if got.0 <= 0 {
                    break;
                }
                // SAFETY: `msg` was filled in by `GetMessageW` just above.
                unsafe {
                    let _ = TranslateMessage(&raw const msg);
                    DispatchMessageW(&raw const msg);
                }
            }
        }
        Err(error) => {
            let _ = ready.send(Err(error));
        }
    }
    // SAFETY: `handler` came from `Box::into_raw` above. The window is destroyed (the loop ended
    // on `WM_QUIT`, posted by `WM_DESTROY`, or creation failed), `WM_NCDESTROY` cleared the
    // pointer stored in the window, and this thread owned the window, so nothing can reach the
    // handler any more.
    drop(unsafe { Box::from_raw(handler) });
}

fn create<H: WindowHandler>(class_name: &'static str, handler: *mut H) -> Result<HWND, Error> {
    let class = HSTRING::from(class_name);
    // SAFETY: a null module name asks for this process's own module handle.
    let hinstance = unsafe { GetModuleHandleW(None) }
        .map_err(|e| Error::from_windows("GetModuleHandleW", &e))?;
    let class_info = WNDCLASSEXW {
        cbSize: u32::try_from(std::mem::size_of::<WNDCLASSEXW>()).unwrap_or(0),
        lpfnWndProc: Some(window_proc::<H>),
        hInstance: hinstance.into(),
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };
    // SAFETY: `class_info` is fully initialised and its strings outlive the call; the window
    // procedure is a valid `extern "system"` function for the life of the process.
    let atom = unsafe { RegisterClassExW(&raw const class_info) };
    if atom == 0 {
        // SAFETY: reads this thread's last error, set by the `RegisterClassExW` call above.
        let code = unsafe { GetLastError() };
        // A second window of the same kind (a restarted service) reuses the class.
        if code != ERROR_CLASS_ALREADY_EXISTS {
            return Err(Error::Os {
                op: "RegisterClassExW",
                code: code.0,
            });
        }
    }
    // SAFETY: the class is registered; `handler` is a live `Box<H>` pointer that stays valid
    // until after the window is destroyed (see `run`), and is only read back as `*const H` by
    // `window_proc::<H>`, the procedure of this class. Parent `None`, size 0, never shown.
    unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            PCWSTR(class.as_ptr()),
            w!(""),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinstance.into()),
            Some(handler.cast_const().cast()),
        )
    }
    .map_err(|e| Error::from_windows("CreateWindowExW", &e))
}

/// The window procedure of every hidden window; `H` is the handler stored in the window.
unsafe extern "system" fn window_proc<H: WindowHandler>(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_NCCREATE {
        // SAFETY: for `WM_NCCREATE`, `lparam` points to the `CREATESTRUCTW` of the window being
        // created, valid for the duration of this message; `lpCreateParams` is the handler
        // pointer given to `CreateWindowExW`. Storing it is the documented way to attach state.
        unsafe {
            let create = lparam.0 as *const CREATESTRUCTW;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*create).lpCreateParams as isize);
        }
        // SAFETY: default handling of the same message with the same arguments.
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    // SAFETY: reads the value stored above; 0 before `WM_NCCREATE` and after `WM_NCDESTROY`.
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const H;
    if raw.is_null() {
        // SAFETY: default handling of the same message with the same arguments.
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    if msg == WM_NCDESTROY {
        // SAFETY: detaches the handler before the window goes; later messages (none expected)
        // take the default path above.
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
        // SAFETY: default handling of the same message with the same arguments.
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    // SAFETY: `raw` is the `Box<H>` pointer stored at creation; it stays alive until the thread
    // has left its message loop (see `run`), and is only used through `&H`.
    let handler = unsafe { &*raw };
    // A panic must not unwind through the Win32 caller.
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        handler.message(hwnd, msg, wparam, lparam)
    }))
    .unwrap_or(None);
    if msg == WM_DESTROY {
        // SAFETY: ends this thread's message loop.
        unsafe { PostQuitMessage(0) };
    }
    match outcome {
        Some(result) => result,
        // SAFETY: default handling of the same message with the same arguments.
        None => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use windows::Win32::UI::WindowsAndMessaging::WM_APP;

    use super::*;

    struct Counting(Arc<AtomicU32>);

    impl WindowHandler for Counting {
        fn message(&self, _: HWND, msg: u32, _: WPARAM, _: LPARAM) -> Option<LRESULT> {
            if msg == WM_APP {
                self.0.fetch_add(1, Ordering::SeqCst);
                return Some(LRESULT(0));
            }
            None
        }
    }

    #[test]
    fn a_posted_message_reaches_the_handler_and_drop_joins() {
        let count = Arc::new(AtomicU32::new(0));
        let seen = Arc::clone(&count);
        let window = HiddenWindow::spawn(
            "test-hidden-window",
            "SnipdeckTestHiddenWindow",
            move || Counting(seen),
        )
        .unwrap();
        assert!(window.post(WM_APP));
        // `WM_CLOSE` is queued behind it, so the join below proves the message was handled.
        drop(window);
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
}
