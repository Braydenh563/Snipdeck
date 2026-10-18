//! Regression tests for the independent review of the component library (`.claude/reviews/ui-components.md`):
//! one test per confirmed finding, written before the fix.
//!
//! Slint's testing platform is process-wide and bound to one thread, so every test runs on the shared UI
//! thread (`support::on_ui_thread`).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementRoot};
use slint::{
    ComponentHandle,
    platform::{Key, PointerEventButton},
};
use snipdeck_ui::{
    DeckGallery, DialogGallery, FormGallery, HotkeyState, MenuGallery, PrimitivesGallery,
    SystemLook, TabsGallery,
};

#[path = "support/mod.rs"]
mod support;
use support::{chord, key, on_ui_thread, press};

fn start<C: ComponentHandle>(create: impl FnOnce() -> Result<C, slint::PlatformError>) -> C {
    let ui = create().expect("gallery");
    ui.show().expect("show");
    ui
}

fn settle() {
    slint::platform::update_timers_and_animations();
}

fn find(root: &impl ElementRoot, label: &str, role: AccessibleRole) -> ElementHandle {
    ElementHandle::find_by_accessible_label(root, label)
        .find(|e| e.accessible_role() == Some(role))
        .unwrap_or_else(|| panic!("no {role:?} labelled {label:?}"))
}

fn count_labelled(root: &impl ElementRoot, label: &str) -> usize {
    ElementHandle::find_by_accessible_label(root, label).count()
}

// A control is tall enough for its text when it is at least one line of that text plus the padding it has at 100%.
fn assert_grows(root: &impl ElementRoot, label: &str, role: AccessibleRole, min_height: f32) {
    let h = find(root, label, role).size().height;
    assert!(
        h >= min_height,
        "{role:?} {label:?} is {h}px high at 225%, its text needs {min_height}px"
    );
}

/// True when the machine has a font with Japanese glyphs. Without one every CJK character falls back to
/// a narrow placeholder box, so the width of a CJK title says nothing about the layout rule (CI has none).
fn has_cjk_font() -> bool {
    std::process::Command::new("fc-list")
        .args([":lang=ja", "family"])
        .output()
        .is_ok_and(|out| !out.stdout.is_empty())
}

macro_rules! ui_tests {
    ($(fn $name:ident() $body:block)*) => {
        $(
            #[test]
            fn $name() {
                on_ui_thread(|| $body);
            }
        )*
    };
}

