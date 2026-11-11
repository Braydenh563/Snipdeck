//! Minimal reproducers for the Tab-order findings (docs/reviews/final-audit-1.md rows 17-19): a text field
//! between two buttons must be a Tab stop in both directions.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::many_single_char_names,
    reason = "test"
)]

use i_slint_core::{
    accessibility::AccessibleStringProperty, item_tree::ParentItemTraversalMode,
    window::WindowInner,
};
use slint::{ComponentHandle, platform::Key};
use snipdeck_ui::{
    FocusProbeConditional as Conditional, FocusProbeScrolled as Scrolled, FocusProbeClippedSlot as ClippedSlot, FocusProbeFlick as InFlick, FocusProbePlain as Plain,
    testing::Harness,
};

#[path = "support/mod.rs"]
mod support;

fn name(window: &slint::Window) -> Option<String> {
    let mut cur = WindowInner::from_pub(window).focus_item.borrow().upgrade();
    while let Some(i) = cur {
        if i.is_accessible() {
            return Some(
                i.accessible_string_property(AccessibleStringProperty::Label)
                    .unwrap_or_default()
                    .to_string(),
            );
        }
        cur = i.parent_item(ParentItemTraversalMode::FindAllParents);
    }
    None
}

fn walk(window: &slint::Window, back: bool, n: usize) -> Vec<Option<String>> {
    (0..n)
        .map(|_| {
            if back {
                support::chord(window, &[Key::Shift], &support::key(Key::Tab));
            } else {
                support::press(window, &support::key(Key::Tab));
            }
            name(window)
        })
        .collect()
}

/// Both directions visit the same three stops and wrap straight round.
fn check<C: ComponentHandle>(make: fn() -> Result<C, slint::PlatformError>) {
    let h = Harness::new(make, 300, 300);
    let _ = h.render_settled();
    let w = h.component.window();
    let (fwd, bwd) = (walk(w, false, 4), walk(w, true, 4));
    let s = |t: &str| Some(t.to_string());
    assert_eq!(
        (fwd, bwd),
        (
            vec![s("One"), s("Field"), s("Three"), s("One")],
            vec![s("Three"), s("Field"), s("One"), s("Three")],
        )
    );
}

#[test]
fn plain() {
    check(Plain::new);
}
#[test]
fn in_flickable() {
    check(InFlick::new);
}
#[test]
fn conditional() {
    check(Conditional::new);
}
/// A text field scrolled out of view is still a Tab stop (Slint skips an item hidden by a clipping `Rectangle`
/// that sits between it and the Flickable: the field's own viewport used to clip).
#[test]
fn text_field_scrolled_out_of_a_flickable() {
    check(Scrolled::new);
}

/// Slint behaviour the editor used to trip over (finding 18): a control under a clipping `Rectangle` that sits inside
/// a scroll view is skipped by Tab while it is scrolled away. Pinned so a Slint upgrade that fixes it shows up; the
/// app avoids such a `clip` round focusable content (`editors.slint` content slot, `text_field.slint` viewport).
#[test]
fn control_in_clipping_rectangle_in_a_flickable_is_skipped() {
    let h = Harness::new(ClippedSlot::new, 300, 300);
    let _ = h.render_settled();
    let seen = walk(h.component.window(), false, 4);
    assert!(
        !seen.contains(&Some("Field".to_string())),
        "Slint now keeps a clipped control in the Tab order; the workaround may go: {seen:?}"
    );
}
