# EXTRAS-4 agent

Done: NameTexts (label, icon, spoken key names) and StarterTexts globals; wording::{item_label_name,
list_icon_name, key_words, starter_words}; starter.rs takes StarterWords; first run moved after
language::apply in main.rs; library_is_untouched is structural; translations in 6 locales;
CommandTexts/ColourTexts new-list-name replaced by StarterTexts.
Next: run full tests/clippy/doc, codemap, goldens only if a picture changed.
Blocked / not done: shortcuts.rs Custom/Plain `spoken` strings ("Tab while the suggestion hint
shows", ...) are still English consts; main.rs "creating the Panel: {e}" stays English (no window
exists yet, it is only the detail of the translated startup_failed text).
