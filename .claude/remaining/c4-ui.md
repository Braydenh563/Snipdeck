# c4-ui (branch claude/v1-wave3b)

## Done
- Piece 1 list icon: 24 Phosphor SVGs, `ListIcons` global (ui/list-icons.slint), `icon: image` on TabItem/ChoiceEntry, sidebar list row icon next to the 8px mark, IconPicker in ListEditor (`icon-index`, `icon-names`), `main_editor_save(name, accent, icon, sort)`, wording::list_icon_name. Tests: main_controller icon round trip, editors_logic icon group, panel_logic tab name with icon.

- Piece 2 item colour label: `label`/`label-name` on PanelRow/MainRow, 4px bar + description on ItemCard, label picker in ItemEditor (ColourPicker now takes `hues`), EditorInput.label + draft, design/tools/palette.py checks the 8 hues on hover/selected fills (all >= 3:1).

## Next
- Piece 3 filter chip, 4 DESIGN/SPEC/user-guide.

## Blocked
- none

## NEEDS HUB
- none
