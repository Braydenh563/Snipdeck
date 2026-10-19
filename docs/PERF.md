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

(A real `--release` build needs the production licence key, so the test profile is built
with optimisations instead.) This builds a 10,000 item library and prints the cold and warm Panel load and five searches,
and fails if one is over budget. Do not drop the opt-level: an unoptimised build is many times slower.
Numbers depend on the machine, so compare runs on the same one. One run on a Linux VM
(optimised, 10,000 items, 2026-10-19): Panel load cold 26 ms (builds the index), warm 19 ms;
searches 0.8 to 12 ms, the slowest being a query that matches one item deep in the library.

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
