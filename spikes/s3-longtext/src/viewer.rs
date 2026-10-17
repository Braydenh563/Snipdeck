//! Fallback prototype: a virtualized text viewer/editor.
//!
//! Rust owns the document as a `ropey::Rope` plus a visual-row index (prefix sums of wrapped rows
//! per line, monospace char-wrap). Slint only gets the ~41 visible rows, the selection rectangles
//! and the caret position. Cost of every operation is O(visible rows) + O(log n) rope access,
//! except index (re)builds, which are O(lines).

use crate::gen::{self, Shape};
use crate::harness::{ms, rss_mb, stats, time, Harness, Report};
use crate::{SelRect, ViewerWindow, VisualRow};
use ropey::Rope;
use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

pub const W: u32 = 640;
pub const H: u32 = 720;
const LINE_H: f32 = 18.0;
const GUTTER: f32 = 56.0;

pub struct Viewer {
    rope: Rope,
    wrap: bool,
    cols: usize,
    char_w: f32,
    view_h: f32,
    /// prefix[l] = first visual row of line l; len = lines + 1.
    prefix: Vec<u32>,
    scroll_y: f32,
    scroll_x: usize,
    anchor: usize,
    head: usize,
    rows: Rc<VecModel<VisualRow>>,
    sel: Rc<VecModel<SelRect>>,
    ui: slint::Weak<ViewerWindow>,
}

fn content_len(line: ropey::RopeSlice) -> usize {
    let mut n = line.len_chars();
    if n > 0 && line.char(n - 1) == '\n' {
        n -= 1;
        if n > 0 && line.char(n - 1) == '\r' {
            n -= 1;
        }
    }
    n
}

impl Viewer {
    pub fn new(ui: &ViewerWindow, wrap: bool) -> Self {
        let char_w = ui.get_char_w();
        let view_h = H as f32;
        let cols = (((W as f32 - GUTTER - 16.0) / char_w).floor() as usize).max(1);
        let pool = (view_h / LINE_H).ceil() as usize + 1;
        let rows = Rc::new(VecModel::from(vec![VisualRow::default(); pool]));
        let sel = Rc::new(VecModel::default());
        ui.set_rows(ModelRc::from(rows.clone()));
        ui.set_sel(ModelRc::from(sel.clone()));
        ui.set_line_height(LINE_H);
        ui.set_gutter(GUTTER);
        Viewer {
            rope: Rope::new(),
            wrap,
            cols,
            char_w,
            view_h,
            prefix: vec![0, 1],
            scroll_y: 0.0,
            scroll_x: 0,
            anchor: 0,
            head: 0,
            rows,
            sel,
            ui: ui.as_weak(),
        }
    }

    pub fn load(&mut self, text: &str) {
        self.rope = Rope::from_str(text);
        self.anchor = 0;
        self.head = 0;
        self.scroll_y = 0.0;
    }

    fn rows_for(&self, len: usize) -> u32 {
        if self.wrap {
            len.div_ceil(self.cols).max(1) as u32
        } else {
            1
        }
    }

    /// O(lines): (re)build the visual row index.
    pub fn rebuild_index(&mut self) {
        let mut prefix = Vec::with_capacity(self.rope.len_lines() + 1);
        let mut acc = 0u32;
        for line in self.rope.lines() {
            prefix.push(acc);
            acc += self.rows_for(content_len(line));
        }
        prefix.push(acc);
        self.prefix = prefix;
    }

    fn total_rows(&self) -> u32 {
        *self.prefix.last().unwrap()
    }

    fn line_of_vrow(&self, vr: u32) -> (usize, u32) {
        let l = self.prefix.partition_point(|&p| p <= vr) - 1;
        let l = l.min(self.rope.len_lines() - 1);
        (l, vr - self.prefix[l])
    }

    fn max_scroll(&self) -> f32 {
        (self.total_rows() as f32 * LINE_H - self.view_h).max(0.0)
    }

    pub fn scroll_to(&mut self, y: f32) {
        self.scroll_y = y.clamp(0.0, self.max_scroll());
        self.refresh();
    }

    pub fn scroll_by(&mut self, dy: f32) {
        self.scroll_to(self.scroll_y + dy);
    }

    pub fn half_height(&self) -> f32 {
        self.max_scroll() / 2.0
    }

    /// Char range (within the line) shown on visual sub-row `sub` of line `l`.
    fn row_range(&self, len: usize, sub: u32) -> (usize, usize) {
        if self.wrap {
            let a = sub as usize * self.cols;
            (a.min(len), (a + self.cols).min(len))
        } else {
            (self.scroll_x.min(len), (self.scroll_x + self.cols).min(len))
        }
    }

