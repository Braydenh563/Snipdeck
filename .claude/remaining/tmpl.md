# tmpl agent: LIBRARY-13..25 (remaining work)

## Done
- Core engine (snipdeck-core template*): all of 13..25 with tests `library_13`..`library_25` (template/findings.rs).
- Platform: regional.rs (+ windows GetLocaleInfoEx, unverified), restore_forced, copy_sequence, fakes.
- App: fill model (enabled, number, choice default, locale dates, calendar), selection capture, nested snippets
  (StoreSnippets), key pieces paste, clipboard history, editor help; app lib tests green (1313).
- Slint: MonthGrid + calendar button, editor chips/help, form texts; i18n 6 locales translated (check --strict ok).
- SPEC 5.4a, user guide, CODEMAP.

## Next
- snipdeck-ui tests: month_grid_logic, fill_logic, fill, audit_final1_keys, editors; goldens regenerate if changed
- UI walk (sizes, scales, themes, focus order) via screenshot tests
- fmt, clippy, gate, commit

## Blocked / unverified
- Windows: GetLocaleInfoEx, Ctrl+C selection capture, Win+V history read, key presses between pieces.
