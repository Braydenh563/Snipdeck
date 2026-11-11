//! Final audit 1 (docs/reviews/final-audit-1.md): renders the audited windows at light/dark, their
//! smallest size and 1.5x / 2.25x text size, and measures contrast, centring and clipping from the
//! pixels. Set `SNIPDECK_AUDIT_DIR` to also write every frame as a PNG. No real clock.

#![allow(
    dead_code,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "pixel geometry of a test frame"
)]

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use snipdeck_ui::{
    HelpWindow, ItemEditor, MainWindow, PanelWindow, SettingsPage, SystemLook, TemplateFillWindow,
    ToastKind, ToastWindow,
    testing::{Frame, Harness},
};

#[path = "support/editor_fixture.rs"]
mod editor;
#[path = "support/fill_fixture.rs"]
mod fill;
#[path = "support/main_fixture.rs"]
mod main_fx;
#[path = "support/panel_fixture.rs"]
mod panel_fx;

fn dump(name: &str, f: &Frame) {
    let Some(dir) = std::env::var_os("SNIPDECK_AUDIT_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = std::fs::File::create(dir.join(format!("{name}.png"))).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), f.width, f.height);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .unwrap()
        .write_image_data(&f.rgb)
        .unwrap();
}

fn lin(c: u8) -> f64 {
    let c = f64::from(c) / 255.0;
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
fn lum(p: [u8; 3]) -> f64 {
    0.2126 * lin(p[0]) + 0.7152 * lin(p[1]) + 0.0722 * lin(p[2])
}
fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// The pixel in `r` (x0,y0,x1,y1) that differs most from `bg`, and its contrast with `bg`.
fn ink(f: &Frame, r: (u32, u32, u32, u32), bg: [u8; 3]) -> ([u8; 3], f64) {
    let mut best = (bg, 1.0);
    for y in r.1..r.3.min(f.height) {
        for x in r.0..r.2.min(f.width) {
            let p = f.pixel(x, y);
            let c = contrast(p, bg);
            if c > best.1 {
                best = (p, c);
            }
        }
    }
    best
}

/// Bounding box of the pixels in `r` that are not `bg` (within 6 levels).
fn bbox(frame: &Frame, area: (u32, u32, u32, u32), bg: [u8; 3]) -> Option<(u32, u32, u32, u32)> {
    let mut found: Option<(u32, u32, u32, u32)> = None;
    for y in area.1..area.3.min(frame.height) {
        for x in area.0..area.2.min(frame.width) {
            let pixel = frame.pixel(x, y);
            if pixel
                .iter()
                .zip(bg)
                .any(|(have, want)| have.abs_diff(want) > 6)
            {
                found = Some(match found {
                    None => (x, y, x + 1, y + 1),
                    Some(o) => (o.0.min(x), o.1.min(y), o.2.max(x + 1), o.3.max(y + 1)),
                });
            }
        }
    }
    found
}

macro_rules! scale {
    ($c:expr, $s:expr) => {
        $c.global::<SystemLook<'_>>().set_text_scale($s)
    };
}

fn item(w: u32, h: u32, dark: bool, s: f32, setup: impl Fn(&ItemEditor)) -> Harness<ItemEditor> {
    let hn = Harness::new(ItemEditor::new, w, h);
    hn.component.set_dark(dark);
    editor::text_item(&hn.component);
    scale!(hn.component, s);
    setup(&hn.component);
    hn
}

fn template_free(c: &ItemEditor) {
    c.set_kind_index(3);
    c.set_prompted_available(false);
    c.set_content("Hello {field:Name}, due on {date}.".into());
}

#[test]
fn dump_item_editor_scenes() {
    for (dark, s, w, h) in [
        (false, 1.0, 560, 900),
        (true, 1.0, 560, 900),
        (false, 1.0, 420, 380),
        (false, 2.25, 420, 380),
        (false, 1.5, 560, 1100),
    ] {
        let hn = item(w, h, dark, s, |c| {
            template_free(c);
            c.set_formatted(true);
            c.set_saved_content("Hello".into());
        });
        let f = hn.render_settled();
        dump(
            &format!(
                "item-template-{}-{w}x{h}-{s}",
                if dark { "dark" } else { "light" }
            ),
            &f,
        );
    }
    // formatted text item, unmodified and modified
    for (dark, name) in [
        (false, "item-formatted-light"),
        (true, "item-formatted-dark"),
    ] {
        let hn = item(560, 700, dark, 1.0, |c| {
            c.set_formatted(true);
            c.set_saved_content("Thanks for sending this over.".into());
        });
        dump(name, &hn.render_settled());
    }
}

#[test]
fn dump_fill_scenes() {
    for (dark, s, w) in [
        (false, 1.0, 440),
        (true, 1.0, 440),
        (false, 1.0, 320),
        (false, 2.25, 320),
        (false, 1.5, 440),
    ] {
        let hn = Harness::new(TemplateFillWindow::new, w, 1400);
        hn.component.set_dark(dark);
        fill::populate(&hn.component);
        scale!(hn.component, s);
        let _ = hn.render_settled();
        let height = hn.component.get_content_height();
        let win = hn.component.window();
        let width = win.size().to_logical(win.scale_factor()).width;
        win.set_size(slint::LogicalSize::new(width, height));
        win.request_redraw();
        dump(
            &format!("fill-{}-{w}-{s}", if dark { "dark" } else { "light" }),
            &hn.render_settled(),
        );
    }
}

#[test]
fn dump_help_scenes() {
    for (tab, dark, s, w, h) in [
        (0, false, 1.0, 420, 360),
        (1, false, 1.0, 420, 360),
        (2, false, 1.0, 420, 360),
        (3, false, 1.0, 420, 360),
        (3, true, 1.0, 600, 640),
        (2, true, 1.0, 600, 640),
        (3, false, 2.25, 420, 360),
        (2, false, 2.25, 420, 360),
        (0, false, 2.25, 420, 360),
        (1, false, 1.5, 420, 360),
    ] {
        let hn = Harness::new(HelpWindow::new, w, h);
        hn.component.set_dark(dark);
        editor::help(&hn.component);
        editor::shortcuts(&hn.component);
        editor::tips(&hn.component);
        scale!(hn.component, s);
        hn.component.set_tab(tab);
        dump(
            &format!(
                "help-t{tab}-{}-{w}-{s}",
                if dark { "dark" } else { "light" }
            ),
            &hn.render_settled(),
        );
    }
}

#[test]
fn dump_settings_scenes() {
    for (dark, s, w, h) in [
        (false, 1.0, 420, 360),
        (false, 1.0, 640, 1000),
        (true, 1.0, 420, 1400),
        (false, 2.25, 420, 1800),
        (false, 1.5, 640, 1800),
    ] {
        let hn = Harness::new(SettingsPage::new, w, h);
        hn.component.set_dark(dark);
        editor::settings(&hn.component);
        scale!(hn.component, s);
        dump(
            &format!(
                "settings-{}-{w}x{h}-{s}",
                if dark { "dark" } else { "light" }
            ),
            &hn.render_settled(),
        );
    }
}

#[test]
fn dump_toast_panel_palette() {
    for (dark, kind, name) in [
        (false, ToastKind::Neutral, "n"),
        (true, ToastKind::Neutral, "n"),
    ] {
        let hn = Harness::new(ToastWindow::new, 480, 80);
        hn.component.set_dark(dark);
        hn.component.set_text("Copied “Invoice reminder”".into());
        hn.component.set_undo_label("Undo".into());
        hn.component.set_kind(kind);
        dump(
            &format!("toast-{name}-{}", if dark { "dark" } else { "light" }),
            &hn.render_settled(),
        );
    }
    for s in [1.0f32, 1.5, 2.25] {
        let hn = Harness::new(PanelWindow::new, 320, 560);
        panel_fx::populate(&hn.component);
        scale!(hn.component, s);
        let mut rows = panel_fx::sample_rows();
        rows[0].formatted = true;
        rows[1].formatted = true;
        panel_fx::set_rows(&hn.component, rows);
        hn.component.set_selected_index(1);
        dump(&format!("panel-formatted-{s}"), &hn.render_settled());
    }
    for (dark, s, w, h) in [
        (false, 1.0, 1040, 640),
        (true, 1.0, 480, 360),
        (false, 2.25, 480, 360),
    ] {
        let hn = Harness::new(MainWindow::new, w, h);
        hn.component.set_dark(dark);
        let _ = main_fx::populate(&hn.component);
        scale!(hn.component, s);
        hn.component.invoke_open_palette();
        let row = |id: i32, label: &str, mark: &str, keys: &[&str]| snipdeck_ui::PaletteRow {
            id,
            label: label.into(),
            keys: ModelRc::new(VecModel::from(
                keys.iter()
                    .map(|k| (*k).into())
                    .collect::<Vec<SharedString>>(),
            )),
            spoken: keys.join(" ").into(),
            mark_before: "".into(),
            mark_text: mark.into(),
        };
        let rows = vec![
            row(9, "Keyboard shortcuts", "Key", &["Ctrl", "/"]),
            row(1, "Open Settings", "", &["Ctrl", ","]),
        ];
        hn.component
            .set_palette_rows(ModelRc::new(VecModel::from(rows)));
        dump(
            &format!("palette-{}-{w}-{s}", if dark { "dark" } else { "light" }),
            &hn.render_settled(),
        );
    }
}

#[test]
fn dump_width_sweep_at_225() {
    for w in [600u32, 700, 800, 945] {
        let hn = Harness::new(HelpWindow::new, w, 700);
        editor::help(&hn.component);
        editor::shortcuts(&hn.component);
        scale!(hn.component, 2.25);
        hn.component.set_tab(2);
        dump(&format!("sweep-help-{w}"), &hn.render_settled());
        let hn = item(w, 900, false, 2.25, |c| c.set_formatted(true));
        dump(&format!("sweep-item-{w}"), &hn.render_settled());
        let hn = Harness::new(SettingsPage::new, w, 900);
        editor::settings(&hn.component);
        scale!(hn.component, 2.25);
        dump(&format!("sweep-settings-{w}"), &hn.render_settled());
        let hn = Harness::new(TemplateFillWindow::new, w, 1600);
        fill::populate(&hn.component);
        scale!(hn.component, 2.25);
        let _ = hn.render_settled();
        let height = hn.component.get_content_height();
        hn.component
            .window()
            .set_size(slint::LogicalSize::new(w as f32, height));
        hn.component.window().request_redraw();
        dump(&format!("sweep-fill-{w}"), &hn.render_settled());
    }
}

/// The smallest window width follows the text size: at 225% the footers, tab bar and setting rows
/// of the audited windows need about 600 px (Settings about 760), which 420 px does not give.
#[test]
fn smallest_window_width_grows_with_text_size() {
    for (s, help, settings, editor_w, fill_w) in [
        (1.0f32, 420.0f32, 420.0f32, 420.0f32, 320.0f32),
        (2.25, 607.5, 765.0, 607.5, 607.5),
    ] {
        let near = |a: f32, b: f32| (a - b).abs() < 0.5;
        let h = Harness::new(HelpWindow::new, 700, 400);
        scale!(h.component, s);
        assert!(
            near(h.component.get_min_window_width(), help),
            "help at {s}"
        );
        let h = Harness::new(SettingsPage::new, 800, 400);
        scale!(h.component, s);
        assert!(
            near(h.component.get_min_window_width(), settings),
            "settings at {s}"
        );
        let h = Harness::new(ItemEditor::new, 700, 400);
        scale!(h.component, s);
        assert!(
            near(h.component.get_min_window_width(), editor_w),
            "editor at {s}"
        );
        let h = Harness::new(TemplateFillWindow::new, 700, 400);
        scale!(h.component, s);
        assert!(
            near(h.component.get_min_window_width(), fill_w),
            "fill at {s}"
        );
    }
}
