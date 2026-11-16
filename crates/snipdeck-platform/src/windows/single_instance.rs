//! One running Snipdeck per user and data folder; a second launch hands its request to the first
//! and waits to hear that it was taken.
//!
//! All objects live in the per-session `Local\` namespace and are named from one id
//! ([`crate::instance_names::instance_id`]: a hash of the user's SID and the data folder), so two
//! users in one session, and two copies on different data folders, run side by side. Every object is
//! created with an explicit DACL that grants the current user only (and a medium integrity label, so
//! a normal launch can still open what an elevated copy made).
//!
//! - A named mutex decides who is first.
//! - A shared memory section holds the [`Mailbox`](crate::handover::Mailbox): the first instance's
//!   process id (so a second launch can call `AllowSetForegroundWindow` for it, and wait on the
//!   process) and one pending [`Request`].
//! - Second launches take a named lock mutex, put their request in the mailbox, set the auto-reset
//!   *request* event and wait for the auto-reset *ack* event, which the first instance sets from its
//!   UI thread once it has acted on the request ([`Ack::send`]). The wait also watches the first
//!   process, so a first instance that is quitting is noticed at once and the second launch starts
//!   over (and usually becomes the first). A wait that times out is reported ([`Delivery`]) so the app
//!   can say that the running instance is not answering.
//! - A second auto-reset event (*quit*) is how `snipdeck.exe --quit` asks the first to quit.
//!   [`ask_to_quit`] only opens and sets it; it never creates anything.
//!
//! The decisions (retry, give up, what counts as an answer) are in [`crate::win_logic`], the byte
//! layout in [`crate::handover`]. **Unverified on Windows.**

use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, FALSE,
    GetLastError, HANDLE, HLOCAL, INVALID_HANDLE_VALUE, LocalFree, WAIT_ABANDONED_0, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows::Win32::System::Memory::{
    CreateFileMappingW, FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile,
    PAGE_READWRITE, UnmapViewOfFile,
};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, GetCurrentProcess, GetCurrentProcessId,
    INFINITE, OpenEventW, OpenProcess, OpenProcessToken, PROCESS_SYNCHRONIZE, ReleaseMutex,
    ResetEvent, SetEvent, WaitForMultipleObjects, WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;
use windows::core::{HSTRING, PCWSTR, PWSTR};

use crate::error::Error;
use crate::handover::{Mailbox, Request, SECTION_BYTES};
use crate::instance_names::{
    AppIdError, ack_event_name, lock_mutex_name, mutex_name, quit_event_name, raise_event_name,
    section_name,
};
use crate::win_logic::{
    ACK_TIMEOUT_MS, HandoverNext, LOCK_TIMEOUT_MS, MAX_HANDOVER_ATTEMPTS, WaitEvent, after_handover,
};

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
        // SAFETY: the handle came from a successful Win32 call and is closed exactly once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn app_id_error(error: AppIdError) -> Error {
    match error {
        AppIdError::Length => Error::Unavailable("invalid app id length"),
        AppIdError::Character => Error::Unavailable("invalid character in app id"),
    }
}

// ---- The user's SID and the owner-only security descriptor -----------------------------------

