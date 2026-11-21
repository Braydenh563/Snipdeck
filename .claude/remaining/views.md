# views (LIBRARY-10/11/12/43/48/55): state

Everything in scope is built and merged on claude/fix-audit-library: core Sort/ViewFilter, store V12 to V15, portable 1.3,
app, UI, strings, SPEC, user guide; group colour; nested tags; the Tab-order fix (a collapsed Search input made Slint skip
every later stop: Search has a minimum width and the toolbar sheds Undo/Redo at `room < 340`). The Tab-order test is
`tab_reaches_filter_sort_every_chip_and_clear_in_reading_order_and_scrolls_a_chip_into_view` in
`crates/snipdeck-ui/tests/library_views_walk.rs` (7 size/scale combinations incl. 640 wide at 2.25).

## Still open
- Panel drawing of group colour and nesting: `GroupHeader.colour` is sent, but `panel.slint` draws neither the dot nor an indent
  (PANEL-* area). To finish: `has-dot`/`dot` on the Panel's `GroupHeader`, `group-colour` and a depth on `PanelRow`.
- Editor tag chips and the filter's tag chooser show the bare tag name, not the path (cosmetic: names are unique).

## Unverified
- Windows-only behaviour (owner runs it on Windows). The tag manager dialogs' and group colour dialog's own focus order is not
  covered by the Tab walk.
