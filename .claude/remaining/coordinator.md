# Coordinator state (2026-11-16 ~01:45 UTC)

PR #5, branch claude/compassionate-pascal-pod8ux. CI green on df614a06; pushed head adds checklist, inbox, gate.sh fix.

## Owner orders (in force)
- Deep audit: Fable agents, ONE at a time, owner must re-say "use fable" per launch (plugin guard; mid-turn messages don't count). If blocked: PushNotification the owner.
- No cap on findings: every finding recorded and listed. Then fix absolutely everything; tell owner when nothing is left. Owner tests on Windows after.
- Checklist: .claude/skills/deep-audit/checklist.md. Findings: scratchpad/deep-audit/<agent>.md
  (/tmp/claude-0/-home-user-Snipdeck/924dfb27-0b1b-5011-b96d-4161f926cf4e/scratchpad/deep-audit/).

## Audit progress (10 areas, map in extras.md)
- [x] extras: 15 findings EXTRAS-1..15 (language setting not applied, CI skips python checks, Plex OFL notice + MSIX licences, English-only Rust strings, terminology drift, no log file, empty crash reports, --check-db writes, half-built bits, CLI). Not reached: failure.rs ownership.rs host.rs tips.rs tour.rs demo.rs screenshot.rs main.rs 260-870.
- [x] library: 56 findings LIBRARY-1..56 (4 high: 1,2,26,35). UI walk not reached.
- [x] panel: 45 findings PANEL-1..45 (1 high: 13 paste sleeps on UI thread). ALL code-only (disk ran out); runs owed for PANEL-1,7,9,12,24,45 + 10k perf; reproduce in fix phase.
- [ ] main, privacy: running (Fable).
- [ ] data, tools, platform, ux, perf.

## In flight
- Sonnet local agent: branch fix/sidebar-expand (sidebar toggle hidden below 720px auto-rail -> overlay drawer; help "?" vs "N items" footer gap). Verify, gate, merge.
- Cloud session_01NKjURsVa5FL9t3AAoppKaw (claude/fable2-last): Part A done (bb956d2, gate green); asked to do Part B (G28 two-phase portable API). Then Opus review (no checkout), merge.
- scroll-tooltips: MERGED (2d8ec99f), pushed, gate green.
- Tell owner about PR #29 (against claude/all-free).

## After
Ranked fix list (all findings), fix by class, review, gate --full, CI green, HANDOVER morning summary, retro.
Cargo: flock /tmp/target-panel/.audit.lock; env CARGO_TARGET_DIR=/tmp/target-panel CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0.

## Fix sessions (cloud, Sonnet; owner: start fixes as audits arrive; merge each branch after verify+gate)
- extras -> session_01NKjURvnHFVMoYZ2xeg3MSJ, branch claude/fix-audit-extras
- library -> session_01NKjURvL8svpNDsnWjzGkp8, branch claude/fix-audit-library
- panel -> session_01NKjURu2Ui2oqJRKoriWaT6, branch claude/fix-audit-panel
- Next audits: copy findings to .claude/audit/<area>.md, push, start a session per area.
Older open items: fable2-last Part B (session_01NKjURsVa5FL9t3AAoppKaw), fix/sidebar-expand (local), PR #29 (owner decision), blind usability tests, gate --full, CI, handover, retro.
- 06:20 2026-11-16: all 10 audits in. perf.md + ux.md committed (push after gate24). main fixer handed over partial at 506333b -> part 2 session_01NKjURvCgQT1PLfp8nNrme1. Next: perf + ux cloud fix sessions once pushed; sidebar fix (d4ae69cf) after ux-auditor; CI Windows fix aa114c40.
