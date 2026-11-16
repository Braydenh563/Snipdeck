# fix-audit-library (coordinator) - PAUSED by owner (usage limit)
Branch claude/fix-audit-library holds merged: undo (26-28,33-42,46,47), search (1-9,50-54), tmpl (13-25), items (29 partly,30 marked post-1.0,31,32,44,45,49,56). Pushed WIP, gate NOT run after merge.
Agent runs reported clippy clean on items merge; tests passed per agent.
## In progress
- views agent (worktree /home/user/wt-views, branch claude/fix-audit-library-views, target /tmp/target-views): 10,11,12,43,48,55. Asked to commit WIP and stop.
## Next
- merge claude/fix-audit-library-views; scripts/gate.sh --changed then --full; cargo doc; check clippy 1.99 assert_is_empty lint in new tests (use assert_eq!(x.len(),0)).
- Not built (SPEC post-1.0 with reason): rope viewer, image editor, extra image formats, Replace on Image/File.
- Windows-unverified: GetLocaleInfoEx, {selection} copy, {key:}, clipboard history, CRLF, open link.
- send final per-finding report to session_01NKjURuyvhApCqnss7v9Php.

## Update: views merged (WIP, a9bcb9f), pushed. Gate NOT run.
Open on resume (from .claude/remaining/views.md):
1. Focus-order test tab_reaches_filter_sort_every_chip_and_clear_... fails at 640 wide/2.25 (Tab never reaches Filter/Sort/chips): real bug or test artefact?
2. clippy workspace, full snipdeck-ui tests --no-fail-fast, review changed goldens, cargo fmt (not run after last Slint edits).
3. 5 editor_window tests in snipdeck-app fail on base too (not ours); top toolbar overflows 480px at scale 2.25 (not ours).
4. Then gate --changed/--full, final report to coordinator session.
