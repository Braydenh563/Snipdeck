# settings (branch agent-settings): V1-PLAN step 7, Settings window

## Done
- settings.slint: Licence, Backups, Your data (export, typed-DELETE erase with key question), Privacy sections, toast overlay.
- Toast variants + wording; Store::key_record / restore_key_record; licence_store.rs (licence.key in the data folder).
- controller/settings_ctl.rs (licence, backup now, restore, export, erase); settings_binding.rs (SettingsHost, SettingsUi, bind_settings).
- Wiring: Effect::OpenSettings, Main window settings-requested, main.rs (licence at start, language at start, SettingsUi).

## Next
- (see git log for the latest state)

## Blocked
- nothing.

## TODO for step 7a (onboarding)
- TODO: `Settings` has no `onboarded` flag yet. "Erase library" leaves the settings file and opens a fresh empty library with the
  starter board (starter::ensure_board); step 7a must (a) add `settings.onboarded` and set it false in
  `Controller::erase_library` (controller/settings_ctl.rs), and (b) decide "empty library" for the first-run tour knowing
  that ensure_board always makes the starter board.