/// The SID string (`S-1-5-21-...`) of the user this process runs as.
///
/// # Errors
///
/// [`Error::Os`] if the process token cannot be read.
pub fn user_sid() -> Result<String, Error> {
    let mut token = HANDLE::default();
    // SAFETY: the pseudo handle of this process, a valid out pointer; the token is closed by `Handle`.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
        .map_err(|e| Error::from_windows("OpenProcessToken", &e))?;
    let token = Handle(token);
    let mut needed = 0_u32;
    // SAFETY: the size query: no buffer, a valid out pointer. It fails with "insufficient buffer",
    // which is the expected answer here.
    let _ = unsafe { GetTokenInformation(token.raw(), TokenUser, None, 0, &mut needed) };
    // `u64` units: `TOKEN_USER` wants 8-byte alignment.
    let mut buffer = vec![0_u64; (needed as usize).div_ceil(8).max(1)];
    // SAFETY: `buffer` has at least `needed` bytes; `token` has `TOKEN_QUERY`.
    unsafe {
        GetTokenInformation(
            token.raw(),
            TokenUser,
            Some(buffer.as_mut_ptr().cast::<c_void>()),
            needed,
            &mut needed,
        )
    }
    .map_err(|e| Error::from_windows("GetTokenInformation", &e))?;
    // SAFETY: the call above filled the buffer with a `TOKEN_USER` (8-aligned, large enough).
    let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let mut text = PWSTR::null();
    // SAFETY: `sid` points into `buffer`, which is alive; `text` receives a `LocalAlloc`ed string.
    unsafe { ConvertSidToStringSidW(sid, &mut text) }
        .map_err(|e| Error::from_windows("ConvertSidToStringSidW", &e))?;
    // SAFETY: a NUL-terminated wide string just returned by the call above.
    let result = unsafe { text.to_string() };
    // SAFETY: `text` came from `LocalAlloc` (as the call documents) and is freed once.
    let _ = unsafe { LocalFree(Some(HLOCAL(text.0.cast()))) };
    result.map_err(|_| Error::Unavailable("the user SID is not valid text"))
}

/// A security descriptor that grants the current user everything and nobody else anything, with a
/// medium mandatory label so that a launch at normal integrity can open what an elevated copy made.
struct OwnerOnly {
    descriptor: PSECURITY_DESCRIPTOR,
}

impl OwnerOnly {
    fn new(sid: &str) -> Result<Self, Error> {
        let sddl = HSTRING::from(format!("D:(A;;GA;;;{sid})S:(ML;;NW;;;ME)"));
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: `sddl` is a NUL-terminated wide string that outlives the call; `descriptor`
        // receives a `LocalAlloc`ed block, freed in `Drop`.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl.as_ptr()),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
        }
        .map_err(|e| {
            Error::from_windows("ConvertStringSecurityDescriptorToSecurityDescriptorW", &e)
        })?;
        Ok(Self { descriptor })
    }

    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(0),
            lpSecurityDescriptor: self.descriptor.0,
            bInheritHandle: FALSE,
        }
    }
}

impl Drop for OwnerOnly {
    fn drop(&mut self) {
        // SAFETY: the block came from `LocalAlloc` in `new` and is freed once.
        let _ = unsafe { LocalFree(Some(HLOCAL(self.descriptor.0))) };
    }
}

// ---- The shared section ----------------------------------------------------------------------

/// The mapped mailbox section.
struct Section {
    view: *mut u8,
    handle: Handle,
}

// SAFETY: the view is plain shared memory; access follows the protocol of this module (second
// launches write under the lock mutex, the first instance reads after the request event), and the
// raw pointer is only turned into a slice for the duration of one call.
unsafe impl Send for Section {}
// SAFETY: as above.
unsafe impl Sync for Section {}

impl Section {
    fn create(name: &HSTRING, attributes: &SECURITY_ATTRIBUTES) -> Result<Self, Error> {
        // SAFETY: a pagefile-backed mapping (`INVALID_HANDLE_VALUE`) of `SECTION_BYTES` with a named,
        // owner-only descriptor; `name` outlives the call.
        let handle = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                Some(std::ptr::from_ref(attributes)),
                PAGE_READWRITE,
                0,
                u32::try_from(SECTION_BYTES).unwrap_or(0),
                PCWSTR(name.as_ptr()),
            )
        }
        .map_err(|e| Error::from_windows("CreateFileMappingW", &e))?;
        let handle = Handle(handle);
        // SAFETY: a valid mapping handle with full access; the whole section.
        let view = unsafe { MapViewOfFile(handle.raw(), FILE_MAP_ALL_ACCESS, 0, 0, SECTION_BYTES) };
        if view.Value.is_null() {
            return Err(Error::Unavailable("could not map the hand-over section"));
        }
        Ok(Self {
            view: view.Value.cast::<u8>(),
            handle,
        })
    }

    /// Runs `f` on the mailbox. The caller follows the module's protocol.
    fn with_mailbox<R>(&self, f: impl FnOnce(&mut Mailbox<'_>) -> R) -> Option<R> {
        // SAFETY: the view maps `SECTION_BYTES` bytes and stays mapped while `self` lives; the slice
        // does not outlive this call, and the lock / event protocol keeps writers apart.
        let bytes = unsafe { std::slice::from_raw_parts_mut(self.view, SECTION_BYTES) };
        Mailbox::over(bytes).map(|mut mailbox| f(&mut mailbox))
    }
}

