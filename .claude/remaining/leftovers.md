# leftovers
## Done
- 1: Main item menu Copy copies every selected row via copy_items (a6670ff). Non-text rows are skipped silently as in Ctrl+C (a new toast string would touch toast.rs and the catalogs).
- 2: dropped files read on a worker (drop_read.rs, DropRun, drop_begin / drop_read_done), fe17d81. No progress toast (needs a new string).
## Next
- Run the new tests once snipdeck-ui can be compiled for tests: `cargo test -p snipdeck-app --lib -j1 -- drop item_menu_copy main_copy`, then fmt, clippy, Windows check + clippy, codemap.
## Blocked
- Test builds: snipdeck-ui rustc needs about 10 GB and was OOM-killed repeatedly beside other agents' builds. Only `cargo check -p snipdeck-app --tests` passed (before the last test was added; the added test is unchecked).
- 3: per-tab drop not shipped; design and blockers are in v101-s2.md.
