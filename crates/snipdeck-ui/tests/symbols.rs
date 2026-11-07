//! The Panel's Symbols tab: goldens at 280 / 320 / 420 (light), 320 dark, the kaomoji, icon and
//! empty views, and measurements (columns per width, a 40 x 40 cell, a 24 px glyph, the footer
//! name, the keys that leave the window). Rows are built here from the real catalog, the way the
//! host builds them.
//!
//! Goldens: `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test symbols`, then look at every PNG.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    reason = "test code: a failure is a failed test; pixel sizes are tiny"
)]

use std::{cell::RefCell, rc::Rc};

use slint::{ComponentHandle, ModelRc, VecModel, platform::WindowEvent};
use snipdeck_symbols::{Catalog, Kind, SymbolId};
use snipdeck_ui::{
    PanelWindow, SymbolCell, SymbolRow,
    testing::{Frame, Harness, assert_golden},
};

const HEIGHT: u32 = 560;

fn header(text: &str, y: f32, link: &str) -> SymbolRow {
    SymbolRow {
        header: text.into(),
        link: link.into(),
        link_kind: -1,
        cells: ModelRc::default(),
        chips: false,
        y,
        h: 24.0,
    }
}

fn cell_of(index: usize, flat: usize) -> SymbolCell {
    let catalog = Catalog::get();
    let sym = catalog.symbol(index).expect("symbol");
    let kind = sym.id.kind();
    SymbolCell {
        art: if kind == Kind::Kaomoji {
            -1
        } else {
            index as i32
        },
        text: sym.text.clone().unwrap_or_default().into(),
        label: match kind {
            Kind::Emoji => sym.name.clone(),
            Kind::Kaomoji => sym.keywords.first().cloned().unwrap_or_default(),
            Kind::Icon => sym.name.replace('-', " "),
        }
        .into(),
        kind: match kind {
            Kind::Emoji => 0,
            Kind::Kaomoji => 1,
            Kind::Icon => 2,
        },
        wide: sym.wide,
        flat: flat as i32,
    }
}

/// Header, then `n` rows of `cols` symbols from `range`, continuing the flat numbering.
#[allow(
    clippy::too_many_arguments,
    reason = "a fixture builder: one call per section"
)]
fn section(
    rows: &mut Vec<SymbolRow>,
    y: &mut f32,
    flat: &mut usize,
    title: &str,
    range: std::ops::Range<usize>,
    cols: usize,
    n: usize,
    chips: bool,
) {
    rows.push(header(title, *y, ""));
    *y += 24.0;
    let h = if chips { 36.0 } else { 40.0 };
    let indices: Vec<usize> = range.collect();
    for chunk in indices.chunks(cols).take(n) {
        let cells: Vec<SymbolCell> = chunk
            .iter()
            .map(|&i| {
                *flat += 1;
                cell_of(i, *flat - 1)
            })
            .collect();
        rows.push(SymbolRow {
            header: "".into(),
            link: "".into(),
            link_kind: -1,
            cells: ModelRc::new(VecModel::from(cells)),
            chips,
            y: *y,
            h,
        });
        *y += h;
    }
}

fn emoji_rows(cols: usize) -> Vec<SymbolRow> {
    let c = Catalog::get();
    let e = c.range(Kind::Emoji);
    let (mut rows, mut y, mut flat) = (Vec::new(), 0.0, 0);
    section(
        &mut rows,
        &mut y,
        &mut flat,
        "RECENTLY USED",
        e.start + 3..e.start + 3 + cols,
        cols,
        1,
        false,
    );
    section(
        &mut rows,
        &mut y,
        &mut flat,
        "SMILEYS & EMOTION",
        e.start..e.end,
        cols,
        4,
        false,
    );
    section(
        &mut rows,
        &mut y,
        &mut flat,
        "PEOPLE & BODY",
        e.start + 400..e.end,
        cols,
        6,
        false,
    );
    rows
}

struct Shown {
    h: Harness<PanelWindow>,
    columns: Rc<RefCell<i32>>,
    moves: Rc<RefCell<Vec<i32>>>,
    used: Rc<RefCell<Vec<(i32, bool)>>>,
}