impl Drop for Section {
    fn drop(&mut self) {
        // SAFETY: the view came from `MapViewOfFile` and is unmapped once; the handle closes after.
        let _ = unsafe {
            UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                Value: self.view.cast::<c_void>(),
            })
        };
        let _ = &self.handle;
    }
}

// ---- Creating or opening the objects ----------------------------------------------------------

struct Objects {
    mutex: Handle,
    raise: Handle,
    quit: Handle,
    ack: Arc<Handle>,
    lock: Handle,
    section: Arc<Section>,
    already_running: bool,
}

/// Why the objects could not be had.
enum OpenError {
    /// Access denied: another account made them (or an elevated copy of an older build).
    Denied,
    Failed(Error),
}

fn create_event(
    name: &HSTRING,
    attributes: &SECURITY_ATTRIBUTES,
    what: &'static str,
) -> Result<Handle, OpenError> {
    // SAFETY: `name` is a NUL-terminated wide string that outlives the call; owner-only
    // attributes; auto-reset (`bmanualreset == false`), initially not signaled.
    match unsafe {
        CreateEventW(
            Some(std::ptr::from_ref(attributes)),
            false,
            false,
            PCWSTR(name.as_ptr()),
        )
    } {
        Ok(h) => Ok(Handle(h)),
        Err(e) if e.code() == ERROR_ACCESS_DENIED.to_hresult() => Err(OpenError::Denied),
        Err(e) => Err(OpenError::Failed(Error::from_windows(what, &e))),
    }
}

fn create_mutex(
    name: &HSTRING,
    attributes: &SECURITY_ATTRIBUTES,
    what: &'static str,
) -> Result<(Handle, bool), OpenError> {
    // SAFETY: as `create_event`; the mutex is not owned on creation (existence is what matters).
    match unsafe {
        CreateMutexW(
            Some(std::ptr::from_ref(attributes)),
            false,
            PCWSTR(name.as_ptr()),
        )
    } {
        Ok(h) => {
            // SAFETY: reads this thread's last error, set by the call just above.
            let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
            Ok((Handle(h), existed))
        }
        Err(e) if e.code() == ERROR_ACCESS_DENIED.to_hresult() => Err(OpenError::Denied),
        Err(e) => Err(OpenError::Failed(Error::from_windows(what, &e))),
    }
}

impl Objects {
    /// Creates or opens every object. The events and the section come *before* the mutex so a second
    /// launch can signal even if the first has not started waiting yet.
    fn open(app_id: &str, attributes: &SECURITY_ATTRIBUTES) -> Result<Self, OpenError> {
        let name = |r: Result<String, AppIdError>| {
            r.map(HSTRING::from)
                .map_err(|e| OpenError::Failed(app_id_error(e)))
        };
        let raise = create_event(&name(raise_event_name(app_id))?, attributes, "CreateEventW")?;
        let quit = create_event(&name(quit_event_name(app_id))?, attributes, "CreateEventW")?;
        let ack = create_event(&name(ack_event_name(app_id))?, attributes, "CreateEventW")?;
        let (lock, _) = create_mutex(&name(lock_mutex_name(app_id))?, attributes, "CreateMutexW")?;
        let section =
            Section::create(&name(section_name(app_id))?, attributes).map_err(OpenError::Failed)?;
        let (mutex, already_running) =
            create_mutex(&name(mutex_name(app_id))?, attributes, "CreateMutexW")?;
        Ok(Self {
            mutex,
            raise,
            quit,
            ack: Arc::new(ack),
            lock,
            section: Arc::new(section),
            already_running,
        })
    }
}

