# fix-audit-ux remaining work
Branch claude/fix-audit-ux. Decisions are in DESIGN.md (chords, names, confirm/undo, "Not done" list at the end).

## Review of e9999e6 (coordinator): in progress
1. DESIGN "Confirm and undo": reworded to a convention backed by named tests (done)
2. Main root-scope Tab stop; Tab-order coverage of Main, Panel, toast, Picker, unlock in ux_matrix
3. UX-5 single shortcut table; UX-6 type-ahead uncapped and in palette and Picker
4. Undo/confirm tests confirmed present (named in DESIGN)
5. Doctor label: nothing parses "[Problem] Database"; no stale msgid in the six .po files (checked)
6. This file

## Done and gated
- UX-1..45 fixed with tests, except UX-40 (Slint limit, recorded in DESIGN.md).
- Recent "Open Settings" deep link; fr board-editor first-rows test; ux_matrix; clippy 1.97 and 1.100 clean.

## Unverified (Windows-only or not measurable here)
- UX-34 (high-contrast system colours), UX-37, UX-38, UX-45 (Narrator names/announcements), anything under tools/windows-test/.
