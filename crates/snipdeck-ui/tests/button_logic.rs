//! Accessibility-tree behaviour of Button, driven through Slint's testing backend.

use i_slint_backend_testing::{
    AccessibleRole, ElementHandle, init_integration_test_with_mock_time,
};
use slint::ComponentHandle;
use snipdeck_ui::ButtonGallery;

#[test]
fn the_default_action_clicks_an_enabled_button_only() {
    init_integration_test_with_mock_time();
    let ui = ButtonGallery::new().expect("gallery");
    ui.show().expect("show");

    let buttons: Vec<_> = ElementHandle::find_by_accessible_label(&ui, "Paste")
        .filter(|e| e.accessible_role() == Some(AccessibleRole::Button))
        .collect();
    assert_eq!(buttons.len(), 16, "4 variants x 4 buttons");
    // Row order: sm, md+icon, lg (counts 1), disabled (would add 100).
    buttons[2].invoke_accessible_default_action();
    buttons[3].invoke_accessible_default_action();
    assert_eq!(ui.get_clicks(), 1);
}
