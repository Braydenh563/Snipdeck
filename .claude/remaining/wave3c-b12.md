# wave3c-b12: cached search index, budget tests, PERF note

## Done
- snipdeck-store: ItemRepo::change_counter (SQLite total_changes; Store and Batch); tests/change_counter.rs.
- snipdeck-app: index_cache.rs (IndexCache, rebuild counter); PanelModel/PickerModel::load_cached; Controller owns the cache; old rebuild loops removed.
- tests/budget.rs: stopwatch-injected budget logic, rebuild-count tests, ignored wall-clock benchmark.
- docs/PERF.md, linked from docs/README.md.

## Next
- Run gate, commit own paths only (controller.rs also holds B1's staged hunks: commit my hunks only).

## Blocked
- Windows size/RAM numbers: owner must measure (UNVERIFIED).
