# v11-ui (branch wt/v11-ui): slice 1, then 2a

## Done
- Slice 1: importer Settings wiring.
- Slice 2a core+app: {uuid}, date offsets, transform.rs, Panel "Paste as...", six locales.
- Merge of claude/compassionate-pascal-pod8ux (i18n resolved by re-extract).
- Review fixes: fill preview "(new ID)" for {uuid} (test added); slug/title NFC + combining marks (tests);
  Espanso single .yml choice (PickKind::Yaml, choice 9, six locales, tests).
- Main window "Paste as..." (ItemAction.paste-as, chooser, MainAction::PasteAs, main_paste_as) with tests;
  {uuid} editor chip; user-guide text.
- Tests written: panel_window paste-as name/keys/Esc, use_paste template+use_item_as, main_window_logic x3, main_controller.

## Next (NOT RUN: the shared target dir lock was held by wt-ci for hours)
- Run: ui main_window_logic, app --lib, app tests main_controller/settings_window/panel_window/menu_form_texts/fill_window, core, platform.
- Regenerate the item-editor-template golden (editors.rs, the {uuid} chip changes it), open the PNG.
- scripts/gate.sh --staged; python3 scripts/codemap.py in the final commit; Windows clippy.

## Blocked
- Windows-only checks 47-49 are the owner's.