// ---- The first instance -----------------------------------------------------------------------

/// The result of [`SingleInstance::acquire`].
#[derive(Debug)]
pub enum Acquired {
    /// This is the first instance. Keep the guard alive for as long as the app runs.
    First(SingleInstance),
    /// Another instance is running. Exit; the value says what to tell the user, if anything.
    Second(Delivery),
}

/// What became of a second launch's request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// The running instance took the request: exit quietly.
    Delivered,
    /// The running instance did not answer in time, or cannot be reached (hung, suspended, or run
    /// as administrator): tell the user.
    Unresponsive,
}

/// Lets the first instance say, from its UI thread, that a request has been acted on. A second
/// launch waits for this; it holds its own handle, so it is safe to use after the guard is gone
/// (it then does nothing useful, and the waiting launch sees the process end instead).
#[derive(Clone)]
pub struct Ack(Arc<Handle>);

impl Ack {
    /// Tells the waiting second launch (if any) that its request was taken.
    pub fn send(&self) {
        // SAFETY: a valid event handle with full access, kept open by the `Arc`.
        let _ = unsafe { SetEvent(self.0.raw()) };
    }
}

impl std::fmt::Debug for Ack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ack").finish_non_exhaustive()
    }
}

/// The first instance's claim. Dropping it stops the listener thread and releases the name.
pub struct SingleInstance {
    // Field order is drop order after `Drop::drop`: the handles close after the thread is joined.
    thread: Option<JoinHandle<()>>,
    stop: Handle,
    raise: Handle,
    quit: Handle,
    _ack: Arc<Handle>,
    _lock: Handle,
    _section: Arc<Section>,
    _mutex: Handle,
}

impl std::fmt::Debug for SingleInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SingleInstance").finish_non_exhaustive()
    }
}

impl SingleInstance {
    /// Claims the instance for `app_id`, or hands `request` to the instance that has it.
    ///
    /// `on_request` runs on the listener thread for every request a later launch hands over (a plain
    /// launch is [`Request::Show`]); hand it to the UI thread, act on it there, and call
    /// [`Ack::send`] when done. A panic in it is caught and ignored. It is never called for the
    /// launch that returns [`Acquired::Second`]. `on_quit` runs on the same thread each time
    /// `snipdeck.exe --quit` ([`ask_to_quit`]) signals this instance.
    ///
    /// A second launch blocks for up to [`ACK_TIMEOUT_MS`] (a few times over if the first instance
    /// is quitting under it).
    ///
    /// # Errors
    ///
    /// [`Error::Unavailable`] for an invalid `app_id` or if the listener thread cannot be started;
    /// [`Error::Os`] if an object cannot be created for a reason other than access denied.
    pub fn acquire(
        app_id: &str,
        request: &Request,
        on_request: impl Fn(Request, Ack) + Send + 'static,
        on_quit: impl Fn() + Send + 'static,
    ) -> Result<Acquired, Error> {
        let sid = user_sid()?;
        let owner_only = OwnerOnly::new(&sid)?;
        let attributes = owner_only.attributes();
        for attempt in 0..MAX_HANDOVER_ATTEMPTS {
            let objects = match Objects::open(app_id, &attributes) {
                Ok(objects) => objects,
                Err(OpenError::Denied) => return Ok(Acquired::Second(Delivery::Unresponsive)),
                Err(OpenError::Failed(e)) => return Err(e),
            };
            if !objects.already_running {
                return Self::become_first(objects, on_request, on_quit);
            }
            let event = hand_over(&objects, request);
            match after_handover(attempt, event) {
                HandoverNext::Done => return Ok(Acquired::Second(Delivery::Delivered)),
                HandoverNext::Unresponsive => {
                    return Ok(Acquired::Second(Delivery::Unresponsive));
                }
                HandoverNext::Retry => {
                    // Close every handle so the dying instance's names go away, then start over.
                    drop(objects);
                    std::thread::sleep(Duration::from_millis(150));
                }
            }
        }
        Ok(Acquired::Second(Delivery::Unresponsive))
    }

