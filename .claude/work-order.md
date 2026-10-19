mode: continue
status: active (owner said continue 23:16)
max-continues: 40
max-idle-minutes: 60
started: 2026-10-18 13:45
budget: owner at 40% weekly; own budget line 60%, stop line 65% (owner: wrap up close to 65%, not 5% under); one agent at a time from 60%

- [x] Merge step 4 (Panel binding: kebab, resize, quick edit) when its agent reports; gate; push
- [x] Merge step 6 (maintenance + crash log) when its agent reports; gate; push
- [x] Merge step 9 (tray icon, .ico, Open Panel fix) when its agent reports; gate; push
- [x] Step 8 (merged f4d4d18; gate pending): ItemEditor binding + New snippet (create/edit text, link, template, image, file; sensitive toggle with silent key provisioning)
- [x] Step 7: Settings window (hotkeys, theme, language, autostart, licence entry, Privacy, Backups, Data section)
- [x] Step 5 (merged; gate pending): Picker + TemplateFill windows on their hotkeys
- [ ] Step 7a: Onboarding (4 steps, Getting started list)
- [ ] Step 13: UX learnability U1-U9 (plus tier A of .claude/remaining/polish-backlog.md and the owner's 23:52 findings in now.md) + ux-auditor + blind-tester; fix findings. Owner (23:14): first-hide toast naming the hotkey and the tray click; Settings toggle "Show Snipdeck in the taskbar" (WS_EX_TOOLWINDOW off + taskbar button, default off)
- [ ] Step 3a: Edge dock with reveal (owner 23:20 "Do both"): docked Panel collapses to a slim handle strip on its screen edge (vertical on left/right, horizontal on top/bottom), slides out on hover and/or click (Settings: trigger hover/click/both, hover delay, leave delay), collapses when the pointer leaves or after a paste; handle always on top, non-activating, keyboard via the Panel hotkey; edge remembered per monitor; Linux tests on an injected clock; Windows hit-test unverified until the owner runs it
- [ ] Step 7b: Help & troubleshooting window
- [x] Step 11a: security review items 1-11,13 (4e73f6d)
- [x] Step 11b: security review item 12 (Moved-event clamp, taskbar work area) (compile pending)
- [x] Step 11: PR4 review items 2,5,7,8,9,10,12 (2 already fixed; compile pending) (14 done except deny m2-harness [!] CI edit)
- [ ] Step 13a: de-slop pass over every window golden against DESIGN.md
- [ ] Step 19: docs (README with real screenshots per now.md 02:55, user guide, CHANGELOG, HANDOVER rewrite)
- [ ] Step 20: reviewer on the whole diff since main; fix findings
- [ ] Gate --full; PR #5 body refreshed with the three gate lines; morning summary; retro

## Leftovers from the stopped agents (2026-10-19 05:10, integration branch 8fe541d)
- [ ] Integration gate: cargo check, test, clippy, doc on the merged tree; fix what the merge broke
- [ ] Goldens: regenerate only the ones the merged work changes (settings-*, empty-*, main-*, editor-*), read each
- [ ] Step 13 item 7: first-hide toast naming the hotkey and the tray click (toast window now merged)
- [ ] Step 13 tier A + audit-13a items not yet done (ux.md lists done: A8, A23, audit 1, audit 5); audit 9 menu focus ring needs a design call
- [ ] Picker: decide whether the fill form's live preview masks a sensitive template (default: mask, reveal on hold Space like lists)
- [ ] codemap regenerate; CHANGELOG lines for steps 3a, 5, 8, 13
- [ ] Owner on Windows: editor (DPAPI key, file dialog, raise), picker (caret, focus, popups), dock (strip hover, DPI, taskbar), ux (taskbar buttons, icons, work-area sizing, hide on blur)

### From the control inventory (compile batch)
- Panel: `reveal-requested` (Hold Space on a sensitive card) is not wired; the "Hidden. Hold Space to reveal." hint promises it. Wire it.
- Panel: `context-menu-requested` / `pinned-context-menu-requested` declared, not forwarded by ItemCard, not wired (binding.rs:368). Build the card menu or remove the declarations.
- ListTabBar `dropped(index)` has no consumer: wire drop-onto-tab or remove.
- Verify accessible labels on Segmented, SelectField, NumberStepper, SearchField, HotkeyRecorder.
- [ ] Merge review blocker: all 8 settings-*.png goldens were reverted by merge d9ad7c7 and miss the Window behaviour group and Backups stepper: UPDATE_GOLDEN=1 regenerate in the test batch and read each.
