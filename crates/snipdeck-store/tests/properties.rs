//! Random sequences of create / move / update / trash / restore / purge, checked against a
//! simple in-memory model after every step.
//!
//! The invariants: the order the store reports is the order the model predicts (which also
//! means ranks are unique and consistent per parent), the trash view matches, search finds
//! exactly the live items (sensitive ones by title only), no format is orphaned and no blob is
//! missing or leaked (`check_integrity`, plus a blob sweep that must find nothing).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]
#![allow(
    unknown_lints,
    clippy::assert_is_empty,
    reason = "`assert!(x.is_empty())` reads fine; the lint is newer than our pinned toolchain"
)]
#![allow(
    clippy::many_single_char_names,
    clippy::too_many_lines,
    reason = "one match arm per operation reads best next to the model it mirrors"
)]

mod common;

use std::time::Duration;

use common::*;
use proptest::prelude::*;
use snipdeck_core::{
    Board, BoardId, Format, Formats, INLINE_LIMIT, Item, ItemId, ItemKind, List, ListId, Placement,
    Timestamp,
};
use snipdeck_store::{
    BlobStore, BoardRepo, ItemRepo, ListRepo, Store, TrashEntry, TrashRepo, Trashable,
};

#[derive(Debug, Clone, Copy)]
struct Pos(u8, u16);

#[derive(Debug, Clone, Copy)]
struct Spec {
    sensitive: bool,
    png: Option<u8>,
    big_text: bool,
}

#[derive(Debug, Clone, Copy)]
enum Kind {
    Board,
    List,
    Item,
}

#[derive(Debug, Clone)]
enum Op {
    AddBoard(Pos),
    AddList(u16, Pos),
    AddItem(u16, Pos, Spec),
    MoveBoard(u16, Pos),
    MoveList(u16, u16, Pos),
    MoveItem(u16, u16, Pos),
    UpdateItem(u16, Spec),
    Trash(Kind, u16),
    Restore(Kind, u16),
    Purge(u8),
}

fn pos() -> impl Strategy<Value = Pos> {
    (0u8..3, any::<u16>()).prop_map(|(a, b)| Pos(a, b))
}

fn spec() -> impl Strategy<Value = Spec> {
    (
        any::<bool>(),
        prop::option::of(0u8..3),
        prop::bool::weighted(0.15),
    )
        .prop_map(|(sensitive, png, big_text)| Spec {
            sensitive,
            png,
            big_text,
        })
}

fn kind() -> impl Strategy<Value = Kind> {
    prop_oneof![Just(Kind::Board), Just(Kind::List), Just(Kind::Item)]
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        2 => pos().prop_map(Op::AddBoard),
        4 => (any::<u16>(), pos()).prop_map(|(b, p)| Op::AddList(b, p)),
        10 => (any::<u16>(), pos(), spec()).prop_map(|(l, p, s)| Op::AddItem(l, p, s)),
        1 => (any::<u16>(), pos()).prop_map(|(b, p)| Op::MoveBoard(b, p)),
        3 => (any::<u16>(), any::<u16>(), pos()).prop_map(|(l, b, p)| Op::MoveList(l, b, p)),
        8 => (any::<u16>(), any::<u16>(), pos()).prop_map(|(i, l, p)| Op::MoveItem(i, l, p)),
        4 => (any::<u16>(), spec()).prop_map(|(i, s)| Op::UpdateItem(i, s)),
        5 => (kind(), any::<u16>()).prop_map(|(k, i)| Op::Trash(k, i)),
        4 => (kind(), any::<u16>()).prop_map(|(k, i)| Op::Restore(k, i)),
        2 => any::<u8>().prop_map(Op::Purge),
    ]
}

// ---- the model ----------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct MItem {
    id: ItemId,
    n: u32,
    spec: Spec,
    trashed: Option<i64>,
}

#[derive(Debug, Clone)]
struct MList {
    id: ListId,
    trashed: Option<i64>,
    items: Vec<MItem>,
}

#[derive(Debug, Clone)]
struct MBoard {
    id: BoardId,
    trashed: Option<i64>,
    lists: Vec<MList>,
}

#[derive(Default)]
struct Model {
    boards: Vec<MBoard>,
    clock: i64,
    counter: u32,
    created: Vec<MItem>,
}

enum Where {
    First,
    Last,
    After(usize),
}

