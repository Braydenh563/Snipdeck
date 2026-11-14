//! "Start with Windows" for the packaged (MSIX / Store) build: what a `StartupTask` state means.
//!
//! A packaged app cannot write the `Run` registry value; it declares a startup task in its
//! manifest (`TaskId` [`TASK_ID`]) and asks Windows to enable it. The user can switch the task off
//! in Settings > Apps > Startup, and an organisation can forbid it; neither can be undone from the
//! app. This module holds the portable decisions; `windows::package` makes the calls.

/// The `TaskId` of the `windows.startupTask` extension in the MSIX manifest.
pub const TASK_ID: &str = "SnipdeckStartup";

/// The Microsoft Store's "Downloads and updates" page.
pub const STORE_UPDATES_URL: &str = "ms-windows-store://downloadsandupdates";

/// Windows' `StartupTaskState`, without the Windows types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// Off, and the app may ask to turn it on.
    Disabled,
    /// Switched off by the user in Settings > Apps > Startup; only the user can switch it on.
    DisabledByUser,
    /// Forbidden by the organisation's policy.
    DisabledByPolicy,
    /// On.
    Enabled,
    /// On, by policy; the user cannot switch it off.
    EnabledByPolicy,
}

/// Why the task cannot be enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocked {
    /// The user switched Snipdeck off under Settings > Apps > Startup.
    ByUser,
    /// The organisation's policy forbids it.
    ByPolicy,
}

impl Blocked {
    /// The English reason (for the log; the app shows a translated message).
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::ByUser => {
                "Windows has Snipdeck switched off under Settings > Apps > Startup. Switch it on there."
            }
            Self::ByPolicy => {
                "Your organisation's policy keeps Snipdeck from starting with Windows."
            }
        }
    }
}

impl std::fmt::Display for Blocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.reason())
    }
}

impl std::error::Error for Blocked {}

/// The outcome of asking to enable the task, given the state Windows reports after the request.
///
/// # Errors
///
/// [`Blocked`] when the state is not an enabled one.
pub const fn enable_outcome(after_request: TaskState) -> Result<(), Blocked> {
    match after_request {
        TaskState::Enabled | TaskState::EnabledByPolicy => Ok(()),
        // Still disabled after asking: Windows declined without a policy in the way, which means
        // the user has switched it off in Settings.
        TaskState::Disabled | TaskState::DisabledByUser => Err(Blocked::ByUser),
        TaskState::DisabledByPolicy => Err(Blocked::ByPolicy),
    }
}

/// Whether the task will run at sign-in (the state means "on").
#[must_use]
pub const fn is_registered(state: TaskState) -> bool {
    matches!(state, TaskState::Enabled | TaskState::EnabledByPolicy)
}

/// A start by the startup task cannot carry `--hidden`: it counts as one when the user chose to
/// stay in the tray (`Settings::starts_visible` is false).
#[must_use]
pub const fn start_hidden(launched_by_startup_task: bool, starts_visible: bool) -> bool {
    launched_by_startup_task && !starts_visible
}

/// The page "Check for updates" opens: the Store's updates page in the packaged build (the Store
/// updates the app), `releases_url` otherwise.
#[must_use]
pub const fn updates_url(packaged: bool, releases_url: &'static str) -> &'static str {
    if packaged {
        STORE_UPDATES_URL
    } else {
        releases_url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_states_are_ok() {
        assert_eq!(enable_outcome(TaskState::Enabled), Ok(()));
        assert_eq!(enable_outcome(TaskState::EnabledByPolicy), Ok(()));
    }

    #[test]
    fn disabled_states_say_why() {
        assert_eq!(
            enable_outcome(TaskState::DisabledByUser),
            Err(Blocked::ByUser)
        );
        assert_eq!(enable_outcome(TaskState::Disabled), Err(Blocked::ByUser));
        assert_eq!(
            enable_outcome(TaskState::DisabledByPolicy),
            Err(Blocked::ByPolicy)
        );
    }

    #[test]
    fn the_reasons_are_the_agreed_wording() {
        assert_eq!(
            Blocked::ByUser.to_string(),
            "Windows has Snipdeck switched off under Settings > Apps > Startup. Switch it on there."
        );
        assert_eq!(
            Blocked::ByPolicy.to_string(),
            "Your organisation's policy keeps Snipdeck from starting with Windows."
        );
    }

    #[test]
    fn registered_means_on() {
        for (state, on) in [
            (TaskState::Enabled, true),
            (TaskState::EnabledByPolicy, true),
            (TaskState::Disabled, false),
            (TaskState::DisabledByUser, false),
            (TaskState::DisabledByPolicy, false),
        ] {
            assert_eq!(is_registered(state), on, "{state:?}");
        }
    }

    #[test]
    fn a_task_start_is_hidden_unless_the_user_wants_the_panel() {
        assert!(start_hidden(true, false));
        assert!(!start_hidden(true, true));
        assert!(!start_hidden(false, false));
        assert!(!start_hidden(false, true));
    }

    #[test]
    fn updates_go_to_the_store_when_packaged() {
        assert_eq!(
            updates_url(true, "https://x"),
            "ms-windows-store://downloadsandupdates"
        );
        assert_eq!(updates_url(false, "https://x"), "https://x");
    }
}