    fn caret_vrow_col(&self, ch: usize) -> (u32, usize, usize) {
        let l = self.rope.char_to_line(ch);
        let col = ch - self.rope.line_to_char(l);
        let sub = if self.wrap { col / self.cols } else { 0 };
        let len = content_len(self.rope.line(l));
        let sub = sub.min(self.rows_for(len) as usize - 1);
        (self.prefix[l] + sub as u32, col, sub)
    }

    /// O(visible rows): push the visible window to Slint.
    pub fn refresh(&mut self) {
        let first = (self.scroll_y / LINE_H).floor() as u32;
        let total = self.total_rows();
        let (s0, s1) = (self.anchor.min(self.head), self.anchor.max(self.head));
        let mut rects = Vec::new();
        for k in 0..self.rows.row_count() {
            let vr = first + k as u32;
            let y = vr as f32 * LINE_H - self.scroll_y;
            let row = if vr >= total {
                VisualRow::default()
            } else {
                let (l, sub) = self.line_of_vrow(vr);
                let line = self.rope.line(l);
                let len = content_len(line);
                let (a, b) = self.row_range(len, sub);
                if s0 != s1 {
                    let ls = self.rope.line_to_char(l);
                    let last_sub = sub + 1 == self.rows_for(len);
                    let (ga, gb) = (ls + a, ls + b + usize::from(last_sub));
                    let (x0, x1) = (s0.max(ga), s1.min(gb));
                    if x0 < x1 {
                        rects.push(SelRect {
                            x: (x0 - ga) as f32 * self.char_w,
                            y,
                            w: (x1 - x0) as f32 * self.char_w,
                        });
                    }
                }
                VisualRow {
                    y,
                    text: SharedString::from(String::from(line.slice(a..b)).as_str()),
                    line_no: if sub == 0 {
                        format!("{}", l + 1).into()
                    } else {
                        SharedString::default()
                    },
                }
            };
            if self.rows.row_data(k).as_ref() != Some(&row) {
                self.rows.set_row_data(k, row);
            }
        }
        self.sel.set_vec(rects);
        let (cvr, col, sub) = self.caret_vrow_col(self.head);
        let cx = if self.wrap {
            (col - sub * self.cols) as f32
        } else {
            col as f32 - self.scroll_x as f32
        };
        if let Some(ui) = self.ui.upgrade() {
            ui.set_caret_x(cx * self.char_w);
            ui.set_caret_y(cvr as f32 * LINE_H - self.scroll_y);
            let content = (total as f32 * LINE_H).max(1.0);
            ui.set_thumb_size((self.view_h / content).min(1.0));
            ui.set_thumb_pos(if self.max_scroll() > 0.0 {
                self.scroll_y / self.max_scroll()
            } else {
                0.0
            });
        }
    }

    fn ensure_caret_visible(&mut self) {
        let (cvr, col, _) = self.caret_vrow_col(self.head);
        let y = cvr as f32 * LINE_H;
        if y < self.scroll_y || y + LINE_H > self.scroll_y + self.view_h {
            self.scroll_y = (y - self.view_h / 2.0).clamp(0.0, self.max_scroll());
        }
        if !self.wrap && (col < self.scroll_x || col >= self.scroll_x + self.cols) {
            self.scroll_x = col.saturating_sub(self.cols / 2);
        }
    }

    /// Typing: O(log n) rope insert + O(lines) prefix fix-up (a Fenwick tree would make it
    /// O(log n); not needed at these sizes) + O(visible) refresh.
    pub fn insert(&mut self, s: &str) {
        let (a, b) = (self.anchor.min(self.head), self.anchor.max(self.head));
        if a != b {
            self.rope.remove(a..b);
            self.head = a;
            self.rebuild_index();
        }
        let l = self.rope.char_to_line(self.head);
        let before = self.rows_for(content_len(self.rope.line(l)));
        self.rope.insert(self.head, s);
        self.head += s.chars().count();
        self.anchor = self.head;
        if s.contains('\n') {
            self.rebuild_index();
        } else {
            let after = self.rows_for(content_len(self.rope.line(l)));
            if after != before {
                for p in &mut self.prefix[l + 1..] {
                    *p = *p + after - before;
                }
            }
        }
        self.ensure_caret_visible();
        self.refresh();
    }

    pub fn move_vertical(&mut self, down: bool, extend: bool) {
        let (cvr, col, sub) = self.caret_vrow_col(self.head);
        let target = if down {
            (cvr + 1).min(self.total_rows() - 1)
        } else {
            cvr.saturating_sub(1)
        };
        let in_row = col - sub * if self.wrap { self.cols } else { 0 };
        let (l, s) = self.line_of_vrow(target);
        let len = content_len(self.rope.line(l));
        let base = if self.wrap { s as usize * self.cols } else { 0 };
        self.head = self.rope.line_to_char(l) + (base + in_row).min(len);
        if !extend {
            self.anchor = self.head;
        }
        self.ensure_caret_visible();
        self.refresh();
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.head = self.rope.len_chars();
        self.refresh();
    }

