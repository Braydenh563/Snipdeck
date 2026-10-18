# Review of tray/system-look/settings-store (Opus). No memory-safety bug found. Fix in order, tests first where testable (FakeTray/controller/settings tests); Win32 parts cannot run here -> keep wizard steps accurate.
MED
1 main.rs:138-143 tray_available set once at startup; WinTray re-adds on TaskbarCreated (windows/tray.rs:309-317) without telling controller. Panel close/Esc then quits though an icon exists, or hides with no icon after failed re-add. Fix: tray reports icon added/removed (TrayEvent/status) -> controller.set_tray_available; FakeTray test: show fails, taskbar_created, close() must hide.
2 hidden_window.rs:236-247 + tray.rs:297-323: outside WM_CLOSE (taskkill w/o /f, Restart Manager) destroys the hidden window silently; tray + look watcher die, app continues. Fix: ignore WM_CLOSE (optionally map to TrayEvent::Quit); shut down via private WM_APP+n -> DestroyWindow.
3 platform::connect runs before the process is DPI aware (winit sets it later) -> tray icon 16px stretched, blurry menu at 150/200%. Fix: SetThreadDpiAwarenessContext(PER_MONITOR_AWARE_V2) first in hidden_window::run and/or LoadIconMetric(LIM_SMALL), or start tray after backend.
4 tools/windows-test/tests/m3-app.json: step 2 'next letter still goes into Notepad' cannot pass (clicking tray makes Shell_TrayWnd foreground) - reword/remove expectation; step 3 note: after the right-click menu SetForegroundWindow(hidden window) (tray.rs:359) leaves an invisible window focused so keys go nowhere (consider restoring focus to previous foreground window); step 6 'Esc in empty search box' needs panel keyboard focus which the Panel does not take -> mark skip/known gap.
LOW
5 app paths.rs:34-44 strip leading BOM (\u{feff}) before parsing settings (Notepad adds it).
6 main.rs:72 load_settings (may rename to .bad) runs before the single-instance check -> second launch can move the running copy's file. Load after the check.
7 failed settings read makes defaults the 'saved' baseline -> later change overwrites user file (loses no_paste_apps). Keep a flag 'load_failed' and refuse to overwrite / back up first.
8 effect_handler in main.rs untested; resume-hotkeys uses stale startup chords. (note/test what is cheap)
9 hidden_window.rs:146-150 SAFETY comment overclaims; after loop if IsWindow still true clear USERDATA + DestroyWindow before freeing Box (ghost icon on WM_QUIT/-1).
10 hidden_window.rs:166-178 class keyed by name only: assert one name per handler type.
11 elevated app would not get TaskbarCreated (UIPI): ChangeWindowMessageFilterEx(..MSGFLT_ALLOW) - cheap, add.
12 debounce wall-clock set back: re-arm logic note/fix if cheap.
13 docs: SPEC.md:130 tray menu list vs ADR/code (Open Panel / Open Snipdeck / Pause hotkeys / Quit); ADR-0008 'we already own the message window' outdated.
