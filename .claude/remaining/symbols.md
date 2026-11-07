# symbols agent (wt/symbols)

## Done
- save_icon split: prepare_icon_save / IconSave::ask (outside the controller) / finish_icon_save; atomic temp+rename write, io errors logged; toast::after_error for use errors; IconNotAddable toast. (commit 8b31b61)
- Panel Symbols tab: symbols.slint, panel.slint wiring (list-hidden), symbol_nav.rs, symbols_panel.rs (grid builder), symbols_binding.rs (glue), binding.rs wiring.
- Tests: controller symbols (32), symbol_nav, ui tests/symbols.rs (12, goldens 280/320/420 light, 320 dark, kaomoji, icons dark, empty), panel_window symbols test.
- i18n extract run (new strings fall back to English).

## Next
- Slice 6: context menu on a symbol (cell-menu already sets item-menu-x/y), Save as submenu (prepare_icon_save, ask outside the controller, finish_icon_save as editor_binding browse()), palette entry, toasts wired in the UI.
- Settings toggle for the Symbols tab (settings files belong to another agent): symbols_available is always true.
- Translations for the new strings (English fallback now).

## Blocked / notes
- panel_logic `the_recent_list_has_its_own_keys...` fails at HEAD already (expects Enter = rcopy; code has Enter = recent-used). Not touched.
- Hover fill on a cell not pixel-measured (hover colour did not change pixels in the harness); selection box measured 40x40.
