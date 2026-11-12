//! Packaged (MSIX / Microsoft Store) mode: whether the process has package identity, whether
//! Windows started it as a startup task, and "Start with Windows" through that task.
//!
//! A packaged app has no business writing the `Run` registry value: the package declares a
//! `windows.startupTask` extension (`TaskId` [`TASK_ID`]) and the app asks Windows to enable or
//! disable it. The user can switch the task off in Settings > Apps > Startup, and an
//! organisation can forbid it; [`crate::startup_task`] says what each state means.
//!
//! The `WinRT` calls block until Windows answers, so each runs on its own short-lived MTA thread
//! (the shape of `hello`); call them from a worker or accept a short block on the UI thread.
//!
//! **Unverified on Windows**: this compiles in CI; package identity, the activation kind and the
//! task states have not been run. They need an installed MSIX (`docs/packaging.md`).

use std::sync::OnceLock;

use windows::ApplicationModel::Activation::ActivationKind;
use windows::ApplicationModel::{AppInstance, StartupTask, StartupTaskState};
use windows::Win32::Foundation::APPMODEL_ERROR_NO_PACKAGE;
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::core::HSTRING;

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
        let code = unsafe { GetCurrentPackageFullName(&mut length, None) };
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

/// Turns the package's startup task on or off.
///
/// # Errors
///
/// [`TaskError::Blocked`] when Windows keeps it off (the user switched it off in Settings, or
/// policy); [`TaskError::Os`] for any other failure. **Unverified on Windows.**
pub fn set_startup_task(on: bool) -> Result<(), TaskError> {
    on_worker(move || {
        let task = task()?;
        if on {
            let state = task
                .RequestEnableAsync()
                .and_then(|op| op.join())
                .map_err(|e| from_win("RequestEnableAsync", &e))?;
            startup_task::enable_outcome(state_of(state)).map_err(TaskError::Blocked)
        } else {
            task.Disable().map_err(|e| from_win("Disable", &e))
        }
    })
}

/// Whether the startup task is on now (`false` when it cannot be read).
#[must_use]
pub fn startup_task_enabled() -> bool {
    on_worker(|| {
        let state = task()?
            .State()
            .map_err(|e| from_win("StartupTask::State", &e))?;
        Ok(startup_task::is_registered(state_of(state)))
    })
    .unwrap_or(false)
}
