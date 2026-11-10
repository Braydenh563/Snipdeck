mode: continue
status: active
max-continues: 300
max-idle-minutes: 60
started: 2026-11-07 (resumed)
resumed: 2026-11-09 after idle-guard stop during a long gate run (commits c7f28a7 since)

- [x] Answer artifact comments (crushed text, type logos)
- [x] Panel design v2 on canvas: deep design pass on D, independent critique, publish for owner pick (no Panel build)
- [x] Gate + push non-Panel UX agent work (shortcuts search, Ctrl+Shift+P, theme, density, audit, palette)
- [x] Alt+Tab to the Panel as a setting (Windows-only; owner tests)
- [x] Picker placement setting relative to the caret
- [x] Dock fixes (covers a third of the screen, bottom invisible)
- [x] Feedback audit: every action confirms or explains failure
- [x] Show/hide/move/resize predictability (Reset size then drag broke list layout)
- [x] Review fixes F (toast rule, library_changed refresh, palette label, Picker docs) + gate + push
- [x] Panel v2 spec (Opus): D v3 default + layout options + access methods in Settings, research Win+V, Ditto, Raycast, PowerToys Run; docs/reviews/panel-v2-spec.md
- [x] Panel v2 build: audit 2 fixed (940ff42); item 8 goes to the shared-look slices
- [x] UX gap audit (Opus): docs/reviews/ux-gap-audit.md (d068c9d)
- [x] Windows quick check: a ~10-minute wizard mode that runs every automatable step itself and asks the owner only for eye checks; one file to send back
- [x] UX audit quick wins (audit 2,3,5,9,11,15,18): Recent click pastes; hotkey health with press-to-test + alternatives; template chips insert at caret; onboarding Floating/Docked + Recent offer; Supporter lock badges; Settings search + jump links; Open Picker in tray
- [x] Defaults: Recent on for new installs; new non-AltGr default chords (decisions in now.md)
- [x] Symbols library (slices 1-7 merged; Windows checks in quick-check 13-18) (spec first): emoji, kaomoji, icons; search, one click copies/pastes, add to a list, icons copy as SVG/PNG or save; licence-clean data; colour emoji rendering checked
- [x] Carry the Panel v2 look (spec docs/reviews/shared-look-spec.md; slices 1-7 merged b70b909, DESIGN.md 5bdfdb0; slice 8 test cleanup on wt/look8b) to Main lists, Picker, command palette, editors, menus and Settings
- [x] v1.0.1: Rebuild-from-backup safety (own blob dir for the set-aside db, WAL checkpoint on close, crash-safe move marker; .claude/inbox-archive.md): merged e075b10; Windows rename-under-lock unverified
- [x] v1.0.1: sidebar resize (O54) [compact dock, grips, O50 done]
- [x] v1.0.1: paste or drop to create an item, Ctrl+C on items (O47 a-c; slices 1-2 done); Main quick-add moves to slice 6
- [x] v1.0.1 slice 6: Main quick-add + multi-row drag (merged 01a7292)
- [x] v1.0.1 slice 8: A25, search suggestion dropdown (Picker, Panel, Main); empty-state keycaps (owner)
- [x] Recent captures images, files and rich text (owner 2026-10-26), with size caps and the privacy markers
- [ ] v1.1: placeholders {uuid} + date maths, paste transforms, richer fill-ins
- [ ] v1.1: importers (TextExpander, Espanso, PhraseExpress, Beeftext): backend merged 947c9b4; Settings import chooser wiring + Note warnings display left
- [ ] v1.1: groups inside a list (O46), tips window
- [ ] v1.1: typed-trigger expansion (opt-in, off by default, honours Sensitive + never-paste list)
- [ ] v1.1: suggest-as-you-type at the caret (opt-in), rich paste, Windows Hello unlock
- [ ] Final: full gate, polish pass (ux-auditor + blind-tester on Panel, Settings, palette, shortcuts, toasts), docs, morning summary
- [x] v1.1 light lane: slice 9 serde-saphyr + 3a groups data (merged ed7528b)
- [x] v1.1 slice 5a triggers data (merged)
- [ ] v1.1 UI lane slices 1 + 2a (wt/v11-ui)
- [ ] v1.1 slice 8 platform layer (wt/v11-hello, light lane)
