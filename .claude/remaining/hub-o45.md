# hub-o45 remaining work

## Done
- b: editor More options, Ctrl+Enter tooltip, setting editor_more_open
- c/d/f/g: Panel multi-select (Ctrl/Shift click, Ctrl+A, Shift+arrows, Esc), bulk bar, row More on hover/focus/selected, Move/Colour/Sensitive row menu, Show numbers, Delete with Undo
- a: quick-add row "+ New item" in the Panel (Enter, Shift+Enter, Esc, Create, kind chips, Sensitive), Ctrl+N, Ctrl+Shift+N to the full editor
- footer fix (Picker hint only on a wide Panel, min height counts header + footer) with a scale test
- Reset size and position (Panel menu + palette), test
- goldens (panel, editors), i18n for 6 locales, wizard steps 26-30

## Next
- Toast "Edit" button (controller side done: Effect::EditItem via toast_edit; Slint Toast has no Edit button yet)
- Quick-add in the Main window (component is shared: components/quick-add.slint)
- e: drag a selection (single-row drag works; selection drag and cross-tab drag of a selection are not)
- Colour chip in the quick-add box
- Merge origin/claude/compassionate-pascal-pod8ux before the last push

## Blocked
- nothing; shared disk was full several times (deleted my own big test binaries to continue)
