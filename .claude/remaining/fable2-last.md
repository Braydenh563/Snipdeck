# fable2-last

## Done
- Merged origin/claude/compassionate-pascal-pod8ux (7b8c233).
- Part A (uncommitted until the gate is green): refusals while an archive runs (master password, restore, import), key-record recheck in finish_rebuild, blob release hold during jobs, busy toast/Cause::Busy, redraw on start/end, paste-file deletion join on quit, panic payload logged, `.part` removal on quit, SPEC notes, tests.

## Next
- Commit + push Part A after `scripts/gate.sh --changed` is green.
- Part B (G28 remainder): two-phase portable API (decode on worker -> Decoded plan; apply on UI thread in one transaction; previews from plan + snapshot).

## Blocked
- Nothing.
