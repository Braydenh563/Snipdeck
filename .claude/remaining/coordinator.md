# Coordinator state (2026-11-16 ~01:45 UTC)

PR #5, branch claude/compassionate-pascal-pod8ux. CI green on df614a06; pushed head adds checklist, inbox, gate.sh fix.

## Owner orders (in force)
- Deep audit: Fable agents, ONE at a time, owner must re-say "use fable" per launch (plugin guard; mid-turn messages don't count). If blocked: PushNotification the owner.
- No cap on findings: every finding recorded and listed. Then fix absolutely everything; tell owner when nothing is left. Owner tests on Windows after.
- Checklist: .claude/skills/deep-audit/checklist.md. Findings: scratchpad/deep-audit/<agent>.md
  (/tmp/claude-0/-home-user-Snipdeck/924dfb27-0b1b-5011-b96d-4161f926cf4e/scratchpad/deep-audit/).

## Audit progress (10 areas, map in extras.md)
- [x] extras: 15 findings EXTRAS-1..15 (language setting not applied, CI skips python checks, Plex OFL notice + MSIX licences, English-only Rust strings, terminology drift, no log file, empty crash reports, --check-db writes, half-built bits, CLI). Not reached: failure.rs ownership.rs host.rs tips.rs tour.rs demo.rs screenshot.rs main.rs 260-870.
- [ ] library: running (Fable), told: no cap, list all ids.
- [ ] panel, main, privacy, data, tools, platform, ux, perf.

## In flight
- Sonnet local agent: branch fix/sidebar-expand (sidebar toggle hidden below 720px auto-rail -> overlay drawer; help "?" vs "N items" footer gap). Verify, gate, merge.
- Cloud session_01NKjURsVa5FL9t3AAoppKaw (claude/fable2-last): Part A done (bb956d2, gate green); asked to do Part B (G28 two-phase portable API). Then Opus review (no checkout), merge.
- Cloud session_01NKjURwKPmGuEHMfw1o6HF4 (claude/scroll-tooltips, 52de0f0): 4 low follow-ups sent; merge after.
- Tell owner about PR #29 (against claude/all-free).

## After
Ranked fix list (all findings), fix by class, review, gate --full, CI green, HANDOVER morning summary, retro.
Cargo: flock /tmp/target-panel/.audit.lock; env CARGO_TARGET_DIR=/tmp/target-panel CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0.
