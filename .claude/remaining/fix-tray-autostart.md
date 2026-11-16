# fix-tray-autostart

## Done (platform crate: native tests and Windows-target check passed earlier; Windows behaviour unverified)
P12 status/rewrite_if_stale/clear_approval; P14 async StartupTask; P17 dialog thread and callbacks; P18 DONTADDTORECENT flags; P23 AddRetry; P24 DPI/TaskbarCreated icon reload; P25 KeySelect debounce and double_click_opens in TrayModel.

## App side (committed as WIP, NOT compiled)
ui_thread::back_on_ui; converted callers: settings_binding (pick_export_folder, pick_import, autostart), editor_binding::browse, IconSave::ask and binding.rs, test fakes. cargo check of snipdeck-app was OOM-killed twice compiling snipdeck-ui (memory shared with other workers).

## Next
1. cargo check -p snipdeck-app --all-targets -j2 when no other rustc runs; fix errors; run settings_window, main_window, editor_controller, symbols and ui_thread tests.
2. fmt, clippy, scripts/i18n.py check (no new msgids expected), gate.sh.
3. P25: no Settings toggle for double_click_opens (needs core field, UI, 7 languages).

## Windows run list (all unverified)
- Tray: 10 sign-ins with autostart on; restart explorer.exe 5x; scale 100% to 200%; Win+B then Enter/Space opens Panel once.
- Autostart: disable in Task Manager, reopen Settings (not plain "on"); move portable folder and start (Run value repoints); toggling on clears the disabled record.
- MSIX: consent prompt; disable in Settings > Apps > Startup then toggle (toast explains); sign in, starts hidden.
- Dialogs: clipboard auto-clear on time with Export open; hotkey works with dialog open; no unpainted areas when dragging; exported file not in shell:recent.
