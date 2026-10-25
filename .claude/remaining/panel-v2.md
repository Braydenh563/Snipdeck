# panel-v2 remaining work (spec: docs/reviews/panel-v2-spec.md, section 5)

Done: slices 1, 2, 3 committed. Slice 4 built and tested (panel_logic 97, panel 25, deck, app lib + panel_window green);
last step is gate --staged + the slice-4 commit (WIP commits so far are squash-free: the final commit is "Panel v2 slice 4").
Next: slice 5 (Compact and Cards: rows 36 / cards 149x96, 2 columns at >= 288, goldens panel-compact/cards light+dark,
  panel-cards-280-light, regenerated panel-awkward-*), then slice 6 (Menu switcher, goldens panel-lists-menu-light and -open-light).
  Slice 5 must keep a golden of the real Compact layout. Row tooltips: any new row instance must report `tip` through
  `row-tip-text` (panel.slint) so the tip closes when rows change.
Blocked: CI step for `python3 design/tools/palette.py --check` (hook refuses edits to .github/workflows; the owner decides).
Deviations so far: search.mark light = blue-300 (spec blue-200 is 1.16:1 vs bg.selected); kind tiles light X-200 / dark X-900;
  collapsed search keeps its focus in a zero-size FocusScope `rest` (Slint drops keys for a clipped-away field), so
  `search-focused` = search or rest; pinned strip chips are hand-placed (no layout) to avoid a binding loop;
  More menu keeps all existing rows plus Undo/Redo (spec lists a shorter menu).
