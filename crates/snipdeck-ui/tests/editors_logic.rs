//! Behaviour of the editors, the settings page and the form controls through Slint's testing backend: keyboard
//! flows, validation, discard and delete confirmations, hotkey recording and conflicts, accessibility roles and
//! labels, and hit targets. Time is mocked, never the real clock.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use std::{cell::RefCell, rc::Rc};

use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementQuery, ElementRoot};
use slint::{ComponentHandle, ModelRc, VecModel, platform::Key};
use snipdeck_ui::{
    BoardEditor, FormControlsGallery, ItemEditor, ListEditor, SettingsPage, TagManager,
};

#[path = "support/mod.rs"]
mod support;
use support::{chord, key, on_ui_thread, press, type_text};

#[path = "support/editor_fixture.rs"]
mod fixture;

const MIN_HIT: f32 = 24.0;

type Log = Rc<RefCell<Vec<String>>>;

fn take(log: &Log) -> Vec<String> {
    std::mem::take(&mut *log.borrow_mut())
}

fn settle() {
    slint::platform::update_timers_and_animations();
}

fn all(root: &impl ElementRoot, label: &str, role: AccessibleRole) -> Vec<ElementHandle> {
    ElementHandle::find_by_accessible_label(root, label)
        .filter(|e| e.accessible_role() == Some(role))
        .collect()
}

fn find(root: &impl ElementRoot, label: &str, role: AccessibleRole) -> ElementHandle {
    all(root, label, role)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no {role:?} labelled {label:?}"))
}

fn missing(root: &impl ElementRoot, label: &str, role: AccessibleRole) -> bool {
    all(root, label, role).is_empty()
}

/// Every element with `role`, de-duplicated by geometry and name.
fn by_role(root: &impl ElementRoot, role: AccessibleRole) -> Vec<ElementHandle> {
    let mut seen = Vec::new();
    ElementQuery::from_root(root)
        .match_descendants()
        .match_accessible_role(role)
        .find_all()
        .into_iter()
        .filter(|e| {
            let (p, s) = (e.absolute_position(), e.size());
            let k = (
                p.x.to_bits(),
                p.y.to_bits(),
                s.width.to_bits(),
                s.height.to_bits(),
                e.accessible_label().map(|l| l.to_string()),
            );
            let fresh = !seen.contains(&k);
            seen.push(k);
            fresh
        })
        .collect()
}

fn click(e: &ElementHandle) {
    e.mock_single_click(slint::platform::PointerEventButton::Left);
    settle();
}

fn press_key(ui: &impl ComponentHandle, k: Key) {
    press(ui.window(), &key(k));
    settle();
}

