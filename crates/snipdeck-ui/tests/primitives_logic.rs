//! Accessibility-tree behaviour of the group-A primitives, driven through Slint's testing backend:
//! roles and labels, default actions, disabled controls, hit-target size, keyboard navigation.

use std::{
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::{Mutex, OnceLock, mpsc},
    thread,
    time::Duration,
};

use i_slint_backend_testing::{
    AccessibleLiveness, AccessibleRole, ElementHandle, ElementQuery,
    init_integration_test_with_mock_time, mock_elapsed_time,
};
use slint::{
    ComponentHandle, LogicalPosition, SharedString,
    platform::{Key, PointerEventButton, WindowEvent},
};
use snipdeck_ui::{MenuGallery, PrimitivesGallery, SurfacesGallery, ToastPhase};

type Job = Box<dyn FnOnce() + Send>;

/// The testing backend installs one platform per process and Slint components stay on the thread
/// that created them, so every test body runs, one at a time, on one shared UI thread.
fn on_ui(body: impl FnOnce() + Send + 'static) {
    static QUEUE: OnceLock<Mutex<mpsc::Sender<Job>>> = OnceLock::new();
    let queue = QUEUE.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        thread::spawn(move || {
            init_integration_test_with_mock_time();
            for job in rx {
                job();
            }
        });
        Mutex::new(tx)
    });
    let (done, result) = mpsc::channel();
    let job: Job = Box::new(move || {
        let _ = done.send(catch_unwind(AssertUnwindSafe(body)));
    });
    queue
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .send(job)
        .expect("the UI thread is running");
    if let Err(panic) = result.recv().expect("the UI thread answers") {
        resume_unwind(panic);
    }
}

fn by_label(ui: &impl ComponentHandle, label: &str, role: AccessibleRole) -> ElementHandle {
    // Compiled with `ElementRoot`, which every generated component implements.
    let found: Vec<_> = ElementHandle::find_by_accessible_label(ui, label)
        .filter(|e| e.accessible_role() == Some(role))
        .collect();
    assert_eq!(
        found.len(),
        1,
        "expected exactly one {role:?} labelled {label:?}, found {}",
        found.len()
    );
    found.into_iter().next().expect("one element")
}

/// The toast container itself (the only element with that label that is a live region).
fn toast(ui: &impl ComponentHandle, label: &str) -> ElementHandle {
    ElementHandle::find_by_accessible_label(ui, label)
        .find(|e| e.accessible_live_region() == Some(AccessibleLiveness::Polite))
        .unwrap_or_else(|| panic!("no toast labelled {label:?}"))
}

