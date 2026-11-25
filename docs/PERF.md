# Performance, size and RAM

The budgets are in [`SPEC.md`](SPEC.md) §9. This note says how the code keeps them and how
to measure them.

## Budgets

| What | Budget |
|---|---|
| Search over 10,000 items (title, tags, first 64 characters) | under 16 ms |
| Panel load (boards, lists, items, search index) | under 50 ms |
| Idle RAM, Panel visible | under 40 MB |
| Installer (without the OCR pack) | under 15 MB |

## What is cached, and when it is dropped

The Panel and the Picker search the whole library through one `SearchIndex`
(`snipdeck_core::search`). Building it means reading every tag and indexing every item, so
`IndexCache` (`crates/snipdeck-app/src/index_cache.rs`) builds it once and reuses it.

* **Key:** `ItemRepo::change_counter()`, the number of rows SQLite changed through the open
  handle (SQLite's `total_changes`). Reads never move it. Every write does, in a `Store` and
  in a `Batch` (transaction) alike: boards, lists, items, tags, the trash, imports.
* **Hit:** the counter is unchanged since the last build, so a Panel load, a Panel refresh,
  a list or board switch and a Picker open all share the same `Rc<SearchIndex>`. The tags are
  not even read.
* **Patch:** a counted use, a pin, a move or a colour change made through
  `IndexCache::note_use` / `edit_item` / `edit_item_meta` is not a miss. The snapshot is patched:
  items are shared (`Rc<Item>`, index entries `Arc<Entry>`), so a patch copies pointers and
  one item. The suggestions are rebuilt only if a title, tag or sensitive flag changed, the
  rich set only if content changed, the pinned list only if the item was or is pinned.
* **Miss:** any other write (an import, a tag rename, a trash, a write made without a hint)
  builds a new index on the next load. A rolled-back write also moves the counter, which costs
  one needless rebuild and is never stale.
* **Main:** the Main window reads boards, lists and items through the same cache, not its own
  copy. A hidden (shown once, then hidden) Panel or Main is not drawn; showing draws once.
* **Lazy index:** reading the library builds no search index. `Snapshot::index()` / `suggest()` build it on first use, and the Panel builds it 60 ms after a frame that finds it unbuilt (`arm_warm`), so the first keystroke finds it ready. A patch made before the index exists builds nothing.
* **Recent:** a thumbnail image is built once per entry id (`recent_thumb`).
* **Rows:** `PanelModel::rows()` is cached until the query, the marks or the ghosts change.
* **Owner:** the `Controller` owns the cache and gives a handle to each model it loads.
* **Per board:** there is no separate index per board. The shared index covers the library
  and a board's results are the hits in its lists, which is a filter, not a rebuild.

## How the tests keep it honest

The regular suite has no wall-clock assertions. `crates/snipdeck-store/tests/change_counter.rs`
checks the counter. `index_cache.rs` and `crates/snipdeck-app/tests/budget.rs` check the
rebuild count (a second load with no write does not rebuild, a write does) and the budget
check itself against an injected stopwatch.

## Measuring speed

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=3 CARGO_PROFILE_DEV_DEBUG=0 \
  cargo test -p snipdeck-app --test budget -- --ignored --nocapture
```

(The test profile is built
with optimisations instead of a release build.) This builds a 10,000 item library and prints the cold and warm Panel load and five searches,
and fails if one is over budget. Do not drop the opt-level: an unoptimised build is many times slower.
Numbers depend on the machine, so compare runs on the same one. The probe now uses the audit
seed (5 boards, 50 lists, 200 tags, 10,000 items, each in a seeded-random list with one to
three tags) and also measures the Picker and the command palette.

Run on a 4-core Linux VM (optimised test profile, 2026-11-22, three to five runs each):

| What | Measured | Budget |
|---|---|---|
| Picker open (warm index) | 4.5 ms | 80 ms |
| Picker, worst key of "reply template 9999" | 8.4 ms | 16 ms |
| Picker, clearing the query / changing scope | 0.05 ms | 16 ms |
| Command palette filter, worst of six queries | 0.24 ms | 16 ms |
| Search (index only), five queries | 0.7 to 4.1 ms | 16 ms |
| Panel load, warm | 0.7 to 1.7 ms | 50 ms |
| Panel load, cold (builds the index) | 25 to 34 ms | 50 ms |

The cold load was over budget (89 to 121 ms on this seed) and is fixed. It was reading all
10,000 items with their tags (about 55 ms: a sort of every row, and three look-ups per tag
link), the typing-suggestion index (about 25 ms) and the search index (about 9 ms). Now the
suggestion index is built on first use (`Snapshot::suggest`), not on load, and `all_items`
reads in library order through the three rank indexes (no sort) with one scan of the tag links
(`fetch_all` in `snipdeck-store/src/items.rs`), about 20 to 32 ms. The cold load is paid once
per start and after a write that is not an edit or a use (`IndexCache`). The first typing
suggestion pays the 25 ms build, once per snapshot.

The Picker's worst key was noisy while other builds ran on the same container (9 to 45 ms). On a quiet
machine, five runs: 7.2, 7.3, 7.8, 7.9, 8.1 ms, median 7.8 ms (budget 16 ms).

## Release size and idle RAM

* **Size:** `cargo build --release --locked -p snipdeck-app`, then the size of
  `target/release/snipdeck-app` (`snipdeck-app.exe` on Windows). The release profile is
  fat LTO, one codegen unit, stripped. The installer size comes from the packaging step
  (`docs/packaging.md`).
* **Idle RAM on Windows:** start the release build, let the Panel show once and hide it,
  wait 30 seconds, then read the **Private working set** of `snipdeck-app.exe` in Task
  Manager (Details tab, add the column) or run in PowerShell:
  `(Get-Process snipdeck-app).PrivateMemorySize64 / 1MB`. Repeat with the Panel visible.

**UNVERIFIED:** no Windows size or RAM figure has been recorded yet. The owner runs the two
steps above on Windows and writes the numbers here.

## Probe (10,000 items)

`measure_pin_use_and_rows_at_ten_thousand_items` in `crates/snipdeck-app/tests/budget.rs`
(same command as above) prints the cold snapshot, a pin patch, a use patch and a row build.
Windows figures: **UNVERIFIED** (the owner runs the probe and the size and RAM steps on Windows).

## PERF-17: the tooltip liveness timer (deferred, with numbers)

`tip-host.slint` wakes a 100 ms timer only while a tooltip is wanted and watched (the pointer
rests on a control with a tooltip), 10 wake-ups a second, and not at all otherwise. Slint has no
destruction hook, so a control an `if` removes cannot give its tooltip back; the ping/answer
round is the only way to notice it, and an event-driven version would leave that tooltip stuck.
Measured (a one-off wall-clock run of `a_resting_tooltip_stays_up_across_many_liveness_wake_ups`, optimised build, item editor,
Linux VM, mock clock): **0.0025 ms per wake-up**, so about 0.025 ms of work a second (0.0025% of
one core). Not changed. Windows wake-up cost and the Panel's larger widget count: UNVERIFIED.
