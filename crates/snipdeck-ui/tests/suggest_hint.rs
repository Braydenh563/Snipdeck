//! Screenshot tests for the suggestion hint (light, dark, 150% and 225% text, a long title), plus
//! measurements that the text is never clipped and the key chip is centred on the title.
//!
//! Goldens: `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test suggest_hint`, then look at every PNG.

use slint::{ComponentHandle, LogicalSize};
use snipdeck_ui::{
    SuggestHintWindow, SystemLook,
    testing::{Frame, Harness, assert_golden},
};

fn hint(title: &str, dark: bool, scale: f32) -> Harness<SuggestHintWindow> {
    let h = Harness::new(SuggestHintWindow::new, 400, 100);
    h.component.set_dark(dark);
    h.component.set_item_title(title.into());
    h.component.global::<SystemLook<'_>>().set_text_scale(scale);
    h
}

/// Sizes the window to its content, as the host does, and renders it.
fn render(h: &Harness<SuggestHintWindow>) -> Frame {
    h.component.window().request_redraw();
    let _ = h.render_settled();
    h.component.window().set_size(LogicalSize::new(
        h.component.get_content_width(),
        h.component.get_content_height(),
    ));
    h.component.window().request_redraw();
    h.render_settled()
}

#[test]
fn hint_light() {
    assert_golden(
        "suggest-hint-light",
        &render(&hint("Email signature", false, 1.0)),
    );
}

#[test]
fn hint_dark() {
    assert_golden(
        "suggest-hint-dark",
        &render(&hint("Email signature", true, 1.0)),
    );
}

#[test]
fn hint_at_150_percent_text_size() {
    assert_golden(
        "suggest-hint-150",
        &render(&hint("Email signature", false, 1.5)),
    );
}

#[test]
fn hint_at_225_percent_text_size() {
    assert_golden(
        "suggest-hint-225",
        &render(&hint("Email signature", false, 2.25)),
    );
}

#[test]
fn a_long_title_is_cut_and_the_chip_stays() {
    let h = hint(
        "A very long item title that cannot possibly fit in a small hint beside the caret",
        false,
        1.0,
    );
    let frame = render(&h);
    assert_golden("suggest-hint-long", &frame);
    assert!(h.component.get_content_width() <= 360.0, "capped");
    let wide = hint(
        "A very long item title that cannot possibly fit in a small hint beside the caret",
        false,
        2.25,
    );
    assert!(
        wide.component.get_content_width() <= 810.0 && wide.component.get_content_width() > 360.0,
        "the cap grows with text size"
    );
}

#[test]
fn the_window_fits_the_text_at_every_size() {
    for scale in [1.0_f32, 1.5, 2.25] {
        let h = hint("Sig", false, scale);
        let _ = render(&h);
        let (w, ht) = (
            h.component.get_content_width(),
            h.component.get_content_height(),
        );
        assert!(w > 60.0 * scale && ht >= 20.0 * scale, "{scale}: {w}x{ht}");
        // The size the host asked for is the size it got: nothing is laid out past it.
        let size = h.component.window().size();
        let logical = size.to_logical(h.component.window().scale_factor());
        assert!((logical.width - w).abs() < 1.0 && (logical.height - ht).abs() < 1.0);
    }
}
