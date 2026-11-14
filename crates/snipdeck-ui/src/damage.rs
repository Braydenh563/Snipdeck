//! Marks a whole window as needing a new frame, for any window (O39, render glitches).
//!
//! Slint's software renderer draws only what changed since its last frame. When Windows throws
//! part of a window's pixels away (a resize erases the client area, the window is shown again or
//! dragged back from off screen, a region is uncovered), nothing in the scene changed, so the
//! frame after covers only what Slint thinks changed and the rest stays as Windows left it.
//! These functions damage the whole window through the renderer's own "mark dirty" hook (the one
//! Slint uses when a popup closes), so the next frame is drawn whole.

use std::cell::Cell;

thread_local! {
    static MARKED: Cell<u64> = const { Cell::new(0) };
}

/// Damages all of `window`, so the frame being drawn (or the next one) is drawn whole. Asks for
/// no frame: use it while a frame is about to be drawn (winit's `RedrawRequested`, which Slint
/// always answers with a frame); asking for another there would draw one more, empty frame.
pub fn mark_whole(window: &slint::Window) {
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
    MARKED.with(|m| m.set(m.get() + 1));
}

/// Damages all of `window` and asks for a frame: for a change made outside a frame (a placement,
/// a show, a dock).
pub fn whole_window(window: &slint::Window) {
    mark_whole(window);
    window.request_redraw();
}

/// How many times this thread damaged a whole window. Tests read it: a test backend may draw
/// nothing, so they cannot see the damage itself.
#[must_use]
pub fn marked_whole() -> u64 {
    MARKED.with(Cell::get)
}
