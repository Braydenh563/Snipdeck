//! Screenshot tests for the deck components (`ItemCard`, `DeckEdge`, `PinnedStrip`, `SidebarItem`,
//! `BoardSwitcher`) in light and dark, plus pixel measurements of the claims in DESIGN.md §4/§6.
//!
//! Goldens: `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test deck`, then look at every PNG. Text
//! rendering must not depend on the machine: goldens use only glyphs IBM Plex has (no CJK); the
//! pixel assertions below do not depend on glyph shapes.

use snipdeck_ui::{
    DeckGallery, DeckPartsGallery, DeckScaleGallery, SystemLook,
    testing::{Harness, assert_golden},
};

use slint::ComponentHandle;

const GALLERY: (u32, u32) = (1064, 1280);
const SCALE: (u32, u32) = (760, 1400);
const PARTS: (u32, u32) = (640, 600);

// Layout of `DeckGallery` (ui/gallery/deck.slint): 16px padding, a 20px caption, then the first card.
const CARD_TOP: u32 = 16 + 20;
// The comfortable column spaces its cards 6px apart, caption included.
const COMFORTABLE_TOP: u32 = CARD_TOP + 6;
const COMFORTABLE_X: u32 = 16 + 320 + 16;

// Expected colours from DESIGN.md §2 (the palette is generated from design/tokens/palette.json).
const LIGHT_SURFACE: [u8; 3] = [0xFB, 0xFA, 0xF7];
const LIGHT_APP: [u8; 3] = [0xF5, 0xF3, 0xF1];
const LIGHT_BORDER_SUBTLE: [u8; 3] = [0xDE, 0xDC, 0xD8];
const DARK_SURFACE: [u8; 3] = [0x17, 0x16, 0x13];
const DARK_APP: [u8; 3] = [0x11, 0x0F, 0x0D];
const DARK_BORDER_SUBTLE: [u8; 3] = [0x20, 0x1E, 0x1B];

#[test]
fn deck_gallery_light() {
    let h = Harness::new(DeckGallery::new, GALLERY.0, GALLERY.1);
    assert_golden("deck-light", &h.render());
}

#[test]
fn deck_gallery_dark() {
    let h = Harness::new(DeckGallery::new, GALLERY.0, GALLERY.1);
    h.component.set_dark(true);
    assert_golden("deck-dark", &h.render());
}

#[test]
fn deck_scale_225_light() {
    let h = Harness::new(DeckScaleGallery::new, SCALE.0, SCALE.1);
    h.component.global::<SystemLook<'_>>().set_text_scale(2.25);
    assert_golden("deck-scale-light", &h.render());
}

#[test]
fn deck_scale_225_dark() {
    let h = Harness::new(DeckScaleGallery::new, SCALE.0, SCALE.1);
    h.component.global::<SystemLook<'_>>().set_text_scale(2.25);
    h.component.set_dark(true);
    assert_golden("deck-scale-dark", &h.render());
}

#[test]
fn deck_parts_light() {
    let h = Harness::new(DeckPartsGallery::new, PARTS.0, PARTS.1);
    assert_golden("deck-parts-light", &h.render());
}

#[test]
fn deck_parts_dark() {
    let h = Harness::new(DeckPartsGallery::new, PARTS.0, PARTS.1);
    h.component.set_dark(true);
    assert_golden("deck-parts-dark", &h.render());
}

const LIGHT_ACCENT: [u8; 3] = [0x28, 0x70, 0xD4];
const DARK_ACCENT: [u8; 3] = [0x28, 0x70, 0xD4];

/// First comfortable row spans y 42..98; each next one sits 56 + 6 lower, so the 9th (selected) starts at 538.
const SELECTED_ROW_TOP: u32 = COMFORTABLE_TOP + 8 * 62;

#[test]
fn rows_have_no_border_and_the_selected_row_has_a_2px_accent_bar_on_its_left_edge() {
    for (dark, app, accent) in [
        (false, LIGHT_APP, LIGHT_ACCENT),
        (true, DARK_APP, DARK_ACCENT),
    ] {
        let h = Harness::new(DeckGallery::new, GALLERY.0, GALLERY.1);
        h.component.set_dark(dark);
        let frame = h.render();
        // A plain row is transparent on the app background: no border, no coloured edge.
        let y = COMFORTABLE_TOP + 28;
        for x in [COMFORTABLE_X, COMFORTABLE_X + 1, COMFORTABLE_X + 160] {
            assert_eq!(
                frame.pixel(x, COMFORTABLE_TOP),
                app,
                "row top (dark={dark})"
            );
        }
        assert_eq!(
            frame.pixel(COMFORTABLE_X, y),
            app,
            "left edge of a plain row (dark={dark})"
        );
        // The selected row is a fill with a 2px accent bar on the left edge.
        let mid = SELECTED_ROW_TOP + 28;
        assert_eq!(
            frame.pixel(COMFORTABLE_X + 1, mid),
            accent,
            "bar (dark={dark})"
        );
        assert_ne!(frame.pixel(COMFORTABLE_X + 4, mid), accent, "bar is thin");
        assert_ne!(frame.pixel(COMFORTABLE_X + 4, mid), app, "selected fill");
    }
}

