# wave3c-recent (C1 gaps)

## Done (uncommitted until the gate passes)
- Main-window Recent list (controller main_recent_*, MainScreen.recent_rows/targets, real sidebar count, main_window.slint list + menu + chooser, main_binding wiring).
- Tray "Pause history": TrayEntry::PauseHistory, TrayModel set_history/pick, Tray::set_history, FakeTray, WinTray (cfg windows, UNVERIFIED), controller sink + TrayEvent::HistoryPaused, main.rs wiring.
- Onboarding step 4 history opt-in switch (recent_history).
- Settings: no_history_apps list (ProgramList component shared with "Never paste into"; with_program caps 64 chars / 100 entries).

## Next
- cargo tests, clippy 1.99.1 (+ windows-gnu platform), fmt, i18n extract (+ de entries), codemap, gate --staged, commit.

## Blocked / unverified
- Windows tray menu; look of the Recent list and onboarding step 4 height (no running-app measurement).

## Final
Done: all four gaps, i18n, codemap, gate --staged green, committed locally (not pushed).
Next: none. Blocked: Windows tray item and look are unverified until run on Windows.
