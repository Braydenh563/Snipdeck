# wave3b (branch claude/v1-wave3b, PR into claude/compassionate-pascal-pod8ux)

## Done
- B14 sensitive clipboard clear: `Settings.clipboard_clear_secs` (30, 0 = off, max 600), Privacy row in Settings, SPEC 5.7 + glossary.
- B10 reduced motion: `Settings.reduce_motion` + Windows `SPI_GETCLIENTAREAANIMATION` (`SystemLook.reduce_motion`), effective value `Controller::look()`, set on every window's `SystemLook` global; Settings toggle.
- B4 undo: toast hover pauses (`Controller::toast_hover`), Ctrl+Z (`undo-requested`) in Main + Panel.
- B8 shortcuts: Ctrl+Shift+N (new list), Ctrl+W (close), Ctrl+, / Ctrl+N in Panel; sheet, palette, user guide.

## Next
- gate, PR, report.

## Blocked / left
- Ctrl+D: wave 3a (B6 Duplicate) owns it; not bound here.
- Hub's extra queue (B5, B7, B11, C4, More-button hover bug) arrived by cross-session message, outside the original brief: not started.
- Windows-only parts unverified: SPI_GETCLIENTAREAANIMATION read, Toast hover on the non-activating toast window.