fn choose<T>(siblings: &[T], live: impl Fn(&T) -> bool, p: Pos) -> Where {
    let live: Vec<usize> = (0..siblings.len())
        .filter(|&i| live(&siblings[i]))
        .collect();
    match p.0 {
        0 => Where::First,
        1 => Where::Last,
        _ if live.is_empty() => Where::Last,
        _ => Where::After(live[usize::from(p.1) % live.len()]),
    }
}

fn insert<T>(v: &mut Vec<T>, at: &Where, value: T) {
    match *at {
        Where::First => v.insert(0, value),
        Where::Last => v.push(value),
        Where::After(i) => v.insert(i + 1, value),
    }
}

fn placement<Id: Copy, T>(v: &[T], at: &Where, id: impl Fn(&T) -> Id) -> Placement<Id> {
    match *at {
        Where::First => Placement::First,
        Where::Last => Placement::Last,
        Where::After(i) => Placement::After(id(&v[i])),
    }
}

/// Moves `v[from]` per `at` (an index into `v`); `false` if the anchor is the element itself.
fn reorder<T>(v: &mut Vec<T>, from: usize, at: &Where) -> bool {
    let at = match *at {
        Where::After(a) if a == from => return false,
        Where::After(a) if a > from => Where::After(a - 1),
        Where::After(a) => Where::After(a),
        Where::First => Where::First,
        Where::Last => Where::Last,
    };
    let value = v.remove(from);
    insert(v, &at, value);
    true
}

fn live_board(b: &MBoard) -> bool {
    b.trashed.is_none()
}
fn live_list(b: &MBoard, l: &MList) -> bool {
    live_board(b) && l.trashed.is_none()
}
fn live_item(b: &MBoard, l: &MList, i: &MItem) -> bool {
    live_list(b, l) && i.trashed.is_none()
}

impl Model {
    fn live_lists(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (bi, b) in self.boards.iter().enumerate() {
            for (li, l) in b.lists.iter().enumerate() {
                if live_list(b, l) {
                    out.push((bi, li));
                }
            }
        }
        out
    }

    fn live_items(&self) -> Vec<(usize, usize, usize)> {
        let mut out = Vec::new();
        for (bi, b) in self.boards.iter().enumerate() {
            for (li, l) in b.lists.iter().enumerate() {
                for (ii, i) in l.items.iter().enumerate() {
                    if live_item(b, l, i) {
                        out.push((bi, li, ii));
                    }
                }
            }
        }
        out
    }

    fn tick(&mut self) -> i64 {
        self.clock += 1;
        self.clock
    }
}

fn pick<T: Copy>(options: &[T], index: u16) -> Option<T> {
    (!options.is_empty()).then(|| options[usize::from(index) % options.len()])
}

// ---- building items -----------------------------------------------------------------------

fn build(store: &mut Store, list: ListId, n: u32, spec: Spec) -> (Item, Formats) {
    let body = format!("b{n}b common words");
    let text = if spec.big_text {
        let mut bytes = body.clone().into_bytes();
        bytes.resize(INLINE_LIMIT + 100, b' ');
        store.put(bytes).unwrap()
    } else {
        store.put(body.clone().into_bytes()).unwrap()
    };
    let mut formats = vec![Format::Text(text)];
    if let Some(fill) = spec.png {
        formats.push(Format::Png(
            store.put(vec![fill; INLINE_LIMIT + 1]).unwrap(),
        ));
    }
    let mut item = Item::new(list, format!("q{n}q"), ItemKind::Text, at(0));
    item.sensitive = spec.sensitive;
    item.preview = Item::preview_of(&body, spec.sensitive);
    (item, Formats::new(formats).unwrap())
}

// ---- running a sequence -------------------------------------------------------------------

