# fix-audit-ux remaining work
Branch claude/fix-audit-ux. Decisions are in DESIGN.md (chords, names, confirm/undo, "Not done" list at the end).

## Review of e9999e6 (coordinator): done, awaiting gate and push
1. DESIGN "Confirm and undo": reworded to a convention backed by named tests (done)
2. Main root scope and the unlock scopes no longer Tab stops; ux_matrix walks Main (toast), Panel (toast), Picker, unlock; Panel header at 2.0+ text keeps its close button inside 320px (goldens regenerated after inspection) (done)
3. UX-5: CATALOGUE is the one table, guide checked both ways, 12 guide rows added; UX-6: Menu and Chooser type-ahead scan by ladder (no 40-row cap); palette and Picker filter as you type (done)
4. Undo/confirm tests confirmed present (named in DESIGN)
5. Doctor label: nothing parses "[Problem] Database"; no stale msgid in the six .po files (checked)
6. This file

## Done and gated
- UX-1..45 fixed with tests, except UX-40 (Slint limit, recorded in DESIGN.md).
- Recent "Open Settings" deep link; fr board-editor first-rows test; ux_matrix; clippy 1.97 and 1.100 clean.

## Unverified (Windows-only or not measurable here)
- UX-34 (high-contrast system colours), UX-37, UX-38, UX-45 (Narrator names/announcements), anything under tools/windows-test/.
