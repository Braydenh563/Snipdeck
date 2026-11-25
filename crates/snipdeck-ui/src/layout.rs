//! Running a layout pass on a window that has not been drawn.
//!
//! A conditional row (`if`) is only created or removed by a layout pass, which a frame or a walk
//! of the item tree runs. A hidden editor window that is reused has no frame: its `wanted-height`
//! still describes the rows of the last time it was drawn. [`force_layout`] walks the tree so the
//! next read of a layout-derived property is current.

use std::ops::ControlFlow;

use i_slint_core::{item_tree::ItemRc, window::WindowInner};

/// Brings the rows of `window` (the `if` and `for` instances) up to date with its properties.
///
/// Reading `wanted-height` afterwards gives the height of the window as it would be drawn now.
pub fn force_layout(window: &slint::Window) {
    let root = ItemRc::new_root(WindowInner::from_pub(window).component());
    let _: Option<()> = root.visit_descendants(|_| ControlFlow::Continue(()));
}
