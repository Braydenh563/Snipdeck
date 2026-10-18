//! Screenshot tests for the Template fill form (light, dark, 225% text, required fields flagged, long labels and
//! values, a single field), plus pixel measurements of its layout.
//!
//! Goldens: `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test fill`, then look at every PNG.
//! Goldens use only glyphs IBM Plex has (no CJK, no U+21B5: system fallback fonts differ per machine); the pixel
//! assertions below do not depend on glyph shapes.

use slint::{ComponentHandle, LogicalSize};
use snipdeck_ui::{
    SystemLook, TemplateFillWindow,
    testing::{Frame, Harness, assert_golden},
};

#[path = "support/fill_fixture.rs"]
mod fixture;

// Palette values from design/tokens/palette.json.
const OVERLAY_LIGHT: [u8; 3] = [0xFB, 0xFA, 0xF7];
const OVERLAY_DARK: [u8; 3] = [0x20, 0x1E, 0x1B];
const SUNKEN_LIGHT: [u8; 3] = [0xEB, 0xE9, 0xE7];
const BORDER_SUBTLE_LIGHT: [u8; 3] = [0xDE, 0xDC, 0xD8];
const BORDER_DEFAULT_LIGHT: [u8; 3] = [0xC6, 0xC4, 0xC0];
const BORDER_DEFAULT_DARK: [u8; 3] = [0x2D, 0x2B, 0x28];
const INK_LIGHT: [u8; 3] = [0x17, 0x16, 0x13];
const DANGER_LIGHT: [u8; 3] = [0xAA, 0x1E, 0x25];

const WIDTH: u32 = 440;

fn form(width: u32, dark: bool) -> Harness<TemplateFillWindow> {
    let h = Harness::new(TemplateFillWindow::new, width, 1400);
    h.component.set_dark(dark);
    fixture::populate(&h.component);
    h
}

/// Sizes the window to its content, as the host does, and renders it.
fn render(h: &Harness<TemplateFillWindow>) -> Frame {
    h.component.window().request_redraw();
    let _ = h.render_settled();
    let height = h.component.get_content_height();
    let window = h.component.window();
    let width = window.size().to_logical(window.scale_factor()).width;
    window.set_size(LogicalSize::new(width, height));
    h.component.window().request_redraw();
    h.render_settled()
}

#[test]
fn fill_light() {
    assert_golden("fill-light", &render(&form(WIDTH, false)));
}

#[test]
fn fill_dark() {
    assert_golden("fill-dark", &render(&form(WIDTH, true)));
}

#[test]
fn fill_at_225_percent_text_size() {
    let h = form(800, false);
    h.component.global::<SystemLook<'_>>().set_text_scale(2.25);
    assert_golden("fill-scale-light", &render(&h));
}

#[test]
fn fill_flags_required_fields_left_empty() {
    let h = form(WIDTH, false);
    let mut fields = fixture::fields();
    fields[0] = fixture::text("Name", "", true);
    fixture::set_fields(&h.component, fields);
    h.component.invoke_submit();
    assert!(h.component.get_showing_errors());
    assert_golden("fill-errors-light", &render(&h));
}

#[test]
fn fill_with_long_labels_values_and_choices() {
    let h = form(360, false);
    h.component.set_template_title(
        "A very long template title that will not fit on one line of the header at all".into(),
    );
    h.component.set_submit_label("Copy".into());
    fixture::set_fields(
        &h.component,
        vec![
            fixture::text(
                "A prompt label that is long enough to need more than the width of the form",
                "Pneumonoultramicroscopicsilicovolcanoconiosis_and_other_unbreakable_words",
                true,
            ),
            fixture::choice(
                "Tone",
                &[
                    "A friendly tone with an unusually long description that goes past the edge",
                    "Formal",
                ],
                1,
            ),
        ],
    );
    h.component.set_preview(
        "Line one of a long preview that wraps because it is wider than the preview box allows for.\nLine two\nLine three\nLine four\nLine five\nLine six\nLine seven\nLine eight\nLine nine\nLine ten"
            .into(),
    );
    assert_golden("fill-long-light", &render(&h));
}

