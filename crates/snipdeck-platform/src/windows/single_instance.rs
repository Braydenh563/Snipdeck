//! One running Snipdeck per Windows session; a second launch asks the first to show itself.
//!
//! - A named mutex (`Local\Snipdeck-<app_id>`, see [`crate::instance_names`]) decides who is
//!   first. The `Local\` namespace is per logon session, so two users each get their own.
//! - A named auto-reset event (`Local\Snipdeck.raise-<app_id>`) is how a later launch reaches the
//!   first. The first instance waits on it from a thread and calls `on_second` (on that thread)
//!   once per signal; the second sets it and gets [`Acquired::Second`], so the app can exit.
//!
//! Every launch creates-or-opens the event *before* the mutex, so a second launch can signal even
//! if the first has not started waiting yet: an auto-reset event stays set until a waiter takes it.
//! If the first instance is elevated, a normal-integrity launch is denied access to both objects;
//! that is reported as [`Acquired::Second`] too (there is a running instance), although it cannot
//! be raised. **Unverified on Windows.**

use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::thread::JoinHandle;

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, WAIT_OBJECT_0,
};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, INFINITE, SetEvent, WaitForMultipleObjects,
};
use windows::core::{HSTRING, PCWSTR};

use crate::error::Error;
use crate::instance_names::{mutex_name, raise_event_name};

/// A kernel handle closed on drop.
struct Handle(HANDLE);

// SAFETY: a kernel object handle is a process-wide token, usable from any thread; the only
// operations performed on it are waiting, setting an event and closing, all thread-safe.
unsafe impl Send for Handle {}
// SAFETY: as above; `&Handle` only exposes the raw value, and the Win32 calls made with it are
// safe to run concurrently.
unsafe impl Sync for Handle {}

impl Handle {
    fn raw(&self) -> HANDLE {
        self.0
    }

    /// A handle that owns nothing; closing it is a harmless failing call.
    fn null() -> Self {
        Self(HANDLE(std::ptr::null_mut::<c_void>()))
    }
}

/// A borrowed handle value for the listener thread. It does not close anything.
#[derive(Clone, Copy)]
struct Borrowed(HANDLE);

// SAFETY: a kernel handle value is usable from any thread; the owner (`SingleInstance`) keeps the
// handle open until the thread has been joined.
unsafe impl Send for Borrowed {}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful `Create*W` call and is closed exactly once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn app_id_error(error: crate::instance_names::AppIdError) -> Error {
    match error {
        crate::instance_names::AppIdError::Length => Error::Unavailable("invalid app id length"),
        crate::instance_names::AppIdError::Character => {
            Error::Unavailable("invalid character in app id")
        }
    }
}

/// The result of [`SingleInstance::acquire`].
#[derive(Debug)]
pub enum Acquired {
    /// This is the first instance. Keep the guard alive for as long as the app runs.
    First(SingleInstance),
    /// Another instance is running and has been asked to show itself. Exit.
    Second,
}

/// The first instance's claim. Dropping it stops the listener thread and releases the name.
pub struct SingleInstance {
    // Field order is drop order after `Drop::drop`: the handles close after the thread is joined.
    thread: Option<JoinHandle<()>>,
    stop: Handle,
    raise: Handle,
    _mutex: Handle,
}

impl std::fmt::Debug for SingleInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SingleInstance").finish_non_exhaustive()
    }
}

