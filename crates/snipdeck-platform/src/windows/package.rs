//! Packaged (MSIX / Microsoft Store) mode: whether the process has package identity, whether
//! Windows started it as a startup task, and "Start with Windows" through that task.
//!
//! A packaged app has no business writing the `Run` registry value: the package declares a
//! `windows.startupTask` extension (`TaskId` [`TASK_ID`]) and the app asks Windows to enable or
//! disable it. The user can switch the task off in Settings > Apps > Startup, and an
//! organisation can forbid it; [`crate::startup_task`] says what each state means.
//!
//! The `WinRT` calls block until Windows answers, so each runs on its own short-lived MTA thread
//! (the shape of `hello`). Nothing here blocks the caller except [`launched_by_startup_task`],
//! which `main` asks once, first thing: turning the task on or off is [`request_startup_task`]
//! (the answer comes to a callback on that thread) and the state the Settings page shows is read
//! from a cache that [`refresh_startup_task`] fills ([`startup_task_status`] never waits).
//!
//! **Unverified on Windows**: this compiles in CI; package identity, the activation kind and the
//! task states have not been run. They need an installed MSIX (`docs/packaging.md`).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use windows::ApplicationModel::Activation::ActivationKind;
use windows::ApplicationModel::{AppInstance, StartupTask, StartupTaskState};
use windows::Win32::Foundation::APPMODEL_ERROR_NO_PACKAGE;
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::core::HSTRING;

use crate::command_line::AutostartStatus;
use crate::error::Error;
use crate::startup_task::{self, Blocked, TASK_ID, TaskState};

/// Why the startup task could not be changed.
#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    /// Windows or the organisation keeps the task off; see [`Blocked::reason`].
    #[error("{0}")]
    Blocked(Blocked),
    /// A call failed.
    #[error("{0}")]
    Os(Error),
}

/// Whether this process runs from an installed package (MSIX / Store), read once.
#[must_use]
pub fn is_packaged() -> bool {
    static PACKAGED: OnceLock<bool> = OnceLock::new();
    *PACKAGED.get_or_init(|| {
        let mut length = 0_u32;
        // SAFETY: `length` is a valid out-pointer and no buffer is passed, so the call only
        // reports the size it would need (or that there is no package).
        let code = unsafe { GetCurrentPackageFullName(&raw mut length, None) };
        // A process with identity answers "buffer too small" for a zero-length buffer; one without
        // answers APPMODEL_ERROR_NO_PACKAGE.
        code != APPMODEL_ERROR_NO_PACKAGE
    })
}

/// Whether Windows started this process as the package's startup task (so there was no
/// `--hidden` argument to read). Packaged builds only.
///
/// Windows hands out the activation arguments the first time they are asked for, so the first call
/// does the work and the answer is kept: call this once, early in `main`, before anything else
/// could ask. **Unverified on Windows.**
#[must_use]
pub fn launched_by_startup_task() -> bool {
    static BY_TASK: OnceLock<bool> = OnceLock::new();
    *BY_TASK.get_or_init(|| {
        is_packaged()
            && on_worker(|| {
                let args = AppInstance::GetActivatedEventArgs()
                    .map_err(|e| from_win("GetActivatedEventArgs", &e))?;
                let kind = args.Kind().map_err(|e| from_win("Kind", &e))?;
                Ok(kind == ActivationKind::StartupTask)
            })
            .unwrap_or(false)
    })
}

fn from_win(op: &'static str, error: &windows::core::Error) -> TaskError {
    TaskError::Os(Error::from_windows(op, error))
}

/// Runs `job` on a fresh MTA thread and waits for it.
fn on_worker<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, TaskError> + Send + 'static,
) -> Result<T, TaskError> {
    std::thread::spawn(move || {
        // SAFETY: balanced with `CoUninitialize` below on this same thread; a failure (the thread
        // already has another apartment) is ignored and the job still runs.
        let initialised = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        let out = job();
        if initialised {
            // SAFETY: pairs the successful `CoInitializeEx` above.
            unsafe { CoUninitialize() };
        }
        out
    })
    .join()
    .map_err(|_| TaskError::Os(Error::Unavailable("the startup-task worker panicked")))?
}

