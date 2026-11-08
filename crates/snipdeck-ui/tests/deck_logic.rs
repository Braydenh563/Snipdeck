//! Behaviour of the deck components through Slint's testing backend: accessibility tree, clicks,
//! keys, press-and-hold. Time is mocked (`mock_elapsed_time`), never the real clock.
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
    platform::{Key, PointerEventButton, WindowEvent},
};
use snipdeck_ui::{DeckCjkProbe, DeckGallery, DeckPartsGallery, SystemLook};

/// Variant order inside `DeckGallery`: compact column, comfortable column, tile column.
const COMPACT: usize = 0;
const COMFORTABLE: usize = 1;

/// Moves the mocked clock forward by `ms` milliseconds.
fn advance(ms: u64) {
    mock_elapsed_time(std::time::Duration::from_millis(ms));
}

/// Installs the testing backend once per process (tests run on parallel threads; each thread has
/// its own Slint state and mocked clock).
fn init() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(init_integration_test_with_mock_time);
}

fn gallery() -> DeckGallery {
    init();
    let ui = DeckGallery::new().expect("gallery");
    ui.show().expect("show");
    ui
}

/// Every `ItemCard` (role list-item) of the gallery, in tree order.
fn cards(ui: &DeckGallery) -> Vec<ElementHandle> {
    let mut seen = Vec::new();
    ElementQuery::from_root(ui)
        .match_descendants()
        .match_accessible_role(AccessibleRole::ListItem)
        .find_all()
        .into_iter()
        // The query reports repeated elements more than once; one card is one position and size.
        .filter(|c| {
            let (p, s) = (c.absolute_position(), c.size());
            let key = (
                p.x.to_bits(),
                p.y.to_bits(),
                s.width.to_bits(),
                s.height.to_bits(),
            );
            let fresh = !seen.contains(&key);
            seen.push(key);
            fresh
        })
        .collect()
}

/// The three cards (compact, comfortable, tile) whose accessible label starts with `title`.
fn trio(ui: &DeckGallery, title: &str) -> Vec<ElementHandle> {
    let found: Vec<_> = cards(ui)
        .into_iter()
        .filter(|c| {
            c.accessible_label()
                .is_some_and(|l| l.as_str().starts_with(title))
        })
        .collect();
    assert_eq!(found.len(), 3, "one card per variant for {title:?}");
    found
}

fn centre(card: &ElementHandle) -> LogicalPosition {
    let p = card.absolute_position();
    let s = card.size();
    LogicalPosition::new(p.x + s.width / 2.0, p.y + s.height / 2.0)
}

fn press(ui: &DeckGallery, at: LogicalPosition, button: PointerEventButton) {
    let w = ui.window();
    w.dispatch_event(WindowEvent::PointerMoved { position: at });
    w.dispatch_event(WindowEvent::PointerPressed {
        position: at,
        button,
    });
}

fn release(ui: &DeckGallery, at: LogicalPosition, button: PointerEventButton) {
    ui.window().dispatch_event(WindowEvent::PointerReleased {
        position: at,
        button,
    });
}

fn click(ui: &DeckGallery, card: &ElementHandle, button: PointerEventButton) {
    let at = centre(card);
    press(ui, at, button);
    advance(50);
    release(ui, at, button);
}

fn key(ui: &DeckGallery, k: Key) {
    ui.window()
        .dispatch_event(WindowEvent::KeyPressed { text: k.into() });
    ui.window()
        .dispatch_event(WindowEvent::KeyReleased { text: k.into() });
}

/// Moves keyboard focus to the `n`th focusable thing (1 = first card) with Tab.
fn tab_to(ui: &DeckGallery, n: usize) {
    for _ in 0..n {
        key(ui, Key::Tab);
    }
}

fn cards_are_list_items_labelled_with_title_and_preview() {
    let ui = gallery();
    let all = cards(&ui);
    assert_eq!(all.len(), 14 * 3, "14 samples x 3 variants");

    for card in trio(&ui, "Thanks, received") {
        assert_eq!(
            card.accessible_label().as_deref(),
            Some(
                "Thanks, received, Thanks for sending this over. I'll review it today and get back to you by end of day."
            ),
            "label = title + preview"
        );
        assert_eq!(card.accessible_item_selectable(), Some(true));
        assert_eq!(card.accessible_item_selected(), Some(false));
    }
    for card in trio(&ui, "Out of office") {
        assert_eq!(
            card.accessible_item_selected(),
            Some(true),
            "selected state"
        );
    }
}

