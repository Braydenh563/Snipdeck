//! The Windows clipboard history through `WinRT` (`Windows.ApplicationModel.DataTransfer.Clipboard`).
//!
//! `IsHistoryEnabled` is checked first; then `GetHistoryItemsAsync` and, for each item that holds
//! text, `Content().GetTextAsync()` and `Timestamp()`. The async operations are waited on with
//! `join()`, so call [`read_current_session`] from a worker thread, not the UI thread. Pure
//! decisions (ticks to time, which text is kept) live in [`crate::clipboard_history`].
//!
//! UNVERIFIED: written against the documented API and checked only by compiling. It has not run
//! on Windows. Things to confirm there: history on and off, a non-text item in the history
//! (image), a pinned item, a policy-blocked machine (`AccessDenied`), and calling from a
//! thread that has not initialised COM (the `windows` crate does not do it for `WinRT` `join()`).

use windows::ApplicationModel::DataTransfer::{
    Clipboard, ClipboardHistoryItemsResultStatus, StandardDataFormats,
};

use crate::clipboard_history::{HistoryRead, entry_from, read_failure};
use crate::error::Error;

/// Reads the text entries of this session's clipboard history.
///
/// # Errors
///
/// [`Error::Os`] when a `WinRT` call fails other than by the history being off or access denied.
pub fn read_current_session() -> Result<HistoryRead, Error> {
    let enabled = match Clipboard::IsHistoryEnabled() {
        Ok(enabled) => enabled,
        Err(e) => {
            return read_failure(e.code().0)
                .map_or_else(|| Err(Error::from_windows("IsHistoryEnabled", &e)), Ok);
        }
    };
    if !enabled {
        return Ok(HistoryRead::HistoryDisabled);
    }
    let result = match Clipboard::GetHistoryItemsAsync().and_then(|op| op.join()) {
        Ok(result) => result,
        Err(e) => {
            return read_failure(e.code().0)
                .map_or_else(|| Err(Error::from_windows("GetHistoryItemsAsync", &e)), Ok);
        }
    };
    let status = result
        .Status()
        .map_err(|e| Error::from_windows("Status", &e))?;
    if status == ClipboardHistoryItemsResultStatus::AccessDenied {
        return Ok(HistoryRead::AccessDenied);
    }
    if status == ClipboardHistoryItemsResultStatus::ClipboardHistoryDisabled {
        return Ok(HistoryRead::HistoryDisabled);
    }
    let items = result
        .Items()
        .map_err(|e| Error::from_windows("Items", &e))?;
    let text_format = StandardDataFormats::Text().map_err(|e| Error::from_windows("Text", &e))?;
    let mut entries = Vec::new();
    for item in items {
        let Ok(content) = item.Content() else {
            continue;
        };
        if !content.Contains(&text_format).unwrap_or(false) {
            continue;
        }
        // One unreadable item is skipped; it should not fail the whole import.
        let Ok(text) = content.GetTextAsync().and_then(|op| op.join()) else {
            continue;
        };
        let ticks = item.Timestamp().map_or(0, |t| t.UniversalTime);
        entries.push(entry_from(text.to_string_lossy(), ticks));
    }
    Ok(HistoryRead::Entries(entries))
}