/// Runs `job` on a fresh MTA thread and does not wait for it.
fn spawn_worker(job: impl FnOnce() + Send + 'static) {
    std::thread::spawn(move || {
        // SAFETY: balanced with `CoUninitialize` below on this same thread; a failure (the thread
        // already has another apartment) is ignored and the job still runs.
        let initialised = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        job();
        if initialised {
            // SAFETY: pairs the successful `CoInitializeEx` above.
            unsafe { CoUninitialize() };
        }
    });
}

fn state_of(state: StartupTaskState) -> TaskState {
    match state {
        StartupTaskState::Enabled => TaskState::Enabled,
        StartupTaskState::EnabledByPolicy => TaskState::EnabledByPolicy,
        StartupTaskState::DisabledByPolicy => TaskState::DisabledByPolicy,
        StartupTaskState::DisabledByUser => TaskState::DisabledByUser,
        // `Disabled` and any state a later Windows adds: off, and not the user's doing as far as
        // we know; enabling it is still attempted.
        _ => TaskState::Disabled,
    }
}

fn task() -> Result<StartupTask, TaskError> {
    StartupTask::GetAsync(&HSTRING::from(TASK_ID))
        .and_then(|op| op.join())
        .map_err(|e| from_win("StartupTask::GetAsync", &e))
}

/// Turns the package's startup task on or off, without blocking the caller.
///
/// `done` is called once, **on the worker thread**, with the outcome: [`TaskError::Blocked`] when
/// Windows keeps the task off (the user switched it off in Settings > Apps > Startup, or policy;
/// the app cannot undo either), [`TaskError::Os`] for any other failure. A successful change also
/// updates what [`startup_task_status`] reports. **Unverified on Windows.**
pub fn request_startup_task(on: bool, done: impl FnOnce(Result<(), TaskError>) + Send + 'static) {
    spawn_worker(move || {
        let result = (|| {
            let task = task()?;
            if on {
                let state = task
                    .RequestEnableAsync()
                    .and_then(|op| op.join())
                    .map_err(|e| from_win("RequestEnableAsync", &e))?;
                CACHE.store(encode(state_of(state)), Ordering::Relaxed);
                startup_task::enable_outcome(state_of(state)).map_err(TaskError::Blocked)
            } else {
                task.Disable().map_err(|e| from_win("Disable", &e))?;
                CACHE.store(encode(TaskState::Disabled), Ordering::Relaxed);
                Ok(())
            }
        })();
        done(result);
    });
}

/// The last startup-task state read, as [`encode`] writes it (0: not read yet).
static CACHE: AtomicU8 = AtomicU8::new(0);

const fn encode(state: TaskState) -> u8 {
    match state {
        TaskState::Disabled => 1,
        TaskState::DisabledByUser => 2,
        TaskState::DisabledByPolicy => 3,
        TaskState::Enabled => 4,
        TaskState::EnabledByPolicy => 5,
    }
}

/// When the state was last asked for, so a screen that asks on every redraw starts one read per
/// [`REFRESH_EVERY`], not one per redraw.
static LAST_READ: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
const REFRESH_EVERY: std::time::Duration = std::time::Duration::from_secs(2);

/// Reads the task's state on a worker thread into the cache that [`startup_task_status`] reports
/// (at most one read every two seconds; a read in between is skipped).
pub fn refresh_startup_task() {
    if let Ok(mut last) = LAST_READ.lock() {
        let now = std::time::Instant::now();
        if last.is_some_and(|at| now.duration_since(at) < REFRESH_EVERY) {
            return;
        }
        *last = Some(now);
    }
    spawn_worker(|| {
        if let Ok(state) =
            task().and_then(|task| task.State().map_err(|e| from_win("StartupTask::State", &e)))
        {
            CACHE.store(encode(state_of(state)), Ordering::Relaxed);
        }
    });
}

/// What the startup task is, from the last state read ([`refresh_startup_task`], or the last
/// change). Until one has been read it is `On`, which trusts the saved setting.
#[must_use]
pub fn startup_task_status() -> AutostartStatus {
    startup_task::status_of(match CACHE.load(Ordering::Relaxed) {
        2 => Some(TaskState::DisabledByUser),
        3 => Some(TaskState::DisabledByPolicy),
        1 => Some(TaskState::Disabled),
        4 => Some(TaskState::Enabled),
        5 => Some(TaskState::EnabledByPolicy),
        _ => None,
    })
}
