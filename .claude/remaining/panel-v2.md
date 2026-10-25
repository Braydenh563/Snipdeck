# panel-v2 remaining work (spec: docs/reviews/panel-v2-spec.md, section 5)

Done: slices 1, 2, 3 and 4 committed (slice 4 = the WIP commits since 04dbdb9 plus the "Panel v2 slice 4" commit).
Step 0 (slice 4 verified, `GATE_BASE=origin/claude/compassionate-pascal-pod8ux scripts/gate.sh --changed`, HEAD 8700dd9):
  ran: fast-checks ok changed-tests ok codemap ok i18n ok i18n-tests ok
  failed: none
  skipped: none
  `cargo +1.99.1 clippy --workspace --all-targets --locked -j1 -- -D warnings` on 8700dd9: ONE error, clippy::assert_is_empty at
  crates/snipdeck-ui/tests/panel_logic.rs:1452 (`assert!(take(&log).is_empty())`, no message). Everything else is clean (run with --keep-going).
  Fix = `assert_eq!(take(&log), Vec::<String>::new())`; verified clean under 1.99.1 on a worktree; committed next as its own commit
  committed as 80d018c (gate --staged green). Slice 4 is pushable on 1.99.1 from 80d018c.
Slice 5 (Compact + Cards) done in its commit: logic tests green (panel_logic 111), goldens panel-compact/cards light+dark,
  panel-cards-280-light, panel-awkward-compact-light regenerated; no other golden changed.
Next: slice 6 (Menu switcher, goldens panel-lists-menu-light and -open-light; merge the two `changed lists-in-menu` handlers),
  then slice 6b from docs/reviews/ux-gap-audit.md section 5 (coordinator request), tests first:
  (1) close the Panel after paste/copy when opened by hotkey or tray, pinned stays; `panel_hide_after_use` default ON, update spec 4 note;
  (10) search marks matches in the preview too, "tag: <name>" when only a tag matched;
  (12) empty Panel hides "+ New item" while the empty state shows its two create actions;
  (13) Panel more-menu order per spec, fix the pale more-glyph contrast.
  Row tooltips: any new row instance must report `tip` through `row-tip-text` (panel.slint).
Blocked: CI step for `python3 design/tools/palette.py --check` (hook refuses edits to .github/workflows; the owner decides).
Deviations so far: search.mark light = blue-300 (spec blue-200 is 1.16:1 vs bg.selected); kind tiles light X-200 / dark X-900;
  collapsed search keeps its focus in a zero-size FocusScope `rest` (Slint drops keys for a clipped-away field), so
  `search-focused` = search or rest; pinned strip chips are hand-placed (no layout) to avoid a binding loop;
  More menu keeps all existing rows plus Undo/Redo (spec lists a shorter menu).

  Slice 5: Cards use List rows while a query is typed; List row gap stays 6 (spec says 4; min-h uses the spec's 4);
  Cards title starts at y=32 and the preview shows 0-2 lines depending on the title's lines.
