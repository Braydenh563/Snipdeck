# Final pre-release audit (1.0.0), merge fallout first

Read-only, by code. No cargo was run (the build is held elsewhere), so nothing below was run as a test or seen in the running app. Windows-only paths were not exercised.

## Findings, most severe first

1. **medium: in the Picker, holding Space on a sensitive row shows a blank row** (`crates/snipdeck-app/src/picker_binding.rs:391-399`)
   - Scenario: the row says "Hidden. Hold Space to reveal." (`picker.slint:669`). The user holds Space, the mask goes, and the preview is empty. Press-and-hold with the pointer gives the same result.
   - Evidence: `picker_row` shows `entry.preview` when the row is revealed. For a sensitive item that field is always "": `Item::preview_of` returns "" when `sensitive` (`snipdeck-core/src/item.rs:127-129`), and the schema enforces it (`snipdeck-store/src/schema.rs:67`, `CHECK (sensitive = 0 OR preview = '')`). The Panel works because `view()` puts in the decrypted text with `usecase::item_text` (`controller.rs:772-781`). `PickerView` only carries an index (`controller.rs:847-849`).
   - Why the tests miss it: the controller test checks only the index (`controller/tests/picker.rs:304-339`). The UI test sets the preview text itself (`snipdeck-ui/tests/picker_logic.rs:811-829`).
   - Related, latent: `lock_now` clears `revealed` and `panel_revealed` but not `picker_revealed` (`controller/lock_ctl.rs:693-694`; field at `controller.rs:362`). Once the Picker does supply text, a lock while Space is held would leave it on screen.

2. **medium: a custom backup folder is ignored in two places** (`crates/snipdeck-app/src/settings_binding.rs:1986-1993` and `crates/snipdeck-app/src/paths.rs:230-232`)
   - "Open backups folder" in Settings opens `<data>/backups`, an empty folder it creates, not the folder the user chose.
   - At start-up, when the library cannot be opened and is moved aside, `has_backup` looks only in the default folder. A user whose backups are in the chosen folder reads "There is no backup to restore from yet" (`toast_texts.slint:231`) at the moment they lost data.
   - Evidence: both call `backup::backups_dir(data_dir)` or `dir.join("backups")`. `Settings::backup_folder(data_dir)` (`settings.rs:1294-1300`) is the resolver every other consumer uses (`settings_binding.rs:1360`, `help_binding.rs:441,650`, `headless.rs:132`, `settings_ctl.rs:343`).

3. **low: the docs still say deleting a group asks first** (`docs/user-guide.md:225`, `docs/user-guide.md:232`, `docs/SPEC.md:66` twice, `docs/HANDOVER.md:42`)
   - The UX merge (182ba8c7) dropped the Library side's confirmation for a folded or nested group: `group_delete`, `main_group_delete_confirm` and `main_group_delete_cancel`, the `group-delete-*` dialog in `main_window.slint`, and its tests.
   - `main_group_delete` now acts at once with Undo (`controller/group_ctl.rs:822-831`), which matches the recorded decision (`.claude/now.md:10`). The user guide and SPEC still promise a question with the item count.

4. **low: four Rust commands have no UI entry** (only tests call them)
   - `Controller::rename_saved_colour` (`controller/colour_ctl.rs:610`)
   - `drop_create` (`controller/create_ctl.rs:110`; the windows use `drop_begin`)
   - `main_show_panel` (`controller/main_ctl/lists.rs`; called only from `tests/main_controller.rs`)
   - `picker_use` (`controller/use_ctl.rs`; called only from `controller/tests/picker.rs`)
   - They are leftovers from replaced paths; their tests cover code users never reach.

## Checked and clean

- **Lost lines:** for merges 55f50fc4 and 182ba8c7, every line over 25 characters that either side added (vs the merge base, in `*.rs`/`*.slint` under `crates/`) was looked for in the result.
  - 55f50fc4 lost only intended renames: "Keep Panel on top" became "Keep Panel open", "Close Panel" became "Close panel", and the palette's Page keys went to the shared `ListNav.page`.
  - 182ba8c7 dropped the UX-20 confirmations on purpose: BoardDelete, ListDelete, ClearList and group delete. No other logic was dropped.
  - 72ad5d83 merged with no conflicts.
- **The new merge fields are all set and read:** `typed_allowed` and `SkipReason`, `undo_expansion`, `last_use_chars`, `fill_erase` (which now keeps the window, so PANEL-21 is fixed), `picker_tag`, `picker_detail`, `FormClosed`, `StackRunning`.
- **Expansion undo:** the stored `Expansion` does not keep its window (`trigger_ctl.rs:29-34`). The typing hook sends a Reset on every foreground change and mouse press (`windows/typing_hook.rs:11-12, 288-305`), and that clears `last_fire`, so a Backspace in another field does not undo. Unverified (Windows).
- **Slint wiring:** every `callback` in `crates/snipdeck-ui/ui/**/*.slint` is both handled (in Slint or by a Rust `on_*`) and invoked (script scan). The window `in` properties Rust never sets are in-out UI state or have defaults.
- **Translations:** every `@tr` string is in the .pot and the .pot has no unused entries. All six .po files have every string, none fuzzy, and the plurals are filled.
- **Shortcuts:** `CATALOGUE` has 101 rows and `ShortcutTexts.titles` has 101. The Picker's Ctrl+P, Ctrl+D, Ctrl+O, Delete, F3 and Ctrl+Tab are handled in `picker.slint` and wired in `picker_binding.rs:213-245`.
- **Earlier fixes that survived the merges:** PANEL-7 (`field-owns-edit-keys`), PANEL-37, DATA-7 (`is_library_file`), LIBRARY-35 (tag delete is undoable), DATA-9 (installing triggers on import is opt-in), sensitive items left out of a multi-item copy, the rebuild resetting history and the index cache. Migrations v11 to v16 are in sequence and tested.

## Not verified

- Nothing was compiled or run; the tests were not executed.
- Every Windows path: the typing hook, `in_password_box` (UIA), clipboard restore, and the Backspace counts for expansion undo.
- `tools/windows-test/m2-harness` after 6d7a9f33: it is not in the workspace or CI, so it was not compiled.