fn a_sensitive_card_never_speaks_its_content_until_revealed() {
    let ui = gallery();
    for card in trio(&ui, "Server password") {
        assert_eq!(
            card.accessible_label().as_deref(),
            Some("Server password"),
            "masked: title only"
        );
    }
    for card in trio(&ui, "Revealed secret") {
        let label = card.accessible_label().expect("label");
        assert!(label.contains("hunter2"), "revealed: {label}");
    }
}

fn every_card_is_at_least_24px_tall_a_compact_row_is_36_and_a_list_row_56() {
    let ui = gallery();
    for (i, card) in cards(&ui).iter().enumerate() {
        let h = card.size().height;
        assert!(h >= 24.0, "card {i} is {h}px tall");
        let label = card.accessible_label().unwrap_or_default();
        assert!(card.size().width >= 24.0, "card {label:?} too narrow");
    }
    // Compact rows are the first 14 cards (compact column); 36px, or 52px with a where line (samples 10 and 11 carry a list name).
    let all = cards(&ui);
    for (i, card) in all.iter().take(14).enumerate() {
        let expected = if matches!(i, 10 | 11) { 52.0 } else { 36.0 };
        assert_eq!(card.size().height, expected, "compact row {i}");
    }
    // A List row without a where line: a 20px title line at y 4 and the 32px preview band under it.
    assert_eq!(all[14].size().height, 56.0, "List row");
    // A row that says where it lives adds a 16px line: 68.
    let home = &trio(&ui, "Home address")[COMFORTABLE];
    assert_eq!(home.size().height, 68.0, "List row with a where line");
}

fn the_default_action_uses_the_item() {
    let ui = gallery();
    for (variant, card) in trio(&ui, "Design spec").iter().enumerate() {
        ui.set_last_action("".into());
        card.invoke_accessible_default_action();
        assert_eq!(ui.get_last_action(), "use", "variant {variant}");
        assert_eq!(ui.get_last_title(), "Design spec");
    }
}

fn click_uses_shift_click_copies_right_click_asks_for_the_menu() {
    let ui = gallery();
    for (variant, card) in trio(&ui, "Retry helper").iter().enumerate() {
        click(&ui, card, PointerEventButton::Left);
        assert_eq!(ui.get_last_action(), "use", "click, variant {variant}");

        ui.window().dispatch_event(WindowEvent::KeyPressed {
            text: Key::Shift.into(),
        });
        click(&ui, card, PointerEventButton::Left);
        ui.window().dispatch_event(WindowEvent::KeyReleased {
            text: Key::Shift.into(),
        });
        assert_eq!(
            ui.get_last_action(),
            "copy",
            "shift-click, variant {variant}"
        );

        click(&ui, card, PointerEventButton::Right);
        assert_eq!(
            ui.get_last_action(),
            "menu",
            "right-click, variant {variant}"
        );
        assert_eq!(ui.get_last_title(), "Retry helper");
    }
}

fn a_release_outside_the_card_is_not_a_use() {
    let ui = gallery();
    let card = &trio(&ui, "Retry helper")[COMFORTABLE];
    let before = ui.get_actions();
    let at = centre(card);
    press(&ui, at, PointerEventButton::Left);
    let away = LogicalPosition::new(at.x, at.y + 200.0);
    ui.window()
        .dispatch_event(WindowEvent::PointerMoved { position: away });
    release(&ui, away, PointerEventButton::Left);
    assert_eq!(ui.get_actions(), before);
}

