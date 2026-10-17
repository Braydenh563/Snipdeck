//! Scenario: stock Slint `TextEdit` (fluent style) with large text.

use crate::gen::{self, Shape};
use crate::harness::{ms, rss_mb, stats, time, Harness, Report};
use crate::EditorWindow;
use slint::platform::Key;
use slint::{ComponentHandle, SharedString};
use std::time::Duration;

pub const W: u32 = 640;
pub const H: u32 = 720;

pub fn run(size: usize, shape: Shape, wrap: bool, label: &str) {
    let h = Harness::init(W, H);
    let mut r = Report::new("editor", label);
    let (rss0, _) = rss_mb();

    let ui = EditorWindow::new().unwrap();
    ui.set_wrap(wrap);
    ui.show().unwrap();
    h.size_window();
    h.frame();
    let (rss_ui, _) = rss_mb();

    let text = gen::text(size, shape, 42);
    let len = text.len();
    let shared = SharedString::from(text.as_str());
    // Only Slint's copy stays alive, so RSS deltas measure Slint (string + layout), not the generator.
    drop(text);

    // 1. set text (property write only; Slint evaluates lazily)
    let ((), t_set) = time(|| ui.set_text(shared.clone()));
    drop(shared);
    // 2. layout: reading content-height forces shaping + line breaking of the TextInput
    let (content_h, t_layout) = time(|| ui.get_content_h());
    // 3. first frame after load
    let t_first = h.frame();
    let (rss_loaded, _) = rss_mb();
    r.d("set", t_set);
    r.d("layout", t_layout);
    r.d("first_frame", t_first);
    r.f("load_total", ms(t_set + t_layout + t_first));
    r.f("content_h_px", content_h as f64);

    let expensive = ms(t_layout + t_first) > 2000.0;
    let reps = if expensive { 2 } else { 5 };
    let steps = if expensive { 5 } else { 30 };

    // 4. repaint with nothing changed (layout cache warm)
    let rep: Vec<Duration> = (0..reps).map(|_| h.force_frame()).collect();
    r.f("repaint", stats(&rep).median);

    // 5. scroll steps of 60px (≈ one wheel notch, 3 lines) from the top
    let mut sc = Vec::new();
    for i in 1..=steps {
        ui.set_scroll_y(-(i as f32) * 60.0);
        sc.push(h.frame());
    }
    let s = stats(&sc);
    r.f("scroll_med", s.median);
    r.f("scroll_max", s.max);

    // 6. jump to middle, then scroll there
    let mid_y = -(content_h / 2.0).max(0.0);
    ui.set_scroll_y(mid_y);
    r.d("jump_mid", h.frame());
    let mut sc = Vec::new();
    for i in 1..=steps {
        ui.set_scroll_y(mid_y - i as f32 * 60.0);
        sc.push(h.frame());
    }
    r.f("scroll_mid_med", stats(&sc).median);

    // 7. click into the middle of the view: hit-test byte offset + caret + frame
    let ((), t_click) = time(|| h.click(W as f32 / 2.0, H as f32 / 2.0));
    let t_click_frame = h.frame();
    r.f("click", ms(t_click + t_click_frame));

    // 8. typing one character at the caret (middle of the document)
    ui.invoke_focus_editor();
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

    // 9. caret movement (down arrow) incl. frame
    let down: SharedString = Key::DownArrow.into();
    let mut ar = Vec::new();
    for _ in 0..reps {
        let ((), d) = time(|| h.key(&down));
        ar.push(d + h.frame());
    }
    r.f("arrow_down", stats(&ar).median);

    // 10. select all + frame, then scroll with the whole document selected
    let ((), d) = time(|| ui.invoke_select_all());
    r.f("select_all", ms(d + h.frame()));
    let mut sc = Vec::new();
    for i in 1..=steps.min(10) {
        ui.set_scroll_y(mid_y + i as f32 * 60.0);
        sc.push(h.frame());
    }
    r.f("scroll_sel_med", stats(&sc).median);

    // 11. shift+down extending a selection (selection drag/keyboard)
    ui.invoke_select_range((len / 2) as i32, (len / 2) as i32);
    h.frame();
    let shift: SharedString = Key::Shift.into();
    h.key_press(&shift);
    let mut sd = Vec::new();
    for _ in 0..reps {
        let ((), d) = time(|| h.key(&down));
        sd.push(d + h.frame());
    }
    h.key_release(&shift);
    r.f("shift_down", stats(&sd).median);

    // 12. find: scan in Rust (text is mirrored on the Rust side), then select the
    //     first match after the middle (TextEdit scrolls it into view).
    let cur: SharedString = ui.get_text();
    let needle = "kubernetes";
    let (positions, t_scan) = time(|| {
        cur.as_str()
            .match_indices(needle)
            .map(|(i, _)| i)
            .collect::<Vec<_>>()
    });
    r.d("find_scan", t_scan);
    r.f("find_hits", positions.len() as f64);
    let target = positions
        .iter()
        .copied()
        .find(|p| *p > cur.len() * 3 / 4)
        .unwrap_or(0);
    let ((), d) = time(|| ui.invoke_select_range(target as i32, (target + needle.len()) as i32));
    r.f("find_select", ms(d + h.frame()));

    // 13. replace all: Rust string replace + set_text + layout + frame
    let (replaced, t_repl) = time(|| cur.as_str().replace(needle, "k8s"));
    drop(cur);
    let ((), t_set2) = time(|| ui.set_text(replaced.as_str().into()));
    let (_, t_layout2) = time(|| ui.get_content_h());
    let t_f2 = h.frame();
    r.f("replace_all", ms(t_repl + t_set2 + t_layout2 + t_f2));

    let (rss_end, hwm) = rss_mb();
    r.f("rss_base", rss0);
    r.f("rss_ui", rss_ui);
    r.f("rss_loaded", rss_loaded);
    r.f("rss_end", rss_end);
    r.f("hwm", hwm);
    r.emit();
}

/// Idle baseline: empty editor window + list window machinery, nothing loaded.
pub fn idle() {
    let h = Harness::init(W, H);
    let mut r = Report::new("idle", "empty TextEdit window");
    let (rss0, _) = rss_mb();
    let ui = EditorWindow::new().unwrap();
    ui.set_text("hello".into());
    ui.show().unwrap();
    h.size_window();
    let t = h.frame();
    let (rss1, hwm) = rss_mb();
    r.d("first_frame", t);
    r.f("rss_base", rss0);
    r.f("rss_ui", rss1);
    r.f("hwm", hwm);
    r.emit();
}
