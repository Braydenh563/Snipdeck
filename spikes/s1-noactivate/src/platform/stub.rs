//! Non-Windows stub: keeps the crate compiling on Linux CI. Nothing here is a real implementation.

use slint::winit_030::winit::window::WindowAttributes;
use slint::ComponentHandle;

use super::Route;

pub fn window_attributes(attributes: WindowAttributes) -> WindowAttributes {
    attributes.with_active(false)
}

pub fn install(_ui: &impl ComponentHandle, _on_foreground_change: impl Fn() + 'static) {}

pub fn route() -> Route {
    Route::Copy {
        reason: "paste is only implemented on Windows".into(),
    }
}

pub fn fullscreen_in_front() -> bool {
    false
}

pub fn shift_held() -> bool {
    false
}

pub fn begin_text_entry() {}

pub fn end_text_entry() {}

pub fn use_text(_text: &str, _copy_only: bool, done: impl FnOnce(String) + 'static) {
    done("Stub: nothing copied (not Windows)".into());
}
