//! Scenario: 10k ItemCard-like rows. ListView (built-in virtualization), plain Flickable with all
//! rows instantiated, and a Flickable with a Rust-managed row pool (manual culling).

use crate::gen;
use crate::harness::{ms, rss_mb, stats, time, Harness, Report};
use crate::search::{haystack, rank};
use crate::{CardData, FlickAllWindow, FlickCulledWindow, ListWindow, PooledCard};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32String};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::rc::Rc;
use std::time::Duration;

pub const W: u32 = 400;
pub const H: u32 = 800;
const ROW: f32 = 64.0;
const FILTER_STEPS: &[&str] = &["d", "de", "dep", "depl", "deplo", "deploy", ""];

fn card(it: &gen::Item, pos: usize) -> CardData {
    CardData {
        number: if pos < 9 {
            format!("{}", pos + 1).into()
        } else {
            "".into()
        },
        title: it.title.as_str().into(),
        preview: it.body.as_str().into(),
        list: it.list.into(),
    }
}

/// Filters with nucleo and returns item indices in display order ("" = all, original order).
fn filter(q: &str, hay: &[Utf32String], m: &mut Matcher) -> Vec<usize> {
    if q.is_empty() {
        return (0..hay.len()).collect();
    }
    let p = Pattern::parse(q, CaseMatching::Smart, Normalization::Smart);
    rank(&p, hay, m).into_iter().map(|(i, _)| i as usize).collect()
}

trait ListUi {
    fn set_cards(&self, idx: &[usize], items: &[gen::Item]);
    fn scroll_to(&self, y: f32);
}

struct LvUi(ListWindow);
impl ListUi for LvUi {
    fn set_cards(&self, idx: &[usize], items: &[gen::Item]) {
        let v: Vec<CardData> = idx.iter().enumerate().map(|(p, i)| card(&items[*i], p)).collect();
        self.0.set_items(ModelRc::from(Rc::new(VecModel::from(v))));
    }
    fn scroll_to(&self, y: f32) {
        self.0.set_scroll_y(-y);
    }
}

struct AllUi(FlickAllWindow);
impl ListUi for AllUi {
    fn set_cards(&self, idx: &[usize], items: &[gen::Item]) {
        let v: Vec<CardData> = idx.iter().enumerate().map(|(p, i)| card(&items[*i], p)).collect();
        self.0.set_items(ModelRc::from(Rc::new(VecModel::from(v))));
    }
    fn scroll_to(&self, y: f32) {
        self.0.set_scroll_y(-y);
    }
}

/// Manual culling: a fixed pool of rows; Rust rewrites the rows' data/y on every scroll.
struct CulledUi {
    ui: FlickCulledWindow,
    pool: Rc<VecModel<PooledCard>>,
    order: std::cell::RefCell<Vec<usize>>,
    items: Rc<Vec<gen::Item>>,
}
impl CulledUi {
    fn refill(&self, y: f32) {
        let items = &self.items;
        let order = self.order.borrow();
        let first = (y / ROW).floor().max(0.0) as usize;
        let n = self.pool.row_count();
        for k in 0..n {
            let pos = first + k;
            let data = match order.get(pos) {
                Some(i) => card(&items[*i], pos),
                None => CardData::default(),
            };
            self.pool.set_row_data(
                k,
                PooledCard {
                    y: pos as f32 * ROW,
                    data,
                },
            );
        }
    }
}
impl ListUi for CulledUi {
    fn set_cards(&self, idx: &[usize], _items: &[gen::Item]) {
        *self.order.borrow_mut() = idx.to_vec();
        self.ui.set_total_rows(idx.len() as i32);
        self.ui.set_scroll_y(0.0);
        self.refill(0.0);
    }
    fn scroll_to(&self, y: f32) {
        self.ui.set_scroll_y(-y);
        self.refill(y);
    }
}