fn key(ui: &impl ComponentHandle, key: Key) {
    let text: SharedString = key.into();
    ui.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    ui.window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

fn gallery() -> PrimitivesGallery {
    let ui = PrimitivesGallery::new().expect("gallery");
    ui.show().expect("show");
    ui
}

#[test]
fn icon_button_is_named_by_its_tooltip_and_clicks_only_when_enabled() {
    on_ui(|| {
        let ui = gallery();
        let copy = by_label(&ui, "Copy", AccessibleRole::Button);
        copy.invoke_accessible_default_action();
        assert_eq!(ui.get_icon_clicks(), 1);

        let more = by_label(&ui, "More", AccessibleRole::Button);
        assert_eq!(more.accessible_enabled(), Some(false));
        more.invoke_accessible_default_action();
        assert_eq!(
            ui.get_icon_clicks(),
            1,
            "a disabled IconButton ignores activation"
        );
    });
}

#[test]
fn icon_button_sizes_are_24_and_28() {
    on_ui(|| {
        let ui = gallery();
        let sm = by_label(&ui, "Copy", AccessibleRole::Button).size();
        let md = by_label(&ui, "Edit", AccessibleRole::Button).size();
        assert_eq!((sm.width, sm.height), (24.0, 24.0));
        assert_eq!((md.width, md.height), (28.0, 28.0));
    });
}

#[test]
fn icon_button_clicks_with_the_pointer_and_the_keyboard() {
    on_ui(|| {
        let ui = gallery();
        by_label(&ui, "Copy", AccessibleRole::Button).mock_single_click(PointerEventButton::Left);
        assert_eq!(ui.get_icon_clicks(), 1);
        // Tab reaches the first focusable control (the Copy icon button); Space activates it.
        key(&ui, Key::Tab);
        ui.window()
            .dispatch_event(WindowEvent::KeyPressed { text: " ".into() });
        assert_eq!(ui.get_icon_clicks(), 2);
    });
}

#[test]
fn toggle_flips_and_reports() {
    on_ui(|| {
        let ui = gallery();
        let off = by_label(&ui, "Off", AccessibleRole::Switch);
        assert_eq!(off.accessible_checkable(), Some(true));
        assert_eq!(off.accessible_checked(), Some(false));
        off.invoke_accessible_default_action();
        assert_eq!(off.accessible_checked(), Some(true), "toggle turns on");
        assert_eq!(ui.get_toggles(), 1);
        off.invoke_accessible_default_action();
        assert_eq!(off.accessible_checked(), Some(false), "and back off");
        assert_eq!(ui.get_toggles(), 2);

        let on = by_label(&ui, "On", AccessibleRole::Switch);
        assert_eq!(on.accessible_checked(), Some(true));
    });
}

#[test]
fn disabled_toggle_checkbox_radio_tag_swatch_ignore_activation() {
    on_ui(|| {
        let ui = gallery();
        let toggle = by_label(&ui, "Disabled off", AccessibleRole::Switch);
        assert_eq!(toggle.accessible_enabled(), Some(false));
        toggle.invoke_accessible_default_action();
        assert_eq!(toggle.accessible_checked(), Some(false));

        let checkbox = by_label(&ui, "Disabled", AccessibleRole::Checkbox);
        assert_eq!(checkbox.accessible_enabled(), Some(false));
        checkbox.invoke_accessible_default_action();
        assert_eq!(checkbox.accessible_checked(), Some(false));

        let tag = by_label(&ui, "locked", AccessibleRole::Button);
        tag.invoke_accessible_default_action();
        tag.mock_single_click(PointerEventButton::Left);

        let radio = by_label(&ui, "Disabled", AccessibleRole::RadioButton);
        assert_eq!(radio.accessible_enabled(), Some(false));
        radio.invoke_accessible_default_action();
        radio.mock_single_click(PointerEventButton::Left);

        let swatch = by_label(&ui, "Slate disabled", AccessibleRole::RadioButton);
        assert_eq!(swatch.accessible_enabled(), Some(false));
        swatch.invoke_accessible_default_action();
        swatch.mock_single_click(PointerEventButton::Left);

        assert_eq!(ui.get_tag_clicks(), 0);
        assert_eq!(ui.get_swatch_clicks(), 0);
        assert_eq!(ui.get_radio_activations(), 0);
    });
}

#[test]
fn checkbox_mixed_resolves_to_checked_and_exposes_its_value() {
    on_ui(|| {
        let ui = gallery();
        let mixed = by_label(&ui, "Mixed", AccessibleRole::Checkbox);
        assert_eq!(
            mixed.accessible_value().as_deref(),
            Some("Partially checked")
        );
        assert_eq!(
            mixed.accessible_checked(),
            Some(false),
            "mixed is not plain checked"
        );
        mixed.invoke_accessible_default_action();
        assert_eq!(
            mixed.accessible_value().as_deref(),
            Some(""),
            "no longer mixed"
        );
        assert_eq!(
            mixed.accessible_checked(),
            Some(true),
            "activating mixed turns it on"
        );
        mixed.invoke_accessible_default_action();
        assert_eq!(mixed.accessible_checked(), Some(false));

        let plain = by_label(&ui, "Unchecked", AccessibleRole::Checkbox);
        assert_eq!(plain.accessible_checked(), Some(false));
        plain.mock_single_click(PointerEventButton::Left);
        assert_eq!(plain.accessible_checked(), Some(true));
    });
}

#[test]
fn radio_activates_when_unselected_only() {
    on_ui(|| {
        let ui = gallery();
        let one = by_label(&ui, "One", AccessibleRole::RadioButton);
        let two = by_label(&ui, "Two", AccessibleRole::RadioButton);
        assert_eq!(one.accessible_checked(), Some(false));
        assert_eq!(two.accessible_checked(), Some(true));
        one.invoke_accessible_default_action();
        assert_eq!(ui.get_radio_activations(), 1);
        two.invoke_accessible_default_action();
        assert_eq!(
            ui.get_radio_activations(),
            1,
            "already selected: nothing to do"
        );
    });
}

#[test]
fn tag_clicks_and_removes() {
    on_ui(|| {
        let ui = gallery();
        let work = by_label(&ui, "work", AccessibleRole::Button);
        work.invoke_accessible_default_action();
        assert_eq!(ui.get_tag_clicks(), 1);
        assert_eq!(
            by_label(&ui, "email", AccessibleRole::Button).accessible_checked(),
            Some(true)
        );

        let remove = by_label(&ui, "Remove tag snippets", AccessibleRole::Button);
        remove.invoke_accessible_default_action();
        assert_eq!(ui.get_tag_removals(), 1);
        remove.mock_single_click(PointerEventButton::Left);
        assert_eq!(ui.get_tag_removals(), 2);
        assert_eq!(
            ui.get_tag_clicks(),
            1,
            "the x control does not also click the chip"
        );
    });
}

#[test]
fn tag_delete_key_removes_a_removable_chip() {
    on_ui(|| {
        let ui = gallery();
        // Focus order: Copy, Edit, Delete, Settings, Pinned (5 icon buttons), then work, email, snippets.
        for _ in 0..8 {
            key(&ui, Key::Tab);
        }
        key(&ui, Key::Delete);
        assert_eq!(ui.get_tag_removals(), 1);
        key(&ui, Key::Backspace);
        assert_eq!(ui.get_tag_removals(), 2);
    });
}

#[test]
fn swatches_are_radio_buttons_named_by_hue_and_show_the_selection() {
    on_ui(|| {
        let ui = gallery();
        for hue in [
            "Blue", "Teal", "Green", "Amber", "Orange", "Red", "Rose", "Violet", "Slate",
        ] {
            by_label(&ui, hue, AccessibleRole::RadioButton);
        }
        assert_eq!(
            by_label(&ui, "Teal", AccessibleRole::RadioButton).accessible_checked(),
            Some(true)
        );
        let blue = by_label(&ui, "Blue", AccessibleRole::RadioButton);
        assert_eq!(blue.accessible_checked(), Some(false));
        blue.invoke_accessible_default_action();
        assert_eq!(ui.get_swatch_clicks(), 1);
    });
}

#[test]
fn every_interactive_primitive_has_a_hit_target_of_at_least_24px() {
    on_ui(|| {
        let ui = gallery();
        let interactive = [
            AccessibleRole::Button,
            AccessibleRole::Switch,
            AccessibleRole::Checkbox,
            AccessibleRole::RadioButton,
        ];
        let mut checked = 0;
        for role in interactive {
            let all = ElementQuery::from_root(&ui)
                .match_descendants()
                .match_accessible_role(role)
                .find_all();
            for element in all {
                let size = element.size();
                assert!(
                    size.width >= 24.0 && size.height >= 24.0,
                    "{role:?} {:?} is {}x{}",
                    element.accessible_label(),
                    size.width,
                    size.height
                );
                checked += 1;
            }
        }
        // 6 icon buttons, 3 tags + 1 x, 5 toggles, 5 checkboxes, 4 radios, 10 swatches.
        assert!(checked >= 38, "only {checked} interactive elements found");
    });
}

// ---------- Menu ----------

fn menu() -> MenuGallery {
    let ui = MenuGallery::new().expect("menu gallery");
    ui.show().expect("show");
    ui
}

#[test]
fn menu_exposes_a_list_of_list_items() {
    on_ui(|| {
        let ui = menu();
        let list = by_label(&ui, "Item actions", AccessibleRole::List);
        assert_eq!(list.accessible_label().as_deref(), Some("Item actions"));
        let copy = by_label(&ui, "Copy", AccessibleRole::ListItem);
        assert_eq!(copy.accessible_item_index(), Some(0));
        assert_eq!(copy.accessible_item_count(), Some(7));
        assert_eq!(copy.accessible_description().as_deref(), Some("Ctrl+C"));
        assert_eq!(
            by_label(&ui, "Delete", AccessibleRole::ListItem).accessible_item_index(),
            Some(6)
        );
        assert_eq!(
            by_label(&ui, "Pin", AccessibleRole::ListItem).accessible_enabled(),
            Some(false)
        );
        let size = copy.size();
        assert!(size.height >= 24.0, "menu row is {}px high", size.height);
    });
}

#[test]
fn menu_arrow_keys_skip_separators_and_disabled_rows_and_wrap() {
    on_ui(|| {
        let ui = menu();
        assert_eq!(ui.get_current(), 1, "starts on Edit");
        key(&ui, Key::DownArrow);
        assert_eq!(
            ui.get_current(),
            4,
            "skips the separator and the disabled Pin"
        );
        key(&ui, Key::DownArrow);
        assert_eq!(ui.get_current(), 6, "Delete");
        key(&ui, Key::DownArrow);
        assert_eq!(ui.get_current(), 0, "wraps to the first row");
        key(&ui, Key::UpArrow);
        assert_eq!(ui.get_current(), 6, "wraps to the last row");
        key(&ui, Key::UpArrow);
        assert_eq!(ui.get_current(), 4);
        key(&ui, Key::Home);
        assert_eq!(ui.get_current(), 0);
        key(&ui, Key::End);
        assert_eq!(ui.get_current(), 6);
    });
}

#[test]
fn menu_enter_activates_the_current_row_and_escape_dismisses() {
    on_ui(|| {
        let ui = menu();
        key(&ui, Key::DownArrow);
        key(&ui, Key::Return);
        assert_eq!(ui.get_activated_index(), 4);
        key(&ui, Key::Escape);
        assert_eq!(ui.get_dismissals(), 1);
        assert_eq!(ui.get_activated_index(), 4, "Escape activates nothing");
    });
}

#[test]
fn menu_disabled_row_ignores_the_pointer_and_the_default_action() {
    on_ui(|| {
        let ui = menu();
        let pin = by_label(&ui, "Pin", AccessibleRole::ListItem);
        pin.invoke_accessible_default_action();
        pin.mock_single_click(PointerEventButton::Left);
        assert_eq!(ui.get_activated_index(), -1);
        assert_eq!(
            ui.get_current(),
            1,
            "hovering a disabled row does not move the highlight"
        );
    });
}

#[test]
fn menu_pointer_highlights_and_click_activates() {
    on_ui(|| {
        let ui = menu();
        let target = by_label(&ui, "Move to list", AccessibleRole::ListItem);
        let at = target.absolute_position();
        let size = target.size();
        ui.window().dispatch_event(WindowEvent::PointerMoved {
            position: LogicalPosition::new(at.x + size.width / 2.0, at.y + size.height / 2.0),
        });
        assert_eq!(ui.get_current(), 4, "hover moves the highlight");
        target.mock_single_click(PointerEventButton::Left);
        assert_eq!(ui.get_activated_index(), 4);
        by_label(&ui, "Delete", AccessibleRole::ListItem).invoke_accessible_default_action();
        assert_eq!(ui.get_activated_index(), 6);
    });
}

// ---------- Toast and Skeleton ----------

fn surfaces() -> SurfacesGallery {
    let ui = SurfacesGallery::new().expect("surfaces gallery");
    ui.show().expect("show");
    ui
}

#[test]
fn toast_is_a_polite_live_region_and_undo_works() {
    on_ui(|| {
        let ui = surfaces();
        let toast = toast(&ui, "Could not paste");
        assert_eq!(
            toast.accessible_live_region(),
            Some(AccessibleLiveness::Polite)
        );
        by_label(&ui, "Undo", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(ui.get_undos(), 1);
    });
}

#[test]
fn toast_phases_fade_in_hold_and_fade_out() {
    on_ui(|| {
        let ui = surfaces();
        let opacity = |ui: &SurfacesGallery| toast(ui, "Copied").computed_opacity();
        ui.set_phase(ToastPhase::Hold);
        mock_elapsed_time(Duration::from_millis(500));
        assert!((opacity(&ui) - 1.0).abs() < 0.001, "hold is fully visible");

        ui.set_phase(ToastPhase::Exit);
        mock_elapsed_time(Duration::from_millis(45));
        let mid = opacity(&ui);
        assert!(mid > 0.05 && mid < 0.95, "mid fade-out is {mid}");
        mock_elapsed_time(Duration::from_millis(200));
        assert!(opacity(&ui) < 0.001, "exit ends invisible");

        ui.set_phase(ToastPhase::Enter);
        mock_elapsed_time(Duration::from_millis(500));
        assert!((opacity(&ui) - 1.0).abs() < 0.001, "enter ends visible");
    });
}

#[test]
fn skeletons_announce_loading_with_the_given_label() {
    on_ui(|| {
        let ui = surfaces();
        assert_eq!(
            by_label(&ui, "Loading tile", AccessibleRole::ProgressIndicator)
                .accessible_label()
                .as_deref(),
            Some("Loading tile")
        );
        let row = ElementHandle::find_by_accessible_label(&ui, "Loading row")
            .filter(|e| e.accessible_role() == Some(AccessibleRole::ProgressIndicator))
            .count();
        assert_eq!(row, 2);
    });
}

#[test]
fn tooltips_are_decorative_for_assistive_technology() {
    on_ui(|| {
        // The control they describe already carries the same text as its label.
        let ui = surfaces();
        let tooltips = ElementHandle::find_by_accessible_label(&ui, "Paste into Notepad")
            .filter(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .count();
        assert_eq!(tooltips, 0);
    });
}