fn log_hook(log: &Log, line: &'static str) -> impl Fn() + 'static {
    let log = log.clone();
    move || log.borrow_mut().push(line.to_string())
}

/// Same shape as `panel_logic`: Slint's testing platform is process-wide and bound to one thread.
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

fn item_editor() -> (ItemEditor, Log) {
    let ui = ItemEditor::new().expect("item editor");
    fixture::text_item(&ui);
    ui.window().set_size(slint::LogicalSize::new(640.0, 2000.0));
    ui.show().expect("show");
    settle();
    let log: Log = Rc::default();
    ui.on_save_requested(log_hook(&log, "save"));
    ui.on_cancel_requested(log_hook(&log, "cancel"));
    ui.on_delete_requested(log_hook(&log, "delete"));
    ui.on_edited(log_hook(&log, "edited"));
    {
        let log = log.clone();
        ui.on_tag_toggled(move |i| log.borrow_mut().push(format!("tag:{i}")));
    }
    {
        let log = log.clone();
        ui.on_tag_added(move |n| log.borrow_mut().push(format!("tag-added:{n}")));
    }
    (ui, log)
}

fn list_editor() -> (ListEditor, Log) {
    let ui = ListEditor::new().expect("list editor");
    fixture::list_editor_existing(&ui);
    ui.show().expect("show");
    settle();
    let log: Log = Rc::default();
    ui.on_save_requested(log_hook(&log, "save"));
    ui.on_cancel_requested(log_hook(&log, "cancel"));
    ui.on_delete_confirmed(log_hook(&log, "delete"));
    (ui, log)
}

fn settings() -> (SettingsPage, Log) {
    let ui = SettingsPage::new().expect("settings");
    fixture::settings(&ui);
    // Tall enough that the whole page is on screen: the testing backend only lists what is visible.
    ui.window().set_size(slint::LogicalSize::new(640.0, 2000.0));
    ui.show().expect("show");
    settle();
    let log: Log = Rc::default();
    macro_rules! hook {
        ($on:ident, |$($arg:ident),*| $line:expr) => {{
            let log = log.clone();
            ui.$on(move |$($arg),*| log.borrow_mut().push($line));
        }};
    }
    hook!(on_hotkey_recorded, |i, c| format!("hk:{i}:{c}"));
    hook!(on_hotkey_cleared, |i| format!("hk-clear:{i}"));
    hook!(on_theme_chosen, |i| format!("theme:{i}"));
    hook!(on_language_chosen, |i| format!("lang:{i}"));
    hook!(on_autostart_toggled, |on| format!("autostart:{on}"));
    hook!(on_close_to_tray_toggled, |on| format!("tray:{on}"));
    hook!(on_site_icons_toggled, |on| format!("icons:{on}"));
    hook!(on_panel_width_chosen, |w| format!("width:{w}"));
    hook!(on_no_paste_app_added, |n| format!("app+:{n}"));
    hook!(on_no_paste_app_removed, |i| format!("app-:{i}"));
    ui.on_close_requested(log_hook(&log, "close"));
    (ui, log)
}

fn gallery() -> FormControlsGallery {
    let ui = FormControlsGallery::new().expect("gallery");
    ui.show().expect("show");
    settle();
    ui
}

ui_tests! {
    // ---- ItemEditor ----------------------------------------------------------------------------

    fn the_item_editor_starts_in_the_title_and_typing_marks_it_changed() {
        let (ui, log) = item_editor();
        assert!(!ui.get_dirty());
        // The host sets the values, then asks for the focus: the cursor lands after the text.
        ui.invoke_focus_title();
        settle();
        type_text(ui.window(), "!");
        settle();
        assert_eq!(ui.get_item_title(), "Thanks, received!");
        assert!(ui.get_dirty());
        assert_eq!(take(&log), ["edited"]);
        assert_eq!(find(&ui, "Title", AccessibleRole::TextInput).accessible_value().as_deref(), Some("Thanks, received!"));
    }

    fn ctrl_enter_saves_a_valid_item_and_an_empty_one_shows_what_is_missing() {
        let (ui, log) = item_editor();
        chord(ui.window(), &[Key::Control], "\n");
        assert_eq!(take(&log), ["save"]);

        ui.set_content("".into());
        settle();
        chord(ui.window(), &[Key::Control], "\n");
        assert!(take(&log).is_empty(), "no save without content");
        assert_eq!(
            find(&ui, "Content", AccessibleRole::TextInput).accessible_description().as_deref(),
            Some("Add some text, or cancel.")
        );

        ui.set_content("Hello".into());
        settle();
        assert_eq!(
            find(&ui, "Content", AccessibleRole::TextInput).accessible_description().as_deref(),
            Some(""),
            "the message goes away once there is content"
        );
        chord(ui.window(), &[Key::Control], "\n");
        assert_eq!(take(&log), ["save"]);
    }

    fn an_error_from_the_host_blocks_saving_and_is_spoken() {
        let (ui, log) = item_editor();
        ui.set_kind_index(2);
        ui.set_content_error("A link needs a scheme such as https://".into());
        settle();
        chord(ui.window(), &[Key::Control], "\n");
        assert_eq!(take(&log), Vec::<String>::new());
        assert_eq!(
            find(&ui, "Link", AccessibleRole::TextInput).accessible_description().as_deref(),
            Some("A link needs a scheme such as https://")
        );
        ui.set_content_error("".into());
        settle();
        chord(ui.window(), &[Key::Control], "\n");
        assert_eq!(take(&log), ["save"]);
    }

    fn escape_leaves_a_clean_editor_and_asks_before_discarding_changes() {
        let (ui, log) = item_editor();
        press_key(&ui, Key::Escape);
        assert_eq!(take(&log), ["cancel"]);

        type_text(ui.window(), "x");
        take(&log);
        press_key(&ui, Key::Escape);
        assert!(ui.get_discard_open(), "unsaved changes: ask first");
        assert_eq!(take(&log), Vec::<String>::new());
        // The dialog is labelled and its default action is the safe one.
        let dialog = find(&ui, "Discard your changes?", AccessibleRole::Groupbox);
        assert_eq!(dialog.accessible_description().as_deref(), Some("What you changed in this item has not been saved."));

        press_key(&ui, Key::Escape);
        assert!(!ui.get_discard_open(), "Esc closes the dialog only");
        assert_eq!(take(&log), Vec::<String>::new());

        press_key(&ui, Key::Escape);
        assert!(ui.get_discard_open());
        find(&ui, "Discard", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["cancel"]);
    }

    fn keep_editing_returns_to_the_form_and_keeps_the_changes() {
        let (ui, log) = item_editor();
        type_text(ui.window(), "x");
        press_key(&ui, Key::Escape);
        assert!(ui.get_discard_open());
        find(&ui, "Keep editing", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert!(!ui.get_discard_open());
        assert!(ui.get_dirty());
        assert!(!take(&log).contains(&"cancel".to_string()));
    }

    fn the_kind_group_is_one_tab_stop_with_arrow_keys() {
        let (ui, _log) = item_editor();
        assert_eq!(ui.get_kind_index(), 0);
        let text = find(&ui, "Text", AccessibleRole::RadioButton);
        assert_eq!(text.accessible_checked(), Some(true));
        // Shift+Tab from the title reaches the group.
        press_key(&ui, Key::Backtab);
        press_key(&ui, Key::RightArrow);
        assert_eq!(ui.get_kind_index(), 1);
        press_key(&ui, Key::End);
        assert_eq!(ui.get_kind_index(), 3);
        press_key(&ui, Key::RightArrow);
        assert_eq!(ui.get_kind_index(), 3, "the end of the group stops");
        press_key(&ui, Key::Home);
        assert_eq!(ui.get_kind_index(), 0);
        // And with the pointer or assistive technology.
        find(&ui, "Link", AccessibleRole::RadioButton).invoke_accessible_default_action();
        assert_eq!(ui.get_kind_index(), 2);
        assert!(ui.get_dirty());
    }

    fn template_placeholders_are_listed_and_insert_at_the_end() {
        let (ui, _log) = item_editor();
        assert!(missing(&ui, "Insert {date}", AccessibleRole::Button), "only templates show the help");
        ui.set_kind_index(3);
        ui.set_content("Due on ".into());
        settle();
        find(&ui, "Insert {date}", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert_eq!(ui.get_content(), "Due on {date}");
        assert_eq!(
            find(&ui, "Template text", AccessibleRole::TextInput).accessible_value().as_deref(),
            Some("Due on {date}"),
            "the field shows the inserted text"
        );
        assert!(ui.get_dirty());

        ui.set_prompted_available(false);
        settle();
        assert_eq!(find(&ui, "Insert {field:Name}", AccessibleRole::Button).accessible_enabled(), Some(false));
        assert_eq!(find(&ui, "Insert {date}", AccessibleRole::Button).accessible_enabled(), Some(true));
    }

    fn the_list_select_opens_in_place_chooses_and_closes_with_escape() {
        let (ui, log) = item_editor();
        let combo = find(&ui, "List", AccessibleRole::Combobox);
        assert_eq!(combo.accessible_value().as_deref(), Some("Email replies"));
        assert_eq!(combo.accessible_expanded(), Some(false));
        combo.invoke_accessible_default_action();
        settle();
        assert_eq!(combo.accessible_expanded(), Some(true));
        for name in ["Email replies", "Addresses", "Code"] {
            let item = find(&ui, name, AccessibleRole::ListItem);
            assert!(item.size().height >= MIN_HIT, "{name}");
        }
        assert_eq!(find(&ui, "Email replies", AccessibleRole::ListItem).accessible_checked(), Some(true));

        // Esc closes the list and leaves the editor open.
        press_key(&ui, Key::Escape);
        assert_eq!(combo.accessible_expanded(), Some(false));
        assert!(!ui.get_discard_open());
        assert_eq!(take(&log), Vec::<String>::new());

        combo.invoke_accessible_default_action();
        settle();
        find(&ui, "Code", AccessibleRole::ListItem).invoke_accessible_default_action();
        settle();
        assert_eq!(ui.get_list_index(), 2);
        assert_eq!(combo.accessible_value().as_deref(), Some("Code"));
        assert_eq!(combo.accessible_expanded(), Some(false));
        assert!(ui.get_dirty());
    }

    fn tags_remove_from_the_chips_choose_from_the_list_and_add_by_name() {
        let (ui, log) = item_editor();
        find(&ui, "Remove tag billing", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["tag:0", "edited"]);

        assert!(missing(&ui, "Tags", AccessibleRole::List));
        find(&ui, "Choose tags", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        let _ = find(&ui, "Tags", AccessibleRole::List);
        let billing = find(&ui, "billing", AccessibleRole::ListItem);
        assert_eq!(billing.accessible_checkable(), Some(true));
        assert_eq!(billing.accessible_checked(), Some(true));
        find(&ui, "meeting", AccessibleRole::ListItem).invoke_accessible_default_action();
        assert_eq!(take(&log), ["tag:1", "edited"]);
        press_key(&ui, Key::Escape);
        assert!(!ui.get_tags_open(), "Esc closes the tag list first");
        assert!(!ui.get_discard_open());

        let field = find(&ui, "New tag", AccessibleRole::TextInput);
        click(&field);
        type_text(ui.window(), "urgent");
        press(ui.window(), "\n");
        settle();
        assert_eq!(take(&log), ["tag-added:urgent", "edited"]);
        assert_eq!(field.accessible_value().as_deref(), Some(""), "the field empties for the next tag");
    }

    fn sensitive_explains_itself_and_is_off_limits_to_images_and_files() {
        let (ui, _log) = item_editor();
        let toggle = find(&ui, "Sensitive item", AccessibleRole::Switch);
        assert_eq!(toggle.accessible_checked(), Some(false));
        let text = toggle.accessible_description().expect("description").to_string();
        assert!(text.contains("30 seconds") && text.contains("clipboard"), "{text}");
        toggle.invoke_accessible_default_action();
        assert!(ui.get_sensitive());
        assert!(ui.get_dirty());

        ui.set_clear_seconds(90);
        settle();
        assert!(toggle.accessible_description().unwrap().contains("90 seconds"));

        ui.set_kind_index(4);
        settle();
        assert_eq!(toggle.accessible_enabled(), Some(false));
        assert!(toggle.accessible_description().unwrap().starts_with("Only text, code, link"));
    }

    fn large_text_is_read_only_with_a_notice() {
        let (ui, _log) = item_editor();
        assert_eq!(find(&ui, "Content", AccessibleRole::TextInput).accessible_read_only(), Some(false));
        ui.set_content_large(true);
        ui.set_content_size("2.4 MB".into());
        settle();
        assert_eq!(find(&ui, "Content", AccessibleRole::TextInput).accessible_read_only(), Some(true));
        let _ = find(&ui, "This text is 2.4 MB. Anything over 16 KB opens read-only here.", AccessibleRole::Text);
    }

    fn delete_is_offered_for_an_existing_item_only() {
        let (ui, log) = item_editor();
        find(&ui, "Delete", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["delete"]);
        ui.set_is_new(true);
        settle();
        assert!(missing(&ui, "Delete", AccessibleRole::Button));
    }

    fn the_item_editor_has_roles_labels_and_big_enough_targets_in_every_kind() {
        let (ui, _log) = item_editor();
        for kind in 0..=5 {
            ui.set_kind_index(kind);
            ui.set_attachment_name("report.pdf".into());
            settle();
            let _ = find(&ui, "Title", AccessibleRole::TextInput);
            let _ = find(&ui, "Save", AccessibleRole::Button);
            let _ = find(&ui, "Cancel", AccessibleRole::Button);
            let _ = find(&ui, "List", AccessibleRole::Combobox);
            if kind == 0 {
                let _ = find(&ui, "Choose tags", AccessibleRole::Button);
            }
            // The tag field lives in the open tag list, and the button then closes it.
            ui.set_tags_open(true);
            settle();
            let _ = find(&ui, "Close the tag list", AccessibleRole::Button);
            let _ = find(&ui, "New tag", AccessibleRole::TextInput);
            let _ = find(&ui, "Sensitive item", AccessibleRole::Switch);
            let mut checked = 0;
            for role in [
                AccessibleRole::Button,
                AccessibleRole::RadioButton,
                AccessibleRole::Switch,
                AccessibleRole::Combobox,
                AccessibleRole::TextInput,
            ] {
                for e in by_role(&ui, role) {
                    let s = e.size();
                    assert!(
                        e.accessible_label().is_some_and(|l| !l.is_empty()),
                        "kind {kind}: an unlabelled {role:?}"
                    );
                    assert!(
                        s.width >= MIN_HIT && s.height >= MIN_HIT,
                        "kind {kind}: {role:?} {:?} is {}x{}",
                        e.accessible_label(),
                        s.width,
                        s.height
                    );
                    checked += 1;
                }
            }
            assert!(checked >= 9, "kind {kind}: checked {checked} targets");
        }
    }

    // ---- List and board editors --------------------------------------------------------------

    fn a_list_needs_a_name_and_a_taken_name_comes_back_from_the_host() {
        let (ui, log) = list_editor();
        ui.set_name("".into());
        settle();
        chord(ui.window(), &[Key::Control], "\n");
        assert_eq!(take(&log), Vec::<String>::new());
        assert_eq!(
            find(&ui, "List name", AccessibleRole::TextInput).accessible_description().as_deref(),
            Some("Give the list a name.")
        );
        type_text(ui.window(), "Notes");
        settle();
        assert_eq!(ui.get_name(), "Notes");
        chord(ui.window(), &[Key::Control], "\n");
        assert_eq!(take(&log), ["save"]);

        ui.set_name_error("A list named Notes already exists.".into());
        settle();
        chord(ui.window(), &[Key::Control], "\n");
        assert!(take(&log).is_empty(), "a refused name blocks the next save");
        assert_eq!(
            find(&ui, "List name", AccessibleRole::TextInput).accessible_description().as_deref(),
            Some("A list named Notes already exists.")
        );
    }

    fn the_colour_group_chooses_with_the_pointer_and_arrow_keys() {
        let (ui, _log) = list_editor();
        assert_eq!(ui.get_accent(), 1);
        assert_eq!(find(&ui, "Teal", AccessibleRole::RadioButton).accessible_checked(), Some(true));
        assert_eq!(find(&ui, "Blue", AccessibleRole::RadioButton).accessible_checked(), Some(false));
        for e in by_role(&ui, AccessibleRole::RadioButton) {
            let s = e.size();
            assert!(s.width >= MIN_HIT && s.height >= MIN_HIT, "{:?}", e.accessible_label());
        }
        find(&ui, "Violet", AccessibleRole::RadioButton).invoke_accessible_default_action();
        assert_eq!(ui.get_accent(), 7);
        find(&ui, "No colour", AccessibleRole::RadioButton).invoke_accessible_default_action();
        assert_eq!(ui.get_accent(), -1);
        assert!(ui.get_dirty());

        // Tab from the name reaches the group; arrows walk it, No colour first.
        press_key(&ui, Key::Tab);
        press_key(&ui, Key::RightArrow);
        assert_eq!(ui.get_accent(), 0);
        press_key(&ui, Key::End);
        assert_eq!(ui.get_accent(), 8);
        press_key(&ui, Key::Home);
        assert_eq!(ui.get_accent(), -1);
        press_key(&ui, Key::LeftArrow);
        assert_eq!(ui.get_accent(), -1, "No colour is the first stop");
    }

    fn sort_is_a_choice_of_three() {
        let (ui, _log) = list_editor();
        find(&ui, "Most used", AccessibleRole::RadioButton).invoke_accessible_default_action();
        assert_eq!(ui.get_sort_index(), 1);
        find(&ui, "Recently used", AccessibleRole::RadioButton).invoke_accessible_default_action();
        assert_eq!(ui.get_sort_index(), 2);
        ui.set_show_sort(false);
        ui.set_show_colour(false);
        settle();
        assert!(missing(&ui, "Most used", AccessibleRole::RadioButton));
        assert!(missing(&ui, "Teal", AccessibleRole::RadioButton));
    }

    fn deleting_a_list_asks_first_and_says_how_many_items_go() {
        let (ui, log) = list_editor();
        find(&ui, "Delete list", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert!(ui.get_delete_open());
        let dialog = find(&ui, "Delete this list?", AccessibleRole::Groupbox);
        assert_eq!(
            dialog.accessible_description().as_deref(),
            Some("24 items will move to Trash with it. You can restore them for 30 days.")
        );
        ui.set_item_count(1);
        settle();
        assert!(find(&ui, "Delete this list?", AccessibleRole::Groupbox)
            .accessible_description().unwrap().starts_with("1 item will move"));
        ui.set_item_count(24);

        // Enter does nothing in a destructive dialog; Esc closes it and the editor stays.
        press_key(&ui, Key::Return);
        assert!(ui.get_delete_open());
        press_key(&ui, Key::Escape);
        assert!(!ui.get_delete_open());
        assert_eq!(take(&log), Vec::<String>::new());

        find(&ui, "Delete list", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        // The dialog's own confirm button has the same name; it is the last one in tree order.
        let buttons = all(&ui, "Delete list", AccessibleRole::Button);
        buttons.last().expect("confirm").invoke_accessible_default_action();
        assert_eq!(take(&log), ["delete"]);
        assert!(!ui.get_delete_open());
    }

    fn a_board_that_cannot_be_deleted_says_why() {
        let ui = BoardEditor::new().expect("board editor");
        ui.set_name("Personal".into());
        ui.set_can_delete(false);
        ui.set_delete_hint("The last board can't be deleted.".into());
        ui.show().expect("show");
        settle();
        assert!(missing(&ui, "Delete board", AccessibleRole::Button));
        let _ = find(&ui, "The last board can't be deleted.", AccessibleRole::Text);
        ui.set_can_delete(true);
        settle();
        let _ = find(&ui, "Delete board", AccessibleRole::Button);
        let _ = find(&ui, "Board name", AccessibleRole::TextInput);
    }

    // ---- Tag management ----------------------------------------------------------------------

    fn tags_rename_with_enter_and_cancel_with_escape() {
        let ui = TagManager::new().expect("tag manager");
        fixture::tag_manager(&ui);
        ui.show().expect("show");
        settle();
        let log: Log = Rc::default();
        {
            let log = log.clone();
            ui.on_rename_requested(move |i, n| log.borrow_mut().push(format!("rename:{i}:{n}")));
        }
        ui.on_close_requested(log_hook(&log, "close"));

        let rows = by_role(&ui, AccessibleRole::ListItem);
        assert!(rows.len() >= 3);
        assert_eq!(rows[0].accessible_label().as_deref(), Some("billing"));
        assert_eq!(rows[0].accessible_description().as_deref(), Some("9 items"));

        find(&ui, "Rename tag meeting", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert_eq!(ui.get_editing(), 1);
        assert_eq!(ui.get_rename_text(), "meeting");
        // The field is focused with the whole name selected, as in a file manager: typing replaces it.
        type_text(ui.window(), "meetings");
        press(ui.window(), "\n");
        settle();
        assert_eq!(take(&log), ["rename:1:meetings"]);

        // The host refuses: the error shows and the field stays open.
        ui.set_rename_error("A tag named meetings already exists.".into());
        settle();
        assert_eq!(ui.get_editing(), 1);
        assert_eq!(
            find(&ui, "New name for meeting", AccessibleRole::TextInput).accessible_description().as_deref(),
            Some("A tag named meetings already exists.")
        );
        press_key(&ui, Key::Escape);
        assert_eq!(ui.get_editing(), -1, "Esc cancels the rename first");
        assert_eq!(take(&log), Vec::<String>::new());
        press_key(&ui, Key::Escape);
        assert_eq!(take(&log), ["close"]);
    }

    fn deleting_a_tag_asks_first_and_says_how_many_items_lose_it() {
        let ui = TagManager::new().expect("tag manager");
        fixture::tag_manager(&ui);
        ui.show().expect("show");
        settle();
        let log: Log = Rc::default();
        {
            let log = log.clone();
            ui.on_delete_confirmed(move |i| log.borrow_mut().push(format!("delete:{i}")));
        }
        find(&ui, "Delete tag billing", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert_eq!(ui.get_delete_index(), 0);
        assert_eq!(
            find(&ui, "Delete this tag?", AccessibleRole::Groupbox).accessible_description().as_deref(),
            Some("It is removed from 9 items. The items stay.")
        );
        press_key(&ui, Key::Escape);
        assert_eq!(ui.get_delete_index(), -1);
        assert_eq!(take(&log), Vec::<String>::new());

        find(&ui, "Delete tag sql", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        let buttons = all(&ui, "Delete tag", AccessibleRole::Button);
        buttons.last().expect("confirm").invoke_accessible_default_action();
        assert_eq!(take(&log), ["delete:2"]);
    }

    fn tag_rows_have_big_enough_targets() {
        let ui = TagManager::new().expect("tag manager");
        fixture::tag_manager(&ui);
        ui.show().expect("show");
        settle();
        let mut checked = 0;
        for role in [AccessibleRole::Button, AccessibleRole::ListItem] {
            for e in by_role(&ui, role) {
                let s = e.size();
                assert!(s.width >= MIN_HIT && s.height >= MIN_HIT, "{:?} {}x{}", e.accessible_label(), s.width, s.height);
                checked += 1;
            }
        }
        assert!(checked >= 8, "{checked}");
    }

    // ---- Settings -------------------------------------------------------------------------------

    fn shortcut_rows_record_clear_and_show_a_conflict() {
        let (ui, log) = settings();
        let names = [
            "Show or hide the panel",
            "Open the picker",
            "Capture a region",
            "Paste the next item",
        ];
        for name in names {
            let _ = find(&ui, name, AccessibleRole::Button);
        }
        let first = find(&ui, names[0], AccessibleRole::Button);
        assert_eq!(first.accessible_value().as_deref(), Some("Ctrl+Alt+V"));
        first.invoke_accessible_default_action();
        chord(ui.window(), &[Key::Control, Key::Alt], "k");
        settle();
        assert_eq!(take(&log), ["hk:0:Ctrl+Alt+K"]);

        // The host cannot register it: the message shows in the row and is spoken.
        let mut hotkeys = fixture::hotkeys();
        hotkeys[0].conflict = "Another program already uses Ctrl+Alt+K.".into();
        ui.set_hotkeys(ModelRc::new(VecModel::from(hotkeys)));
        settle();
        assert_eq!(
            find(&ui, names[0], AccessibleRole::Button).accessible_description().as_deref(),
            Some("Another program already uses Ctrl+Alt+K.")
        );

        find(&ui, "Clear the shortcut for Open the picker", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["hk-clear:1"]);
        // A shortcut that is not set says so.
        assert_eq!(find(&ui, names[3], AccessibleRole::Button).accessible_value().as_deref(), Some(""));
    }

    fn escape_ends_a_recording_first_and_then_closes_the_page() {
        let (ui, log) = settings();
        find(&ui, "Open the picker", AccessibleRole::Button).invoke_accessible_default_action();
        press_key(&ui, Key::Escape);
        assert!(take(&log).is_empty(), "Esc only cancelled the recording");
        press_key(&ui, Key::Escape);
        assert_eq!(take(&log), ["close"]);
    }

    fn theme_language_and_switches_report_their_new_value() {
        let (ui, log) = settings();
        assert_eq!(find(&ui, "Windows", AccessibleRole::RadioButton).accessible_checked(), Some(true));
        find(&ui, "Dark", AccessibleRole::RadioButton).invoke_accessible_default_action();
        assert_eq!(take(&log), ["theme:2"]);
        // The page shows what the host says, not what was clicked.
        assert_eq!(find(&ui, "Dark", AccessibleRole::RadioButton).accessible_checked(), Some(false));
        ui.set_theme_index(2);
        settle();
        assert_eq!(find(&ui, "Dark", AccessibleRole::RadioButton).accessible_checked(), Some(true));

        let combo = find(&ui, "Language", AccessibleRole::Combobox);
        assert_eq!(combo.accessible_value().as_deref(), Some("English"));
        combo.invoke_accessible_default_action();
        settle();
        find(&ui, "Deutsch", AccessibleRole::ListItem).invoke_accessible_default_action();
        assert_eq!(take(&log), ["lang:2"]);

        let autostart = find(&ui, "Start with Windows", AccessibleRole::Switch);
        assert_eq!(autostart.accessible_checked(), Some(true));
        autostart.invoke_accessible_default_action();
        assert_eq!(take(&log), ["autostart:false"]);
        assert!(!ui.get_autostart());
        find(&ui, "Show website icons on links", AccessibleRole::Switch).invoke_accessible_default_action();
        assert_eq!(take(&log), ["icons:true"]);
        let tray = find(&ui, "Keep running in the tray when the window closes", AccessibleRole::Switch);
        tray.invoke_accessible_default_action();
        assert_eq!(take(&log), ["tray:false"]);
        // The host can put a value back (autostart could not be registered).
        ui.set_autostart(true);
        settle();
        assert_eq!(autostart.accessible_checked(), Some(true));
    }

    fn the_panel_width_steps_by_ten_inside_its_limits() {
        let (ui, log) = settings();
        let spin = find(&ui, "Panel width", AccessibleRole::Spinbox);
        assert_eq!(spin.accessible_value().as_deref(), Some("320"));
        assert_eq!(spin.accessible_value_minimum(), Some(260.0));
        assert_eq!(spin.accessible_value_maximum(), Some(480.0));
        assert_eq!(spin.accessible_value_step(), Some(10.0));
        spin.invoke_accessible_increment_action();
        spin.invoke_accessible_decrement_action();
        assert_eq!(take(&log), ["width:330", "width:310"]);

        find(&ui, "Make the panel wider", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["width:330"]);

        ui.set_panel_width(480);
        settle();
        spin.invoke_accessible_increment_action();
        assert!(take(&log).is_empty(), "no step past the maximum");
        assert_eq!(find(&ui, "Make the panel wider", AccessibleRole::Button).accessible_enabled(), Some(false));
        ui.set_panel_width(260);
        settle();
        assert_eq!(find(&ui, "Make the panel narrower", AccessibleRole::Button).accessible_enabled(), Some(false));
    }

    fn programs_the_panel_never_pastes_into_can_be_added_and_removed() {
        let (ui, log) = settings();
        let _ = find(&ui, "keepass.exe", AccessibleRole::ListItem);
        find(&ui, "Remove 1password.exe from the list", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["app-:1"]);

        let field = find(&ui, "Program to add", AccessibleRole::TextInput);
        click(&field);
        type_text(ui.window(), "notepad.exe");
        press(ui.window(), "\n");
        settle();
        assert_eq!(take(&log), ["app+:notepad.exe"]);
        assert_eq!(field.accessible_value().as_deref(), Some(""));

        type_text(ui.window(), "calc.exe");
        find(&ui, "Add", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), ["app+:calc.exe"]);
        // An empty field adds nothing.
        find(&ui, "Add", AccessibleRole::Button).invoke_accessible_default_action();
        assert_eq!(take(&log), Vec::<String>::new());
    }

    fn the_settings_page_has_roles_labels_and_big_enough_targets() {
        let (ui, _log) = settings();
        let mut checked = 0;
        for role in [
            AccessibleRole::Button,
            AccessibleRole::RadioButton,
            AccessibleRole::Switch,
            AccessibleRole::Combobox,
            AccessibleRole::Spinbox,
            AccessibleRole::TextInput,
        ] {
            for e in by_role(&ui, role) {
                let s = e.size();
                assert!(
                    e.accessible_label().is_some_and(|l| !l.is_empty()),
                    "an unlabelled {role:?}"
                );
                assert!(
                    s.width >= MIN_HIT && s.height >= MIN_HIT,
                    "{role:?} {:?} is {}x{}",
                    e.accessible_label(),
                    s.width,
                    s.height
                );
                checked += 1;
            }
        }
        assert!(checked >= 18, "checked {checked} targets");
    }

    // ---- Form controls ------------------------------------------------------------------------

    fn the_segmented_control_is_a_radio_group_with_one_tab_stop() {
        let ui = gallery();
        assert_eq!(find(&ui, "Most used", AccessibleRole::RadioButton).accessible_checked(), Some(true));
        let _ = find(&ui, "Sort items by", AccessibleRole::RadioGroup);
        ui.invoke_focus_seg();
        assert!(ui.get_seg_focused());
        press_key(&ui, Key::RightArrow);
        assert_eq!(ui.get_seg(), 2);
        press_key(&ui, Key::RightArrow);
        assert_eq!(ui.get_seg(), 2);
        press_key(&ui, Key::LeftArrow);
        press_key(&ui, Key::LeftArrow);
        assert_eq!(ui.get_seg(), 0);
        assert_eq!(ui.get_changes(), 3);
        // Tab leaves the group; it does not step through the segments.
        press_key(&ui, Key::Tab);
        assert!(!ui.get_seg_focused());
        assert!(ui.get_sel_focused());
        // A disabled group ignores the pointer.
        let locked = find(&ui, "Most used", AccessibleRole::RadioButton);
        assert_eq!(locked.accessible_enabled(), Some(true));
        let disabled = all(&ui, "Recent", AccessibleRole::RadioButton);
        assert_eq!(disabled[0].accessible_enabled(), Some(false));
        disabled[0].invoke_accessible_default_action();
        assert_eq!(ui.get_changes(), 3);
    }

    fn the_select_field_opens_with_the_keyboard_and_the_highlight_chooses() {
        let ui = gallery();
        ui.invoke_focus_sel();
        assert!(!ui.get_sel_open());
        press_key(&ui, Key::DownArrow);
        assert!(ui.get_sel_open(), "Down opens");
        press_key(&ui, Key::DownArrow);
        press_key(&ui, Key::DownArrow);
        assert_eq!(ui.get_sel(), 0, "moving the highlight chooses nothing");
        press_key(&ui, Key::Return);
        assert!(!ui.get_sel_open());
        assert_eq!(ui.get_sel(), 2);
        // Esc closes without choosing; Home and End jump.
        press(ui.window(), " ");
        assert!(ui.get_sel_open());
        press_key(&ui, Key::End);
        press_key(&ui, Key::Escape);
        assert!(!ui.get_sel_open());
        assert_eq!(ui.get_sel(), 2);
        press_key(&ui, Key::DownArrow);
        press_key(&ui, Key::Home);
        press(ui.window(), " ");
        assert_eq!(ui.get_sel(), 0);
        // Tab closes it.
        press_key(&ui, Key::DownArrow);
        assert!(ui.get_sel_open());
        press_key(&ui, Key::Tab);
        assert!(!ui.get_sel_open());
        // An open list keeps its rows reachable by the pointer.
        ui.invoke_focus_sel();
        press_key(&ui, Key::DownArrow);
        find(&ui, "Addresses", AccessibleRole::ListItem).invoke_accessible_default_action();
        assert_eq!(ui.get_sel(), 1);
    }

    fn the_number_stepper_takes_arrows_pages_and_the_ends() {
        let ui = gallery();
        ui.invoke_focus_num();
        assert!(ui.get_num_focused());
        press_key(&ui, Key::UpArrow);
        assert_eq!(ui.get_num(), 330);
        press_key(&ui, Key::DownArrow);
        press_key(&ui, Key::DownArrow);
        assert_eq!(ui.get_num(), 310);
        press_key(&ui, Key::PageUp);
        assert_eq!(ui.get_num(), 410);
        press_key(&ui, Key::PageUp);
        assert_eq!(ui.get_num(), 480, "clamped to the maximum");
        press_key(&ui, Key::Home);
        assert_eq!(ui.get_num(), 260);
        press_key(&ui, Key::End);
        assert_eq!(ui.get_num(), 480);
        let before = ui.get_changes();
        press_key(&ui, Key::UpArrow);
        assert_eq!(ui.get_changes(), before, "no change event at the limit");
        let spin = find(&ui, "Locked width", AccessibleRole::Spinbox);
        assert_eq!(spin.accessible_enabled(), Some(false));
    }

    fn form_controls_meet_the_hit_target_minimum() {
        let ui = gallery();
        ui.set_sel_open(true);
        settle();
        let mut checked = 0;
        for role in [
            AccessibleRole::RadioButton,
            AccessibleRole::Combobox,
            AccessibleRole::Spinbox,
            AccessibleRole::Button,
            AccessibleRole::Switch,
            AccessibleRole::ListItem,
        ] {
            for e in by_role(&ui, role) {
                let s = e.size();
                assert!(
                    s.width >= MIN_HIT && s.height >= MIN_HIT,
                    "{role:?} {:?} is {}x{}",
                    e.accessible_label(),
                    s.width,
                    s.height
                );
                checked += 1;
            }
        }
        assert!(checked >= 14, "checked {checked}");
    }
}
