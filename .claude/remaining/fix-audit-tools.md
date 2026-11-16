# fix-audit-tools remaining (branch claude/fix-audit-tools)
Source: .claude/audit/tools.md (TOOLS-1..63).

## Done
- Colour crate TOOLS-1..14, 31 (tests: crates/snipdeck-colour/tests/audit_tools.rs, saved_recents_picker.rs).
- Colours tab app layer + slint 15..30 (controller/tests/colours.rs, tests/colours_window.rs), accent 4.5:1 (accent.rs), chip-menu Remove fix.
- Eyedropper platform 32-43 (merged) and host side (settle, readout, also, key-busy toasts, hide/restore main).
- Symbols 44..63 (merged), SPEC Symbols section.
- More formats… popup (copy_extra_format test + 3 harness tests), no-hue scheme note.
- i18n: 3 new strings translated in six locales, check --strict passes.
- TOOLS-45 live equality test: scripts/test_symbols.py (SNIPDECK_LIVE=1) plus manifest/lock hash test.
- TOOLS-58 and Tailwind-nearest, custom template editor, hex-case UI: SPEC post-1.0 list with reasons.
- Not-reached: screen_clip.rs (2 tests added: wraparound, pre-existing picture), colour_block.slint (read; every cell is one button with label and copy action), generator scripts (77 python tests pass), Colours tab at text x1.0/1.5/2.25 (new harness test).

## Next
- gate --changed, cargo +1.99.1 clippy, Windows check, push, retro, report.

## Blocked / left to others
- TOOLS-54 Down key in panel.slint search field (still emits symbol-move(3); fix is `symbol-enter-grid-ticks += 1`): PANEL-7/8/11 key block belongs to the panel worker, not touched.
- Symbols footer shows the name only; no emoji/character group names in i18n.tsv.
- Windows-only code unverified: eyedropper cloak wait, DISPLAYCONFIG struct, DIB reads, touch, fallback hotkeys; Symbols fonts.