fn enter_uses_shift_enter_copies_the_menu_key_opens_the_context_menu() {
    let ui = gallery();
    // Focus order is tree order: the first compact row ("Thanks, received") is first.
    tab_to(&ui, 1);
    key(&ui, Key::Return);
    assert_eq!(ui.get_last_action(), "use");
    assert_eq!(ui.get_last_title(), "Thanks, received");

    ui.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Shift.into(),
    });
    key(&ui, Key::Return);
    ui.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Shift.into(),
    });
    assert_eq!(ui.get_last_action(), "copy");

    key(&ui, Key::Menu);
    assert_eq!(ui.get_last_action(), "menu");

    // Arrow keys are the list's business: the card leaves them alone (focus stays put).
    ui.set_last_action("".into());
    key(&ui, Key::DownArrow);
    assert_eq!(ui.get_last_action(), "");
}

/// The buttons labelled `label` inside the whole gallery (the query reports one more than once).
fn buttons_named(ui: &DeckGallery, label: &str) -> Vec<ElementHandle> {
    let mut seen = Vec::new();
    ElementQuery::from_root(ui)
        .match_descendants()
        .match_accessible_role(AccessibleRole::Button)
        .find_all()
        .into_iter()
        .filter(|b| b.accessible_label().is_some_and(|l| l == label))
        .filter(|b| {
            let (p, s) = (b.absolute_position(), b.size());
            let key = (p.x.to_bits(), p.y.to_bits(), s.width.to_bits());
            let fresh = !seen.contains(&key);
            seen.push(key);
            fresh
        })
        .collect()
}

/// The button of `card` among `buttons`: the one whose box lies inside the card's.
fn inside<'a>(buttons: &'a [ElementHandle], card: &ElementHandle) -> &'a ElementHandle {
    let (origin, size) = (card.absolute_position(), card.size());
    buttons
        .iter()
        .find(|b| {
            let (p, s) = (b.absolute_position(), b.size());
            p.x >= origin.x
                && p.y >= origin.y
                && p.x + s.width <= origin.x + size.width
                && p.y + s.height <= origin.y + size.height
        })
        .expect("a button inside the card")
}

/// The row actions (Copy, Edit, More) show on hover and on the selected row only, never at rest on another row.
/// A click on one is that action's own and is never a use.
fn row_actions_show_on_hover_and_the_selected_row_and_a_click_on_one_is_not_a_use() {
    let ui = gallery();
    let selected_only = |label: &str| {
        let all = buttons_named(&ui, label);
        let selected = trio(&ui, "Out of office");
        assert_eq!(
            all.len(),
            3,
            "{label}: only the selected row's three at rest"
        );
        for card in &selected {
            let _ = inside(&all, card);
        }
    };
    for label in ["Copy", "Edit", "More"] {
        selected_only(label);
    }

    // Hovering another row brings its actions in.
    let card = &trio(&ui, "Thanks, received")[COMFORTABLE];
    ui.window().dispatch_event(WindowEvent::PointerMoved {
        position: centre(card),
    });
    advance(120);
    let (more, edit, copy) = (
        buttons_named(&ui, "More"),
        buttons_named(&ui, "Edit"),
        buttons_named(&ui, "Copy"),
    );
    assert_eq!(
        more.len(),
        4,
        "the hovered row's More joins the selected row's"
    );
    for b in more.iter().chain(&edit).chain(&copy) {
        let s = b.size();
        assert!(s.width >= 24.0 && s.height >= 24.0, "target size {s:?}");
    }
    let (more_here, edit_here, copy_here) = (
        inside(&more, card),
        inside(&edit, card),
        inside(&copy, card),
    );
    let (c, e, m) = (
        copy_here.absolute_position(),
        edit_here.absolute_position(),
        more_here.absolute_position(),
    );
    assert!(
        c.x + copy_here.size().width <= e.x + 0.5 && e.x + edit_here.size().width <= m.x + 0.5,
        "Copy, Edit, More sit left to right without overlapping: {c:?} {e:?} {m:?}"
    );

    // Each click reports its own action and no use.
    for (button, expected) in [
        (copy_here, "copy"),
        (edit_here, "edit"),
        (more_here, "menu"),
    ] {
        let before = ui.get_actions();
        ui.set_last_action("".into());
        let at = centre(button);
        press(&ui, at, PointerEventButton::Left);
        release(&ui, at, PointerEventButton::Left);
        assert_eq!(ui.get_last_action(), expected);
        assert_eq!(ui.get_actions(), before + 1, "one {expected} and no use");
    }

    // The default action (assistive technology) does the same.
    for (button, expected) in [
        (copy_here, "copy"),
        (edit_here, "edit"),
        (more_here, "menu"),
    ] {
        ui.set_last_action("".into());
        button.invoke_accessible_default_action();
        assert_eq!(ui.get_last_action(), expected);
    }

    // They go when the pointer leaves (only the selected row keeps them).
    ui.window().dispatch_event(WindowEvent::PointerExited);
    advance(120);
    assert_eq!(buttons_named(&ui, "More").len(), 3);
    assert_eq!(buttons_named(&ui, "Edit").len(), 3);
    assert_eq!(buttons_named(&ui, "Copy").len(), 3);
}

