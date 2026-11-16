# items agent (branch claude/fix-audit-library-items)

## Done (committed)
- 31 link scheme check, 56 Open in browser, 49 title limit (200) + counter, toast button wording translatable
- 44/45 trigger fires-at (Immediate/Delimiter) + case mode (Exact/Ignore/Adapt): core, store V11, portable, app retype, editor selectors, pre-save notes
- 32 duplicate text asked before saving in the item editor (Already saved / Open it / Save anyway)
- i18n: all new strings in pot + six .po

## Next
- Not run (memory-cgroup OOM: snipdeck-ui needs ~7 GB and other agents' clippy runs took the rest):
  `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test editors` (3 goldens change: the title counter, and
  item-editor-trigger now 420x1250 to show the trigger options), `--test focus_scroll_logic`, the new
  editors_logic test `the_trigger_options_show_with_a_trigger_below_its_field`, and clippy on snipdeck-app/ui
- 32 for capture / paste-as-new (not done)
- 29, 30, rope viewer, image editor: not built (see SPEC 5.4, HANDOVER)

## Blocked
- none
