# ux-b (Sonnet impl agent, worktree /tmp/fable-uxb, branch claude/v1-fable-uxb)
## Done
- U1 Panel keyboard: PopupHost gets `release`; `Controller::bind` takes the host; ShowPanel (hotkey/tray/second launch) activates after the loop turn; search-focus-gained activates; HidePanel releases before hide (Esc, close, toggle, foreground change). Dock hover never emits ShowPanel. Tests in tests/panel_window.rs. UNVERIFIED ON WINDOWS (SetForegroundWindow / end_text_entry behaviour; first --hidden start ordering vs attach_panel).
- U3 + U10 failure toasts: new `failure::Cause` (from store `Trouble` + portable/store error variants); `ToastMessage::{UseFailed,BackupFailed,RestoreFailed,ImportFailed,ExportFailed}(Cause)`, `DailyBackupFailed(Cause)`, `TrashPurgeFailed`; `Jobs::backup` returns `JobError` (a locked-library deferral stays quiet); texts in toast_texts.slint, i18n extracted (168 untranslated, check exits 0). NOT done: 'Last backup: <date>' in Settings > Backups (settings agent's window); the real `purge_expired_trash` returns a count and swallows errors, so TrashPurgeFailed cannot occur from the real jobs yet (controller.rs); 'no program to paste into' is already the Copied* toasts, not an error. `Failed` (unit) text now ends with an action; controller.rs still uses it for organise failures.
## Next
U5 U7 U12 U15 U16 U17 U18 U14 U21 (in that order)
## Blocked (controller.rs)
(none yet)
## Notes
- Disk on this box is near-full (shared, ~39G cap): a build failed once with "No space left"; other targets are 18G+8G.