fn press_and_hold_reveals_a_sensitive_item_without_using_it() {
    let ui = gallery();
    let card = &trio(&ui, "Server password")[COMPACT];
    // On the title, left of the row's actions: a hovered row has Use, Copy, Edit, More and the eye at its right end,
    // and a press on one of them is that action's own.
    let at = LogicalPosition::new(card.absolute_position().x + 80.0, centre(card).y);

    press(&ui, at, PointerEventButton::Left);
    advance(400);
    assert_eq!(ui.get_actions(), 0, "not yet revealed before 500ms");
    advance(200);
    assert_eq!(ui.get_last_action(), "reveal");
    release(&ui, at, PointerEventButton::Left);
    assert_eq!(
        ui.get_last_action(),
        "hide",
        "release hides, it is not a use"
    );
    assert_eq!(ui.get_actions(), 2, "reveal + hide only");

    // A quick click on the same card is an ordinary use.
    click(&ui, card, PointerEventButton::Left);
    assert_eq!(ui.get_last_action(), "use");
}

fn holding_space_on_a_focused_sensitive_card_reveals_it() {
    let ui = gallery();
    // Compact rows: Thanks(1) Invoice(2) Retry(3) Design(4) Screenshot(5) Q3(6) Server password(7).
    tab_to(&ui, 7);
    ui.window()
        .dispatch_event(WindowEvent::KeyPressed { text: " ".into() });
    assert_eq!(ui.get_last_action(), "reveal");
    assert_eq!(ui.get_last_title(), "Server password");
    ui.window()
        .dispatch_event(WindowEvent::KeyReleased { text: " ".into() });
    assert_eq!(ui.get_last_action(), "hide");
}

fn deck_parts_report_clicks_menus_and_have_accessible_names() {
    init();
    let ui = DeckPartsGallery::new().expect("parts gallery");
    ui.show().expect("show");

    // BoardSwitcher: a button named after the board; the open one is expanded.
    let work = ElementHandle::find_by_accessible_label(&ui, "Work")
        .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
        .expect("board switcher");
    assert!(work.size().height >= 24.0);
    work.invoke_accessible_default_action();
    assert_eq!(ui.get_last(), "board-menu");
    let personal = ElementHandle::find_by_accessible_label(&ui, "Board: Personal")
        .next()
        .expect("expanded switcher");
    assert_eq!(personal.accessible_expanded(), Some(true));

    // PinnedStrip: chips are buttons; the default action uses the item.
    let chip = ElementHandle::find_by_accessible_label(&ui, "Home address")
        .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
        .expect("pinned chip");
    assert!(chip.size().height >= 24.0);
    chip.invoke_accessible_default_action();
    assert_eq!(ui.get_last(), "pinned:1");

    // SidebarItem: list items with a count as value and a selected state.
    let recent = ElementHandle::find_by_accessible_label(&ui, "Recent")
        .find(|e| e.accessible_role() == Some(AccessibleRole::ListItem))
        .expect("sidebar item");
    assert_eq!(recent.accessible_value().as_deref(), Some("12"));
    assert_eq!(recent.accessible_item_selected(), Some(false));
    assert!(recent.size().height >= 24.0);
    recent.invoke_accessible_default_action();
    assert_eq!(ui.get_last(), "sidebar:Recent");
    let selected = ElementHandle::find_by_accessible_label(&ui, "Addresses")
        .find(|e| e.accessible_role() == Some(AccessibleRole::ListItem))
        .expect("selected list");
    assert_eq!(selected.accessible_item_selected(), Some(true));

    // Right-click and the Menu key ask for the context menu.
    let at = {
        let p = recent.absolute_position();
        let s = recent.size();
        LogicalPosition::new(p.x + s.width / 2.0, p.y + s.height / 2.0)
    };
    ui.window()
        .dispatch_event(WindowEvent::PointerMoved { position: at });
    ui.window().dispatch_event(WindowEvent::PointerPressed {
        position: at,
        button: PointerEventButton::Right,
    });
    advance(50);
    ui.window().dispatch_event(WindowEvent::PointerReleased {
        position: at,
        button: PointerEventButton::Right,
    });
    assert_eq!(ui.get_last(), "sidebar-menu:Recent");

    // Past two chips the strip says how many are left, as a button that asks the host for them.
    let more = ElementHandle::find_by_accessible_label(&ui, "+5 more")
        .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
        .expect("the +N more button");
    assert!(more.size().height >= 24.0 && more.size().width >= 24.0);
    more.invoke_accessible_default_action();
    assert_eq!(ui.get_last(), "pinned-more");
}

