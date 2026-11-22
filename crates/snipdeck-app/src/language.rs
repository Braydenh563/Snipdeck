//! The interface language chosen in Settings.
//!
//! Slint registers the bundled catalogues when the first component is created, so a language
//! can only be selected after that: calling it earlier fails with `NoTranslationsBundled` and the
//! window then follows the system language whatever was saved.

/// Applies the saved language `tag` ("de", "pt-BR", ...). Call it right after the first
/// component exists. `""` ("Follow Windows") does nothing: the system language is what Slint
/// picked when it registered the catalogues, and selecting `""` afterwards would force English.
///
/// # Errors
/// The reason, when `tag` names no bundled language (the interface then stays in the system's).
pub fn apply(tag: &str) -> Result<(), String> {
    if tag.is_empty() {
        return Ok(());
    }
    slint::select_bundled_translation(tag).map_err(|e| format!("no translation for {tag:?}: {e}"))
}
