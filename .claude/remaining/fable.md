# Fable v1 sweep (F1), branch claude/v1-fable, 2026-10-20

## Done (pushed)
- Findings ranked in `docs/reviews/fable-analysis.md` "v1 sweep (2026-10-20)": 0 Critical, 6 High (B1, A1, F1, U1-U3), 31 Medium, 28 Low; `[x]` fixed, `[~]` ticketed, `[D5]` waits for the controller split.
- B1 store `synchronous=FULL` (+ test); F3/F5 Slint fixes (chooser and skeleton text-scale, Space guard).
- Backend fixes af4327c: B3 encrypted-read budget, B4 title/source_app caps, B7 seal guard, B8 .bak tmp+rename, B9 prune future dates, B10 unreadable subfolder/shard, B11 take-by-value + blob_refs helper, I4 create_new in blobs.rs, dead `into_vec`.
- F1 i18n 2b0caf8 + 56df7f8: MenuTexts/FormTexts globals, 57 msgids, all 6 locales translated, `i18n.py check` clean.
- Gate: fast-checks passed on the backend commit (staged run). The full gate + `cargo +1.99.1 clippy` on HEAD 56df7f8 was running in `/tmp/fable-gate-wt` (log `scratchpad/gate3.log`) when the hub stopped the session: NOT confirmed green. Rerun `scripts/gate.sh --full` and 1.99.1 clippy first.

- App Groups 1-3 merged and pushed (e972a85, 75cc0f8): A1-A5, A8-A10, A12, A14-A21, A24, A26, B2; backend-2 (3b87f31, 8aa983a): B3-B12, I4, licence verify.
- UX audit ranked as U1-U21 (10e8820). F6 kebab goldens: found the always-drawn kebab overlapped the trailing list label and preview (the row never reserved its slot); item-card.slint now keeps `more-reserve` right of the labels, goldens regenerating.

## Next (in order; each id is described in fable-analysis.md)
0. Running: UX-B agent (worktree /tmp/fable-uxb, branch claude/v1-fable-uxb: U1, U3 toast, U5, U7, U10, U12, U14 editors/picker/fill, U15-U18, U21) and UX-A agent (/tmp/fable-uxa, claude/v1-fable-uxa: U2, U4, U19, U11, U13, U20, U3 settings line, U14 settings). Merge each with a merge commit after its gate; inspect U18 goldens before committing them; then a Haiku translation pass for the new msgids in all 6 locales.
1. (done) App Group 1 (Sonnet agent was mid-way; its state in `.claude/remaining/app-fix.md`; uncommitted WIP may be in the tree if the session died before its commit): A8 index revision excluding record_use/Recent writes, A9 cache all_items + list labels beside the index, A10 recent_view cache, A5 `{cursor}` Left presses (Key::Left, VK_LEFT), A12 delete sync master-password fns, B2 gc_blobs daily job.
2. (done) App Group 2: A1 windows_subsystem + AttachConsole for CLI modes (High), A2 data folder not gated by `persist`, A3 saver baseline after failed salvage write, A4 erase closes item editor/queue/index cache.
3. (done) App Group 3 (Low): A14 draft on Quit/close, A24 single-shot autosave, A15 drafts sync_all, A16 sync_recent_gate after unlock_library, A17 hotkey re-insert, A18 licence read cap, A19 create_new temp names, A20 crash stderr scrub, A21 DPAPI error mapping, A26 dead loaders; restore the settings refusal-text assertion dropped by the i18n agent (hotkey_refusal_text/program_refusal_text need a SettingsPage window).
4. After D5 merges: merge release tip (merge commit), then U6 Picker close on foreground change, U8 panel_pinned, U9 undo until next change (Ctrl+Z), A6 clipboard clear retry, A7 post-write sequence, F2 error-toast hold (WCAG 2.2.1), A22 clipboard read only with {clipboard}, A23 log count_failed, A25 index after first show.
5. Ticketed for HANDOVER: A11 paste/backup/import off the UI thread, A13 Library trait for backup/import/erase, A27 duplication, B5 existing_hashes one pass, B6 verify key record before UPDATE, B12 items cap pre-parse/holds_exactly, B13 file splits (L4), F4 Supporter mailto (owner G2), F6 kebab on touch.
6. PR from claude/v1-fable into claude/compassionate-pascal-pod8ux; message the hub.

## Blocked
- controller.rs edits until D5 merges.
- Windows runtime behaviour: everything in snipdeck-platform/src/windows, A1, A5 key injection, A17, A21.