    fn become_first(
        objects: Objects,
        on_request: impl Fn(Request, Ack) + Send + 'static,
        on_quit: impl Fn() + Send + 'static,
    ) -> Result<Acquired, Error> {
        // SAFETY: no arguments.
        let pid = unsafe { GetCurrentProcessId() };
        objects
            .section
            .with_mailbox(|mailbox| mailbox.set_owner(pid));

        // SAFETY: an anonymous manual-reset event, initially not signaled, default security.
        let stop = match unsafe { CreateEventW(None, true, false, PCWSTR::null()) } {
            Ok(h) => Handle(h),
            Err(e) => return Err(Error::from_windows("CreateEventW", &e)),
        };
        let thread = {
            // The thread borrows the raw handle values. They stay valid because `Drop` joins the
            // thread before the `Handle`s close.
            let raise_raw = Borrowed(objects.raise.raw());
            let quit_raw = Borrowed(objects.quit.raw());
            let stop_raw = Borrowed(stop.raw());
            let ack = Ack(Arc::clone(&objects.ack));
            let section = Arc::clone(&objects.section);
            std::thread::Builder::new()
                .name("snipdeck-single-instance".into())
                .spawn(move || {
                    listen(
                        raise_raw,
                        quit_raw,
                        stop_raw,
                        &section,
                        &ack,
                        &on_request,
                        &on_quit,
                    );
                })
                .map_err(|_| Error::Unavailable("could not start the single-instance thread"))?
        };
        let Objects {
            mutex,
            raise,
            quit,
            ack,
            lock,
            section,
            ..
        } = objects;
        Ok(Acquired::First(Self {
            thread: Some(thread),
            stop,
            raise,
            quit,
            _ack: ack,
            _lock: lock,
            _section: section,
            _mutex: mutex,
        }))
    }
}

/// Releases the lock mutex on drop.
struct Locked<'a>(&'a Handle);

impl Drop for Locked<'_> {
    fn drop(&mut self) {
        // SAFETY: this thread owns the mutex (it waited on it successfully).
        let _ = unsafe { ReleaseMutex(self.0.raw()) };
    }
}

/// One hand-over attempt of a second launch: lock, write, signal, wait.
fn hand_over(objects: &Objects, request: &Request) -> WaitEvent {
    // SAFETY: a valid mutex handle.
    let got = unsafe { WaitForSingleObject(objects.lock.raw(), LOCK_TIMEOUT_MS) };
    if got != WAIT_OBJECT_0 && got != WAIT_ABANDONED_0 {
        return WaitEvent::TimedOut;
    }
    let _locked = Locked(&objects.lock);
    let owner = objects.section.with_mailbox(|mailbox| {
        mailbox.put(request);
        mailbox.owner()
    });
    let Some(owner) = owner else {
        return WaitEvent::NotReached;
    };
    // SAFETY: a valid auto-reset event: drop an answer left over from an earlier, timed-out launch.
    let _ = unsafe { ResetEvent(objects.ack.raw()) };
    if let Some(pid) = owner {
        // The first instance should be allowed to take the foreground when it raises its window.
        // SAFETY: plain call; failure only means the window may flash instead of coming forward.
        let _ = unsafe { AllowSetForegroundWindow(pid) };
    }
    // SAFETY: a valid event handle with full access.
    if unsafe { SetEvent(objects.raise.raw()) }.is_err() {
        return WaitEvent::NotReached;
    }
    // SAFETY: `SYNCHRONIZE` only, not inheritable. `None` when the pid is not known yet or the
    // process is already gone; the ack alone is waited for then.
    let process = owner
        .and_then(|pid| unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }.ok())
        .map(Handle);
    let mut handles = vec![objects.ack.raw()];
    if let Some(process) = &process {
        handles.push(process.raw());
    }
    // SAFETY: every handle is valid and open; `bwaitall == false`.
    let signaled = unsafe { WaitForMultipleObjects(&handles, false, ACK_TIMEOUT_MS) };
    if signaled == WAIT_OBJECT_0 {
        WaitEvent::Acked
    } else if signaled.0 == WAIT_OBJECT_0.0 + 1 && process.is_some() {
        WaitEvent::FirstExited
    } else if signaled == WAIT_TIMEOUT {
        WaitEvent::TimedOut
    } else {
        WaitEvent::NotReached
    }
}