ui_tests! {

// ---- 1. A secret revealed with Space is hidden again when focus leaves the card ----------------------

fn a_secret_revealed_with_space_is_hidden_when_focus_leaves_the_card() {
    let ui = start(DeckGallery::new);
    let revealed = "Server password, hunter2-correct-horse-7f3a";
    // Compact rows: Thanks(1) Invoice(2) Retry(3) Design(4) Screenshot(5) Q3(6) Server password(7).
    for _ in 0..7 {
        press(ui.window(), &key(Key::Tab));
    }
    // Hold Space (press without release).
    ui.window().dispatch_event(slint::platform::WindowEvent::KeyPressed { text: " ".into() });
    settle();
    assert_eq!(ui.get_last_action(), "reveal");
    assert_eq!(count_labelled(&ui, revealed), 1, "revealed while Space is held");
    // Focus leaves while Space is still held (Tab, a click elsewhere, the window deactivating).
    ui.window().dispatch_event(slint::platform::WindowEvent::KeyPressed { text: key(Key::Tab).into() });
    settle();
    assert_eq!(ui.get_last_action(), "hide", "the card reports that it hid the secret");
    assert_eq!(count_labelled(&ui, revealed), 0, "the secret is no longer in the accessibility tree");
}

// ---- 2. The first click on a HotkeyRecorder starts recording --------------------------------------------

fn the_first_click_on_a_hotkey_recorder_starts_recording() {
    let ui = start(FormGallery::new);
    let hk = find(&ui, "Open panel", AccessibleRole::Button);
    hk.mock_single_click(PointerEventButton::Left);
    settle();
    assert!(ui.get_hk_recording(), "one click begins recording");
    assert_eq!(ui.get_hk_state(), HotkeyState::Recording);
}

// ---- 3. A text TemplateField is not preceded by an invisible Tab stop ---------------------------------------

fn tab_reaches_a_text_template_field_without_an_invisible_stop() {
    let ui = start(FormGallery::new);
    ui.invoke_focus_hk();
    settle();
    // Stops after "Open panel": the empty, hover, recording and conflict recorders (the disabled one is
    // skipped), then the first TemplateField's TextField ("Recipient").
    for _ in 0..5 {
        press(ui.window(), &key(Key::Tab));
    }
    press(ui.window(), "z");
    assert_eq!(ui.get_tf_name(), "z", "the fifth Tab lands in the text field, which takes the typing");
}

// ---- 4. HotkeyRecorder: what counts as a shortcut -----------------------------------------------------------

fn shift_alone_is_not_a_shortcut_and_shift_tab_leaves_the_recorder() {
    let ui = start(FormGallery::new);
    let hk = find(&ui, "Open panel", AccessibleRole::Button);

    hk.invoke_accessible_default_action();
    chord(ui.window(), &[Key::Shift], "a");
    assert!(ui.get_hk_recording(), "Shift+A would swallow plain typing system-wide");
    assert_eq!(ui.get_hk_recorded(), 0);

    // With Shift held a keyboard delivers the shifted symbol, which names no key: not recordable.
    chord(ui.window(), &[Key::Control, Key::Alt, Key::Shift], "!");
    assert!(ui.get_hk_recording(), "Ctrl+Alt+Shift+! is not a key name");
    assert_eq!(ui.get_hk_recorded(), 0);

    // Keys that have no printable name (F13..F24, Caps Lock, ...) come out as private-use characters.
    chord(ui.window(), &[Key::Control, Key::Alt], &key(Key::F13));
    assert!(ui.get_hk_recording(), "F13 has no name in the combo format");
    chord(ui.window(), &[Key::Control, Key::Alt], &key(Key::CapsLock));
    assert!(ui.get_hk_recording(), "Caps Lock has no name in the combo format");
    assert_eq!(ui.get_hk_recorded(), 0);

    // Shift+Tab moves focus away like anywhere else (and so cancels the recording) instead of being swallowed.
    chord(ui.window(), &[Key::Shift], &key(Key::Tab));
    settle();
    assert!(!ui.get_hk_has_focus(), "Shift+Tab leaves the recorder");
    assert!(!ui.get_hk_recording());
    assert_eq!(ui.get_hk_cancels(), 1);
}

// ---- 7. Dialog: Tab stays inside, closing reports so the host can restore focus ----------------------------

fn tab_never_leaves_an_open_dialog() {
    let ui = start(DialogGallery::new);
    ui.set_behind_live(true);
    settle();
    // Forward and backward, more presses than there are stops: focus must keep cycling inside the card.
    for _ in 0..8 {
        press(ui.window(), &key(Key::Tab));
        settle();
        press(ui.window(), " ");
    }
    for _ in 0..8 {
        chord(ui.window(), &[Key::Shift], &key(Key::Tab));
        settle();
        press(ui.window(), " ");
    }
    assert_eq!(ui.get_behind_clicks(), 0, "the button behind the dialog never took focus");
    press(ui.window(), "x");
    assert_eq!(ui.get_behind_text(), "Quarterly report", "the field behind never took focus");
    assert!(ui.get_cancels() + ui.get_confirms() > 0, "the dialog's own buttons did take it");
}

fn closing_a_dialog_reports_it_so_the_host_can_restore_focus() {
    let ui = start(DialogGallery::new);
    assert_eq!(ui.get_closes(), 0);
    ui.set_open(false);
    settle();
    assert_eq!(ui.get_closes(), 1);
    ui.set_open(true);
    settle();
    assert_eq!(ui.get_closes(), 1, "opening is not closing");
}

// ---- 8. The tab bar's add button has a keyboard path ---------------------------------------------------------

fn the_add_list_button_is_reachable_and_operable_from_the_keyboard() {
    let ui = start(TabsGallery::new);
    // Stop 1: the tab bar (one stop, arrows inside). Stop 2: the "+" button.
    press(ui.window(), &key(Key::Tab));
    press(ui.window(), &key(Key::Tab));
    press(ui.window(), "\n");
    assert_eq!(ui.get_adds(), 1, "Enter on the focused + adds a list");
    press(ui.window(), " ");
    assert_eq!(ui.get_adds(), 2, "Space does too");
}

// ---- 6. At 225% text size the fixed-height controls grow with their text ---------------------------------------

fn at_225_percent_primitives_grow_with_their_text() {
    let ui = start(PrimitivesGallery::new);
    ui.global::<SystemLook<'_>>().set_text_scale(2.25);
    settle();
    // line-xs / line-sm are 16px x 2.25 = 36px, line-base is 20px x 2.25 = 45px.
    assert_grows(&ui, "work", AccessibleRole::Button, 36.0 + 8.0);
    assert_grows(&ui, "Control Shift V", AccessibleRole::Text, 36.0 + 4.0);
    assert_grows(&ui, "Start with Windows", AccessibleRole::Switch, 45.0 + 4.0);
    assert_grows(&ui, "Unchecked", AccessibleRole::Checkbox, 45.0 + 4.0);
    assert_grows(&ui, "One", AccessibleRole::RadioButton, 45.0 + 4.0);
}

fn at_225_percent_menu_rows_hotkey_recorder_choice_and_tabs_grow_with_their_text() {
    let menu = start(MenuGallery::new);
    menu.global::<SystemLook<'_>>().set_text_scale(2.25);
    settle();
    assert_grows(&menu, "Copy", AccessibleRole::ListItem, 45.0 + 8.0);

    let form = start(FormGallery::new);
    form.global::<SystemLook<'_>>().set_text_scale(2.25);
    settle();
    assert_grows(&form, "Open panel", AccessibleRole::Button, 45.0 + 8.0);
    assert_grows(&form, "Medium", AccessibleRole::RadioButton, 45.0 + 8.0);

    let tabs = start(TabsGallery::new);
    tabs.global::<SystemLook<'_>>().set_text_scale(2.25);
    settle();
    assert_grows(&tabs, "Work", AccessibleRole::Tab, 45.0 + 12.0);
}

// ---- 14. A masked secret says how to reveal it -------------------------------------------------------------------

fn a_masked_secret_is_announced_with_how_to_reveal_it() {
    let ui = start(DeckGallery::new);
    // Cards only: the pinned strip also has a "Server password" entry, which is not a masked card.
    let masked: Vec<_> = ElementHandle::find_by_accessible_label(&ui, "Server password")
        .filter(|e| e.accessible_role() == Some(AccessibleRole::ListItem))
        .collect();
    // The gallery shows the masked sample in every variant, so there are several.
    assert!(masked.len() >= 3, "one masked card per variant, found {}", masked.len());
    for card in masked {
        assert_eq!(
            card.accessible_description().as_deref(),
            Some("Hidden. Hold Space to reveal."),
            "a masked secret is not announced as a bare title"
        );
    }
    // Once revealed the content speaks for itself, and an ordinary card never gets the hint.
    let shown = ElementHandle::find_by_accessible_label(&ui, "Revealed secret, hunter2-correct-horse-7f3a")
        .next()
        .expect("revealed card");
    assert_ne!(shown.accessible_description().as_deref(), Some("Hidden. Hold Space to reveal."));
}

// ---- 15. The 12-character title floor holds for CJK too ---------------------------------------------------------

fn a_cjk_title_keeps_twelve_characters_next_to_a_list_label() {
    use snipdeck_ui::DeckCjkProbe;
    if !has_cjk_font() {
        eprintln!("skipped: no CJK font installed, glyph widths are not meaningful");
        return;
    }
    let ui = start(DeckCjkProbe::new);
    let title = "請求書のお願いと支払い期限についての確認事項です";
    let titles: Vec<_> = ElementHandle::find_by_accessible_label(&ui, title)
        .filter(|e| e.accessible_role() == Some(AccessibleRole::Text) && e.size().width < 200.0)
        .collect();
    assert!(!titles.is_empty(), "the visible title text is in the tree");
    for t in titles {
        // Type.base is 13px and a CJK character is one em wide: twelve characters need 156px.
        assert!(t.size().width >= 150.0, "title is only {}px wide, 12 CJK characters need 156px", t.size().width);
    }
}

// ---- 10. Menu items are numbered among the actions, not the separators -----------------------------------

fn menu_positions_count_actions_not_separators() {
    let ui = start(MenuGallery::new);
    // Copy, Edit, Pin (disabled), Move to list, Delete: five items and two separators.
    for (label, index) in [("Copy", 0), ("Edit", 1), ("Pin", 2), ("Move to list", 3), ("Delete", 4)] {
        let item = find(&ui, label, AccessibleRole::ListItem);
        assert_eq!(item.accessible_item_index(), Some(index), "{label}");
        assert_eq!(item.accessible_item_count(), Some(5), "{label}");
    }
}

}
