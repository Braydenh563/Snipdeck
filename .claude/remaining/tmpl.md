# tmpl (LIBRARY-13..25): built

Core engine, platform (regional.rs, restore_forced, copy_sequence), app fill model, nested snippets, key pieces, clipboard history,
Slint calendar and editor chips/help, strings (6 locales), SPEC 5.4a, user guide, UI tests and goldens: all merged on
claude/fix-audit-library. fmt and clippy were clean at merge.

## Unverified (Windows)
- GetLocaleInfoEx, Ctrl+C selection capture, Win+V history read, key presses between pieces.
- Not measured: real-window calendar interaction, minimum window size with the grid open, 1.5 text scale pixels.
