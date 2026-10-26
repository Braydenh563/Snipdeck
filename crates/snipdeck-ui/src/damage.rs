//! Marks a whole window as needing a new frame, for any window (O39, render glitches).
//!
//! Slint's software renderer draws only what changed since its last frame. When Windows throws
//! part of a window's pixels away (the window is dragged back from off screen, a region is
//! uncovered), nothing in the scene changed, so no frame is presented and the area stays as
//! Windows left it. `RepaintNudge` does this for the Panel and the Main window through a property;
//! [`whole_window`] does it for any window, through the renderer's own "mark dirty" hook (the one
//! Slint uses when a popup closes).

/// Damages all of `window`, so its next frame is drawn whole, and asks for that frame.
pub fn whole_window(window: &slint::Window) {
    use i_slint_core::{
        lengths::{LogicalPoint, LogicalRect, LogicalSize},
        partial_renderer::DirtyRegion,
        window::WindowInner,
    };
    let size = window.size().to_logical(window.scale_factor());
    let rect = LogicalRect::new(
        LogicalPoint::new(0.0, 0.0),
        LogicalSize::new(size.width, size.height),
    );
    WindowInner::from_pub(window)
        .window_adapter()
        .renderer()
        .mark_dirty_region(DirtyRegion::from(rect));
    window.request_redraw();
}