fn apply(store: &mut Store, m: &mut Model, op: &Op) {
    match op {
        Op::AddBoard(p) => {
            let w = choose(&m.boards, live_board, *p);
            let board = Board::new(name("B"), at(0));
            let pl = placement(&m.boards, &w, |b| b.id);
            store.insert_board(&board, pl).unwrap();
            insert(
                &mut m.boards,
                &w,
                MBoard {
                    id: board.id,
                    trashed: None,
                    lists: vec![],
                },
            );
        }
        Op::AddList(b, p) => {
            let live: Vec<usize> = (0..m.boards.len())
                .filter(|&i| live_board(&m.boards[i]))
                .collect();
            let Some(bi) = pick(&live, *b) else { return };
            let board = &mut m.boards[bi];
            let w = choose(&board.lists, |l| l.trashed.is_none(), *p);
            let list = List::new(board.id, name("L"), at(0));
            let pl = placement(&board.lists, &w, |l| l.id);
            store.insert_list(&list, pl).unwrap();
            insert(
                &mut board.lists,
                &w,
                MList {
                    id: list.id,
                    trashed: None,
                    items: vec![],
                },
            );
        }
        Op::AddItem(l, p, spec) => {
            let Some((bi, li)) = pick(&m.live_lists(), *l) else {
                return;
            };
            m.counter += 1;
            let n = m.counter;
            let list = &mut m.boards[bi].lists[li];
            let (item, formats) = build(store, list.id, n, *spec);
            let w = choose(&list.items, |i| i.trashed.is_none(), *p);
            let pl = placement(&list.items, &w, |i| i.id);
            store.insert_item(&item, &formats, pl).unwrap();
            let entry = MItem {
                id: item.id,
                n,
                spec: *spec,
                trashed: None,
            };
            m.created.push(entry.clone());
            insert(&mut list.items, &w, entry);
        }
        Op::MoveBoard(b, p) => {
            let live: Vec<usize> = (0..m.boards.len())
                .filter(|&i| live_board(&m.boards[i]))
                .collect();
            let Some(from) = pick(&live, *b) else { return };
            let w = choose(&m.boards, live_board, *p);
            let pl = placement(&m.boards, &w, |b| b.id);
            store.move_board(m.boards[from].id, pl).unwrap();
            reorder(&mut m.boards, from, &w);
        }
        Op::MoveList(l, b, p) => {
            let Some((fb, fl)) = pick(&m.live_lists(), *l) else {
                return;
            };
            let live: Vec<usize> = (0..m.boards.len())
                .filter(|&i| live_board(&m.boards[i]))
                .collect();
            let Some(tb) = pick(&live, *b) else { return };
            let id = m.boards[fb].lists[fl].id;
            let w = choose(&m.boards[tb].lists, |l| l.trashed.is_none(), *p);
            let target_id = m.boards[tb].id;
            let pl = placement(&m.boards[tb].lists, &w, |l| l.id);
            store.move_list(id, target_id, pl).unwrap();
            if fb == tb {
                reorder(&mut m.boards[fb].lists, fl, &w);
            } else {
                let moved = m.boards[fb].lists.remove(fl);
                insert(&mut m.boards[tb].lists, &w, moved);
            }
        }
        Op::MoveItem(i, l, p) => {
            let Some((fb, fl, fi)) = pick(&m.live_items(), *i) else {
                return;
            };
            let Some((tb, tl)) = pick(&m.live_lists(), *l) else {
                return;
            };
            let id = m.boards[fb].lists[fl].items[fi].id;
            let w = choose(&m.boards[tb].lists[tl].items, |i| i.trashed.is_none(), *p);
            let target_id = m.boards[tb].lists[tl].id;
            let pl = placement(&m.boards[tb].lists[tl].items, &w, |i| i.id);
            store.move_item(id, target_id, pl).unwrap();
            if (fb, fl) == (tb, tl) {
                reorder(&mut m.boards[fb].lists[fl].items, fi, &w);
            } else {
                let moved = m.boards[fb].lists[fl].items.remove(fi);
                insert(&mut m.boards[tb].lists[tl].items, &w, moved);
            }
        }
        Op::UpdateItem(i, spec) => {
            let Some((bi, li, ii)) = pick(&m.live_items(), *i) else {
                return;
            };
            let list = &mut m.boards[bi].lists[li];
            let entry = &mut list.items[ii];
            let (item, formats) = build(store, list.id, entry.n, *spec);
            let mut item = item;
            item.id = entry.id;
            store.update_item(&item, Some(&formats)).unwrap();
            entry.spec = *spec;
        }
        Op::Trash(k, i) => {
            let t = m.tick();
            match k {
                Kind::Board => {
                    let live: Vec<usize> = (0..m.boards.len())
                        .filter(|&i| live_board(&m.boards[i]))
                        .collect();
                    let Some(bi) = pick(&live, *i) else { return };
                    store
                        .trash(Trashable::Board(m.boards[bi].id), at(t))
                        .unwrap();
                    m.boards[bi].trashed = Some(t);
                }
                Kind::List => {
                    let Some((bi, li)) = pick(&m.live_lists(), *i) else {
                        return;
                    };
                    store
                        .trash(Trashable::List(m.boards[bi].lists[li].id), at(t))
                        .unwrap();
                    m.boards[bi].lists[li].trashed = Some(t);
                }
                Kind::Item => {
                    let Some((bi, li, ii)) = pick(&m.live_items(), *i) else {
                        return;
                    };
                    let item = &mut m.boards[bi].lists[li].items[ii];
                    store.trash(Trashable::Item(item.id), at(t)).unwrap();
                    item.trashed = Some(t);
                }
            }
        }
        Op::Restore(k, i) => match k {
            Kind::Board => {
                let all: Vec<usize> = (0..m.boards.len()).collect();
                let Some(bi) = pick(&all, *i) else { return };
                store.restore(Trashable::Board(m.boards[bi].id)).unwrap();
                m.boards[bi].trashed = None;
            }
            Kind::List => {
                let all: Vec<_> = (0..m.boards.len())
                    .flat_map(|b| (0..m.boards[b].lists.len()).map(move |l| (b, l)))
                    .collect();
                let Some((bi, li)) = pick(&all, *i) else {
                    return;
                };
                store
                    .restore(Trashable::List(m.boards[bi].lists[li].id))
                    .unwrap();
                m.boards[bi].trashed = None;
                m.boards[bi].lists[li].trashed = None;
            }
            Kind::Item => {
                let all: Vec<_> = (0..m.boards.len())
                    .flat_map(|b| (0..m.boards[b].lists.len()).map(move |l| (b, l)))
                    .flat_map(|(b, l)| {
                        (0..m.boards[b].lists[l].items.len()).map(move |i| (b, l, i))
                    })
                    .collect();
                let Some((bi, li, ii)) = pick(&all, *i) else {
                    return;
                };
                store
                    .restore(Trashable::Item(m.boards[bi].lists[li].items[ii].id))
                    .unwrap();
                m.boards[bi].trashed = None;
                m.boards[bi].lists[li].trashed = None;
                m.boards[bi].lists[li].items[ii].trashed = None;
            }
        },
        Op::Purge(back) => {
            let cutoff = m.clock - i64::from(*back % 8) + 1;
            let expected = expected_trash(m)
                .iter()
                .filter(|e| e.deleted_at < at(cutoff))
                .count();
            let purged = store.purge(at(cutoff)).unwrap();
            assert_eq!(purged, expected, "purge count");
            let old = |t: Option<i64>| t.is_some_and(|t| t < cutoff);
            m.boards.retain(|b| !old(b.trashed));
            for b in &mut m.boards {
                b.lists.retain(|l| !old(l.trashed));
                for l in &mut b.lists {
                    l.items.retain(|i| !old(i.trashed));
                }
            }
        }
    }
}

