# wave3b (branch claude/v1-wave3b, PR into claude/compassionate-pascal-pod8ux)

## Done
- B14 sensitive clipboard clear: `Settings.clipboard_clear_secs` (30, 0 = off, max 600), Privacy row in Settings, SPEC 5.7 + glossary.
- B10 reduced motion: `Settings.reduce_motion` + Windows `SPI_GETCLIENTAREAANIMATION` (`SystemLook.reduce_motion`), effective value `Controller::look()`, set on every window's `SystemLook` global; Settings toggle.
- B4 undo: toast hover pauses (`Controller::toast_hover`), Ctrl+Z (`undo-requested`) in Main + Panel.
- B8 shortcuts: Ctrl+Shift+N (new list), Ctrl+W (close), Ctrl+, / Ctrl+N in Panel; sheet, palette, user guide.

## Next
- PR #7 is open; wait for CI/review events.
- Hub queue (B5, B7 + More-button hover bug, B11, C4) not started.

## Blocked / left
- Ctrl+D: wave 3a (B6 Duplicate) owns it; not bound here.
- Hub's extra queue (B5, B7, B11, C4, More-button hover bug) arrived by cross-session message, outside the original brief: not started.
- Windows-only parts unverified: SPI_GETCLIENTAREAANIMATION read, Toast hover on the non-activating toast window.

## NEEDS HUB
- PR #7 (B4, B8, B10, B14) is ready against the release branch. Confirm that B5, B7 (+ More hover bug), B11 and C4 should now start on this branch.

## Update (B5, B7)
- Done: B5 Getting started seed (`starter::ensure_first_run`, `library_is_untouched`); B7 More button on cards (not a Tab stop; Menu key / Shift+F10 stay), dock no longer collapses/closes a menu on pointer-leave.
- Unverified on Windows: the More-menu-disappears report (not reproducible in the testing backend; the dock fix is a hypothesis).
- Remaining UI test failures after merging the release branch look like goldens/pixels (editors, settings, empty states, focus ring, IME); `backups_to_keep_steps...`, `escape_is_skip_on_every_step`, `the_wanted_height_grows...` not diagnosed.
- Next: B11 (drag/drop + keyboard alternatives), C4 (colour label + list icon).
