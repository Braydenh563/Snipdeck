# editor (V1-PLAN step 8: ItemEditor binding + New snippet)

## Done
- platform: `file_dialog.rs` (trait `FilePicker`, `FakeFilePicker`, pure filter/path helpers + tests), `windows/file_dialog.rs` (`GetOpenFileNameW`, cross-checked with `cargo check --target x86_64-pc-windows-msvc -p snipdeck-platform`), windows crate feature `Win32_UI_Controls_Dialogs`.
- app: `item_editor.rs` (view/input/problem types, link/title/template/text checks, `read_source`, PNG preview), `drafts.rs` (drafts/ folder, 2 s `AUTOSAVE_EVERY`), `controller/editor_ctl.rs` (open new/existing, save for all kinds, key provisioning, drafts, Pro gate, browse), `editor_binding.rs` (window binding + autosave timer), `key_provider.rs` (DPAPI on Windows, none elsewhere), `platform::file_picker`, Library trait key-ring methods.
- wiring: Panel kebab/Ctrl+E/F2 via `Effect::NewItem/EditItem` in `binding.rs::apply`, Main window `new-item-requested` + `MainAction::Edit`, `Controller::picker_edit` (Picker window itself is not bound in the app yet).
- leftovers: crash toast "Open log" button (`Controller::toast_action`, `set_log_opener`), `Binding::redraw` replaces the `hotkey_problems(&[])` nudge.
- Slint: new-item Image/File kinds with path field + Browse, banner, privacy error, restored notice, encryption note; po files translated; goldens regenerated.
- tests: tests/editor_controller.rs, tests/editor_window.rs, ui editors_logic.rs additions.

## Next
- (see final report)

## Blocked
- Windows-only behaviour unverified: DPAPI provider, GetOpenFileNameW dialog, owner window handle, focus_window raise.
