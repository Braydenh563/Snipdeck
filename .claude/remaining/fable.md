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
5. Ticketed for HANDOVER: A11 paste/backup/import off the UI thread, A13 Library trait for backup/import/erase, A27 duplication, B5 existing_hashes one pass, B6 verify key record before UPDATE, B12 items cap pre-parse/holds_exactly, B13 file splits (L4), F4 Supporter mailto (done 2026-10-20: the owner supplied the address; app constant, pinned test, site links), F6 kebab on touch.
6. PR from claude/v1-fable into claude/compassionate-pascal-pod8ux; message the hub.

## Blocked
- controller.rs edits until D5 merges.
- Windows runtime behaviour: everything in snipdeck-platform/src/windows, A1, A5 key injection, A17, A21.

## Owner's extra checks (word for word, added at the hub's request; each is checked and either fixed, ticketed with an id, or marked blocked with the reason)
- O1 "Can you make sure there are no small missing key features or functions users would expect but that aren't there?" -> the product/UX audit (U1-U21); its "checked, works" list closes the rest.
- O2 "Make sure the app is properly optimised with no high complexity or constant redraws etc." -> A8-A10, A25, U12 (redraws and index rebuilds); idle cost checked (dock tick and 6 h job only).
- O3 "there are no bugs, no security flaws" -> B1-B13, A1-A7, A14-A23, I4/I5 closure; checked-clean lists in fable-analysis.md.
- O4 "the architecture is the best shape and design etc it can be" -> A11, A13, A27, B13 (after D5).
- O5 "There is proper information architecture, maximal and best ui especially UX design and structure and function across the app." -> U13 settings order, U14 help reach, U16 naming, DESIGN.md checks (F-items).
- O6 "Best usability, learnability, efficiency, easy navigation." -> U1, U5, U7, U14, U17, U21.
- O7 "Maximised user satisfaction when using the app, it doesn't negatively affect the user or any other applications or anything else in any way" -> U4, U19 (hotkeys other apps own), A22 (clipboard read only when needed), A6/A7 (clipboard clear).
- O8 "it has proper ui and ux design following principles like CARP (contrast, alignment, repetition, proximity etc) and more." -> F-items (WCAG/contrast checked clean), the kebab slot fix (F6), U18.
- O9 "Proper accessibility." -> U2 text size, F2 toast hold, F3/F5 text-scale, WCAG checks in fable-analysis.md.
- O10 "Learnability is a big thing." -> U14, U21, first-hide toast and tour (checked).
- O11 "not losing any data or work" -> B1, A2-A4, A14, A15, U3, U9.
- O12 "not having a hard time trying to use it or figure out how to use it" -> U14, U21, U10 (error toasts say what to do).
- O13 "like moving windows around as they block my other work or it taking up too much space on my screen etc." -> dock settings checked (edge, delays, width); U8 pinned state persisted.
- O14 "like is there a feature for me to be able to alt tab to it?? Users might prefer to access it that way." -> exists: Settings "Show Snipdeck in the taskbar" (Alt+Tab entry); U21 tells the user in the tour.
- O15 "What makes an application truly professional and refined is all the small things usually overlooked but that add up." -> the Low lists (A14-A27, B7-B13, F5-F6, U16-U21).
- O16 "if I can't trust the app in it's reliability" -> B1, U3, A3, A17.
- O17 "how it can be used (can't be too overcomplicated)" -> U13, U16.
- O18 "must protect data and not negatively affect the user" -> B3-B6, A18-A21, A6/A7.
- O19 "must be easily accessible and dismissable" -> U1, U6, Esc paths checked in every window.
- O20 "must be customisable in function for to the user's preferences" -> U11 per-program paste key, U2 text size, hotkeys/dock/theme checked.
- O21 "be able to easily and very fast flick it open and closed and quickly make changes" -> U1 (keys work after the hotkey), U5, A25 (cold start).
- O22 "like can I easily edit list items from the small panel?? I should be able to." -> exists: Ctrl+E, F2, the row menu (Panel quick edit); U7 removes the double-click that pasted first.
- O23 "Can you yourself audit some key areas to make sure opus and sonnet audit agents haven't missed anything?? Or like give them really really detailed instructions on exactly what to look for." -> done: the paste pipeline and clipboard restore, idle lock against open editors, dismiss and focus return, draft persistence (no new defect; U1 confirmed from the audit); each agent brief names the lines to verify.
- O24 "Are you using haiku to be more token saving and efficient?" -> Haiku for translations and docs; Sonnet for fixes; this session for judgement calls and merges.
- Blocked: Windows runtime behaviour (every `[unverified]` item) until the owner runs the Windows test script.
- O25 (hub, owner ask verbatim) "should there be a slightly larger space between the item text and the categories to make it a little easier to read?" -> item-card.slint `label-gap` = Space.s4 (16 px, was 8) between the title/preview and the list chip or use label, in the Panel and Main rows (shared ItemCard); goldens regenerated with the kebab slot fix.
- O26 (hub, owner ask verbatim) "add to the ui/ux review just to do a quick double check for overly tight spacing and proximity and that it doesnt impact readability, not too excessive but just right" -> sweep every window (Panel, Main list and cards, editor, Settings, dialogs, toasts, tray menu) for cramped gaps between adjacent text runs, chips and icons and for unrelated items grouped too close (CARP proximity); fix only where it hurts readability, on the spacing token scale, without inflating density; measure bounding boxes in the test harness at 100 % and 225 %; goldens regenerated once with the result. Queued for a Sonnet agent when an agent slot frees.
