# Fable v1 sweep (F1), branch claude/v1-fable, 2026-10-20

## Done (pushed)
- Findings ranked in `docs/reviews/fable-analysis.md` "v1 sweep (2026-10-20)": 0 Critical, 3 High (B1, A1, F1), 19 Medium, 22 Low; `[x]` fixed, `[~]` ticketed, `[D5]` waits for the controller split.
- B1 store `synchronous=FULL` (+ test); F3/F5 Slint fixes (chooser and skeleton text-scale, Space guard).
- Backend fixes af4327c: B3 encrypted-read budget, B4 title/source_app caps, B7 seal guard, B8 .bak tmp+rename, B9 prune future dates, B10 unreadable subfolder/shard, B11 take-by-value + blob_refs helper, I4 create_new in blobs.rs, dead `into_vec`.
- F1 i18n 2b0caf8 + 56df7f8: MenuTexts/FormTexts globals, 57 msgids, all 6 locales translated, `i18n.py check` clean.
- Gate: fast-checks passed on the backend commit (staged run). The full gate + `cargo +1.99.1 clippy` on HEAD 56df7f8 was running in `/tmp/fable-gate-wt` (log `scratchpad/gate3.log`) when the hub stopped the session: NOT confirmed green. Rerun `scripts/gate.sh --full` and 1.99.1 clippy first.

## Next (in order; each id is described in fable-analysis.md)
1. App Group 1 (Sonnet agent was mid-way; its state in `.claude/remaining/app-fix.md`; uncommitted WIP may be in the tree if the session died before its commit): A8 index revision excluding record_use/Recent writes, A9 cache all_items + list labels beside the index, A10 recent_view cache, A5 `{cursor}` Left presses (Key::Left, VK_LEFT), A12 delete sync master-password fns, B2 gc_blobs daily job.
2. App Group 2: A1 windows_subsystem + AttachConsole for CLI modes (High), A2 data folder not gated by `persist`, A3 saver baseline after failed salvage write, A4 erase closes item editor/queue/index cache.
3. App Group 3 (Low): A14 draft on Quit/close, A24 single-shot autosave, A15 drafts sync_all, A16 sync_recent_gate after unlock_library, A17 hotkey re-insert, A18 licence read cap, A19 create_new temp names, A20 crash stderr scrub, A21 DPAPI error mapping, A26 dead loaders; restore the settings refusal-text assertion dropped by the i18n agent (hotkey_refusal_text/program_refusal_text need a SettingsPage window).
4. After D5 merges: merge release tip (merge commit), then A6 clipboard clear retry, A7 post-write sequence, F2 error-toast hold (WCAG 2.2.1), A22 clipboard read only with {clipboard}, A23 log count_failed, A25 index after first show.
5. Ticketed for HANDOVER: A11 paste/backup/import off the UI thread, A13 Library trait for backup/import/erase, A27 duplication, B5 existing_hashes one pass, B6 verify key record before UPDATE, B12 items cap pre-parse/holds_exactly, B13 file splits (L4), F4 Supporter mailto (owner G2), F6 kebab on touch.
6. PR from claude/v1-fable into claude/compassionate-pascal-pod8ux; message the hub.

## Blocked
- controller.rs edits until D5 merges.
- Windows runtime behaviour: everything in snipdeck-platform/src/windows, A1, A5 key injection, A17, A21.
