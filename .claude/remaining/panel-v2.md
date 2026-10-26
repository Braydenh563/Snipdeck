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
Slice 5 committed as 48b03a0. Gate `GATE_BASE=ccb7164 scripts/gate.sh --changed` (run in a clean worktree /tmp/wt-s5, the main tree
  carries the symbols agent's uncommitted files and a Cargo.lock that does not match HEAD):
  ran: fast-checks ok changed-tests ok codemap ok i18n ok i18n-tests ok
  failed: none
  skipped: none
Slice 5 (Compact + Cards) content: logic tests green (panel_logic 111), goldens panel-compact/cards light+dark,
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

Slice 6b (agent, this session): committed 3a0a932 (1: hide after use, `Controller::after_use`; unpinned only, pinned stays, so with
  panel_pinned default true a new install must unpin; spec 4 note updated), 1f13dd6 (3: empty list hides the New item row),
  8ab4ff8 (4: menu order Undo, Redo, Dock, Shortcuts, Commands, Settings, then the rest; bold kebab dots-three-bold, 2.50:1 -> 5.47:1 light, 6.33:1 dark;
  row More in item-card still uses the pale dots-three).
Next: (2) search marks in the preview and "tag: name" when only a tag matched: needs core `SearchIndex` to name the matched tag
  (Hit score tier >> 24 == Tag), Row.preview_mark/tag_hit, PanelRow props, item-card preview mark span like the title one (qr/qc), @tr("tag: {}").
  Also not done: "Paste stack" row in the Panel menu; Panel menu golden fixture is a shorter list than APP_MENU.
  An_unpinned_panel_hides test failed once in a multi-filter run (passes alone); re-check for flakiness.

Slice 6b items 2 and 13 finished (agent, later session): 2706bb3 (search marks the preview match; "tag: <name>" when only a tag matched; Hit.tag,
  Row.preview_mark/tag_hit, PanelRow preview-before/preview-match/tag-hit; sensitive rows never; goldens panel-search-light/dark regenerated;
  the new "tag: {}" msgid is untranslated in the 6 locales, English fallback) and 0d93401 (row More glyph dots-three-bold + text-secondary:
  2.07:1 -> 5.81:1 light, 6.05:1 dark on the deck card; deck goldens pass within tolerance, not regenerated; Panel's quiet rows use RowAction, text-primary, unchanged).
  Tests run: snipdeck-core search, snipdeck-app panel::/binding::, snipdeck-ui panel/deck/main_window/panel_logic: green.
  Clippy 1.99.1 on core/app/ui: only errors are in settings_binding.rs (other agent's WIP: too_many_lines page_view, collapsible_if, semicolon at 1646); ui test targets not reached.
Still open: Paste stack row in the Panel menu; Panel menu golden fixture shorter than APP_MENU; the copy action glyph in comfortable cards is still text-tertiary.

Slices 7 and 8 (agent, this session): committed (WIP commit with code + .po, then goldens commit).
  7: groups "Panel layout" (new FormSection before "Panel") and "Opening the Panel" (new, after "Panel": tray click, panel opens,
  after paste/copy, when Windows starts). Reused rows (shortcuts, taskbar, blur, autostart, picker side, dock) were NOT moved into
  the new group (would reshuffle existing goldens): decide. Goldens settings-panel-layout-light/-dark new (crop of a tall render);
  settings-light/-dark/-scale/-full-*/-demo/-recording changed only by the scrollbar thumb and the new rows; narrow-conflict by a 3x40 px thumb sliver.
  Page test window is now 6000 tall (the page outgrew 4000).
  8: tray_click (on_tray TogglePanel -> OpenMainWindow), panel_open_at (TargetSource::cursor/monitor_at; Controller::show reads the anchor,
  restore_frame places once; none when docked), start-visible (SettingsHost::set_autostart(on, hidden): "--hidden" unless panel_start_visible;
  re-written when the choice changes with autostart on). Spec 4 shortcut text fixed (Ctrl+Shift+Y).
  Unverified: Windows Tracker::cursor/monitor_at (monitor id there is "monitor"-based, size is taken from the Panel's current monitor);
  registry rewrite; real placement. .po files carry the other agent's strings too (extract regenerates all).