#[test]
fn fill_with_a_single_field() {
    let h = form(WIDTH, false);
    fixture::set_fields(&h.component, vec![fixture::text("Name", "", true)]);
    h.component.set_preview("Hi ".into());
    assert_golden("fill-single-light", &render(&h));
}

// ---- Measurements ----------------------------------------------------------------------------

const HEADER_H: u32 = 40;
const FOOTER_H: u32 = 48;

#[test]
fn the_window_is_outlined_with_a_1px_default_border_on_the_overlay_surface() {
    for (dark, outline, overlay) in [
        (false, BORDER_DEFAULT_LIGHT, OVERLAY_LIGHT),
        (true, BORDER_DEFAULT_DARK, OVERLAY_DARK),
    ] {
        let f = render(&form(WIDTH, dark));
        for (x, y) in [(0, 100), (WIDTH - 1, 100), (300, 0), (300, f.height - 1)] {
            assert_eq!(f.pixel(x, y), outline, "outline at ({x},{y}) dark={dark}");
        }
        assert_eq!(f.pixel(1, 100), overlay, "dialog surface (dark={dark})");
    }
}

#[test]
fn the_header_is_40px_and_the_footer_48px_each_with_a_subtle_hairline() {
    let f = render(&form(WIDTH, false));
    // Clear of glyphs: the header text is at the left, the close button at the right.
    assert_eq!(
        f.pixel(250, HEADER_H - 1),
        BORDER_SUBTLE_LIGHT,
        "header hairline"
    );
    assert_eq!(
        f.pixel(250, HEADER_H),
        OVERLAY_LIGHT,
        "form surface below it"
    );
    let top = f.height - FOOTER_H;
    assert_eq!(f.pixel(250, top), BORDER_SUBTLE_LIGHT, "footer hairline");
    assert_eq!(
        f.pixel(250, top + 1),
        OVERLAY_LIGHT,
        "footer surface below it"
    );
}

#[test]
fn the_preview_sits_in_a_sunken_box_above_the_footer() {
    let f = render(&form(WIDTH, false));
    // The box ends 16px above the footer's hairline; its last row of padding is plain sunken fill.
    let bottom = f.height - FOOTER_H - 16;
    assert_eq!(f.pixel(300, bottom - 3), SUNKEN_LIGHT, "sunken fill");
    assert_eq!(
        f.pixel(300, bottom - 1),
        BORDER_SUBTLE_LIGHT,
        "subtle bottom border"
    );
    assert_eq!(
        f.pixel(300, bottom),
        OVERLAY_LIGHT,
        "16px gap to the footer"
    );
}

#[test]
fn the_submit_button_is_ink_and_a_required_field_in_error_has_a_danger_border() {
    let h = form(WIDTH, false);
    let mut fields = fixture::fields();
    fields[0] = fixture::text("Name", "", true);
    fixture::set_fields(&h.component, fields);
    let f = render(&h);
    // The primary button is the last control in the footer: right-aligned inside 16px of padding.
    let (x, y) = (WIDTH - 16 - 4, f.height - FOOTER_H / 2);
    assert_eq!(f.pixel(x, y), INK_LIGHT, "Insert is ink, never blue");

    h.component.invoke_submit();
    // The border colour fades in over `Motion.instant`.
    h.advance(std::time::Duration::from_millis(200));
    let f = render(&h);
    // Column 300 is clear of every glyph: the box's top and bottom edges are the only danger-coloured pixels.
    let edges = (0..f.height)
        .filter(|y| f.pixel(300, *y) == DANGER_LIGHT)
        .count();
    assert_eq!(
        edges, 2,
        "the invalid field's 1px border is danger-coloured"
    );
}
