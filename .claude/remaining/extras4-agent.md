# EXTRAS-4 agent

Done: NameTexts (label, icon, spoken key names) and StarterTexts globals; wording::{item_label_name,
list_icon_name, key_words, starter_words}; starter.rs takes StarterWords; first run moved after
language::apply in main.rs; library_is_untouched is structural; translations in 6 locales;
CommandTexts/ColourTexts new-list-name replaced by StarterTexts.
Next: run full tests/clippy/doc, codemap, goldens only if a picture changed.
Blocked / not done: shortcuts.rs Custom/Plain `spoken` strings ("Tab while the suggestion hint
shows", ...) are still English consts; main.rs "creating the Panel: {e}" stays English (no window
exists yet, it is only the detail of the translated startup_failed text).

Status at hand-over: snipdeck-app lib + all tests except settings_window passed (1310 lib tests, panel_window 93,
main_window 44+, name_texts 3, etc.). Full clippy/doc/ui tests could not finish: other in-progress work
(trash_days in main_binding.rs/settings, unused import in maintenance.rs, ui tests/locale_fit.rs) breaks
the build. Re-run `cargo clippy --workspace --all-targets --locked -- -D warnings`, ui tests and doc once that lands.
