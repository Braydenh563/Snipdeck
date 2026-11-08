mode: continue
status: active
max-continues: 300
max-idle-minutes: 60
started: 2026-11-07 (resumed)

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
- [ ] Symbols library (slices 1-5 + 7 docs done; slice 6 menu/Save as/palette/setting + About line left) (spec first): emoji, kaomoji, icons; search, one click copies/pastes, add to a list, icons copy as SVG/PNG or save; licence-clean data; colour emoji rendering checked
- [ ] Carry the Panel v2 look to Main lists, Picker, command palette, editors, menus and Settings (type tiles, quiet rows, search, toasts), spec addendum first
- [ ] v1.0.1: Rebuild-from-backup safety (own blob dir for the set-aside db, WAL checkpoint on close, crash-safe move marker; .claude/inbox-archive.md): plan 407e8ec; store layer + review fixes on wt/rebuild (ae73f2c); app side written uncompiled, toast wording + app tests left
- [ ] v1.0.1: compact dock + dock grips (U22/O28), sidebar resize (O54), header select and delete (O49/O50)
- [ ] v1.0.1: paste or drop to create an item, Ctrl+C on items (O47 a-c; branch claude/o47-abc-wip), Main quick-add
- [ ] v1.0.1: multi-row drag, toast Edit button, UX-C (O30/O26/O31), U9, A25, search suggestion dropdown (Picker, Panel, Main)
- [ ] Recent captures images, files and rich text (owner 2026-10-26), with size caps and the privacy markers
- [ ] v1.1: placeholders {uuid} + date maths, paste transforms, richer fill-ins
- [ ] v1.1: importers (TextExpander, Espanso, PhraseExpress, Beeftext): backend merged 947c9b4; Settings import chooser wiring + Note warnings display left
- [ ] v1.1: groups inside a list (O46), tips window
- [ ] v1.1: typed-trigger expansion (opt-in, off by default, honours Sensitive + never-paste list)
- [ ] v1.1: suggest-as-you-type at the caret (opt-in), rich paste, Windows Hello unlock
- [ ] Final: full gate, polish pass (ux-auditor + blind-tester on Panel, Settings, palette, shortcuts, toasts), docs, morning summary