fn expected_trash(m: &Model) -> Vec<TrashEntry> {
    let mut out = Vec::new();
    let label = |s: &str| s.to_owned();
    for b in &m.boards {
        if let Some(t) = b.trashed {
            out.push(TrashEntry {
                target: Trashable::Board(b.id),
                label: label("B"),
                deleted_at: at(t),
            });
            continue;
        }
        for l in &b.lists {
            if let Some(t) = l.trashed {
                out.push(TrashEntry {
                    target: Trashable::List(l.id),
                    label: label("L"),
                    deleted_at: at(t),
                });
                continue;
            }
            for i in &l.items {
                if let Some(t) = i.trashed {
                    out.push(TrashEntry {
                        target: Trashable::Item(i.id),
                        label: format!("q{}q", i.n),
                        deleted_at: at(t),
                    });
                }
            }
        }
    }
    out.sort_by_key(|e| std::cmp::Reverse(e.deleted_at));
    out
}

fn check(store: &Store, m: &Model) {
    // Order of everything live.
    let live_boards: Vec<_> = m.boards.iter().filter(|b| live_board(b)).collect();
    let got: Vec<BoardId> = store.boards().unwrap().iter().map(|b| b.id).collect();
    assert_eq!(
        got,
        live_boards.iter().map(|b| b.id).collect::<Vec<_>>(),
        "boards"
    );
    let mut all_items = Vec::new();
    for b in &m.boards {
        if !live_board(b) {
            // The children of a parent that is not live are `NotFound` (review L10).
            assert!(store.lists(b.id).is_err(), "lists of a trashed board");
            for l in &b.lists {
                assert!(store.items(l.id).is_err(), "items of a hidden list");
            }
            continue;
        }
        let got: Vec<ListId> = store.lists(b.id).unwrap().iter().map(|l| l.id).collect();
        let want: Vec<ListId> = b
            .lists
            .iter()
            .filter(|l| live_list(b, l))
            .map(|l| l.id)
            .collect();
        assert_eq!(got, want, "lists of a board");
        for l in &b.lists {
            if !live_list(b, l) {
                assert!(store.items(l.id).is_err(), "items of a trashed list");
                continue;
            }
            let got: Vec<ItemId> = store.items(l.id).unwrap().iter().map(|i| i.id).collect();
            let want: Vec<ItemId> = l
                .items
                .iter()
                .filter(|i| live_item(b, l, i))
                .map(|i| i.id)
                .collect();
            assert_eq!(got, want, "items of a list");
            all_items.extend(want);
        }
    }
    let got: Vec<ItemId> = store.all_items().unwrap().iter().map(|i| i.id).collect();
    assert_eq!(got, all_items, "all_items");

    // Trash view.
    let mut want = expected_trash(m);
    let mut got = store.trashed().unwrap();
    want.sort_by_key(|e| std::cmp::Reverse(e.deleted_at));
    got.sort_by_key(|e| std::cmp::Reverse(e.deleted_at));
    assert_eq!(got, want, "trash");

    // Search finds exactly the live items; sensitive ones by title only.
    let live: std::collections::HashMap<ItemId, &MItem> = m
        .boards
        .iter()
        .flat_map(|b| b.lists.iter().map(move |l| (b, l)))
        .flat_map(|(b, l)| l.items.iter().filter(move |i| live_item(b, l, i)))
        .map(|i| (i.id, i))
        .collect();
    for created in &m.created {
        let by_title = store.search(&format!("q{}q", created.n), 5).unwrap();
        let by_body = store.search(&format!("b{}b", created.n), 5).unwrap();
        match live.get(&created.id) {
            None => assert!(by_title.is_empty() && by_body.is_empty(), "dead item found"),
            Some(entry) => {
                assert_eq!(by_title, [created.id], "live item by title");
                if entry.spec.sensitive {
                    assert!(by_body.is_empty(), "sensitive content was indexed");
                } else {
                    assert_eq!(by_body, [created.id], "live item by content");
                }
            }
        }
    }
}

