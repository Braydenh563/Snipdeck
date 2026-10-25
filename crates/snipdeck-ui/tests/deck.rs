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
const COMPACT_X: u32 = 16;
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
const ACCENT: [u8; 3] = [0x28, 0x70, 0xD4];

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

#[test]
fn the_number_gutter_rule_is_a_1px_border_subtle_line_24px_in() {
    for (dark, surface, rule) in [
        (false, LIGHT_SURFACE, LIGHT_BORDER_SUBTLE),
        (true, DARK_SURFACE, DARK_BORDER_SUBTLE),
    ] {
        let h = Harness::new(DeckGallery::new, GALLERY.0, GALLERY.1);
        h.component.set_dark(dark);
        let frame = h.render();
        // Middle of the first compact row (28px tall), clear of text and of the row hairline.
        let y = CARD_TOP + 14;
        let x = COMPACT_X + 24;
        assert_eq!(frame.pixel(x, y), rule, "rule at gutter edge (dark={dark})");
        assert_eq!(frame.pixel(x - 1, y), surface, "gutter side (dark={dark})");
        assert_eq!(frame.pixel(x + 1, y), surface, "content side (dark={dark})");
    }
}

#[test]
fn cards_have_no_coloured_left_border() {
    for (dark, border) in [(false, LIGHT_BORDER_SUBTLE), (true, DARK_BORDER_SUBTLE)] {
        let h = Harness::new(DeckGallery::new, GALLERY.0, GALLERY.1);
        h.component.set_dark(dark);
        let frame = h.render();
        let y = COMFORTABLE_TOP + 30;
        let left = frame.pixel(COMFORTABLE_X, y);
        assert_eq!(
            left, border,
            "left edge of a plain comfortable card (dark={dark})"
        );
        assert_ne!(left, ACCENT);
        // The selected card is the 9th: its border is the accent on all four sides, not only the left.
        // (Heights of the preceding cards are measured in deck_logic.rs; here only the colour.)
        let any_accent =
            (COMFORTABLE_TOP..GALLERY.1).any(|row| frame.pixel(COMFORTABLE_X, row) == ACCENT);
        assert!(
            any_accent,
            "the selected card draws an accent border (dark={dark})"
        );
    }
}

#[test]
fn light_cards_have_a_level_1_shadow_and_dark_cards_only_tone() {
    // First comfortable card: 8 + 20 + 4 + 2 x 16 + 8 = 72px tall.
    let below = COMFORTABLE_TOP + 72;
    let x = COMFORTABLE_X + 160;

    let light = Harness::new(DeckGallery::new, GALLERY.0, GALLERY.1);
    let frame = light.render();
    let shadowed = frame.pixel(x, below);
    assert_ne!(
        shadowed, LIGHT_APP,
        "level-1 shadow tints the pixel under the card"
    );
    assert!(shadowed[0] < LIGHT_APP[0], "and darkens it: {shadowed:?}");
    drop(light);

    let dark = Harness::new(DeckGallery::new, GALLERY.0, GALLERY.1);
    dark.component.set_dark(true);
    let frame = dark.render();
    assert_eq!(frame.pixel(x, below), DARK_APP, "no shadow in dark");
}

// ---- Card pull (DESIGN.md §4): lift 2px over 140ms, check in the gutter, settle after 700ms ----

/// Centre of the first comfortable card ("Thanks, received", x 352..672, y 42..114).
const FIRST_CARD: (f32, f32) = (512.0, 78.0);

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

/// Pixels in the first comfortable card's gutter that are clearly blue (the check in `accent-text`).
fn blue_pixels_in_first_gutter(frame: &snipdeck_ui::testing::Frame) -> usize {
    let mut n = 0;
    for y in 42..114 {
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
fn using_a_card_lifts_it_2px_swaps_the_number_for_a_check_and_settles_after_700ms() {
    use std::time::Duration;
    let h = Harness::new(DeckGallery::new, GALLERY.0, GALLERY.1);
    let x = COMFORTABLE_X + 160;

    let rest = h.render();
    assert_eq!(rest.pixel(x, 42), LIGHT_BORDER_SUBTLE, "top border at rest");
    assert_eq!(
        blue_pixels_in_first_gutter(&rest),
        0,
        "the number is not blue"
    );

    click_first_card(&h);
    h.advance(Duration::from_millis(200));
    let lifted = h.render();
    assert_eq!(lifted.pixel(x, 40), LIGHT_BORDER_SUBTLE, "card rose by 2px");
    assert_eq!(
        lifted.pixel(x, 42),
        LIGHT_SURFACE,
        "old border row is now card face"
    );
    assert!(
        blue_pixels_in_first_gutter(&lifted) > 5,
        "the gutter shows a check in accent-text"
    );

    // Held for 700ms, then settles over 200ms.
    h.advance(Duration::from_millis(500));
    assert_eq!(
        h.render().pixel(x, 40),
        LIGHT_BORDER_SUBTLE,
        "still lifted at ~750ms"
    );
    h.advance(Duration::from_millis(300));
    let settled = h.render();
    assert_eq!(settled.pixel(x, 40), LIGHT_APP, "back down after the hold");
    assert_eq!(settled.pixel(x, 42), LIGHT_BORDER_SUBTLE);
    assert_eq!(blue_pixels_in_first_gutter(&settled), 0, "number is back");
}

#[test]
fn under_reduced_motion_a_pulled_card_does_not_move_but_still_shows_the_check() {
    use std::time::Duration;
    let h = Harness::new(DeckGallery::new, GALLERY.0, GALLERY.1);
    h.component
        .global::<SystemLook<'_>>()
        .set_reduced_motion(true);
    let x = COMFORTABLE_X + 160;
    click_first_card(&h);
    h.advance(Duration::from_millis(10));
    let frame = h.render();
    assert_eq!(frame.pixel(x, 42), LIGHT_BORDER_SUBTLE, "no translation");
    assert_eq!(frame.pixel(x, 40), LIGHT_APP);
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
        // First compact row spans x 16..336, y 36..64; its ring sits inside the row.
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
        // Bottom edge, 236..246: the near edge at 236..240 and the far edge at 242..246.
        assert_eq!(col(236), border, "near edge top (dark={dark})");
        assert_eq!(col(237), surface);
        assert_eq!(col(238), surface);
        assert_eq!(col(239), border, "near edge bottom");
        assert_eq!(col(240), app, "gap between the edges");
        assert_eq!(col(241), app);
        assert_eq!(col(242), border, "far edge top");
        assert_eq!(col(245), border, "far edge bottom");
        assert_eq!(col(246), app, "nothing below the 10px component");
        // Insets: 6px for the near edge, 12px for the far one.
        assert_eq!(f.pixel(21, 237), app);
        assert_eq!(f.pixel(22, 237), border, "near edge starts 6px in");
        assert_eq!(f.pixel(27, 243), app);
        assert_eq!(f.pixel(28, 243), border, "far edge starts 12px in");
        // Top edge, 282..292: mirrored, the far edge first.
        assert_eq!(col(282), border, "top variant: far edge top");
        assert_eq!(col(285), border);
        assert_eq!(col(286), app);
        assert_eq!(col(288), border, "top variant: near edge top");
        assert_eq!(col(291), border);
        assert_eq!(f.pixel(27, 284), app);
        assert_eq!(f.pixel(28, 284), border);
        assert_eq!(f.pixel(21, 290), app);
        assert_eq!(f.pixel(22, 290), border);
    }
}