impl SingleInstance {
    /// Claims the instance for `app_id`, or signals the instance that has it.
    ///
    /// `on_second` runs on the listener thread each time a later launch signals this instance
    /// (raise the panel from there, for example by posting to the UI thread). A panic in it is
    /// caught and ignored. It is never called for the launch that returns [`Acquired::Second`].
    ///
    /// # Errors
    ///
    /// [`Error::Unavailable`] for an invalid `app_id` or if the listener thread cannot be started;
    /// [`Error::Os`] if an event or mutex cannot be created for a reason other than access denied.
    pub fn acquire(app_id: &str, on_second: impl Fn() + Send + 'static) -> Result<Acquired, Error> {
        let mutex_name = HSTRING::from(mutex_name(app_id).map_err(app_id_error)?);
        let event_name = HSTRING::from(raise_event_name(app_id).map_err(app_id_error)?);

        // SAFETY: `event_name` is a NUL-terminated wide string that outlives the call; default
        // security attributes; auto-reset (`bmanualreset == false`), initially not signaled.
        let created = unsafe { CreateEventW(None, false, false, PCWSTR(event_name.as_ptr())) };
        let raise = match created {
            Ok(h) => Handle(h),
            Err(e) if e.code() == ERROR_ACCESS_DENIED.to_hresult() => return Ok(Acquired::Second),
            Err(e) => return Err(Error::from_windows("CreateEventW", &e)),
        };

        // SAFETY: `mutex_name` is a NUL-terminated wide string that outlives the call; default
        // security attributes; the mutex is not owned on creation (existence is what matters).
        let created = unsafe { CreateMutexW(None, false, PCWSTR(mutex_name.as_ptr())) };
        let mutex = match created {
            Ok(h) => Handle(h),
            Err(e) if e.code() == ERROR_ACCESS_DENIED.to_hresult() => return Ok(Acquired::Second),
            Err(e) => return Err(Error::from_windows("CreateMutexW", &e)),
        };
        // SAFETY: reads this thread's last error, set by the `CreateMutexW` call just above (the
        // `windows` wrapper makes no other Win32 call on success).
        let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;

        if already_running {
            // SAFETY: `raise` is a valid event handle with `EVENT_MODIFY_STATE` (created with full
            // access). Failure means nobody gets raised; there is nothing better to do than exit.
            let _ = unsafe { SetEvent(raise.raw()) };
            return Ok(Acquired::Second);
        }

        // SAFETY: an anonymous manual-reset event, initially not signaled, default security.
        let stop = match unsafe { CreateEventW(None, true, false, PCWSTR::null()) } {
            Ok(h) => Handle(h),
            Err(e) => return Err(Error::from_windows("CreateEventW", &e)),
        };

        let thread = {
            // The thread borrows the raw handle values. They stay valid because `Drop` joins the
            // thread before the `Handle`s close.
            let raise_raw = Borrowed(raise.raw());
            let stop_raw = Borrowed(stop.raw());
            std::thread::Builder::new()
                .name("snipdeck-single-instance".into())
                .spawn(move || listen(raise_raw, stop_raw, &on_second))
                .map_err(|_| Error::Unavailable("could not start the single-instance thread"))?
        };

        Ok(Acquired::First(Self {
            thread: Some(thread),
            stop,
            raise,
            _mutex: mutex,
        }))
    }
}

/// Waits for the raise event (call `on_second`) or the stop event (return). Never panics.
fn listen(raise: Borrowed, stop: Borrowed, on_second: &dyn Fn()) {
    let handles = [raise.0, stop.0];
    loop {
        // SAFETY: both handles are valid, open event handles (owned by the `SingleInstance`, which
        // joins this thread before closing them); the slice has two elements, `bwaitall == false`.
        let signaled = unsafe { WaitForMultipleObjects(&handles, false, INFINITE) };
        if signaled == WAIT_OBJECT_0 {
            let _ = catch_unwind(AssertUnwindSafe(on_second));
        } else {
            // The stop event (index 1), or WAIT_FAILED / abandoned: stop listening.
            break;
        }
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        // SAFETY: `self.stop` is a valid event handle created with full access.
        if unsafe { SetEvent(self.stop.raw()) }.is_err() {
            // The thread cannot be told to stop. Joining would hang, and closing the handles under
            // it would be a use-after-close, so leak both (process exit reclaims them).
            std::mem::forget(thread);
            std::mem::forget(std::mem::replace(&mut self.stop, Handle::null()));
            std::mem::forget(std::mem::replace(&mut self.raise, Handle::null()));
            return;
        }
        let _ = thread.join();
    }
}
