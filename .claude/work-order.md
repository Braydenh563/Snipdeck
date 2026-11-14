mode: continue
status: active
max-continues: 40
max-idle-minutes: 60
finish-up-line: 97%   # owner: "until 97% weekly usage"
started: 2026-11-14 13:40

- [x] Push d0186e01 render merge + Store docs once gate --changed and 1.99.1 clippy pass
- [x] colour-tool (merged locally, push pending gate): worker tidy (tests below use super::*), merge PR head; re-verify, merge, gate, push
- [~] owner-bug-sweep (round 2 merged aa4a232b, push pending gate; follow-up auto-scroll + group-header top-half drop awaited): worker fixes (Main grid drop dead zones, Main list failing-at-base test, strict panel_logic assertion); re-verify, merge, gate, push
- [ ] store-ready: worker (quick-check cuts, free-only Store build, clipboard first-run ask, manifest locales + MaxVersionTested, privacy.md, store-listing.md, logo, screenshots); reviewer + ux check, merge, gate, push
- [ ] Task 5 (CHANGELOG + user-guide colours/quick add/reorder done e1616e4c a3ff026b; Recent first-run text waits on store-ready): CHANGELOG/SPEC/user-guide entries for colour tool, preview toggle, docking, render fix, quick add, reorder, Store build; release-build dispatch (Windows build); gate --full; CI green on PR #5
- [ ] Handover morning summary + retro
- [ ] NOT without owner go: Fable full-app audit, blind usability tests
