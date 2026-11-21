//! Using an item: the request and what actually happened (SPEC §5.1a).

/// What the user asked a use to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseMode {
    /// Write the item to the clipboard, paste it into the target, then restore the previous
    /// clipboard.
    Paste,
    /// Like [`UseMode::Paste`], but with the plain-text format only (`Alt+click`).
    PastePlain,
    /// Write the item to the clipboard and leave it there (`Shift+click`, or no target).
    Copy,
}

/// What a use actually did; drives the toast and the card-pull feedback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseOutcome {
    /// The item was pasted into the target.
    Pasted,
    /// The item was pasted, but the user's earlier clipboard could not be put back (the snapshot
    /// or the restore failed), so the item may still be on the clipboard: callers treat it as a
    /// paste for the toast and still arm the clear timer for a sensitive item.
    PastedNotRestored,
    /// The item was copied, as asked.
    Copied,
    /// A paste was asked for, but the item was copied instead.
    CopiedInstead(PasteFailure),
}

/// Why a paste fell back to a copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteFailure {
    /// The target runs elevated, so Windows blocks input to it (UIPI).
    TargetElevated,
    /// The target went away or did not accept the paste.
    TargetUnavailable,
}
