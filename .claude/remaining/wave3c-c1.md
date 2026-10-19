# wave3c-c1 (Recent inbox)

## Done
- Store: migration V3 `recent_entries`, `RecentRepo` (injected now), tests/recent.rs (c8f2d54).
- App: `recent.rs` (RecentCapture::consider, promote, view model), `controller/recent_ctl.rs`, settings
  `recent_history` (off) + `no_history_apps`, Windows listener `clipboard_watch.rs` + `windows/clipboard.rs`
  (UNVERIFIED on Windows; type-checks on x86_64-pc-windows-gnu). Panel clock tab, Copy / Save to... / Remove (882a3a4).
- Settings: "Keep a Recent inbox of what you copy" toggle, retention note, "Clear recent history" button.

## Edits in wave-3b areas (kept minimal)
- main_window.slint: `recent-on` property, Recent section text when on.
- controller/main_ctl.rs `MainScreen.recent_on`; main_binding.rs sets it.
- panel.slint / binding.rs / tab.slint: clock tab, recent deck, menus.
- settings.slint / settings_binding.rs: new "Recent" section (toggle + clear).

## Next
- Main-window Recent section lists no entries (points to Panel clock tab); sidebar count stays 0.
- Tray "Pause history": not one line (needs TrayEntry, model, Windows menu). `Controller::set_recent_paused` exists.
- Onboarding step 4: no recent step exists; not wired.
- No-history app list has a setting (`no_history_apps`) but no Settings UI.
- Images/files not captured (text only, v1). Click copies; no paste-into-target for Recent.
- .pot/.po not updated (i18n gate fails); CODEMAP stale.

## Blocked / unverified
- Windows listener behaviour; Panel look and keyboard (no running-app measurement).
- panel_logic `space_on_a_plain_selected_row_does_not_reveal` fails ("query: " typed); its code path is untouched by C1 (not confirmed at HEAD).
- ui `editors` goldens missing (pre-existing); settings_window `every_import_warning_reads_as_a_plain_sentence` fails (fix agent's area).