fn at_225_percent_text_the_rows_grow_and_stay_hittable() {
    let ui = gallery();
    ui.global::<SystemLook<'_>>().set_text_scale(2.25);
    let all = cards(&ui);
    for (i, card) in all.iter().take(14).enumerate() {
        // Title line 20px x 2.25 + 2 x 8px; a row with a where line also grows by that line.
        let h = card.size().height;
        if matches!(i, 10 | 11) {
            assert!(
                h > 61.0 && h <= 61.0 + 16.0 * 2.25,
                "compact row {i} at 225%: {h}"
            );
        } else {
            assert_eq!(h, 61.0, "compact row {i} at 225%");
        }
    }
    for (i, card) in all.iter().enumerate() {
        assert!(
            card.size().height >= 24.0 && card.size().width >= 24.0,
            "card {i}"
        );
    }
}

/// CJK is measured here, never put in a golden: IBM Plex has no CJK glyphs, so the pixels would depend
/// on the system fallback font. Sizes and labels do not.
fn cjk_cards_keep_their_height_and_stay_inside_the_column() {
    init();
    let ui = DeckCjkProbe::new().expect("probe");
    ui.show().expect("show");
    let found: Vec<_> = ElementQuery::from_root(&ui)
        .match_descendants()
        .match_accessible_role(AccessibleRole::ListItem)
        .find_all();
    assert!(
        found.len() >= 6,
        "three samples x two variants, got {}",
        found.len()
    );
    for card in &found {
        let (p, s) = (card.absolute_position(), card.size());
        assert!(
            s.height >= 24.0,
            "{:?} is {} tall",
            card.accessible_label(),
            s.height
        );
        assert!(
            p.x >= 0.0 && p.x + s.width <= 200.5,
            "{:?} x {} w {}",
            card.accessible_label(),
            p.x,
            s.width
        );
    }
    // A compact row is exactly 36px whatever the script.
    assert_eq!(found[0].size().height, 36.0);
    assert!(
        found.iter().any(|c| c
            .accessible_label()
            .is_some_and(|l| l.starts_with("請求書のお願い, お世話になっております"))),
        "title and preview in the label"
    );
}

/// One test function: the testing backend can be installed once per process, so every scenario
/// runs here in turn, each on a fresh gallery.
#[test]
fn deck_behaviour() {
    init();
    cards_are_list_items_labelled_with_title_and_preview();
    a_sensitive_card_never_speaks_its_content_until_revealed();
    every_card_is_at_least_24px_tall_a_compact_row_is_36_and_a_list_row_56();
    the_default_action_uses_the_item();
    click_uses_shift_click_copies_right_click_asks_for_the_menu();
    a_release_outside_the_card_is_not_a_use();
    enter_uses_shift_enter_copies_the_menu_key_opens_the_context_menu();
    row_actions_show_on_hover_and_the_selected_row_and_a_click_on_one_is_not_a_use();
    press_and_hold_reveals_a_sensitive_item_without_using_it();
    holding_space_on_a_focused_sensitive_card_reveals_it();
    deck_parts_report_clicks_menus_and_have_accessible_names();
    at_225_percent_text_the_rows_grow_and_stay_hittable();
    cjk_cards_keep_their_height_and_stay_inside_the_column();
    the_use_button_uses_the_item_once_and_never_only_selects_it();
    the_use_button_is_a_button_for_assistive_technology_only_on_a_hovered_or_selected_row();
}

