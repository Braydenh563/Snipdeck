mode: continue
status: active
max-continues: 40
max-idle-minutes: 60
finish-up-line: 97%   # owner: "until 97% weekly usage"
started: 2026-11-14 13:40

- [x] Push d0186e01 render merge + Store docs once gate --changed and 1.99.1 clippy pass
- [x] colour-tool (merged locally, push pending gate): worker tidy (tests below use super::*), merge PR head; re-verify, merge, gate, push
- [x] owner-bug-sweep: rounds 1-4 merged (aa4a232b, 92aecf09 parent), reviewed twice
- [x] store-ready: merged 92aecf09 after review fixes (22176105); follow-up store-ready-2: Recent question goldens + ux-auditor
- [ ] Task 5 (docs done: e1616e4c a3ff026b 92aecf09): CHANGELOG/SPEC/user-guide entries for colour tool, preview toggle, docking, render fix, quick add, reorder, Store build; release-build dispatch (Windows build); gate --full; CI green on PR #5
- [ ] store-ready-2: review, merge, gate, push
- [ ] Handover morning summary + retro
- [!] NOT without owner go (waits on the owner): Fable full-app audit, blind usability tests
