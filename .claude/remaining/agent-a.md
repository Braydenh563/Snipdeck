# agent-a remaining (PAUSED by coordinator, usage limit)

## Done
- MAIN-14 shared tile height (probe removed), MAIN-29 cached entries / group map / keyed row reuse + test
- MAIN-23 board field in list editor, Move chooser (Board · List); tests in main_controller.rs
- MAIN-21 ThumbCache + thumb-wanted queue; tests for bounds (no real clock)
- MAIN-22 Detail pane + setting main_detail_open; golden + fit + a11y-name tests
- i18n six locales applied; SPEC 5.3 / §15 and DESIGN Detail row updated
- Clippy clean (snipdeck-app, snipdeck-ui, all targets, -D warnings)
- Passing earlier: lib tests, main_controller (115), ui main_window goldens, main_window_logic, editors, main_fit

## Next
- Re-run `cargo test -p snipdeck-app -p snipdeck-ui --locked --tests` (last run killed by pause).
  Only known failure was snipdeck-app main_window add_list_sizes_the_editor_to_its_content:
  wanted height 454 with the new Board field; bound raised 440 -> 520 (uncommitted at pause, committed as WIP). Unconfirmed.
- Check app screenshot/settings_window/repaint_hook and ui audit_final1/ux_fit/repaint/tooltips/deck results; update goldens if the list editor changed.
- cargo fmt --all, scripts/gate.sh --changed, final commit.

## Blocked
- Nothing, except slow shared-target builds (/tmp/target-main lock).

## Windows run
- Narrator reads the Detail pane ("Details" group, "name: value" fields, Preview)
- Thumbnail look of image rows on Windows
