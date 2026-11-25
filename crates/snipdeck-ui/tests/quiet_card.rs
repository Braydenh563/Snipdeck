//! Screenshot goldens of the quiet `ItemCard`'s shared-look props (`QuietCardGallery`): the Picker's inline-place Compact row at
//! rest and with its actions, the Main window's List row with a Use action, and the inline-tile Card.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers: a failure here is a failed test"
)]

use snipdeck_ui::{
    QuietCardGallery,
    testing::{Harness, assert_golden},
};

#[test]
fn quiet_cards_light() {
    let h = Harness::new(QuietCardGallery::new, 600, 400);
    assert_golden("quiet-cards-light", &h.render());
}

#[test]
fn quiet_cards_dark() {
    let h = Harness::new(QuietCardGallery::new, 600, 400);
    h.component.set_dark(true);
    assert_golden("quiet-cards-dark", &h.render());
}
