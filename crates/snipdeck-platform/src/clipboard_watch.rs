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

/// Whether the markers on the clipboard say the content must stay out of history:
/// `ExcludeClipboardContentFromMonitorProcessing` present (any value), or
/// `CanIncludeInClipboardHistory` present with the value 0.
#[must_use]
pub fn excluded_by_markers(
    exclude_from_monitoring_present: bool,
    can_include_in_history: Option<u32>,
) -> bool {
    exclude_from_monitoring_present || can_include_in_history == Some(0)
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
        // (monitor marker present, history value, excluded)
        let cases = [
            (false, None, false),
            (false, Some(1), false),
            (false, Some(0), true),
            (true, None, true),
            (true, Some(1), true),
        ];
        for (monitor, history, excluded) in cases {
            assert_eq!(
                excluded_by_markers(monitor, history),
                excluded,
                "{monitor} {history:?}"
            );
        }
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
