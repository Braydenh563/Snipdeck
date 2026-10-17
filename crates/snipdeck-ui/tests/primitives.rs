//! Screenshot goldens and pixel measurements for the group-A primitives.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers: a failure here is a failed test"
)]

use snipdeck_ui::{
    MenuGallery, PrimitivesGallery, SurfacesGallery,
    testing::{Harness, assert_golden},
};

#[test]
fn primitives_gallery_light() {
    let h = Harness::new(PrimitivesGallery::new, 560, 290);
    assert_golden("primitives-light", &h.render());
}

#[test]
fn primitives_gallery_dark() {
    let h = Harness::new(PrimitivesGallery::new, 560, 290);
    h.component.set_dark(true);
    assert_golden("primitives-dark", &h.render());
}

#[test]
fn surfaces_gallery_light() {
    let h = Harness::new(SurfacesGallery::new, 560, 380);
    assert_golden("surfaces-light", &h.render());
}

#[test]
fn surfaces_gallery_dark() {
    let h = Harness::new(SurfacesGallery::new, 560, 380);
    h.component.set_dark(true);
    assert_golden("surfaces-dark", &h.render());
}

#[test]
fn menu_gallery_light() {
    let h = Harness::new(MenuGallery::new, 260, 200);
    assert_golden("menu-light", &h.render());
}

#[test]
fn menu_gallery_dark() {
    let h = Harness::new(MenuGallery::new, 260, 200);
    h.component.set_dark(true);
    assert_golden("menu-dark", &h.render());
}