#[test]
fn cards_have_a_level_1_shadow_in_light_and_only_tone_in_dark_and_rows_have_none() {
    // First tile (x 688..864, y 36..132): the shadow tints the pixels just under its bottom border.
    let (tile_x, below) = (688 + 88, 36 + 96);
    let light = Harness::new(DeckGallery::new, GALLERY.0, GALLERY.1);
    let frame = light.render();
    let shadowed = frame.pixel(tile_x, below);
    assert_ne!(
        shadowed, LIGHT_APP,
        "level-1 shadow tints the pixel under a Card"
    );
    assert!(shadowed[0] < LIGHT_APP[0], "and darkens it: {shadowed:?}");
    // A resting row has no shadow: the pixel under the first comfortable row is the app background.
    assert_eq!(
        frame.pixel(COMFORTABLE_X + 160, COMFORTABLE_TOP + 56),
        LIGHT_APP,
        "rows are flat"
    );
    drop(light);

    let dark = Harness::new(DeckGallery::new, GALLERY.0, GALLERY.1);
    dark.component.set_dark(true);
    let frame = dark.render();
    assert_eq!(frame.pixel(tile_x, below), DARK_APP, "no shadow in dark");
}

// ---- Card pull (DESIGN.md §4): lift 2px over 140ms, check in the gutter, settle after 700ms ----

/// Centre of the first comfortable row ("Thanks, received", x 352..672, y 42..98).
const FIRST_CARD: (f32, f32) = (512.0, 70.0);

fn click_first_card(h: &Harness<DeckGallery>) {
    use slint::{
        LogicalPosition,
        platform::{PointerEventButton, WindowEvent},
    };
    let position = LogicalPosition::new(FIRST_CARD.0, FIRST_CARD.1);
    let window = h.component.window();
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    h.advance(std::time::Duration::from_millis(50));
    window.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}

