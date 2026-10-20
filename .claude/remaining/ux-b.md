# ux-b (Sonnet impl agent, worktree /tmp/fable-uxb, branch claude/v1-fable-uxb)
## Done
- U1 Panel keyboard: PopupHost gets `release`; `Controller::bind` takes the host; ShowPanel (hotkey/tray/second launch) activates after the loop turn; search-focus-gained activates; HidePanel releases before hide (Esc, close, toggle, foreground change). Dock hover never emits ShowPanel. Tests in tests/panel_window.rs. UNVERIFIED ON WINDOWS (SetForegroundWindow / end_text_entry behaviour; first --hidden start ordering vs attach_panel).
## Next
U3 U5 U7 U10 U12 U15 U16 U17 U18 U14 U21 (in that order)
## Blocked (controller.rs)
(none yet)
## Notes
- Disk on this box is near-full (shared, ~39G cap): a build failed once with "No space left"; other targets are 18G+8G.
