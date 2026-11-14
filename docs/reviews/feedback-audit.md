# Feedback audit (2026-10-24)

Goal: every user action confirms or explains failure. A scout read the source only: no cargo runs and no telemetry. Line numbers come from grep.

| Action | Where | Success feedback | Failure feedback |
| --- | --- | --- | --- |
| Paste/Copy (use_item/run_use) | use_ctl.rs:408-475 | yes (after_use) | yes; a failed refresh after a paste shows failed() (:458-459) |
| Paste next / stack | use_ctl.rs:369-406 | yes | n/a (NothingToPasteNext) |
| Quick add / save text | use_ctl.rs:217-306 | yes (Saved) | yes; a failed save_text refresh is hidden (:289-305) |
| Copy text (settings) | settings_ctl.rs:291-305 | yes | yes (ClipboardBusy) |
| Pin/unpin | panel.rs:447-494; main_ctl/items.rs:66-87 | **no** | yes |
| Duplicate | panel.rs:482; items.rs:93-133 | **no** | yes |
| Edit item (opens editor) | editor_ctl/open.rs:50-106 | n/a | yes (open_failed) |
| Trash / bulk delete | items.rs:135-159; panel_select.rs:249 | yes (Undo) | yes |
| Restore / delete forever / empty trash | items.rs:163-199, 371-390 | yes | yes |
| Move to list | items.rs:216-252; panel_ctl.rs:156-188 | yes (ItemsMoved) | yes |
| Panel drag reorder | panel_ctl.rs:192-210 -> panel.rs:239-275 | **no** | yes |
| Alt+Up/Down reorder | panel_ctl.rs:213; lists.rs:128-141 | yes | yes |
| Main drag reorder | main_ctl/lists.rs:111-233 | **no** | yes |
| Tag toggle / rename | main_ctl/items.rs:257-345 | **no** (rename shows the reason inline) | rename yes; toggle not checked |
| Label/colour | panel_select.rs:161-200 | **no** | yes |
| Bulk copy / move | panel_select.rs:95-160 | yes | yes |
| Editor save (list/board) | main_ctl/editors.rs:200-243 | **no** | yes |
| Delete list/board | lists.rs:95-108; panel_ctl.rs:387-399 | yes (Undo) | yes |
| Add list (+ tab) | panel_ctl.rs:410-421 | **no** | yes |
| Switch board/tab | panel_ctl.rs:146-153, 429-433 | no (the view change is the feedback) | yes |
| Theme | panel_ctl.rs:62-71 | yes | the save error is shown once |
| Other settings (update_settings) | panel_ctl.rs:32-48,110; main_ctl.rs:375 | **no** | the save error is shown once |
| Recent copy / save-to | recent_ctl.rs:220-288 | yes | yes; a failed save-to refresh is hidden |
| Recent delete / clear | recent_ctl.rs:242-254 | **no** | yes |
| Undo / Redo | history_ctl.rs:153-217 | yes | yes |
| Hotkey register | toast_ctl.rs:141-167 | no | yes (names the chord) |
| Export / backup / restore / import / licence / lock | lists.rs, settings_ctl.rs, lock_ctl.rs | yes | yes |

## Gaps, ranked by how often a user meets them (a guess)

1. Pin/unpin: no success message.
2. Drag reorder in the Panel and in Main: no success message. Only the keyboard Alt+Up/Down announces the move.
3. Duplicate (Ctrl+D): no success message.
4. Saving a new list or board from the editor, and the "+" add-list button: no success message.
5. Settings toggles that go through update_settings (on-top, density, recent, dock): no success message.
6. Recent delete/clear: no success message.
7. A refresh failure after a successful paste shows "failed" (use_ctl.rs:458-459). Refresh failures after save_text and recent save-to are hidden behind the success message.
8. Ignored `let _ = self.refresh_panel()` leaves the UI stale with no message: settings_ctl.rs:356,417,488,718,817 and lock_ctl.rs:443,465,499,593.
9. There is one toast slot (toast_ctl.rs:38-50), so a later toast silently replaces an earlier error. Errors should not be replaced by a success toast.

Not audited: the Windows hotkey host code and screen-clip success.