    pub fn click(&mut self, x: f32, y: f32) {
        let vr = ((self.scroll_y + y) / LINE_H).floor() as u32;
        let vr = vr.min(self.total_rows() - 1);
        let (l, sub) = self.line_of_vrow(vr);
        let len = content_len(self.rope.line(l));
        let (a, b) = self.row_range(len, sub);
        let col = (((x - GUTTER) / self.char_w).round().max(0.0) as usize + a).min(b);
        self.head = self.rope.line_to_char(l) + col;
        self.anchor = self.head;
        self.refresh();
    }

    pub fn on_key(&mut self, text: &str, shift: bool, ctrl: bool) {
        let down: SharedString = Key::DownArrow.into();
        let up: SharedString = Key::UpArrow.into();
        if text == down.as_str() {
            self.move_vertical(true, shift);
        } else if text == up.as_str() {
            self.move_vertical(false, shift);
        } else if ctrl && text == "a" {
            self.select_all();
        } else if !ctrl && text.chars().all(|c| !c.is_control() && !('\u{f700}'..='\u{f8ff}').contains(&c)) {
            self.insert(text);
        }
    }

    /// Byte offsets of all matches; scans rope chunks with a carry so matches spanning chunk
    /// boundaries are found. O(n), no full-text copy.
    pub fn find_all(&self, needle: &str) -> Vec<usize> {
        let mut out = Vec::new();
        let mut carry = String::new();
        let mut carry_start = 0usize; // byte offset of carry[0] in the document
        let mut offset = 0usize; // byte offset of the current chunk
        for chunk in self.rope.chunks() {
            let mut hay = std::mem::take(&mut carry);
            let carry_len = hay.len();
            hay.push_str(chunk);
            for (i, _) in hay.match_indices(needle) {
                if i + needle.len() > carry_len {
                    out.push(carry_start + i);
                }
            }
            offset += chunk.len();
            let mut keep = hay.len().saturating_sub(needle.len().saturating_sub(1));
            while !hay.is_char_boundary(keep) {
                keep -= 1;
            }
            carry_start = offset - (hay.len() - keep);
            carry = hay[keep..].to_string();
        }
        out
    }

    pub fn select_match(&mut self, byte: usize, needle_chars: usize) {
        self.anchor = self.rope.byte_to_char(byte);
        self.head = self.anchor + needle_chars;
        self.ensure_caret_visible();
        self.refresh();
    }

    pub fn replace_all(&mut self, needle: &str, rep: &str) {
        let s = String::from(&self.rope).replace(needle, rep);
        self.rope = Rope::from_str(&s);
        drop(s);
        self.rebuild_index();
        self.head = self.head.min(self.rope.len_chars());
        self.anchor = self.head;
        self.scroll_y = self.scroll_y.min(self.max_scroll());
        self.refresh();
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }
}

