# fix-tray-autostart

## Done
- P12 three-state autostart (Off / On / SwitchedOffInWindows via StartupApproved\Run), stale Run path rewrite at start-up, enable() clears the Windows-disabled record.
- P14 async StartupTask (request_startup_task, cached status); DisabledByUser and DisabledByPolicy map to their own toast and Settings text.
- P17 file dialogs on a dedicated STA thread with the owner handle; answers return to the UI thread via ui_thread::back_on_ui. All app callers converted (settings import/export, editor Browse, icon Save as, test fakes).
- P18 DONTADDTORECENT flags built in one place (portable, tested; cfg(windows) test asserts equality with the windows crate constants).
- P23 AddRetry back-off state machine (injected ticks), shared by start-up and TaskbarCreated.
- P24 icon reload plus NIM_MODIFY on WM_DPICHANGED and TaskbarCreated.
- P25 NIN_KEYSELECT debounce, and a Settings toggle "Double-click opens the Main window" (setting `tray_double_click_opens`, default on; off makes a single click act at once). Strings in .pot and all six .po (i18n check --strict passes).
- Verified natively: snipdeck-app lib (1295), settings_window, editor_controller, main_window tests pass; platform Windows-target check passed earlier.

## Next / blocked
- Clippy and scripts/gate.sh --changed run last (see report).
- Workspace Windows check is impossible here (no mingw gcc); app cfg(windows) code is unverified by compile.

## Windows run list (all unverified)
- Tray: 10 sign-ins with autostart on; restart explorer.exe 5x; scale 100% to 200%; Win+B then Enter/Space opens the Panel once; with "Double-click opens" off a single click acts at once; with it on a double click opens Main and a single click still works after the double-click time.
- Autostart: disable in Task Manager, reopen Settings (not plain "on"); move the portable folder and start it (Run value repoints); toggling on in Snipdeck clears the Windows-disabled record.
- MSIX: consent prompt; disable in Settings > Apps > Startup then toggle (toast explains); sign in again, app starts hidden.
- Dialogs: clipboard auto-clear on time with Export open; hotkey works with dialog open; drag the dialog over the owner (no unpainted areas); exported file not in shell:recent.
