//! Screenshot tests for the button component.

use snipdeck_ui::{
    ButtonGallery,
    testing::{Harness, assert_golden},
};

#[test]
fn button_gallery_light() {
    let h = Harness::new(ButtonGallery::new, 420, 220);
    assert_golden("buttons-light", &h.render());
}

#[test]
fn button_gallery_dark() {
    let h = Harness::new(ButtonGallery::new, 420, 220);
    h.component.set_dark(true);
    assert_golden("buttons-dark", &h.render());
}