fn run(ops: &[Op]) {
    let mut store = Store::open_in_memory().unwrap();
    let mut model = Model::default();
    for op in ops {
        apply(&mut store, &mut model, op);
        check(&store, &model);
        assert_healthy(&store);
    }
    // Nothing is stored that no item refers to.
    let swept = store.gc_blobs(Duration::ZERO).unwrap();
    assert_eq!(swept.blobs, 0, "leaked blobs after {ops:?}");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, ..ProptestConfig::default() })]

    #[test]
    fn random_operation_sequences_keep_every_invariant(ops in prop::collection::vec(op(), 1..45)) {
        run(&ops);
    }
}

#[test]
fn a_hand_written_sequence_exercises_the_interesting_paths() {
    let spec = Spec {
        sensitive: false,
        png: Some(1),
        big_text: true,
    };
    let secret = Spec {
        sensitive: true,
        png: Some(1),
        big_text: false,
    };
    run(&[
        Op::AddBoard(Pos(1, 0)),
        Op::AddList(0, Pos(1, 0)),
        Op::AddItem(0, Pos(1, 0), spec),
        Op::AddItem(0, Pos(0, 0), secret),
        Op::AddItem(0, Pos(2, 0), spec),
        Op::Trash(Kind::Item, 0),
        Op::Trash(Kind::List, 0),
        Op::Restore(Kind::Item, 0),
        Op::UpdateItem(0, secret),
        Op::Trash(Kind::Board, 0),
        Op::Purge(0),
        Op::Restore(Kind::Board, 0),
    ]);
}

#[allow(dead_code, reason = "silences an unused import on some cfgs")]
fn _t(_: Timestamp) {}
