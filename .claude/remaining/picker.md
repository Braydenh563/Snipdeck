# picker agent: V1-PLAN step 5 (Picker + TemplateFill windows)

Worktree: .claude/worktrees/agent-picker, branch agent-picker.

## Done
- placement.rs (pure place/place_popup/place_toast + PopupHost trait + FakePopups), unit tests.
- platform: ScreenRect, TargetTracker::{register_popup, begin_text_entry_in, caret}, windows/screen.rs.
- popup_window.rs (Shell), picker_binding.rs, fill_binding.rs, toast_binding.rs + toast_window.slint.
- main.rs wiring (PickerUi, FillUi, ToastUi); controller picker_edit / target_caret.
- tests/picker_window.rs (15 pass), tests/fill_window.rs (8 pass), app lib (278 pass), platform (138 pass).
- Compiles clean (cargo check --all-targets, no warnings); fmt clean; i18n extract/check 0.
- Security pass: the Picker row masks sensitive previews; a sensitive template with no title no
  longer puts the first line of its body into the fill form's title (usecase::template_source).

## Next
- Owner: run the Windows-unverified list below on Windows (tools/windows-test/).
- The fill form's live preview shows the filled text of a sensitive template (the user chose it
  to fill); decide in review whether it should be masked.

## Blocked
- nothing.

## Windows-unverified
- caret read (GetGUIThreadInfo + ClientToScreen), work area and DPI (MonitorFromPoint, GetDpiForMonitor),
  SetForegroundWindow for the Picker on hotkey, non-activating style on the popups, drag, multi-DPI move.
