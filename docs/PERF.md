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
* **Miss:** any write, for example a counted use, an edit, a tag rename or a trash. The next
  load builds a new index. A rolled-back write also moves the counter, which costs one
  needless rebuild and is never stale.
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
| **Panel load, cold (builds the index)** | **89 to 121 ms** | **50 ms: over** |

The cold load is the open finding: it is about twice its budget on this seed (the earlier
26 ms figure, 2026-10-19, was a simpler library). Where it goes, from the phase line the probe
prints: reading all 10,000 items with their tags about 55 ms, the typing-suggestion index about
25 ms, the search index about 9 ms. It is paid once per start and after a write that is not an
edit or a use (`IndexCache`), not on every open. The Picker's open budget holds because it
opens on the warm index. Not changed here: a fix would read items faster (`fetch` in
`snipdeck-store/src/items.rs`) or build the suggestion index off the load path.

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
