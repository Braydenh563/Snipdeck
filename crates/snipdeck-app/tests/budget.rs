//! Performance budgets (SPEC §9): search under 16 ms over 10,000 items, Panel load under
//! 50 ms. The regular tests check the budget logic and the caching it relies on with an
//! injected stopwatch, never the wall clock. The measurement itself is the `#[ignore]`d
//! test: `cargo test -p snipdeck-app --release --test budget -- --ignored --nocapture`.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::print_stdout,
    reason = "test code: a failure is a failed test, and the benchmark prints its numbers"
)]

use snipdeck_app::{index_cache::IndexCache, panel::PanelModel};
use snipdeck_core::{
    Board, Content, Format, Formats, Inline, Item, ItemKind, List, Name, Placement, Timestamp,
};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, Store};

/// Search over this many items must answer within [`SEARCH_BUDGET_MS`].
const LIBRARY: usize = 10_000;
const SEARCH_BUDGET_MS: f64 = 16.0;
const PANEL_BUDGET_MS: f64 = 50.0;

/// Where elapsed time comes from, so the budget logic runs against a fake.
trait Stopwatch {
    /// Runs `f` and returns its value with the milliseconds it took.
    fn measure<T>(&self, f: impl FnOnce() -> T) -> (T, f64);
}

/// The real clock. Used only by the ignored benchmark.
struct Wall;

impl Stopwatch for Wall {
    fn measure<T>(&self, f: impl FnOnce() -> T) -> (T, f64) {
        let start = std::time::Instant::now();
        let value = f();
        (value, start.elapsed().as_secs_f64() * 1000.0)
    }
}

/// A stopwatch that always reports the same time.
struct Fixed(f64);

impl Stopwatch for Fixed {
    fn measure<T>(&self, f: impl FnOnce() -> T) -> (T, f64) {
        (f(), self.0)
    }
}

/// Runs `f` under `watch` and fails with a readable message when it exceeds `limit_ms`.
fn within<T>(
    watch: &impl Stopwatch,
    what: &str,
    limit_ms: f64,
    f: impl FnOnce() -> T,
) -> Result<(T, f64), String> {
    let (value, ms) = watch.measure(f);
    if ms <= limit_ms {
        Ok((value, ms))
    } else {
        Err(format!(
            "{what}: {ms:.1} ms is over the {limit_ms:.0} ms budget"
        ))
    }
}

fn at() -> Timestamp {
    Timestamp::from_second(1_800_000_000).unwrap()
}

/// A store with `count` items spread over 10 lists of one board.
fn library(count: usize) -> Store {
    let mut store = Store::open_in_memory().unwrap();
    let board = Board::new(Name::new("Work").unwrap(), at());
    store.insert_board(&board, Placement::Last).unwrap();
    let lists: Vec<_> = (0..10)
        .map(|n| {
            let list = List::new(board.id, Name::new(&format!("List {n}")).unwrap(), at());
            store.insert_list(&list, Placement::Last).unwrap();
            list.id
        })
        .collect();
    store
        .transaction::<_, snipdeck_store::Error>(|tx| {
            for n in 0..count {
                let title = format!("Snippet number {n} about topic {}", n % 97);
                let body = format!("Dear customer, reply template {n} with some body text.");
                let mut item = Item::new(lists[n % lists.len()], title, ItemKind::Text, at());
                item.preview = Item::preview_of(&body, false);
                let content = Content::Inline(Inline::new(body.into_bytes()).unwrap());
                let formats = Formats::new(vec![Format::Text(content)]).unwrap();
                tx.insert_item(&item, &formats, Placement::Last)?;
            }
            Ok(())
        })
        .unwrap();
    store
}

#[test]
fn the_budget_check_passes_and_fails_on_the_stopwatch_alone() {
    assert!(within(&Fixed(15.9), "search", SEARCH_BUDGET_MS, || ()).is_ok());
    let err = within(&Fixed(16.1), "search", SEARCH_BUDGET_MS, || ()).unwrap_err();
    assert!(err.contains("search") && err.contains("16 ms"), "{err}");
}

#[test]
fn a_cached_panel_load_does_no_index_work() {
    let store = library(300);
    let cache = IndexCache::new();
    PanelModel::load_cached(&store, None, None, &cache).unwrap();
    assert_eq!(cache.rebuilds(), 1, "the first load builds once");
    for _ in 0..5 {
        PanelModel::load_cached(&store, None, None, &cache).unwrap();
    }
    assert_eq!(cache.rebuilds(), 1, "later loads reuse the index");
}

#[test]
fn search_finds_its_item_in_a_large_library() {
    let store = library(2_000);
    let cache = IndexCache::new();
    let index = cache.index(&store, &store.all_items().unwrap()).unwrap();
    assert_eq!(index.len(), 2_000);
    let hits = index.query("number 1999", 20);
    hits.first().expect("the item is found");
}

/// Measures the budgets with the real clock. Run in release mode; see the module note. The
/// numbers are printed; the only assertions are the budgets themselves.
#[test]
#[ignore = "wall-clock measurement: run by hand with --ignored (docs/PERF.md)"]
fn measure_search_and_panel_load_budgets() {
    let store = library(LIBRARY);
    let cache = IndexCache::new();
    let watch = Wall;

    let (_, cold_ms) =
        watch.measure(|| PanelModel::load_cached(&store, None, None, &cache).unwrap());
    let (_, warm_ms) =
        watch.measure(|| PanelModel::load_cached(&store, None, None, &cache).unwrap());
    println!(
        "panel load, {LIBRARY} items: cold {cold_ms:.1} ms (builds the index), warm {warm_ms:.1} ms (budget {PANEL_BUDGET_MS} ms)"
    );

    let index = cache.index(&store, &store.all_items().unwrap()).unwrap();
    let mut worst: f64 = 0.0;
    for q in [
        "snippet",
        "topic 42",
        "reply template 9999",
        "dear customer",
        "zzz",
    ] {
        let (hits, ms) = watch.measure(|| index.query(q, 50));
        println!(
            "search {q:?}: {ms:.2} ms, {} hits (budget {SEARCH_BUDGET_MS} ms)",
            hits.len()
        );
        worst = worst.max(ms);
    }

    println!("NOTE: debug builds are far slower than release; judge the budgets on --release.");
    assert!(warm_ms <= PANEL_BUDGET_MS, "warm Panel load over budget");
    assert!(cold_ms <= PANEL_BUDGET_MS, "cold Panel load over budget");
    assert!(worst <= SEARCH_BUDGET_MS, "search over budget");
}
