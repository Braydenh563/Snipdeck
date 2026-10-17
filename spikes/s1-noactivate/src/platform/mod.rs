//! OS integration. Only Windows is real; other OSes get a stub so the crate still builds.

use std::fmt;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(not(windows))]
mod stub;
#[cfg(not(windows))]
pub use self::stub::*;

/// Where a click on an item goes right now (drives the Target footer).
#[derive(Clone, Debug)]
pub enum Route {
    /// Paste into the target; the label names the app and window.
    #[cfg_attr(not(windows), expect(dead_code, reason = "only Windows can paste"))]
    Paste { label: String },
    /// Only copy; the reason is shown to the user.
    Copy { reason: String },
}

impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Route::Paste { label } => write!(f, "Pastes into {label}"),
            Route::Copy { reason } => write!(f, "Copies to clipboard ({reason})"),
        }
    }
}
