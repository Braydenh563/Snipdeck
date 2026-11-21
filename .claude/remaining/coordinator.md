# Coordinator state (2026-11-16 14:08 UTC)

PR #5 head pushed (tools merge, 1.99 clippy fixes, CodeQL keyring fix). CI: Windows, audits ok; Linux and CodeQL Rust were running.

## Open CI
- CodeQL: 1 new critical alert on 1c45c00 (likely hard-coded crypto value, keyring.rs NFC tests). Read the alert in the Security tab; fix with runtime-random stems or dismiss as test-only. Commented on PR #5.

## Areas
- data: DONE, 7d3c212 on claude/fix-audit-data (toast queue added), gate green. Next: Sonnet reviewer (no checkout) on a1db682..7d3c212, then merge into PR #5.
- perf: f5306da done; perf pt2 session_01NKjURujwKd9MubADsQ4Ane on PERF-8 (pushed 66d0bce, fixing a test compile), PERF-4/11/19.
- main: pt4 session_01NKjURuzGHcwKpr8pazWMZj on all deferred MAIN items + 10 binding-audit findings (GATE_BASE=506333b).
- panel: 0f54f85; told to un-defer Import/Export/Back up/Empty Trash palette commands and PANEL-43.
- platform, UX, library, extras: working; awaiting reports.
- tools, privacy: merged.

## Merge recipe
toast_texts.rs ids + VARIANTS; union .po hunks, `python3 scripts/i18n.py extract` + `check --strict`; `python3 scripts/codemap.py`; clippy +1.99.1 (CARGO_TARGET_DIR=/tmp/target-199, delete after); gate --changed; push. Disk fills fast: prune /tmp/target-panel/debug/deps test binaries.

## Final
blind tests, gate --full, green CI, HANDOVER morning summary, Windows checklist for owner, retro.

## Paused 14:5x UTC (owner request)
Told to push WIP, update remaining file, and wait: main pt4 session_01NKjURuzGHcwKpr8pazWMZj, panel session_01NKjURu2Ui2oqJRKoriWaT6, platform session_01NKjURtegHP2MFAY8JHxrGw, UX session_01NKjURrwFsjEnJjdxN6rPuQ, library session_01NKjURvL8svpNDsnWjzGkp8, extras session_01NKjURvnHFVMoYZ2xeg3MSJ. Resume each with send_message "resume from your remaining file".
Done, awaiting review+merge: data 7d3c212 (claude/fix-audit-data), perf 923f2ba (claude/fix-audit-perf).
CodeQL alert 98: keyring.rs:500 hard-coded password in the NFC test.
Pause confirmed: platform 1a037fa (WIP, merged rounds not gated together; disk tight); panel ac77643 (WIP after gated d58fff5). Panel still lists 21, 27, 43 and the Import/Export/Back up/Empty Trash palette commands as post-1.0 in SPEC: on resume, tell it again to un-defer them.

## Resumed 2026-11-21 07:30 UTC
All six resumed. CodeQL 98 fix 027abc90 (random stem, code points); gate33 running, pushes if green (log in scratchpad gate33.log).
extras fa4963d: 15/15 done except owner CI edits EXTRAS-2/13; auditing main.rs 260-870 + strings.
library 4b0dcca WIP: LIBRARY-1..56 code in; told to do 43/48 (not post-1.0); 29/30 accepted post-1.0.
Next: review+merge data 7d3c212 then perf 923f2ba into PR #5.