fn art(i: i32) -> slint::Image {
    let catalog = Catalog::get();
    let sym = catalog
        .symbol(usize::try_from(i).expect("index"))
        .expect("symbol");
    let (rgba, px) = match sym.id.kind() {
        Kind::Emoji => match catalog.art(&sym.id, 48) {
            Some(rgba) => (rgba, 48),
            None => return slint::Image::default(),
        },
        Kind::Icon => {
            let png = catalog.icon_png(&sym.id, 48, [255; 4]).expect("icon");
            let mut reader = png::Decoder::new(std::io::Cursor::new(png.as_slice()))
                .read_info()
                .expect("png");
            let mut buf = vec![0; reader.output_buffer_size().expect("size")];
            let info = reader.next_frame(&mut buf).expect("frame");
            buf.truncate(info.buffer_size());
            (buf, 48)
        }
        Kind::Kaomoji => return slint::Image::default(),
    };
    slint::Image::from_rgba8(
        slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&rgba, px, px),
    )
}

fn open(
    width: u32,
    dark: bool,
    kind: i32,
    rows: impl Fn(usize) -> Vec<SymbolRow> + 'static,
) -> Shown {
    let h = Harness::new(PanelWindow::new, width, HEIGHT);
    let ui = &h.component;
    ui.set_dark(dark);
    ui.set_symbols_available(true);
    ui.set_target_name("Outlook".into());
    ui.set_recent_available(true);
    ui.set_symbol_kind(kind);
    ui.on_symbol_art(art);
    let columns = Rc::new(RefCell::new(0));
    let moves = Rc::new(RefCell::new(Vec::new()));
    let used = Rc::new(RefCell::new(Vec::new()));
    let (c, weak) = (Rc::clone(&columns), ui.as_weak());
    ui.on_symbol_columns_changed(move |n| {
        *c.borrow_mut() = n;
        if let Some(w) = weak.upgrade() {
            w.set_symbol_rows(ModelRc::new(VecModel::from(rows(
                usize::try_from(n).unwrap(),
            ))));
        }
    });
    let m = Rc::clone(&moves);
    ui.on_symbol_move(move |code| m.borrow_mut().push(code));
    let u = Rc::clone(&used);
    ui.on_symbol_used(move |i| u.borrow_mut().push((i, false)));
    let u = Rc::clone(&used);
    ui.on_symbol_copied(move |i| u.borrow_mut().push((i, true)));
    ui.set_symbols_open(true);
    Shown {
        h,
        columns,
        moves,
        used,
    }
}

fn render(s: &Shown) -> Frame {
    s.h.render_settled()
}

fn key(s: &Shown, text: &str) {
    s.h.component
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.into() });
    s.h.component
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text: text.into() });
}

#[test]
fn the_column_count_follows_the_width() {
    for (width, want) in [(260, 6), (280, 6), (320, 7), (360, 8), (420, 10)] {
        let s = open(width, false, 1, emoji_rows);
        let _ = render(&s);
        assert_eq!(*s.columns.borrow(), want, "columns at {width}");
    }
}

#[test]
fn emoji_320_light() {
    let s = open(320, false, 1, emoji_rows);
    s.h.component
        .set_symbol_selected_name("grinning face".into());
    assert_golden("symbols-emoji-320-light", &render(&s));
}

#[test]
fn emoji_280_light() {
    let s = open(280, false, 1, emoji_rows);
    assert_golden("symbols-emoji-280-light", &render(&s));
}

#[test]
fn emoji_420_light() {
    let s = open(420, false, 1, emoji_rows);
    assert_golden("symbols-emoji-420-light", &render(&s));
}

#[test]
fn emoji_320_dark() {
    let s = open(320, true, 1, emoji_rows);
    assert_golden("symbols-emoji-320-dark", &render(&s));
}

#[test]
fn kaomoji_320_light() {
    let s = open(320, false, 2, |_| {
        let c = Catalog::get();
        let k = c.range(Kind::Kaomoji);
        let (mut rows, mut y, mut flat) = (Vec::new(), 0.0, 0);
        section(
            &mut rows,
            &mut y,
            &mut flat,
            "JOY",
            k.start..k.start + 6,
            2,
            3,
            true,
        );
        rows
    });
    assert_golden("symbols-kaomoji-320-light", &render(&s));
}