/// The use buttons ("Paste <title>") the gallery shows right now (the query reports a repeated one more than once).
fn use_buttons(ui: &DeckGallery) -> Vec<ElementHandle> {
    let mut seen = Vec::new();
    ElementQuery::from_root(ui)
        .match_descendants()
        .match_accessible_role(AccessibleRole::Button)
        .find_all()
        .into_iter()
        .filter(|b| {
            b.accessible_label()
                .is_some_and(|l| l.starts_with("Paste "))
        })
        .filter(|b| {
            let p = b.absolute_position();
            let key = (p.x.to_bits(), p.y.to_bits());
            let fresh = !seen.contains(&key);
            seen.push(key);
            fresh
        })
        .collect()
}

/// The use button inside `card`, once the card shows it.
fn use_button_in(ui: &DeckGallery, card: &ElementHandle) -> ElementHandle {
    let (p, s) = (card.absolute_position(), card.size());
    use_buttons(ui)
        .into_iter()
        .find(|b| {
            let bp = b.absolute_position();
            bp.x >= p.x && bp.x <= p.x + s.width && bp.y >= p.y && bp.y <= p.y + s.height
        })
        .expect("a use button inside the card")
}

fn the_use_button_uses_the_item_once_and_never_only_selects_it() {
    let ui = gallery();
    // O33: in the main window (`pick-mode`) a click on the row selects it; a click on the row's
    // "Paste" / "Copy" button must still use the item, exactly once.
    for pick_mode in [false, true] {
        ui.set_pick_mode(pick_mode);
        for (variant, card) in trio(&ui, "Thanks, received").iter().enumerate() {
            let at = centre(card);
            ui.window()
                .dispatch_event(WindowEvent::PointerMoved { position: at });
            advance(120);
            let button = use_button_in(&ui, card);
            assert_eq!(
                button.accessible_label().as_deref(),
                Some("Paste Thanks, received"),
                "named after the item, pick {pick_mode}, variant {variant}"
            );
            let before = ui.get_actions();
            ui.set_last_action("".into());
            let on_button = centre(&button);
            press(&ui, on_button, PointerEventButton::Left);
            advance(50);
            release(&ui, on_button, PointerEventButton::Left);
            assert_eq!(
                ui.get_last_action(),
                "use",
                "pick {pick_mode}, variant {variant}"
            );
            assert_eq!(ui.get_actions(), before + 1, "exactly one gesture reported");
            assert_eq!(ui.get_last_title(), "Thanks, received");

            // The row body keeps its own meaning: a pick in selection mode, a use otherwise.
            ui.set_last_action("".into());
            click(&ui, card, PointerEventButton::Left);
            assert_eq!(
                ui.get_last_action(),
                if pick_mode { "pick" } else { "use" },
                "row body, pick {pick_mode}, variant {variant}"
            );
        }
        ui.window().dispatch_event(WindowEvent::PointerMoved {
            position: LogicalPosition::new(1.0, 1.0),
        });
        advance(120);
    }
}

fn the_use_button_is_a_button_for_assistive_technology_only_on_a_hovered_or_selected_row() {
    let ui = gallery();
    // At rest only the selected sample shows its row actions, so only it has a Use button to announce.
    let rest = use_buttons(&ui);
    assert!(
        rest.len() == 3
            && rest
                .iter()
                .all(|b| b.accessible_label().as_deref() == Some("Paste Out of office")),
        "only the selected row at rest: {}",
        rest.len()
    );

    let card = &trio(&ui, "Design spec")[COMFORTABLE];
    ui.window().dispatch_event(WindowEvent::PointerMoved {
        position: centre(card),
    });
    advance(120);
    let button = use_button_in(&ui, card);
    ui.set_last_action("".into());
    button.invoke_accessible_default_action();
    assert_eq!(ui.get_last_action(), "use");
    assert_eq!(ui.get_last_title(), "Design spec");
}
