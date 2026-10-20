//! Clipboard change events for the Recent inbox (SPEC 5.6): what the listener reports and the
//! pure rule for the "keep me out of history" markers.
//!
//! The Windows listener lives in `windows::clipboard` (unverified on Windows until the owner
//! runs it); this module has the parts that run anywhere.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// One clipboard change, read on the clipboard thread.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClipboardChange {
    /// The text on the clipboard; `None` when there is none, when the source asked to be left
    /// out (the text is then never read), or when the clipboard stayed busy.
    pub text: Option<String>,
    /// The executable that owns the clipboard, when it could be found.
    pub source_app: Option<String>,
    /// The source set `ExcludeClipboardContentFromMonitorProcessing` or
    /// `CanIncludeInClipboardHistory` = 0.
    pub excluded_format: bool,
    /// Snipdeck's own clipboard window wrote it.
    pub own: bool,
}

/// What a listener calls with each change (on the clipboard thread).
pub type ChangeSink = Box<dyn Fn(ClipboardChange) + Send>;

/// The on/off switch of a listener: it reads nothing while the switch is off, so a user who
/// has not opted in has no clipboard text read at all.
#[derive(Debug, Clone, Default)]
pub struct WatchGate(Arc<AtomicBool>);

impl WatchGate {
    /// A closed gate.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens or closes the gate.
    pub fn set(&self, open: bool) {
        self.0.store(open, Ordering::Release);
    }

    /// Whether changes are being collected.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// The registered clipboard format that password managers add to say "do not monitor this".
pub const EXCLUDE_MONITOR_FORMAT: &str = "ExcludeClipboardContentFromMonitorProcessing";
/// The older marker (older `KeePass` and others): present means "viewers must ignore this".
pub const VIEWER_IGNORE_FORMAT: &str = "Clipboard Viewer Ignore";
/// The DWORD format that is 0 when the content must stay out of Windows clipboard history.
pub const HISTORY_FORMAT: &str = "CanIncludeInClipboardHistory";

/// Which "keep me out of history" markers are on the clipboard, read while it is open.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Markers {
    /// `ExcludeClipboardContentFromMonitorProcessing` present (any value).
    pub exclude_from_monitoring: bool,
    /// `Clipboard Viewer Ignore` present (any value).
    pub viewer_ignore: bool,
    /// The value of `CanIncludeInClipboardHistory`, when present.
    pub can_include_in_history: Option<u32>,
}

/// Whether the markers on the clipboard say the content must stay out of history: either
/// presence marker, or `CanIncludeInClipboardHistory` present with the value 0.
#[must_use]
pub fn excluded_by_markers(markers: Markers) -> bool {
    markers.exclude_from_monitoring
        || markers.viewer_ignore
        || markers.can_include_in_history == Some(0)
}

/// The value of a `DWORD` format's bytes (little-endian); `None` for fewer than four bytes.
#[must_use]
pub fn dword(bytes: &[u8]) -> Option<u32> {
    let head: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
    Some(u32::from_le_bytes(head))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_markers_decide_exclusion() {
        // (monitor marker present, viewer-ignore present, history value, excluded)
        let cases = [
            (false, false, None, false),
            (false, false, Some(1), false),
            (false, false, Some(0), true),
            (true, false, None, true),
            (true, false, Some(1), true),
            (false, true, None, true),
            (false, true, Some(1), true),
            (true, true, Some(0), true),
        ];
        for (monitor, viewer_ignore, history, excluded) in cases {
            let markers = Markers {
                exclude_from_monitoring: monitor,
                viewer_ignore,
                can_include_in_history: history,
            };
            assert_eq!(
                excluded_by_markers(markers),
                excluded,
                "{monitor} {viewer_ignore} {history:?}"
            );
        }
    }

    #[test]
    fn the_older_viewer_ignore_marker_alone_excludes_a_change() {
        let markers = Markers {
            viewer_ignore: true,
            ..Markers::default()
        };
        assert!(excluded_by_markers(markers));
        assert_eq!(VIEWER_IGNORE_FORMAT, "Clipboard Viewer Ignore");
    }

    #[test]
    fn a_dword_needs_four_bytes() {
        assert_eq!(dword(&[0, 0, 0, 0]), Some(0));
        assert_eq!(dword(&[1, 0, 0, 0, 9]), Some(1));
        assert_eq!(dword(&[1, 0, 0]), None);
    }

    #[test]
    fn the_gate_starts_closed_and_is_shared_by_clones() {
        let gate = WatchGate::new();
        let other = gate.clone();
        assert!(!gate.is_open());
        other.set(true);
        assert!(gate.is_open());
    }
}
