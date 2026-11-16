# tmpl: LIBRARY-13..25

## Done
- snipdeck-core template engine: all of 13-25 (parse, render, locale, calc, transforms, snippets, keys, selection,
  clipboard by position). Tests: crates/snipdeck-core/src/template/findings.rs (library_NN_*).

## Next
- app wiring: fill.rs (active prompts, number field, choice default, locale date), fill_binding.rs, usecase.rs
  (StoreSnippets, keys paste sequence, clipboard history, selection), use_ctl.rs, item_editor/item_preview/editor_ctl,
  wording.rs (new error arms, transform names), platform (selection capture, regional_patterns Windows).
- Slint: template_field (number kind, none label, disabled, calendar grid), template_fill, form_texts, template_texts,
  menu_texts, editor help/inserter.
- i18n extract + 6 locales; SPEC template section; user-guide; UI walk screenshots.

## Blocked
- none