pub fn run(variant: &str, n: usize) {
    let h = Harness::init(W, H);
    let (rss0, _) = rss_mb();
    let items = Rc::new(gen::items(n, 7));
    let hay: Vec<Utf32String> = items
        .iter()
        .map(|it| Utf32String::from(haystack(it).as_str()))
        .collect();
    let mut matcher = Matcher::new(Config::DEFAULT);
    let (rss_data, _) = rss_mb();

    let ui: Box<dyn ListUi> = match variant {
        "listview" => {
            let w = ListWindow::new().unwrap();
            w.show().unwrap();
            Box::new(LvUi(w))
        }
        "flickall" => {
            let w = FlickAllWindow::new().unwrap();
            w.show().unwrap();
            Box::new(AllUi(w))
        }
        "culled" => {
            let w = FlickCulledWindow::new().unwrap();
            w.show().unwrap();
            let rows = (H as f32 / ROW).ceil() as usize + 1;
            let pool = Rc::new(VecModel::from(vec![PooledCard::default(); rows]));
            w.set_pool(ModelRc::from(pool.clone()));
            Box::new(CulledUi {
                ui: w,
                pool,
                order: Default::default(),
                items: items.clone(),
            })
        }
        _ => panic!("unknown list variant {variant}"),
    };
    h.size_window();
    h.frame();

    let label = match variant {
        "listview" => "ListView (built-in virtualization)",
        "flickall" => "Flickable, all rows instantiated",
        _ => "Flickable + Rust row pool (manual culling)",
    };
    let mut r = Report::new("list", format!("{label}, {n} rows"));

    let all: Vec<usize> = (0..n).collect();
    let ((), t_model) = time(|| ui.set_cards(&all, &items));
    let t_first = h.frame();
    let (rss_loaded, _) = rss_mb();
    r.d("set_model", t_model);
    r.d("first_frame", t_first);

    // scroll: 150 steps of 40px (wheel), then jumps
    let mut sc = Vec::new();
    for i in 1..=150 {
        let (_, d) = time(|| ui.scroll_to(i as f32 * 40.0));
        sc.push(d + h.frame());
    }
    let s = stats(&sc);
    r.f("scroll_med", s.median);
    r.f("scroll_p95", s.p95);
    r.f("scroll_max", s.max);
    let total = n as f32 * ROW;
    let (_, d) = time(|| ui.scroll_to(total / 2.0));
    r.f("jump_mid", ms(d + h.frame()));
    let (_, d) = time(|| ui.scroll_to(total - H as f32));
    r.f("jump_end", ms(d + h.frame()));
    let repaint: Vec<Duration> = (0..10).map(|_| h.force_frame()).collect();
    r.f("repaint", stats(&repaint).median);
    ui.scroll_to(0.0);
    h.frame();

    // filter while typing: nucleo rank + model rebuild + frame
    let mut tot = Vec::new();
    let mut tm = Vec::new();
    let mut tmodel = Vec::new();
    let mut tf = Vec::new();
    for q in FILTER_STEPS {
        let (idx, d_match) = time(|| filter(q, &hay, &mut matcher));
        let ((), d_model) = time(|| ui.set_cards(&idx, &items));
        let d_frame = h.frame();
        tm.push(d_match);
        tmodel.push(d_model);
        tf.push(d_frame);
        tot.push(d_match + d_model + d_frame);
    }
    r.f("filter_match_med", stats(&tm).median);
    r.f("filter_model_med", stats(&tmodel).median);
    r.f("filter_frame_med", stats(&tf).median);
    let st = stats(&tot);
    r.f("filter_total_med", st.median);
    r.f("filter_total_max", st.max);

    let (rss_end, hwm) = rss_mb();
    r.f("rss_base", rss0);
    r.f("rss_data", rss_data);
    r.f("rss_loaded", rss_loaded);
    r.f("rss_end", rss_end);
    r.f("hwm", hwm);
    r.emit();
}
