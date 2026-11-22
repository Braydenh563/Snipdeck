# merge-library: remaining work (branch merge-library, worktree /home/user/Snipdeck-lib)

Merging origin/claude/fix-audit-library (ea4cadd5) into PR #5 head (db4fc55a). NOT committed yet.

## Done
- core/store/portable: resolved and staged (helper finished them). search.rs = library + Arc perf; tags.rs union (find_tag, merge_tag, link/unlink); portable import keeps reuse_existing two-phase.
- Compiles; `cargo clippy --keep-going --workspace --all-targets --locked -- -D warnings` clean (needs CARGO_BUILD_JOBS=1: snipdeck-ui clippy-driver uses ~7 GB, two at once get SIGKILLed).
- .po/.pot: ours + extract + glossary-adapted library translations; `i18n.py check --strict` was ok (1691). THIRD_PARTY_NOTICES regenerated. SPEC.md merged (schema versions shifted: group colour v14, nested groups v16).
- Toasts: 139 variants, `VARIANTS` and `position()` updated.
- Test edits worth reviewing (no assertion weakened): toast_texts second test expects typographic `Deleted the tag “work”` / `Merged the tag “work” into “Work”` (ours' wording; no Decision covers it); main_window.rs tests for the merge dialog now use `get_merge_into()` (string) instead of `get_merge_from()`; store tests/crypto.rs forges a v9 file by DROP COLUMN of the v12..v16 columns before `user_version = 9`; open.rs schema_version literal -> SCHEMA_VERSION.

## Non-trivial resolutions (app/ui, textually resolved, NOT yet `git add`ed: 26 files)
- controller.rs, host.rs, lib.rs, settings.rs, starter.rs, picker.rs, panel.rs: both sides' fields kept (StarterWords, copy_title, board chooser + nested tags, Sort).
- editor_ctl.rs, save.rs: ours' long-text parts (content-large, PART_BYTES) win; library `large: Option<u64>` removed; trigger end/case from library kept.
- main_ctl/editors.rs: list editor keeps board chooser (MovedList) + Rewrite/MoveList history steps; board editor carries accent/icon.
- main_ctl/items.rs: undoable main_delete_tag / main_merge_tag(index, into:&str) (library); ours' index merge and TagRenameError::Merge removed.
- main_window.rs: Row has depth/image_stamp/thumb; ours' IndexCache + row cache (generation) kept; GroupWalk/ViewFilter/Sort from library. Just added `generation += 1` in `set_section_sorts` (fixes stale rows after a reverse sort; test main_views::every_section_sorts...). `list_labels_of` helper extracted for clippy too_many_lines.
- main_binding.rs: ours' convert_rows kept; library tag manager (nest, colour, merge-into dialog) wired.
- index_cache.rs: Snapshot has `names: SearchNames` and lazy `index()`.
- colour_ctl.rs (not in the 26): `rename_saved_colour` (ours) now records an `Action::Edit` history step via `history_ctl::update_recorded`, so the library's history_bypass test passes.
- Slint: command_texts (only copy-title), template_field (library calendar + ours private-copy wiring), editors.slint (library tag manager dialogs; ours content-large parts mode; merge-from/merge-count removed), main_window.slint (ours' toolbar tiers + `heading-compact`; library sort menu, sidebar nest/indent), panel/picker ("Search items").

## Migration order status (coordinator's rule: never renumber PR #5's versions)
- PR #5 (97fb6251/db4fc55a) really ends at schema v10 (key record dropped); its v11 exists only in unit tests. So: v1-10 as on PR #5, then the library's 10-15 appended as 11-16 (trigram 11, trigger end/case 12, sort/tag colour/board accent 13, group colour 14, nested tags 15, nested groups 16). SCHEMA_VERSION = 16 (not 17: the coordinator's 17 assumed a PR #5 v11).
- Done in schema.rs (V10..V16 consts), migrations.rs (list, `from < 10` backup scrub, tests re-sliced, new test `a_version_10_database_migrates_to_the_latest_keeping_its_data`), fts.rs (`rebuild_v11`), docs/store.md, SPEC.md.
- Too-new test is found 17 / supported 16 (helper's edit is right).

## Left to do
1. Re-run (was running when asked): store `migrations`, `an_older_library`, `a_damaged_database`; app `history_bypass`, `main_views`, `main_window` (`renaming_onto`, `cancelling_the_merge`), app lib `colour`. Last full run (before these fixes) had 6 failures, all addressed above but unverified: migrations x2, crypto, open, history_bypass, main_views; plus main_window `renaming_onto_another_tag_asks_and_the_confirmation_merges` (merge button label "Merge" and `merge_confirmed(0, name)` - unverified).
2. Then `cargo test -p snipdeck-ui` with `UPDATE_GOLDEN=1` for affected golden tests (30 PNGs currently ours' placeholders; list in `git diff --name-only --diff-filter=U | grep png`), spot-check diffs, rerun without the flag.
3. `python3 scripts/codemap.py`, then `--check`; `cargo fmt --all --check`; `bash scripts/checks.sh`; `python3 scripts/i18n.py check --strict` (new strings may need extract).
4. `git add -A` the 26 files, commit the merge (trailers: Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>, Claude-Session: https://claude.ai/code/session_01NKjURuyvhApCqnss7v9Php). Do NOT push. Coordinator said wait for their go before committing.
5. Not verified: toolbar look (no running app), Windows clippy and cargo deny (coordinator runs them). core has unused deps serde/serde_json (same on the library branch): warning only.

## Session 2 status (WIP commit)
- Fixed: an_older_binary_refuses_a_newer_file (supported: 13 matches the [..13] slice); removed unused `APP_NAME` and `History::redo_len` (halfbuilt check clean); codemap regenerated; i18n strict ok; cargo fmt run.
- Store lib run before the fix: 148 pass, 1 fail (the one now fixed). NOT re-verified: store crypto/open/trash, app history_bypass/main_views/main_window/toast_texts/main_controller, goldens (UPDATE_GOLDEN), clippy. Cargo was starved for 2h by the shared flock queue; the queued script /tmp/ms-lib-run.sh (touches sources first) runs all of these into /tmp/ms-lib-run.out.
- Next: read /tmp/ms-lib-run.out; touch sources before each cargo run (shared target dir); UPDATE_GOLDEN for affected ui goldens and spot-check; clippy once; scripts/checks.sh.
