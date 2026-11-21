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

use snipdeck_app::{
    commands::{self, Command, Context},
    index_cache::IndexCache,
    panel::PanelModel,
    picker::{PickerModel, Scope},
    settings::Hotkeys,
};
use snipdeck_core::{
    Board, Content, Format, Formats, Inline, Item, ItemKind, List, Name, Placement, Tag, Timestamp,
};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, Store, TagRepo};

/// Search over this many items must answer within [`SEARCH_BUDGET_MS`].
const LIBRARY: usize = 10_000;
const SEARCH_BUDGET_MS: f64 = 16.0;
const PANEL_BUDGET_MS: f64 = 50.0;
/// SPEC §9: the Picker opens within 80 ms; each key of its search answers within 16 ms.
const PICKER_OPEN_BUDGET_MS: f64 = 80.0;
/// The command palette filters its catalogue within one frame.
const PALETTE_BUDGET_MS: f64 = 16.0;
/// The audit seed: 5 boards of 10 lists each (50 lists) and 200 tags.
const BOARDS: usize = 5;
const LISTS_PER_BOARD: usize = 10;
const TAGS: usize = 200;

/// A tiny seeded generator (xorshift64*), so the library is the same on every run.
struct Seeded(u64);

impl Seeded {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).unwrap()
    }
}

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

/// A store with `count` items spread over the audit seed: 5 boards, 50 lists, 200 tags, with a seeded
/// generator choosing each item's list and one to three tags.
fn library(count: usize) -> Store {
    let mut store = Store::open_in_memory().unwrap();
    let mut lists = Vec::new();
    for b in 0..BOARDS {
        let board = Board::new(Name::new(&format!("Board {b}")).unwrap(), at());
        store.insert_board(&board, Placement::Last).unwrap();
        for n in 0..LISTS_PER_BOARD {
            let list = List::new(board.id, Name::new(&format!("List {b}-{n}")).unwrap(), at());
            store.insert_list(&list, Placement::Last).unwrap();
            lists.push(list.id);
        }
    }
    let tags: Vec<_> = (0..TAGS)
        .map(|n| {
            let tag = Tag::new(Name::new(&format!("tag{n}")).unwrap());
            store.insert_tag(&tag).unwrap();
            tag.id
        })
        .collect();
    let mut rng = Seeded(0x5EED_0001);
    store
        .transaction::<_, snipdeck_store::Error>(|tx| {
            for n in 0..count {
                let title = format!("Snippet number {n} about topic {}", n % 97);
                let body = format!("Dear customer, reply template {n} with some body text.");
                let mut item =
                    Item::new(lists[rng.below(lists.len())], title, ItemKind::Text, at());
                item.preview = Item::preview_of(&body, false);
                for _ in 0..=rng.below(3) {
                    item.tags.insert(tags[rng.below(tags.len())]);
                }
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
    let index = std::rc::Rc::clone(&cache.snapshot(&store).unwrap().index);
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

    let index = std::rc::Rc::clone(&cache.snapshot(&store).unwrap().index);
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

/// Opens the Picker and types a query key by key, then filters the palette, with the real clock. The Picker
/// opens within 80 ms and every key answers within 16 ms, on the audit seed (SPEC §9).
#[test]
#[ignore = "wall-clock measurement: run by hand with --ignored (docs/PERF.md)"]
fn measure_picker_and_palette_budgets() {
    let store = library(LIBRARY);
    let cache = IndexCache::new();
    let watch = Wall;
    // The Panel opened first, so the index is built: the Picker opens on a warm cache, as in use.
    PanelModel::load_cached(&store, None, None, &cache).unwrap();
    let board = store.boards().unwrap().first().map(|b| b.id);

    let (mut picker, open_ms) =
        watch.measure(|| PickerModel::load_cached(&store, board, &cache).unwrap());
    println!("picker open, {LIBRARY} items: {open_ms:.1} ms (budget {PICKER_OPEN_BUDGET_MS} ms)");
    let mut worst_key: f64 = 0.0;
    let query = "reply template 9999";
    for end in (1..=query.len()).filter(|e| query.is_char_boundary(*e)) {
        let ((), ms) = watch.measure(|| picker.set_query(&query[..end]));
        worst_key = worst_key.max(ms);
    }
    let ((), clear_ms) = watch.measure(|| picker.set_query(""));
    let ((), scope_ms) = watch.measure(|| picker.set_scope(Scope::Board));
    println!(
        "picker per key: worst {worst_key:.2} ms, clearing {clear_ms:.2} ms, scope change {scope_ms:.2} ms (budget {SEARCH_BUDGET_MS} ms)"
    );

    let hotkeys = Hotkeys::default();
    let ctx = Context {
        hotkeys: &hotkeys,
        has_item: true,
        item_pinned: false,
        in_trash: false,
        symbols: true,
        colours: true,
        eyedropper: true,
        saved_colours: true,
        in_list: true,
        group: None,
        panel_shown: true,
        keep_open: false,
        can_undo: true,
        can_redo: true,
        lock_set: true,
        trash_full: true,
    };
    let texts: Vec<(String, String)> = (0..Command::ALL.len())
        .map(|n| {
            (
                format!("Command number {n} paste item"),
                "find open show".to_owned(),
            )
        })
        .collect();
    let mut worst_palette: f64 = 0.0;
    for q in ["", "p", "pa", "paste", "paste it", "stg"] {
        let (entries, ms) = watch.measure(|| commands::entries(&texts, &ctx, q));
        drop(entries);
        worst_palette = worst_palette.max(ms);
    }
    println!("palette filter: worst {worst_palette:.2} ms (budget {PALETTE_BUDGET_MS} ms)");

    assert!(open_ms <= PICKER_OPEN_BUDGET_MS, "Picker open over budget");
    assert!(
        worst_key <= SEARCH_BUDGET_MS,
        "Picker search key over budget"
    );
    assert!(clear_ms <= SEARCH_BUDGET_MS, "Picker clear over budget");
    assert!(
        worst_palette <= PALETTE_BUDGET_MS,
        "palette filter over budget"
    );
}
