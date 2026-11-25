# Deep audit: perf

Head: tip of origin/claude/compassionate-pascal-pod8ux. Agent `perf`, read-only, no commits.

**Status of the numbers.** All timings are from a DEBUG build (opt-level 0, software renderer, Linux
container, about 2 GB RAM free): they are upper bounds, not release numbers. A release build (fat LTO)
did not fit the memory and the shared cargo lock (waits of 20 to 45 min) inside the 2-hour box. An
"OVER" below means "over budget in debug"; it is a release problem only where the code evidence shows
work that grows with the library (those are the findings). The probe ran ONCE and stopped at Main open
(my probe's fault: `Harness::new` resets the frozen clock, so creating the Main harness after a 600 s
advance tripped Slint's monotonic-clock assert). The fixed probe was queued but did not get the lock in
time. Everything after Main open is therefore NOT MEASURED and is marked so.

Seed: 10,000 items, 50 lists, 5 boards, 200 tags, 20 x 1 MB image blobs, 200 Recent entries, on-disk
store (21.2 MB data folder, seeding 8.45 s debug), fixed clock, list 0 holds 3,000 items.

## 1. Measured table (debug upper bounds)

| Act | ms | Budget | Verdict (debug) | Detail |
|---|---|---|---|---|
| Store::open (10k items, on disk) | 0.84 | 500 | ok | |
| ensure_first_run | 0.16 | 500 | ok | |
| Controller::new | 310.0 | 500 | ok | 59% of start-up |
| prune_recent | 0.49 | 500 | ok | |
| PanelWindow::new + show | 47.5 | 500 | ok | |
| Controller::bind (first sync) | 74.2 | 500 | ok | 3,000 rows built |
| first Panel frame | 91.8 | 500 | ok | |
| START-UP TOTAL (controller + panel + first frame) | 524.4 | 500 (SPEC 400) | OVER | |
| Panel search key "d" | 109.6 | 16 | OVER | logic 42.5, 1 frame, 179,200 px (whole window) |
| Panel search key "de" | 155.1 | 16 | OVER | logic 102.8, 136,192 px |
| Panel search key "dear" | 138.4 | 16 | OVER | logic 90.4 |
| Panel search key "dear customer" | 194.5 | 16 | OVER | logic 145.9 |
| Panel search key "snippet 9999" | 209.8 | 16 | OVER | logic 148.9 |
| Panel search key "zzzz" (no hit) | 46.1 | 16 | OVER | logic 19.1 |
| Panel search cleared | 148.8 | 16 | OVER | logic 64.3 |
| Panel tab switch to a small list | 105.7 | 50 | OVER | logic 32.2 |
| Panel tab switch to the 3,000-row list | 148.0 | 50 | OVER | logic 84.4 |
| Panel selection +1 (3,000 rows) | 31.0 | 16 | OVER | logic 9.2; repaints 109,440 px of 179,200 |
| Panel row activated (paste, fake keyboard) | 65.4 | 50 | OVER | logic 54.1; 11,840 px |
| Panel pin an item | 283.1 | 50 | OVER | logic 202.9 |
| Panel unpin it | 263.9 | 50 | OVER | logic 217.4 |
| Panel `redraw()` with nothing changed | 7.45 per call | 16 | ok | but not zero |
| `PanelModel::rows()` alone, 3,000 rows | 2.13 per call | 16 | ok | called several times per act |
| MainWindow::new + show | 18.5 | 50 | ok | |
| Main bind_main (first sync) | 285.6 | 50 | OVER | second read of the whole library |
| Main first frame | 349.6 | 50 | OVER | |
| MAIN OPEN TOTAL | 653.7 | 50 (SPEC 100 show) | OVER | |
| Idle, Panel visible, 600 s stepped | 2 timer wakes, 15 animation steps, 17 frames, 201,280 px | 0 after settle | ok | wakes at +120 ms and +2,356 ms, then silent for 597 s |
| RSS after start-up | 92.7 MB | SPEC 40 MB idle | OVER (debug, test harness, software frame buffer) | not comparable to Windows |

Not measured (probe stopped): Main search, palette, picker, symbols, list switch in Main, select-all
and bulk acts, editor open/save, undo/redo, drag reorder, Recent with 200 entries, 1,000 repeated acts
(memory growth), hidden-window idle, idle after use, maintenance, export, import, backup. Allocation
counts were not possible (the workspace forbids `unsafe`, so no counting allocator).

## 2. Coverage

| Row | State | How |
|---|---|---|
| Start-up to first frame | driven | run |
| Panel open, first sync, first frame | driven | run |
| Main open | driven (3 numbers) | run |
| Panel search per keystroke | driven | run |
| Main search, palette, picker, symbols search | not reached | code only |
| List/tab switch (Panel) | driven | run |
| List switch (Main), view switches | not reached | code |
| Select-all, bulk acts | not reached | code |
| Single acts: paste, pin | driven | run |
| Editor open/save, undo/redo, drag reorder | not reached | code |
| Recent with 200 entries | seeded, not reached | code |
| Import/export/backup | not reached | code (DATA-11 has numbers) |
| Memory over 1,000 acts | not reached | RSS after start only |
| Timers/wake-ups idle, Panel visible | driven, 600 s stepped | run |
| Timers hidden / after use | not reached | code: the timers read are single-shot and guarded |
| Redraw when nothing changed | driven (Panel) | run |
| Whole-model rebuilds, O(n^2) | checked | code |
| binding.rs, panel.rs, index_cache.rs, main_window.rs, main_binding.rs, controller/{main_ctl,panel_ctl,panel_select,recent_ctl,use_ctl}.rs, repaint.rs, ui damage.rs, tip-host.slint, feedback.slint, docs/PERF.md, tests/budget.rs | checked | code |

## 3. Findings

| Id | Sev | Lens | Evidence | Verified | Fix (class) | Effort |
|---|---|---|---|---|---|---|
| PERF-1 | high | Performance | A write without a patch hint (pin measured; by reading, the same path serves other single-item writes) invalidates the whole `IndexCache`: `index_cache.rs` `snapshot()` re-reads `all_items` and rebuilds the search index and the suggest index for all 10,000 items. Pin = 283 ms, unpin = 264 ms (logic 203/217 ms) against 50 ms; a tab switch that needs no rebuild is 32 ms logic, so roughly 170 to 185 ms is the rebuild (an inference from the two numbers, not a separate timing). | run + code | Every write names the item ids it touched; the cache patches those ids and never rebuilds for a single-item write. One rule for all writers, checked by `cache.rebuilds()` in a test per act. | M |
| PERF-2 | high | Performance | `IndexCache::patched` (`index_cache.rs:220-265`) is the cheap path and still copies the library: `old.all.clone()` (10,000 `Item`s, line 246) and `Rc::make_mut(&mut index)` (256, 263), which deep-clones the `SearchIndex` because the old snapshot still holds the other `Rc`; an edit also rebuilds the suggest index. Runs on every paste (use count) and every edit. Paste = 65 ms (logic 54 ms) against 50 ms. | run + code | Keep items in a shared structure patched in place (`Rc<Item>` per item or an id-keyed map); take the old snapshot out of the cache before mutating so `make_mut` does not clone; test: a counted use clones O(1) items. | M |
| PERF-3 | high | Performance | `MainModel::reload` (`main_window.rs:392-478`) does not use `IndexCache`: its own `store.all_items()` (405), `store.lists` per board twice, and its own `search_suggest::Index::build` (449). Main first sync = 286 ms, first frame 350 ms, Main open total 654 ms against 50 ms (SPEC: 100 ms). It also keeps a second full copy of the library in RAM. | run + code | One library snapshot for all windows: Main, Panel, Picker and palette read the same `Rc<Snapshot>`; remove Main's private read. | M |
| PERF-4 | high | Performance | Main is reloaded while hidden. Every `refresh_panel` (`controller/panel_ctl.rs:540`) marks Main stale, including after a paste (`use_ctl.rs:575`), and `redraw_all` (`binding.rs:63`) syncs every bound window; `main_binding.rs:250 sync -> main_screen()` has no visibility check. So once Main has been opened, every Panel paste or write pays the PERF-3 reload for a window nobody sees. NOT timed after Main open (probe stopped); the expected cost is the measured bind_main number (286 ms debug). | code | A hidden window is not synced: `redraw_all` skips windows that are not visible and marks them dirty; they sync once on show. Applies to Panel, Main, Picker, Settings alike. | S |
| PERF-5 | medium | Performance | Panel search does work per keystroke that grows with the library: `PanelModel::rows()` (`panel.rs:422`) is not cached; in search it reruns `index.query` and finds each hit with a linear scan `self.all.all.iter().find(id)` (up to 50 hits x 10,000 items), and `rows()` is called again by `selected`, `selected_position`, `marked_count`, `marked_ids`, `move_selection`, `targets` and others in the same act. Keystroke logic 19 to 149 ms, with frame 46 to 210 ms, against 16 ms. `rows()` alone = 2.1 ms per call on 3,000 rows. Adds whole-keystroke numbers to LIBRARY-50. | run + code | Compute rows once per state change and cache them (invalidate on query, list, marks, data counter); look items up by an id map held in the snapshot. | M |
| PERF-6 | medium | Performance | Panel row build is O(n^2) in rows: `binding.rs:1696` `ghosts_before` counts `view.rows[..i]` for every row; the grouped position back-scans per row; `binding.rs:1715` `stack_position` does `marked.iter().position` per row (select-all on 3,000 rows = up to 9 million compares). `panel.rs:563 place_ghosts` is O(ghosts x rows) with `rows.insert`. Select-all not timed. | code | Running counters in one pass; a `HashMap<ItemId, usize>` for marked positions; build ghosts by merge. | S |
| PERF-7 | medium | Performance | `refreshed_model` (`binding.rs:1977-2013`) sets every middle row unconditionally (each `set_row_data` notifies the view) and removes surplus rows one at a time from the model (O(n^2) when a 3,000-row list is replaced by a short one, as on the first search keystroke: 179,200 px, the whole window, repainted). | code + run (px) | Compare before set; drop the tail in one call; or build a new model when more than half the rows change. | S |
| PERF-8 | medium | Performance | The Panel model holds every row of the list (3,000 built at bind: 74 ms; tab switch 148 ms) though about a dozen are on screen; `load_cached` (`panel.rs:198`) reads `store.items(list)` and groups on each reload. | run + code | A lazy `Model` that maps row index to item on demand (the ListView asks only for visible rows). Same for the Main list and Recent. | L |
| PERF-9 | medium | Performance | Moving the selection by one row costs 31 ms (logic 9.2 ms) against 16 ms and repaints 109,440 of 179,200 px (61% of the window) for a change of two rows. Cause NOT confirmed; PERF-7 (all rows re-set) is the likely one. Key repeat on the arrow key repeats this. | run | Selection is one window property (`selected-index`), not a field written into every row; test: a selection move damages at most two rows. | M |
| PERF-10 | medium | Performance | Start-up: `Controller::new` = 310 ms of the 524 ms total (budget 500, SPEC 400). What inside it costs the time was not split by the probe; the index build for all items is the candidate by reading. | run | Show the first frame from the list query alone; build the index after the first frame or on the first search keystroke, off the UI thread. | M |
| PERF-11 | medium | Performance | `redraw()` with nothing changed is not free: 7.45 ms per call (full `view()` with `rows()`, then a compare of the 3,000-row model). Every toast tick, timer and act calls `redraw_all`, which does this for every bound window. | run + code | A dirty counter per window: `sync` returns at once when neither the controller's change counter nor the window's inputs moved. | S |
| PERF-12 | low | Performance | `render_recent` (`binding.rs:1522`) rebuilds the whole Recent model and re-creates every thumbnail through `recent_thumb` (2164) on any Recent change; with 200 entries that is 200 image builds per copy. Not timed. | code | Diff by entry id; cache the `slint::Image` per thumb. | S |
| PERF-13 | low | Performance | `main_ctl.rs:284` `recent: self.recent_entries().len()` clones all Recent entries to get a count on every Main sync; `recent_ctl.rs:218 main_recent_entries` clones and filters all entries each screen. | code | A `recent_len()` accessor; borrow, do not clone. | S |
| PERF-14 | low | Performance | `panel_select.rs:304 copy_items` is N+1: one `store.item` and one `item_text` per id; select-all + copy on 3,000 rows = 6,000 queries on the UI thread. Not timed. | code | One by-ids query in one transaction. | S |
| PERF-15 | low | Performance | `panel_select.rs:221 clear_list_view` reads `store.items(id)` in every `view()` while the confirm is open (every redraw). | code | Count once when the confirm opens; keep it in the confirm state. | S |
| PERF-16 | low | Performance | `panel.rs:410 pinned()` scans all 10,000 items with `in_board.contains` on each rows build. | code | Keep pinned ids per board in the snapshot. | S |
| PERF-17 | low | Performance | `tip-host.slint:172`: the `alive` timer fires every 100 ms for as long as a tooltip is wanted or watched (10 wake-ups a second while the pointer rests on a control). Not counted in the run (no pointer in the idle section). | code | Single-shot delay timer, then event driven (pointer move/exit) only. | S |
| PERF-18 | low | UX states | No loading state exists for the slow acts above: `Skeleton` is placed in `panel.slint:2578` and `main_window.slint:2354`, but no Rust code sets `loading`; the UI thread just blocks. `feedback.slint:238` Skeleton would animate with `animation-tick()` (a frame per tick) if it were ever shown. | code | Wire `loading` for work over 100 ms (after moving it off the UI thread) or delete the dead Skeleton. | S |
| PERF-19 | low | Performance | A toast runs a timer chain whose every tick calls `redraw_all` (all windows, PERF-11 cost each). `refresh_words` does 4 `@tr` lookups and allocations on every sync. | code | The toast tick touches the toast property only; words refresh on language change only. | S |
| PERF-20 | low | Docs, tests | `docs/PERF.md` says a counted use is a cache miss; `index_cache.rs` says it is patched (PERF-2). Windows RAM and size are "UNVERIFIED" there. `index_cache.rs:401` and `:426` tests read `std::time::Instant` (project rule: fixed clock in tests). Adds to PANEL-45: the only budget test is `#[ignore]`d and covers none of the acts in the table above. | code | Keep a probe like this one as an `#[ignore]`d test with the audit seed that prints the table; assert counters (`rebuilds()`, clones, rows built) in normal tests instead of wall time. | M |
| PERF-21 | low | Performance | RSS after start-up with the audit library is 92.7 MB (debug, test harness, software frame buffer) against SPEC 40 MB idle. The library is held whole in RAM (snapshot items, index, suggest), a second time once Main has opened (PERF-3), and a third time transiently on each patch (PERF-2). Growth over 1,000 acts not measured. | run + code | Fix PERF-2/3; then measure on Windows. | - |

Numbers added to known findings: LIBRARY-50 (whole Panel keystroke 46 to 210 ms debug); MAIN-29 (Main
first sync 286 ms debug at 10,000 items). No new numbers for DATA-11/24, PANEL-13/14/20/24/43,
LIBRARY-53/54, MAIN-14, TOOLS-38/61: those rows were not reached.

Measured good: with the Panel visible and nothing happening, 600 s of stepped time gave 2 timer
wake-ups and 17 frames, all in the first 2.5 s, then silence. `Store::open` on the 10,000-item library
is under 1 ms.

Total: 21 findings (4 high, 7 medium, 10 low).

## 4. Not verified; what a Windows run must check

- Every timing in an optimised build on Windows with the audit seed: the table above plus the rows
  listed as not measured. No release number exists for anything in this file.
- Whether PERF-1/2/3/5 stay over budget in release. The work is O(library) in each, so it grows with
  the library, but the absolute release numbers are unknown.
- PERF-4 by measurement: a Panel paste timed with Main opened once and then hidden.
- Hidden and after-use idle: timer wake-ups and CPU over 10 minutes with the Panel hidden, docked
  (PANEL-43), the typing worker on (PANEL-20), and a tooltip resting (PERF-17).
- Idle RAM and RAM after 1,000 acts with the 10,000-item library (SPEC 40 MB), private working set.
- Cold start to first frame from a cold disk (SPEC 400 ms), with the real renderer.
- Partial repaint size on the real renderer for a selection move (PERF-9).
- Import, export and backup of the 21 MB library (time and peak RAM).