/// Asks the Snipdeck running for `app_id` to quit, as the tray's Quit does. Returns at once; it does
/// not wait for the app to go.
///
/// `Ok(true)`: the request was delivered. `Ok(false)`: nothing to ask: no Snipdeck is running for
/// this user and data folder (the event does not exist), or this process may not signal it (access
/// denied). Nothing is created: only an existing event is opened. **Unverified on Windows.**
///
/// # Errors
///
/// [`Error::Unavailable`] for an invalid `app_id`; [`Error::Os`] for any other failure to open or
/// set the event.
pub fn ask_to_quit(app_id: &str) -> Result<bool, Error> {
    let name = HSTRING::from(quit_event_name(app_id).map_err(app_id_error)?);
    // SAFETY: `name` is a NUL-terminated wide string that outlives the call; the handle is not
    // inheritable and is closed below through `Handle`.
    let opened = unsafe { OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(name.as_ptr())) };
    let event = match opened {
        Ok(h) => Handle(h),
        Err(e)
            if e.code() == ERROR_FILE_NOT_FOUND.to_hresult()
                || e.code() == ERROR_ACCESS_DENIED.to_hresult() =>
        {
            return Ok(false);
        }
        Err(e) => return Err(Error::from_windows("OpenEventW", &e)),
    };
    // SAFETY: `event` is a valid event handle opened with `EVENT_MODIFY_STATE`.
    unsafe { SetEvent(event.raw()) }.map_err(|e| Error::from_windows("SetEvent", &e))?;
    Ok(true)
}

/// Waits for the request event (take the mailbox, call `on_request`), the quit event (call
/// `on_quit`) or the stop event (return). Never panics.
fn listen(
    raise: Borrowed,
    quit: Borrowed,
    stop: Borrowed,
    section: &Section,
    ack: &Ack,
    on_request: &dyn Fn(Request, Ack),
    on_quit: &dyn Fn(),
) {
    let handles = [raise.0, quit.0, stop.0];
    loop {
        // SAFETY: all handles are valid, open event handles (owned by the `SingleInstance`, which
        // joins this thread before closing them); the slice has three elements,
        // `bwaitall == false`.
        let signaled = unsafe { WaitForMultipleObjects(&handles, false, INFINITE) };
        if signaled == WAIT_OBJECT_0 {
            // A launch with nothing in the box (it could not write one) is a plain launch.
            let request = section
                .with_mailbox(|mailbox| mailbox.take())
                .flatten()
                .unwrap_or(Request::Show);
            let ack = ack.clone();
            let _ = catch_unwind(AssertUnwindSafe(|| on_request(request, ack)));
        } else if signaled.0 == WAIT_OBJECT_0.0 + 1 {
            let _ = catch_unwind(AssertUnwindSafe(on_quit));
        } else {
            // The stop event (index 2), or WAIT_FAILED / abandoned: stop listening.
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
            std::mem::forget(std::mem::replace(&mut self.quit, Handle::null()));
            return;
        }
        let _ = thread.join();
    }
}
