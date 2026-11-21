# fix-audit-ux remaining work
Branch claude/fix-audit-ux. Decisions are in DESIGN.md (chords, names, confirm/undo, "Not done" list at the end).
RESUMED on coordinator request. Batch C gating in progress (see Gate status).

## Done and pushed (gate green through 9b7163c)
- UX-1..4, 21 (tests/ux_keys.rs), 5, 19, 22, 25, 26, 27, 6 (batch B), 7..13, 23, 24, 28, 30, 31, 32, 33, 36, 39
- UX-20: list, board, Clear list and editor Delete act at once with Undo (no dialogs)
## Pushed but UNCHECKED (WIP, no gate run): batch C, commits d0977e9 and 4b1d94e
- names, tag-delete dialog, tour step, minimum sizes, wrapping grids, wording lint, contrast, a11y matrix, toast punctuation, tag width tests
- agent's own report (done/recorded per finding) not delivered yet; see .claude/remaining/ux-batch-c.md
## Next when resumed
1. Read ux-batch-c.md; wait for or re-run `cargo test -p snipdeck-ui --test ux_fit_c`
2. Full checks: fmt, clippy -D warnings, i18n check --strict, codemap, affected test binaries (one at a time, --test-threads=2), then `scripts/gate.sh --changed`; regenerate goldens only for deliberate visual changes
3. Remaining: UX-14..18 remainder, 29, 34, 35, 37, 38, 41..45 per batch C; UX-40 record as Slint limit
4. Final report to session_01NKjURuyvhApCqnss7v9Php: head SHA, gate lines, per-finding status
## Blocked / not done
- UX-40 (no inert modal in Slint 1.18); Windows-only items (high contrast, Narrator) unverified

## Gate status (batch C, head after this commit)
- pass: hc_contrast, wording_lint (reworded 'read Windows clipboard history' in 6 .po + settings_window test), ux_fit_c tag-manager footer
- FAIL ux_fit_c: list/board editor and tour step 3 'across the footer' (a11y bounds ignore scroll clipping; both screens scroll above the footer, so likely test artifact; I did not drop the check because that weakens it - needs owner call: add a scrolls flag, or measure the viewport)
- FAIL ux_matrix: tag manager and settings Tab 1 and onboarding Tab 7..80 'changed nothing on screen' (focus not moving or focus ring invisible; not yet diagnosed); list editor 'no name' fixed in the test by setting icon-names (unrun)
- NOT RUN: primitives_logic tag test, toast punctuation, editors_logic, main_window_logic, app tests (toast_texts, settings_window, tour, onboarding, doctor), clippy, codemap, gate