pub fn run(size: usize, shape: Shape, wrap: bool, label: &str) {
    let h = Harness::init(W, H);
    let mut r = Report::new("viewer", label);
    let (rss0, _) = rss_mb();
    let ui = ViewerWindow::new().unwrap();
    ui.show().unwrap();
    h.size_window();
    h.frame();
    let v = Rc::new(RefCell::new(Viewer::new(&ui, wrap)));
    {
        let v = v.clone();
        ui.on_key(move |t, s, c| v.borrow_mut().on_key(&t, s, c));
    }
    {
        let v = v.clone();
        ui.on_scrolled(move |_dx, dy| v.borrow_mut().scroll_by(-dy));
    }
    h.frame();
    let (rss_ui, _) = rss_mb();

    let text = gen::text(size, shape, 42);
    let ((), t_set) = time(|| v.borrow_mut().load(&text));
    drop(text);
    let ((), t_layout) = time(|| v.borrow_mut().rebuild_index());
    let ((), t_refresh) = time(|| v.borrow_mut().refresh());
    let t_first = t_refresh + h.frame();
    let (rss_loaded, _) = rss_mb();
    r.d("set", t_set);
    r.d("layout", t_layout);
    r.d("first_frame", t_first);
    r.f("load_total", ms(t_set + t_layout + t_first));
    r.f(
        "content_h_px",
        (v.borrow().total_rows() as f32 * LINE_H) as f64,
    );

    let reps = 5;
    let steps = 30;
    let rep: Vec<Duration> = (0..reps).map(|_| h.force_frame()).collect();
    r.f("repaint", stats(&rep).median);

    // wheel scrolling goes through Slint event dispatch -> TouchArea -> Rust -> model update
    let wheel = |h: &Harness| {
        let ((), d) = time(|| {
            h.dispatch(WindowEvent::PointerScrolled {
                position: LogicalPosition::new(300.0, 300.0),
                delta_x: 0.0,
                delta_y: -60.0,
            })
        });
        d + h.frame()
    };
    h.dispatch(WindowEvent::PointerMoved {
        position: LogicalPosition::new(300.0, 300.0),
    });
    let sc: Vec<Duration> = (0..steps).map(|_| wheel(&h)).collect();
    let s = stats(&sc);
    r.f("scroll_med", s.median);
    r.f("scroll_max", s.max);

    let mid = v.borrow().half_height();
    let ((), d) = time(|| v.borrow_mut().scroll_to(mid));
    r.f("jump_mid", ms(d + h.frame()));
    let sc: Vec<Duration> = (0..steps).map(|_| wheel(&h)).collect();
    r.f("scroll_mid_med", stats(&sc).median);

    let ((), d) = time(|| v.borrow_mut().click(W as f32 / 2.0, H as f32 / 2.0));
    r.f("click", ms(d + h.frame()));

    let mut kd = Vec::new();
    let mut kf = Vec::new();
    for _ in 0..reps {
        let ((), d) = time(|| h.key("x"));
        kd.push(d);
        kf.push(h.frame());
    }
    let tot: Vec<Duration> = kd.iter().zip(&kf).map(|(a, b)| *a + *b).collect();
    r.f("key_dispatch", stats(&kd).median);
    r.f("key_frame", stats(&kf).median);
    r.f("keystroke", stats(&tot).median);

    let down: SharedString = Key::DownArrow.into();
    let ar: Vec<Duration> = (0..reps)
        .map(|_| {
            let ((), d) = time(|| h.key(&down));
            d + h.frame()
        })
        .collect();
    r.f("arrow_down", stats(&ar).median);

    let ((), d) = time(|| v.borrow_mut().select_all());
    r.f("select_all", ms(d + h.frame()));
    let sc: Vec<Duration> = (0..10).map(|_| wheel(&h)).collect();
    r.f("scroll_sel_med", stats(&sc).median);

    // collapse selection in the middle, then shift+down
    let midc = v.borrow().len_chars() / 2;
    {
        let mut vb = v.borrow_mut();
        vb.anchor = midc;
        vb.head = midc;
        vb.ensure_caret_visible();
        vb.refresh();
    }
    h.frame();
    let shift: SharedString = Key::Shift.into();
    h.key_press(&shift);
    let sd: Vec<Duration> = (0..reps)
        .map(|_| {
            let ((), d) = time(|| h.key(&down));
            d + h.frame()
        })
        .collect();
    h.key_release(&shift);
    r.f("shift_down", stats(&sd).median);

    let needle = "kubernetes";
    let (pos, t_scan) = time(|| v.borrow().find_all(needle));
    r.d("find_scan", t_scan);
    r.f("find_hits", pos.len() as f64);
    let total_bytes = v.borrow().rope.len_bytes();
    let target = pos
        .iter()
        .copied()
        .find(|p| *p > total_bytes * 3 / 4)
        .unwrap_or(0);
    let ((), d) = time(|| v.borrow_mut().select_match(target, needle.len()));
    r.f("find_select", ms(d + h.frame()));

    let ((), d) = time(|| v.borrow_mut().replace_all(needle, "k8s"));
    r.f("replace_all", ms(d + h.frame()));

    let (rss_end, hwm) = rss_mb();
    r.f("rss_base", rss0);
    r.f("rss_ui", rss_ui);
    r.f("rss_loaded", rss_loaded);
    r.f("rss_end", rss_end);
    r.f("hwm", hwm);
    r.emit();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_all_across_chunks_matches_str() {
        let text = gen::text(300_000, Shape::Short, 3);
        let rope = Rope::from_str(&text);
        let expect: Vec<usize> = text.match_indices("kubernetes").map(|(i, _)| i).collect();
        // build a Viewer-free search using the same algorithm
        let v = Viewer {
            rope,
            wrap: true,
            cols: 80,
            char_w: 8.0,
            view_h: 720.0,
            prefix: vec![0, 1],
            scroll_y: 0.0,
            scroll_x: 0,
            anchor: 0,
            head: 0,
            rows: Rc::new(VecModel::default()),
            sel: Rc::new(VecModel::default()),
            ui: slint::Weak::default(),
        };
        assert_eq!(v.find_all("kubernetes"), expect);
    }
}
