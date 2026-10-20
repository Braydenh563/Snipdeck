# ux-b (Sonnet impl agent, worktree /tmp/fable-uxb, branch claude/v1-fable-uxb)
## Done
- U1 Panel keyboard: PopupHost gets `release`; `Controller::bind` takes the host; ShowPanel (hotkey/tray/second launch) activates after the loop turn; search-focus-gained activates; HidePanel releases before hide. Dock hover never emits ShowPanel. UNVERIFIED ON WINDOWS (SetForegroundWindow / end_text_entry; first --hidden start ordering vs attach_panel).
- U3 + U10 failure toasts: `failure::Cause`; `ToastMessage::{UseFailed,BackupFailed,RestoreFailed,ImportFailed,ExportFailed}(Cause)`, `DailyBackupFailed(Cause)`, `TrashPurgeFailed`; `Jobs::backup` returns `JobError` (locked-library deferral stays quiet). `purge_expired_trash` (controller/main_ctl.rs, maintenance.rs) now returns Result so TrashPurgeFailed can occur. NOT done: 'Last backup: <date>' in Settings > Backups.
- U5 Alt+1-9 only (Panel, Picker, shortcuts sheet, item-card a11y label, user guide).
- U7 Panel row double-click no longer edits.
- U12 `binding::refreshed_model`: Panel and Main rows update per row (set_row_data) when the count is unchanged. Test: main_window.rs a_selection_change_updates_the_rows_in_place...
- U14 (non-settings): F1 opens the Guide from ItemEditor, List/Board editors, Picker and fill form (help-requested -> main_open_help). Tests f1_opens_the_user_guide_* in editor_window/picker_window/fill_window. Tag manager has no F1 (not asked).
- U15 search tiebreak: score, then use_count, then last_used, then position (core search + index_cache).
- U16 "New snippet" -> "New item".
- U17 Ctrl+P pins in Panel (`pin_row` in controller/panel.rs), shown in menu and shortcuts sheet.
- U18 Copy icon button beside the kebab on cards; deck_logic test rewritten.
- U21 Onboarding Esc = Back on steps 2-4, Skip on step 1; taskbar row text says the panel is out of the taskbar and Alt+Tab by default.
- i18n extracted, check exits 0 (translations left untranslated).
## Goldens regenerated, NOT committed (UPDATE_GOLDEN=1); pixels differing before regeneration (mix of lead's F6 and my changes)
deck-light 2719, deck-dark 2723 (the other deck-* pngs were rewritten too); onboarding-panel-light 4345; main-rail-light 455;
panel-copy-toast-dark 388, panel-scale-light 390, panel-dark 388, panel-awkward-compact-light 260, panel-light 387, panel-pinned-overflow-light 387, panel-scrolled-light 869, panel-app-menu-light 270;
picker-dark 3531, picker-copy-board-light 3601, picker-empty-light 3163, picker-awkward-light 2481, picker-light 3601, picker-scale-light 431, picker-scrolled-light 3712.
`git status` lists the 40 modified PNGs (deck, main, onboarding, panel, picker); the extra ones changed within the allowed tolerance.
## Blocked (controller.rs)
Nothing blocking. Optional: controller.rs still uses the unit `ToastMessage::Failed` for organise failures (no cause).
## Notes
- Windows-only behaviour (U1 activation) is unverified.
- Shared box OOMs on snipdeck-ui builds with -j 2: use -j 1.
