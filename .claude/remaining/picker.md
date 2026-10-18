# picker agent: V1-PLAN step 5 (Picker + TemplateFill windows)

Worktree: .claude/worktrees/agent-picker, branch agent-picker.

## Done
- placement.rs (pure place/place_popup/place_toast + PopupHost trait + FakePopups), unit tests.
- platform: ScreenRect, TargetTracker::{register_popup, begin_text_entry_in, caret}, windows/screen.rs.
- popup_window.rs (Shell), picker_binding.rs, fill_binding.rs, toast_binding.rs + toast_window.slint.
- main.rs wiring (PickerUi, FillUi, ToastUi); controller picker_edit / target_caret.
- tests/picker_window.rs, tests/fill_window.rs written.

## Next
- compile, fix, run the tests; i18n extract + check; codemap; gate lines; commit.

## Blocked
- nothing.

## Windows-unverified
- caret read (GetGUIThreadInfo + ClientToScreen), work area and DPI (MonitorFromPoint, GetDpiForMonitor),
  SetForegroundWindow for the Picker on hotkey, non-activating style on the popups, drag, multi-DPI move.
