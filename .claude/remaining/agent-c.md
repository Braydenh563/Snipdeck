# agent-c remaining work

## Done (in the working tree or committed; see git log)
- Bindings audit: SET-1, HELP-1 (doctor checks on a worker thread, "Checking..." state, read-only second store handle), SET-2, TOAST-1, SET-3, HELP-2, HELP-3, HELP-4, HELP-5 (DoctorTexts global, six locales), TOAST-2.
- MAIN-43: hold by text length, error guard (4 s), message history (last 10) in Help > Recent messages; keyboard focus on a toast button already pauses it.
- MAIN-44: Cancel takes the focus first in destructive dialogs (decision appended to .claude/now.md).
- MAIN-56: focus returns to the Help / View / Sort button after the menu closes.
- MAIN-57: list scope and dialog scope carry role and label, cursor row is the list's value, root scope is not a Tab stop.
- MAIN-58: 24px splitter target, splitter is the Tab stop right after the sidebar.
- MAIN-60: selected-row action icons use the highlight-text colour (pressed or selected, not hovered); hidden actions are not in the tree.

## Next
- Nothing planned. The coordinator runs the full gate.

## Blocked
- Nothing.

## Windows run
- Narrator reads the name and role when focus enters the item list ("Item list", cursor row as value) and a dialog (title + body); the cursor row is exposed only as the list's value (Slint has no list-item focus delegation from a sibling scope).
- Help > Recent messages dialog read by Narrator; toast text length hold feels right at real reading speed.
- High-contrast Black/White: selected row action icons (Use, Copy, Edit, More) readable on the highlight; hover state (window fill) shows the window-text glyph.
- Splitter: 24px strip drag and Tab order (sidebar, splitter, toolbar) with real Tab key; strip does not steal clicks from the sidebar's right-edge controls or the list scrollbar.
- Doctor window: checks run on a worker (Help > Troubleshoot) with a real library and no hang while the store is busy.
