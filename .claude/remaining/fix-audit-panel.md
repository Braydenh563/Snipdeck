# fix-audit-panel (branch claude/fix-audit-panel, from claude/compassionate-pascal-pod8ux)

Env: `. /tmp/env.sh` (CARGO_TARGET_DIR=/tmp/target-panel, low debug). Order from owner: fix every PANEL-n that reproduces; test first, fixed clock.

## Done (uncommitted until first commit line below says otherwise)
- PANEL-1 destructive never wins a tie (commands.rs)
- PANEL-6 palette JSON parsed once; mark carried from entries
- PANEL-7 field-owns-edit-keys guard in panel.slint
- PANEL-8 Ctrl+F on Symbols/Colours focuses their field; Ctrl+N no-op there
- PANEL-9 ListsMenu has Symbols + Colours rows, header names the open view
- PANEL-11 leave-list-view shared by both tabs
- PANEL-30 name only ("Open Snipdeck Panel"); 6px thickness is the DESIGN §5 exception (record in report)
- PANEL-37 pin/close names (IconButton a11y-name/description)
- PANEL-42 Picker title

## Next (in order)
PANEL-2/3/4/5/10 palette+nav, 12, 13/14/27 platform, 15, 16-22, 23-26/40/41, 28, 29, 31-36, 38, 39, 43-45 (+SPEC notes)

## Blocked
none