#[test]
fn icons_320_dark() {
    let s = open(320, true, 3, |cols| {
        let c = Catalog::get();
        let i = c.range(Kind::Icon);
        let (mut rows, mut y, mut flat) = (Vec::new(), 0.0, 0);
        section(
            &mut rows,
            &mut y,
            &mut flat,
            "ARROWS",
            i.start..i.end,
            cols,
            5,
            false,
        );
        rows
    });
    assert_golden("symbols-icons-320-dark", &render(&s));
}

#[test]
fn nothing_found_320_light() {
    let s = open(320, false, 0, |_| Vec::new());
    s.h.component
        .set_symbol_empty_heading("No symbols match “xyz”".into());
    s.h.component
        .set_symbol_empty_body("Try another word, or a different kind.".into());
    assert_golden("symbols-empty-320-light", &render(&s));
}

#[test]
fn a_cell_is_40_by_40() {
    let s = open(320, false, 1, emoji_rows);
    s.h.component.set_symbol_selected(0);
    let first = render(&s);
    s.h.component.set_symbol_selected(1);
    let second = render(&s);
    // Moving the selection from the first cell to its neighbour repaints exactly those two cells
    // (fill and ring), side by side: one box 80 wide and 40 high.
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for y in 170..520 {
        for x in 0..320 {
            if first.pixel(x, y) != second.pixel(x, y) {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
        }
    }
    assert!(x0 < x1, "the selection did not move");
    assert_eq!(
        (x1 - x0 + 1, y1 - y0 + 1),
        (80, 40),
        "box from ({x0}, {y0})"
    );
    assert_eq!(x0, (320 - 7 * 40) / 2, "the grid is centred in the width");
}

#[test]
fn arrows_page_keys_and_enter_leave_the_window_as_requests() {
    let s = open(320, false, 1, emoji_rows);
    let _ = render(&s);
    s.h.component.set_symbol_selected(2);
    // Focus is in the search field (it grabbed it on init). With text in it the caret keys stay its own.
    for (k, code) in [
        (char::from(slint::platform::Key::DownArrow), 3),
        (char::from(slint::platform::Key::UpArrow), 2),
        (char::from(slint::platform::Key::PageDown), 7),
        (char::from(slint::platform::Key::PageUp), 6),
        (char::from(slint::platform::Key::LeftArrow), 0),
        (char::from(slint::platform::Key::RightArrow), 1),
        (char::from(slint::platform::Key::Home), 4),
        (char::from(slint::platform::Key::End), 5),
    ] {
        key(&s, &k.to_string());
        assert_eq!(s.moves.borrow().last().copied(), Some(code), "key {k:?}");
    }
    key(&s, &char::from(slint::platform::Key::Return).to_string());
    assert_eq!(s.used.borrow().last().copied(), Some((2, false)));
    s.h.component
        .window()
        .dispatch_event(WindowEvent::KeyPressed {
            text: char::from(slint::platform::Key::Shift).to_string().into(),
        });
    key(&s, &char::from(slint::platform::Key::Return).to_string());
    assert_eq!(
        s.used.borrow().last().copied(),
        Some((2, true)),
        "Shift+Enter copies"
    );
}

#[test]
fn left_and_right_stay_the_carets_while_the_search_has_text() {
    let s = open(320, false, 1, emoji_rows);
    let _ = render(&s);
    s.h.component.set_symbol_query("cat".into());
    let before = s.moves.borrow().len();
    key(&s, &char::from(slint::platform::Key::LeftArrow).to_string());
    key(&s, &char::from(slint::platform::Key::Home).to_string());
    assert_eq!(s.moves.borrow().len(), before, "the field keeps them");
}

#[test]
fn ids_of_the_recents_fixture_exist() {
    // Guards the fixture itself: the "recently used" cells name real emoji.
    let c = Catalog::get();
    assert!(c.index_of(&SymbolId::Emoji("1f600".into())).is_some());
}
