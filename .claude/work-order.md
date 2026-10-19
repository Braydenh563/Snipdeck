mode: continue
status: active (owner /away 2026-10-19 10:35: finish the v1 ship list; owner re-confirmed away mode 12:55)
max-continues: 60
max-idle-minutes: 60
started: 2026-10-19 10:35
budget: weekly 64% at start; owner's finish-up line 80% (finish step, push, handover, stop); pro habits (one agent, no Opus) from 75%; at most two cargo users per target dir, never more than three cargo processes in total

Queue = docs/V1-PLAN.md "V1 ship list" (authoritative; tick there too).
- [~] A1-A3 merge review, windows-ui, learnability agents; gate; push
- [ ] B wave 3: agent W3a import+portability (B1 B2 B3), W3b Main polish (B4-B10, B15), W3c perf+CLI+sensitive clear (B12 B13 B14); then B11 drag and drop
- [ ] C wave 4: C1 Recent inbox; C2+C3 paste stack + master password; C4+C5 colour label, list icon, accent; C6 dock check; C7 MSIX/signing docs; C8 Get Pro mailto
- [ ] D1-D2 goldens, README shots, hero images; D3 de-slop; D4 Pages site; D5 controller split
- [ ] E1 docs; E2 pre-public sweep
- [ ] F1 Fable sweep (owner-approved, one agent); F2 Opus review; F3 ux-auditor + blind-tester; F4 gate --full, PR body, summary, retro
- [x] CI workflow line updated by the owner (c8cc4eb).
- [!] CI customisation (owner asked; guard blocks agents): copy docs/ci-proposals/ into the workflow folder (Dependabot groups, Windows clippy, timeouts, release attestation + size report).
