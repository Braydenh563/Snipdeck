# panel-v2-left (Sonnet agent, worktree wt-panel)
Build: CARGO_TARGET_DIR=/tmp/target-panel CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 (snipdeck-ui rustc OOMs at ~8.6 GB with debuginfo when other agents build).
## Done
- 3 Paste stack row + fixture mirrors APP_MENU (24c857e)
- 4 Copy glyph text-secondary (b4703a5)
- 5 panel-* goldens: 0 differing pixels allowed
- 6 flaky test: not reproducible (about 300 runs at 16/64 threads, multi-filter); no real clock or random order found
- 2 focus order: the window FocusScope was the second ring-less stop; it now hands focus on; test cycle == 11
## Next
- 1 225% chips and tabs, number gutter x16
## Blocked
