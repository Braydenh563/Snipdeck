//! The quiet `ItemCard`'s shared-look props (docs/reviews/shared-look-spec.md slice 2): the inline list label, the Use action,
//! no More without a menu, and the inline tile. Positions come from the accessibility tree, never from pixels.
#![allow(
    clippy::expect_used,
    clippy::float_cmp,
    reason = "test file: a missing element is a failed test; sizes are whole logical pixels"
)]

use i_slint_backend_testing::{
    AccessibleRole, ElementHandle, ElementQuery, init_integration_test_with_mock_time,
    mock_elapsed_time,
};
use slint::{
    ComponentHandle, LogicalPosition,
    platform::{PointerEventButton, WindowEvent},
};
use snipdeck_ui::QuietCardGallery;

/// One test function: the testing backend installs once per process and Slint state is per thread, so the scenarios run
/// here in turn, each on a fresh gallery.
#[test]
fn quiet_card_props() {
    init_integration_test_with_mock_time();
    where_inline_label_on_title_line();
    no_more_action_without_menu();
    use_action_fires_use_only();
    inline_tile_title_x_is_40();
}

fn gallery() -> QuietCardGallery {
    let ui = QuietCardGallery::new().expect("gallery");
    ui.show().expect("show");
    ui
}

fn cards(ui: &QuietCardGallery) -> Vec<ElementHandle> {
    let mut found = ElementQuery::from_root(ui)
        .match_accessible_role(AccessibleRole::ListItem)
        .find_all();
    found.sort_by(|a, b| {
        a.absolute_position()
            .y
            .total_cmp(&b.absolute_position().y)
            .then(a.absolute_position().x.total_cmp(&b.absolute_position().x))
    });
    found
}

/// The elements with id `id` (`ItemCard::title-text`) that lie inside `card`.
fn inside(ui: &QuietCardGallery, id: &str, card: &ElementHandle) -> Vec<ElementHandle> {
    let (o, s) = (card.absolute_position(), card.size());
    ElementQuery::from_root(ui)
        .match_id(id)
        .find_all()
        .into_iter()
        .filter(|e| {
            let p = e.absolute_position();
            p.x >= o.x - 0.5 && p.y >= o.y - 0.5 && p.x <= o.x + s.width && p.y <= o.y + s.height
        })
        .collect()
}

fn one(ui: &QuietCardGallery, id: &str, card: &ElementHandle) -> ElementHandle {
    inside(ui, id, card).into_iter().next().expect(id)
}

fn button(ui: &QuietCardGallery, label: &str, card: &ElementHandle) -> Option<ElementHandle> {
    let (o, s) = (card.absolute_position(), card.size());
    ElementQuery::from_root(ui)
        .match_accessible_role(AccessibleRole::Button)
        .find_all()
        .into_iter()
        .find(|b| {
            let p = b.absolute_position();
            b.accessible_label().is_some_and(|l| l == label)
                && p.x >= o.x
                && p.y >= o.y
                && p.x <= o.x + s.width
                && p.y <= o.y + s.height
        })
}

fn click_at(ui: &QuietCardGallery, at: LogicalPosition) {
    let w = ui.window();
    w.dispatch_event(WindowEvent::PointerMoved { position: at });
    w.dispatch_event(WindowEvent::PointerPressed {
        position: at,
        button: PointerEventButton::Left,
    });
    mock_elapsed_time(std::time::Duration::from_millis(50));
    w.dispatch_event(WindowEvent::PointerReleased {
        position: at,
        button: PointerEventButton::Left,
    });
}

fn centre(e: &ElementHandle) -> LogicalPosition {
    let (p, s) = (e.absolute_position(), e.size());
    LogicalPosition::new(p.x + s.width / 2.0, p.y + s.height / 2.0)
}

fn where_inline_label_on_title_line() {
    let ui = gallery();
    let all = cards(&ui);
    let rest = &all[0];
    assert_eq!(rest.size().height, 36.0, "an inline-place row is 36 tall");
    let title = one(&ui, "ItemCard::title-text", rest);
    let label = one(&ui, "ItemCard::where-label", rest);
    let (tp, ts) = (title.absolute_position(), title.size());
    let (lp, ls) = (label.absolute_position(), label.size());
    assert!(ls.width > 0.0, "the label is drawn");
    assert!(
        lp.y >= tp.y && lp.y + ls.height <= tp.y + ts.height + 2.0,
        "label {lp:?} {ls:?} sits on the title line {tp:?} {ts:?}"
    );
    assert!(lp.x >= tp.x + ts.width, "label right of the title");
    let right = rest.absolute_position().x + rest.size().width;
    assert!(
        (right - 4.0 - (lp.x + ls.width)).abs() <= 1.0,
        "right-aligned at the row's edge"
    );
    // With the actions showing, the label moves left of them.
    let shown = &all[1];
    let moved = one(&ui, "ItemCard::where-label", shown);
    let copy = button(&ui, "Copy", shown).expect("Copy");
    assert!(
        moved.absolute_position().x + moved.size().width <= copy.absolute_position().x,
        "label left of the actions"
    );
}

fn no_more_action_without_menu() {
    let ui = gallery();
    let all = cards(&ui);
    let picker = &all[1];
    assert!(button(&ui, "More", picker).is_none(), "no More");
    assert!(button(&ui, "Copy", picker).is_some(), "Copy stays");
    assert!(button(&ui, "Edit", picker).is_some(), "Edit stays");
    assert!(
        button(&ui, "More", &all[2]).is_some(),
        "a row with a menu keeps More"
    );
}

fn use_action_fires_use_only() {
    let ui = gallery();
    let all = cards(&ui);
    let row = &all[2];
    let use_btn = button(&ui, "Paste into Outlook (Enter)", row).expect("Use action");
    let (u, c, e, m) = (
        use_btn.absolute_position().x,
        button(&ui, "Copy", row)
            .expect("Copy")
            .absolute_position()
            .x,
        button(&ui, "Edit", row)
            .expect("Edit")
            .absolute_position()
            .x,
        button(&ui, "More", row)
            .expect("More")
            .absolute_position()
            .x,
    );
    assert!(u < c && c < e && e < m, "order Use, Copy, Edit, More");
    click_at(&ui, centre(&use_btn));
    assert_eq!(ui.get_last_action().as_str(), "use");
    assert_eq!(ui.get_actions(), 1, "one event, and not a copy or a menu");
}

fn inline_tile_title_x_is_40() {
    let ui = gallery();
    let all = cards(&ui);
    let card = &all[3];
    let title = one(&ui, "ItemCard::title-text", card);
    assert_eq!(
        title.absolute_position().x - card.absolute_position().x,
        40.0
    );
    assert!(
        card.size().height >= 96.0,
        "min 96, got {}",
        card.size().height
    );
}