/// Pixels in the first comfortable row's number cell that are clearly blue (the check in `accent-text`).
fn blue_pixels_in_first_gutter(frame: &snipdeck_ui::testing::Frame) -> usize {
    let mut n = 0;
    for y in 38..100 {
        for x in COMFORTABLE_X + 1..COMFORTABLE_X + 24 {
            let [r, g, b] = frame.pixel(x, y);
            if b > 120 && r < 90 && g < 130 {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn using_a_row_lifts_it_2px_swaps_the_number_for_a_check_and_settles_after_700ms() {
    use std::time::Duration;
    let h = Harness::new(DeckGallery::new, GALLERY.0, GALLERY.1);
    // Above the title text, clear of the actions at the right.
    let x = COMFORTABLE_X + 160;

    let rest = h.render();
    assert_eq!(
        rest.pixel(x, 40),
        LIGHT_APP,
        "nothing above the row at rest"
    );
    assert_eq!(rest.pixel(x, 44), LIGHT_APP, "a resting row is transparent");
    assert_eq!(
        blue_pixels_in_first_gutter(&rest),
        0,
        "the number is not blue"
    );

    click_first_card(&h);
    h.advance(Duration::from_millis(200));
    let lifted = h.render();
    assert_ne!(lifted.pixel(x, 40), LIGHT_APP, "row rose by 2px");
    assert_eq!(
        lifted.pixel(x, 40),
        lifted.pixel(x, 44),
        "the raised row's face starts at y 40"
    );
    assert!(
        blue_pixels_in_first_gutter(&lifted) > 5,
        "the number cell shows a check in accent-text"
    );

    // Held for 700ms, then settles over 200ms.
    h.advance(Duration::from_millis(500));
    assert_ne!(h.render().pixel(x, 40), LIGHT_APP, "still lifted at ~750ms");
    h.advance(Duration::from_millis(300));
    let settled = h.render();
    assert_eq!(settled.pixel(x, 40), LIGHT_APP, "back down after the hold");
    assert_eq!(blue_pixels_in_first_gutter(&settled), 0, "number is back");
}

#[test]
fn under_reduced_motion_a_pulled_row_does_not_move_but_still_shows_the_check() {
    use std::time::Duration;
    let h = Harness::new(DeckGallery::new, GALLERY.0, GALLERY.1);
    h.component
        .global::<SystemLook<'_>>()
        .set_reduced_motion(true);
    let x = COMFORTABLE_X + 160;
    click_first_card(&h);
    h.advance(Duration::from_millis(10));
    let frame = h.render();
    assert_eq!(frame.pixel(x, 40), LIGHT_APP, "no translation");
    assert!(blue_pixels_in_first_gutter(&frame) > 5, "check swap only");
}

// ---- Keyboard focus ring: 2px, drawn on keyboard focus only ----

#[test]
fn keyboard_focus_draws_a_2px_ring_and_nothing_else_does() {
    use slint::platform::{Key, WindowEvent};
    for (dark, ring) in [(false, [0x28, 0x70, 0xD4]), (true, [0x74, 0xAC, 0xFF])] {
        let h = Harness::new(DeckGallery::new, GALLERY.0, GALLERY.1);
        h.component.set_dark(dark);
        let before = h.render();
        // First compact row spans x 16..336, y 36..72; its ring sits inside the row.
        assert_ne!(
            before.pixel(17, 50),
            ring,
            "no ring before focus (dark={dark})"
        );

        h.component
            .window()
            .dispatch_event(WindowEvent::KeyPressed {
                text: Key::Tab.into(),
            });
        h.component
            .window()
            .dispatch_event(WindowEvent::KeyReleased {
                text: Key::Tab.into(),
            });
        let after = h.render();
        assert_eq!(after.pixel(16, 50), ring, "ring, outer pixel (dark={dark})");
        assert_eq!(after.pixel(17, 50), ring, "ring, inner pixel (dark={dark})");
        assert_ne!(after.pixel(18, 50), ring, "ring is 2px wide (dark={dark})");
        assert_eq!(after.pixel(170, 36), ring, "top edge (dark={dark})");
        assert_eq!(after.pixel(170, 37), ring);
        assert_ne!(after.pixel(170, 38), ring);
    }
}

// ---- DeckEdge: two 4px edges inset 6px and 12px, 10px tall ----

#[test]
fn deck_edges_are_two_4px_card_edges_inset_6_and_12px() {
    for (dark, surface, border, app) in [
        (false, LIGHT_SURFACE, LIGHT_BORDER_SUBTLE, LIGHT_APP),
        (true, DARK_SURFACE, DARK_BORDER_SUBTLE, DARK_APP),
    ] {
        let h = Harness::new(DeckPartsGallery::new, PARTS.0, PARTS.1);
        h.component.set_dark(dark);
        let f = h.render();
        let col = |y: u32| f.pixel(170, y);
        // Bottom edge, 252..262 (under two 36px rows): the near edge at 252..256 and the far edge at 258..262.
        assert_eq!(col(252), border, "near edge top (dark={dark})");
        assert_eq!(col(253), surface);
        assert_eq!(col(254), surface);
        assert_eq!(col(255), border, "near edge bottom");
        assert_eq!(col(256), app, "gap between the edges");
        assert_eq!(col(257), app);
        assert_eq!(col(258), border, "far edge top");
        assert_eq!(col(261), border, "far edge bottom");
        assert_eq!(col(262), app, "nothing below the 10px component");
        // Insets: 6px for the near edge, 12px for the far one.
        assert_eq!(f.pixel(21, 253), app);
        assert_eq!(f.pixel(22, 253), border, "near edge starts 6px in");
        assert_eq!(f.pixel(27, 259), app);
        assert_eq!(f.pixel(28, 259), border, "far edge starts 12px in");
        // Top edge, 298..308: mirrored, the far edge first.
        assert_eq!(col(298), border, "top variant: far edge top");
        assert_eq!(col(301), border);
        assert_eq!(col(302), app);
        assert_eq!(col(304), border, "top variant: near edge top");
        assert_eq!(col(307), border);
        assert_eq!(f.pixel(27, 300), app);
        assert_eq!(f.pixel(28, 300), border);
        assert_eq!(f.pixel(21, 306), app);
        assert_eq!(f.pixel(22, 306), border);
    }
}

/// WCAG relative luminance of an sRGB colour.
fn luminance(c: [u8; 3]) -> f64 {
    let lin = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[test]
fn the_row_action_glyphs_read_against_the_selected_fill() {
    for dark in [false, true] {
        let h = Harness::new(DeckGallery::new, GALLERY.0, GALLERY.1);
        h.component.set_dark(dark);
        let frame = h.render();
        // The 9th comfortable row is selected, so it shows Use, Copy, Edit and More on its preview line (y +24..+56),
        // four 32px boxes ending 4px from the right edge of the 320px row.
        let fill = frame.pixel(COMFORTABLE_X + 160, SELECTED_ROW_TOP + 1);
        // Use, Copy and Edit are solid glyphs (4.5:1); the three dots of More are the thinnest (3:1, as before).
        for (name, from, min) in [
            ("Use", 188, 4.5),
            ("Copy", 220, 4.5),
            ("Edit", 252, 4.5),
            ("More", 284, 3.0),
        ] {
            let best = (COMFORTABLE_X + from..COMFORTABLE_X + from + 32)
                .flat_map(|x| (SELECTED_ROW_TOP + 24..SELECTED_ROW_TOP + 56).map(move |y| (x, y)))
                .map(|(x, y)| contrast(frame.pixel(x, y), fill))
                .fold(0.0_f64, f64::max);
            assert!(
                best >= min,
                "dark={dark}: the {name} glyph's darkest pixel is {best:.2}:1, needs {min}:1, against the selected fill"
            );
        }
    }
}
