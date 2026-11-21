# fix-audit-ux remaining work
Branch claude/fix-audit-ux. Decisions are in DESIGN.md (chords, names, confirm/undo, "Not done" list at the end).

## Done, pushed and gated
- UX-1..45 all fixed with tests, except: UX-40 (recorded as a Slint limit in DESIGN.md); UX-34 and other Windows-only behaviour (unverified until run on Windows).
- Recent "Open Settings" deep link: test in crates/snipdeck-app/tests/settings_window.rs.
- fr board-editor footer: first two rows whole at the minimum size, all locales (ux_fit_c).
- ux_matrix passes; clippy 1.97 and 1.100 clean; docs build with -D warnings; i18n strict ok.

## Open
- Nothing. The a11y matrix does not cover Panel, Main window, toast, Picker, unlock windows (stated in its header).
