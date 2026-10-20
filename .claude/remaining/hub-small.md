# hub-small
## Done
- Step 1 merges (uxb, uxa final 0ec9cfe); conflicts resolved.
- Step 2 compile fixes (menu_form_texts, Monitor taskbar: None in settings/picker/panel/fill tests).
- Step 3 paste rules patch + test.
- Step 5: U6 (controller.rs on_foreground_changed), U8 panel_pinned, A6 retry, A7 written_sequence, F2 TOAST_ERROR_HOLD 10 s, A22 uses_clipboard, A23 count_failed_line trace, organise-failure cause (ChangeFailed). U13 Licence after Backups.
## Skipped
- U9 (undo until next change + Ctrl+Z, needs reverse ops for move/reorder and keys in two windows: > 150 lines).
- A25 (lazy index after first show: PanelModel/IndexCache refactor, unmeasured).
## Next
- goldens (step 4), translations (61 msgids x locales), UX-C (O30 minimal, O26, O31).
