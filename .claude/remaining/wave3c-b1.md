# wave3c-b1: Settings Import preview + Restore preview

## Done
- Controller (settings_ctl.rs): ImportSource/ImportKind/ImportPreview/ImportProblem, preview_import (dry run), apply_import; restore_backup goes through preview_import first. New sources: add an ImportSource variant, an arm in ImportSource::run, ImportSource::of/kind, one wording line, one Slint choice.
- UI: settings.slint "Import" row + inline preview panel (import-* properties, import-source-chosen/-confirmed/-cancelled); settings_binding.rs wiring, SettingsHost::pick_import; Restore now opens the same preview.
- Tests: crates/snipdeck-app/tests/import_preview.rs; settings_window.rs (restore test now goes through the preview, two import tests, FakeHost::pick_import).

## Touched in areas another session owns (kept minimal)
- toast.rs / wording.rs: ToastMessage::Imported{items}, ImportFailed (+ their text), wording::import_problem_text.
- settings.slint: one SettingRow + panel after "Export library"; Escape cancels an open preview before closing the page.
- settings_binding.rs: SettingsHost gained pick_import (FakeHost in tests updated).

## Next
- Translations for the new @tr strings (.po/.pot not touched here).
- Windows clipboard history source: add the variant (not done here).

## Blocked / unverified
- Windows file dialog: pick_import uses the standard Open dialog (no folder dialog exists); Folder asks for any file inside the folder. Unverified on Windows.
- UI looks not checked in a running app; no golden screenshots regenerated.
