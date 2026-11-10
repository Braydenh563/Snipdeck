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

## Next
- Nothing open for slice 2a. Verified: targeted suites, gate --staged, clippy 1.99.1, Windows clippy.
- Note: editor_window `a_free_user_sees_prompted_fields...` needed a key and a focus event before Ctrl+Enter once the {uuid} chip made the Template form taller (cause not found; the keyboard path is unverified in a real window).

## Blocked
- Windows-only checks 47-49 are the owner's.
