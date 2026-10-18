mode: continue
status: active (owner said continue 23:16)
max-continues: 40
max-idle-minutes: 60
started: 2026-10-18 13:45
budget: owner at 40% weekly; own budget line 55%, stop line 60% (owner needs the rest for other work); one agent at a time from 55%

- [x] Merge step 4 (Panel binding: kebab, resize, quick edit) when its agent reports; gate; push
- [x] Merge step 6 (maintenance + crash log) when its agent reports; gate; push
- [x] Merge step 9 (tray icon, .ico, Open Panel fix) when its agent reports; gate; push
- [ ] Step 8: ItemEditor binding + New snippet (create/edit text, link, template, image, file; sensitive toggle with silent key provisioning)
- [x] Step 7: Settings window (hotkeys, theme, language, autostart, licence entry, Privacy, Backups, Data section)
- [ ] Step 5: Picker + TemplateFill windows on their hotkeys
- [ ] Step 7a: Onboarding (4 steps, Getting started list)
- [ ] Step 13: UX learnability U1-U9 + ux-auditor + blind-tester; fix findings. Owner (23:14): first-hide toast naming the hotkey and the tray click; Settings toggle "Show Snipdeck in the taskbar" (WS_EX_TOOLWINDOW off + taskbar button, default off)
- [ ] Step 3a: Edge dock with reveal (owner 23:20 "Do both"): docked Panel collapses to a slim handle strip on its screen edge (vertical on left/right, horizontal on top/bottom), slides out on hover and/or click (Settings: trigger hover/click/both, hover delay, leave delay), collapses when the pointer leaves or after a paste; handle always on top, non-activating, keyboard via the Panel hotkey; edge remembered per monitor; Linux tests on an injected clock; Windows hit-test unverified until the owner runs it
- [ ] Step 7b: Help & troubleshooting window
- [x] Step 11a: security review items 1-11,13 (4e73f6d)
- [ ] Step 11b: security review item 12 (Moved-event clamp, taskbar work area) after step 8
- [ ] Step 11: PR4 review items 2,5,7,8,9,10,12 (14 done except deny m2-harness [!] CI edit)
- [ ] Step 13a: de-slop pass over every window golden against DESIGN.md
- [ ] Step 19: docs (README, user guide, CHANGELOG, HANDOVER rewrite)
- [ ] Step 20: reviewer on the whole diff since main; fix findings
- [ ] Gate --full; PR #5 body refreshed with the three gate lines; morning summary; retro
