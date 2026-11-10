# v11-ui (branch wt/v11-ui): slice 1, then 2a

## Done
- Slice 1: importer Settings wiring.
- Slice 2a core: {uuid}, date offsets, transform.rs (66 template tests).
- Slice 2a app: UseRequest.transform/uuids, use_item_as (fill form carries the transform), Panel "Paste as..." second level, texts in six locales.

## Next
- Run the wider app tests (panel_window, menu_form_texts, editor_window, settings_window) and `scripts/gate.sh --staged`; Windows clippy. Not run in the time box.
- Main window "Paste as..." (ItemAction in main_window.slint + chooser): not started.
- Editor chip for {uuid} (editors.slint:622 list; regenerate item-editor-template goldens): not started. detect_kind knows {uuid}.
- Help/user-guide text for offsets and {uuid}: not started.

## Blocked
- Windows-only checks 47-49 are the owner's.
