# tmpl agent: LIBRARY-13..25 (remaining work)

## Done
- Core engine (snipdeck-core template*): all of 13..25 with tests `library_13`..`library_25` (template/findings.rs).
- Platform: regional.rs (+ windows GetLocaleInfoEx, unverified), restore_forced, copy_sequence, fakes.
- App: fill model (enabled, number, choice default, locale dates, calendar), selection capture, nested snippets
  (StoreSnippets), key pieces paste, clipboard history, editor help; app lib tests green (1313).
- Slint: MonthGrid + calendar button, editor chips/help, form texts; i18n 6 locales translated (check --strict ok).
- SPEC 5.4a, user guide, CODEMAP.

- snipdeck-ui tests and goldens (fill, fill_logic, month_grid, month_grid_logic, audit_final1_keys, editors);
  UI walk via goldens (light, dark, high contrast, 225%, Sunday start, narrow editor help).
- fmt and workspace clippy clean (-D warnings).

## Next
- gate --staged, commit, report. (doc check clean; full workspace test run: only panel_window paste-as list and
  placeholder_align_logic counts failed, both updated for the 12 transforms / 19 placeholders and green.)

## Blocked / unverified
- Windows: GetLocaleInfoEx, Ctrl+C selection capture, Win+V history read, key presses between pieces.
- Not measured: real-window calendar interaction, min window size with the grid open, 1.5 text scale pixels.
