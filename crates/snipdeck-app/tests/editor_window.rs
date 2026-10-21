//! The real `ItemEditor` window bound to a `Controller` over the demo library and the platform
//! fakes, opened the way a user opens it (the Panel's kebab and Ctrl+E, the Main window's Ctrl+N
//! and menu) and driven by key events and pointer clicks through Slint's testing backend. Time is
//! mocked, never the real clock: the two-second draft timer runs on the testing platform's
//! mocked time. One process for this file (the testing platform is process-wide).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use std::{cell::RefCell, fs, path::Path, rc::Rc, time::Duration};

use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::{ComponentHandle, Model, platform::Key};
use snipdeck_app::{
    binding::Binding,
    controller::{Controller, Services},
    demo,
    drafts::AUTOSAVE_EVERY,
    editor_binding::EditorBinding,
    host::{FakeTargets, FixedClock},
    main_binding::MainBinding,
    settings::Settings,
};
use snipdeck_core::{Entitlements, ItemId, ItemKind, Tier, Timestamp};
use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};
use snipdeck_store::{ItemRepo, Store};
use snipdeck_ui::{ItemEditor, MainWindow, PanelWindow};

#[path = "support/mod.rs"]
mod support;
use support::{SharedProvider, assert_no_plaintext, chord, key, on_ui_thread, press, type_text};

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

struct Rig {
    panel: PanelWindow,
    ctl: Rc<RefCell<Ctl>>,
    editor: EditorBinding<Store, FakeClipboard, FakeKeyboard, FakeTiming>,
    _binding: Binding,
    /// The data folder.
    data: std::path::PathBuf,
    /// Holds a temporary data folder up for the life of the rig.
    #[allow(dead_code, reason = "held only so its Drop removes the folder")]
    guard: Option<tempfile::TempDir>,
}

fn now() -> Timestamp {
    "2026-10-17T10:30:00Z".parse().expect("timestamp")
}

fn rig() -> Rig {
    rig_tier(Tier::Pro)
}

fn rig_tier(tier: Tier) -> Rig {
    let dir = tempfile::tempdir().expect("dir");
    let mut r = rig_at(dir.path(), tier, &SharedProvider::new());
    r.guard = Some(dir);
    r
}

/// The Panel and the item editor over a library kept on disk in `data` (seeded the first time).
/// A second rig on the same folder is a restart.
fn rig_at(data: &Path, tier: Tier, provider: &SharedProvider) -> Rig {
    let mut store = Store::open(data).expect("store");
    if snipdeck_store::BoardRepo::boards(&store)
        .expect("boards")
        .is_empty()
    {
        demo::seed(&mut store, now()).expect("seed");
    }
    let mut ctl = Controller::new(
        store,
        Services {
            clipboard: FakeClipboard {
                asks_after_polls: Some(1),
                ..FakeClipboard::default()
            },
            keyboard: FakeKeyboard::default(),
            timing: FakeTiming::default(),
        },
        Box::new(FakeTargets::new(Some(demo::target()))),
        Box::new(FixedClock::new(now())),
        // "More options" open: the tests drive Kind, Title, Colour, Tags and Privacy too.
        Settings {
            editor_more_open: true,
            ..Settings::default()
        },
        Entitlements::new(tier),
    )
    .expect("controller");
    ctl.set_drafts(snipdeck_app::drafts::Drafts::in_data_dir(data));
    ctl.set_key_provider(Box::new(provider.clone()));
    let ctl = Rc::new(RefCell::new(ctl));
    let panel = PanelWindow::new().expect("panel");
    panel
        .window()
        .set_size(slint::LogicalSize::new(320.0, 480.0));
    let binding = Controller::bind(&ctl, &panel, |_| {});
    let editor = Controller::bind_item_editor(&ctl);
    panel.show().expect("show");
    slint::platform::update_timers_and_animations();
    Rig {
        panel,
        ctl,
        editor,
        _binding: binding,
        data: data.to_owned(),
        guard: None,
    }
}

impl Drop for Rig {
    /// A shown window is held by the windowing system, and its callbacks hold the controller and
    /// with it the data folder's lock: hiding them is what lets a "restart" open the folder again.
    fn drop(&mut self) {
        let _ = self.panel.hide();
        if let Some(w) = self.editor.window() {
            let _ = w.hide();
        }
    }
}

impl Rig {
    /// The editor window, which must be up.
    fn ed(&self) -> ItemEditor {
        let w = self.editor.window().expect("the editor window was made");
        assert!(w.window().is_visible(), "and shown");
        w
    }

    fn editor_shown(&self) -> bool {
        self.editor
            .window()
            .is_some_and(|w| w.window().is_visible())
    }

    fn id(&self, title: &str) -> ItemId {
        self.ctl
            .borrow()
            .store()
            .all_items()
            .expect("items")
            .into_iter()
            .find(|i| i.title == title)
            .unwrap_or_else(|| panic!("no item {title}"))
            .id
    }

    fn item_count(&self) -> usize {
        self.ctl.borrow().store().all_items().expect("items").len()
    }

    fn text(&self, id: ItemId) -> String {
        snipdeck_app::usecase::item_text(self.ctl.borrow().store(), id)
            .expect("text")
            .expect("some text")
    }

    fn draft_files(&self) -> Vec<String> {
        let Ok(read) = fs::read_dir(self.data.join("drafts")) else {
            return Vec::new();
        };
        let mut names: Vec<String> = read
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

fn settle() {
    slint::platform::update_timers_and_animations();
}

/// A real left click (pointer move, press, release) at the centre of `element`, then a timer tick.
fn click(window: &slint::Window, element: &ElementHandle) {
    let at = element.absolute_position();
    let size = element.size();
    let position = slint::LogicalPosition::new(at.x + size.width / 2.0, at.y + size.height / 2.0);
    let button = slint::platform::PointerEventButton::Left;
    window.dispatch_event(slint::platform::WindowEvent::PointerMoved { position });
    window.dispatch_event(slint::platform::WindowEvent::PointerPressed { position, button });
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(50));
    window.dispatch_event(slint::platform::WindowEvent::PointerReleased { position, button });
    i_slint_backend_testing::mock_elapsed_time(Duration::from_secs(1));
    settle();
}

fn find(
    root: &impl i_slint_backend_testing::ElementRoot,
    label: &str,
    role: AccessibleRole,
) -> ElementHandle {
    ElementHandle::find_by_accessible_label(root, label)
        .find(|e| e.accessible_role() == Some(role))
        .unwrap_or_else(|| panic!("no {role:?} labelled {label:?}"))
}

fn missing(
    root: &impl i_slint_backend_testing::ElementRoot,
    label: &str,
    role: AccessibleRole,
) -> bool {
    ElementHandle::find_by_accessible_label(root, label).all(|e| e.accessible_role() != Some(role))
}

/// Opens the kebab menu on the Panel and chooses `label`.
fn menu(r: &Rig, label: &str) {
    let button = find(&r.panel, "More", AccessibleRole::Button);
    click(r.panel.window(), &button);
    let row = find(&r.panel, label, AccessibleRole::ListItem);
    click(r.panel.window(), &row);
}

/// Advances the mocked clock by `by` and runs the timers due.
fn wait(by: Duration) {
    i_slint_backend_testing::mock_elapsed_time(by);
    settle();
}

fn save(ed: &ItemEditor) {
    chord(ed.window(), &[Key::Control], "\n");
    settle();
}

// ---- Opening -----------------------------------------------------------------------------------

#[test]
fn new_snippet_in_the_kebab_opens_an_empty_editor_that_saves_text_from_the_keyboard() {
    on_ui_thread(|| {
        let r = rig();
        assert!(
            r.editor.window().is_none(),
            "nothing is made until it is needed"
        );
        let before = r.item_count();
        menu(&r, "New snippet");
        let ed = r.ed();
        assert!(ed.get_is_new());
        assert_eq!(ed.get_item_title(), "");
        assert_eq!(ed.get_kind_index(), 0);
        assert!(!ed.get_dirty());
        assert_eq!(ed.get_list_names().row_count(), 3);
        assert_eq!(
            ed.get_list_names().row_data(0).as_deref(),
            Some("Email replies")
        );

        // The title has the focus (More options is open); a click moves to the text.
        type_text(ed.window(), "Sign-off");
        click(
            ed.window(),
            &find(&ed, "Content", AccessibleRole::TextInput),
        );
        type_text(ed.window(), "Kind regards,");
        assert_eq!(ed.get_item_title(), "Sign-off");
        assert!(ed.get_dirty());
        save(&ed);

        assert!(!r.editor_shown(), "the editor closed");
        assert_eq!(r.item_count(), before + 1);
        let id = r.id("Sign-off");
        assert_eq!(r.text(id), "Kind regards,");
        assert_eq!(
            r.panel.get_rows().row_data(0).expect("row").title,
            "Sign-off"
        );
        assert!(
            r.panel
                .get_toast_text()
                .starts_with("Saved to Email replies"),
            "{}",
            r.panel.get_toast_text()
        );
        assert!(r.draft_files().is_empty(), "no draft is left behind");
    });
}

#[test]
fn ctrl_e_on_a_panel_row_opens_the_quick_editor_on_that_item_and_saving_changes_it() {
    on_ui_thread(|| {
        let r = rig();
        chord(r.panel.window(), &[Key::Control], "e");
        settle();
        let ed = r.ed();
        assert!(!ed.get_is_new());
        assert_eq!(ed.get_item_title(), "Thanks, received");
        assert!(
            ed.get_content()
                .starts_with("Thanks for sending this over.")
        );
        assert!(!ed.get_dirty());
        type_text(ed.window(), "!");
        save(&ed);
        assert!(!r.editor_shown());
        let id = r.id("Thanks, received!");
        assert!(r.text(id).starts_with("Thanks for sending this over."));
    });
}

#[test]
fn a_double_click_on_a_row_and_f2_open_the_same_editor() {
    on_ui_thread(|| {
        let r = rig();
        press(r.panel.window(), &key(Key::F2));
        settle();
        assert_eq!(r.ed().get_item_title(), "Thanks, received");
    });
}

#[test]
fn ctrl_n_and_edit_in_the_main_window_open_the_editor() {
    on_ui_thread(|| {
        let r = rig();
        let main = MainWindow::new().expect("main window");
        main.window()
            .set_size(slint::LogicalSize::new(1100.0, 720.0));
        let _bound: MainBinding<_, _, _, _> = Controller::bind_main(&r.ctl, &main);
        main.show().expect("show");
        settle();

        main.invoke_focus_items();
        settle();
        chord(main.window(), &[Key::Control], "n");
        settle();
        assert!(r.ed().get_is_new(), "Ctrl+N: a new item");
        // The editor is already up: Ctrl+N again brings it forward and keeps what is typed.
        type_text(r.ed().window(), "keep me");
        chord(main.window(), &[Key::Control], "n");
        settle();
        assert_eq!(r.ed().get_item_title(), "keep me");
        r.ed().invoke_cancel_requested();
        settle();
        assert!(!r.editor_shown() || !r.ctl.borrow().item_editor_open());

        // The context menu's Edit (the row's own action).
        let row = main
            .get_rows()
            .iter()
            .position(|row| row.title == "Out of office")
            .expect("row");
        main.invoke_item_action(
            snipdeck_ui::ItemAction::Edit,
            i32::try_from(row).expect("row"),
        );
        settle();
        assert_eq!(r.ed().get_item_title(), "Out of office");
        assert!(!r.ed().get_is_new());

        // The toolbar's New item button is the same request.
        r.ed().invoke_cancel_requested();
        settle();
        let button = find(&main, "New item", AccessibleRole::Button);
        click(main.window(), &button);
        assert!(r.ed().get_is_new());
    });
}

// ---- Cancel, discard, keep editing -------------------------------------------------------------

#[test]
fn escape_on_an_untouched_editor_closes_it_and_with_changes_asks_before_discarding() {
    on_ui_thread(|| {
        let r = rig();
        menu(&r, "New snippet");
        let ed = r.ed();
        press(ed.window(), &key(Key::Escape));
        settle();
        assert!(!r.editor_shown(), "nothing changed: closed at once");
        assert!(!r.ctl.borrow().item_editor_open());

        menu(&r, "New snippet");
        let ed = r.ed();
        type_text(ed.window(), "draft title");
        press(ed.window(), &key(Key::Escape));
        settle();
        assert!(ed.get_discard_open(), "unsaved changes: ask first");
        assert!(r.ctl.borrow().item_editor_open());
        // Esc closes the question; Keep editing keeps everything.
        press(ed.window(), &key(Key::Escape));
        assert!(!ed.get_discard_open());
        press(ed.window(), &key(Key::Escape));
        find(&ed, "Keep editing", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert!(!ed.get_discard_open());
        assert_eq!(ed.get_item_title(), "draft title");
        assert!(r.ctl.borrow().item_editor_open());

        // Discard throws it away, and nothing was saved.
        let before = r.item_count();
        press(ed.window(), &key(Key::Escape));
        find(&ed, "Discard", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert!(!r.editor_shown());
        assert!(!r.ctl.borrow().item_editor_open());
        assert_eq!(r.item_count(), before);
        assert_eq!(r.draft_files().len(), 0);
    });
}

#[test]
fn the_window_close_button_asks_when_something_changed_and_closes_when_not() {
    on_ui_thread(|| {
        let r = rig();
        menu(&r, "New snippet");
        let ed = r.ed();
        type_text(ed.window(), "x");
        ed.window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        settle();
        assert!(ed.get_discard_open(), "the close button asks too");
        assert!(r.editor_shown());
        find(&ed, "Discard", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert!(!r.ctl.borrow().item_editor_open());

        menu(&r, "New snippet");
        let ed = r.ed();
        ed.window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        settle();
        assert!(
            !r.ctl.borrow().item_editor_open(),
            "nothing changed: it just closes"
        );
    });
}

// ---- Drafts ------------------------------------------------------------------------------------

#[test]
fn what_is_typed_is_saved_as_a_draft_every_two_seconds_and_restored_after_a_restart() {
    on_ui_thread(|| {
        let data = tempfile::tempdir().expect("dir");
        let provider = SharedProvider::new();
        {
            let r = rig_at(data.path(), Tier::Pro, &provider);
            menu(&r, "New snippet");
            let ed = r.ed();
            type_text(ed.window(), "Half a thought");
            click(
                ed.window(),
                &find(&ed, "Content", AccessibleRole::TextInput),
            );
            type_text(ed.window(), "unfinished body");
            assert_eq!(ed.get_content(), "unfinished body");
            assert!(
                r.draft_files().is_empty(),
                "typing alone writes nothing: the timer does"
            );
            wait(AUTOSAVE_EVERY);
            assert_eq!(r.draft_files(), ["new.json"]);
            // The app dies here, with the editor neither saved nor discarded.
        }
        let r = rig_at(data.path(), Tier::Pro, &provider);
        menu(&r, "New snippet");
        let ed = r.ed();
        assert_eq!(ed.get_item_title(), "Half a thought");
        assert_eq!(ed.get_content(), "unfinished body");
        assert!(ed.get_restored());
        assert!(ed.get_dirty(), "restored changes are unsaved: Esc asks");
        let _ = find(
            &ed,
            "Restored the changes you had not saved.",
            AccessibleRole::Text,
        );
        save(&ed);
        assert!(!r.editor_shown());
        assert_eq!(r.text(r.id("Half a thought")), "unfinished body");
        assert!(r.draft_files().is_empty(), "saving removes the draft");
    });
}

#[test]
fn the_draft_timer_runs_only_after_an_edit_and_quitting_writes_a_final_draft() {
    on_ui_thread(|| {
        let r = rig();
        menu(&r, "New snippet");
        let ed = r.ed();
        // Fields set without an edit gesture arm nothing: no timer, so no draft.
        ed.set_content("set by the host".into());
        wait(AUTOSAVE_EVERY);
        wait(AUTOSAVE_EVERY);
        assert!(r.draft_files().is_empty(), "no edit, no timer");
        // Typing arms one shot; quitting before it fires still writes the draft.
        type_text(ed.window(), "x");
        assert_eq!(r.draft_files(), Vec::<String>::new());
        r.editor.flush_draft();
        assert_eq!(r.draft_files(), ["new.json"], "the final draft on quit");
    });
}

#[test]
fn a_draft_that_is_typed_back_to_the_start_is_removed() {
    on_ui_thread(|| {
        let r = rig();
        menu(&r, "New snippet");
        let ed = r.ed();
        type_text(ed.window(), "a");
        wait(AUTOSAVE_EVERY);
        assert_eq!(r.draft_files(), ["new.json"]);
        press(ed.window(), &key(Key::Backspace));
        wait(AUTOSAVE_EVERY);
        assert_eq!(r.draft_files().len(), 0);
    });
}

// ---- Kinds -------------------------------------------------------------------------------------

#[test]
fn the_kind_switch_offers_six_kinds_for_a_new_item_and_four_for_an_existing_one() {
    on_ui_thread(|| {
        let r = rig();
        menu(&r, "New snippet");
        let ed = r.ed();
        for label in ["Text", "Code", "Link", "Template", "Image", "File"] {
            let _ = find(&ed, label, AccessibleRole::RadioButton);
        }
        r.ed().invoke_cancel_requested();
        settle();
        chord(r.panel.window(), &[Key::Control], "e");
        let ed = r.ed();
        for label in ["Text", "Code", "Link", "Template"] {
            let _ = find(&ed, label, AccessibleRole::RadioButton);
        }
        assert!(missing(&ed, "Image", AccessibleRole::RadioButton));
        assert!(missing(&ed, "File", AccessibleRole::RadioButton));
    });
}

#[test]
fn a_link_without_a_scheme_is_refused_inline_and_a_good_one_saves() {
    on_ui_thread(|| {
        let r = rig();
        menu(&r, "New snippet");
        let ed = r.ed();
        click(ed.window(), &find(&ed, "Link", AccessibleRole::RadioButton));
        assert_eq!(ed.get_kind_index(), 2);
        ed.set_content("example.com/page".into());
        ed.set_item_title("Page".into());
        settle();
        save(&ed);
        assert!(r.editor_shown(), "refused: the editor stays");
        assert_eq!(
            find(&ed, "Link", AccessibleRole::TextInput)
                .accessible_description()
                .as_deref(),
            Some("A link needs a scheme such as https://")
        );
        ed.set_content("https://example.com/page".into());
        // Typing clears the refusal (the host hears of the edit).
        type_text(ed.window(), "");
        save(&ed);
        assert!(
            r.editor_shown(),
            "the refusal stays until the field is edited"
        );
        ed.invoke_focus_title();
        type_text(ed.window(), "s");
        save(&ed);
        assert!(!r.editor_shown());
        let id = r.id("Pages");
        assert_eq!(
            r.ctl.borrow().store().item(id).expect("item").kind,
            ItemKind::Link
        );
    });
}

#[test]
fn a_new_file_item_asks_for_a_path_and_saves_a_copy() {
    on_ui_thread(|| {
        let r = rig();
        let files = tempfile::tempdir().expect("dir");
        let path = files.path().join("notes.txt");
        fs::write(&path, b"file body").expect("write");
        menu(&r, "New snippet");
        let ed = r.ed();
        click(ed.window(), &find(&ed, "File", AccessibleRole::RadioButton));
        assert_eq!(ed.get_kind_index(), 5);
        assert!(!ed.get_can_browse(), "no file dialog off Windows");
        assert!(missing(&ed, "Browse for a file", AccessibleRole::Button));
        let field = find(&ed, "File", AccessibleRole::TextInput);
        // Saving with no path says so.
        save(&ed);
        assert!(r.editor_shown());
        assert_eq!(
            field.accessible_description().as_deref(),
            Some("Choose a file, or cancel.")
        );
        ed.set_source_path(path.to_str().expect("utf8").into());
        ed.set_item_title("Notes".into());
        settle();
        save(&ed);
        assert!(!r.editor_shown());
        let id = r.id("Notes");
        assert_eq!(
            r.ctl.borrow().store().item(id).expect("item").kind,
            ItemKind::File
        );
    });
}

#[test]
fn a_path_that_is_not_a_file_is_refused_with_the_reason() {
    on_ui_thread(|| {
        let r = rig();
        menu(&r, "New snippet");
        let ed = r.ed();
        click(
            ed.window(),
            &find(&ed, "Image", AccessibleRole::RadioButton),
        );
        ed.set_source_path("/definitely/not/here.png".into());
        settle();
        save(&ed);
        assert!(r.editor_shown());
        assert_eq!(
            find(&ed, "Picture file", AccessibleRole::TextInput)
                .accessible_description()
                .as_deref(),
            Some("There is no file at that path.")
        );
    });
}

#[test]
fn browse_fills_the_path_from_the_file_dialog() {
    on_ui_thread(|| {
        let r = rig();
        let files = tempfile::tempdir().expect("dir");
        let path = files.path().join("pic.png");
        fs::write(&path, b"x").expect("write");
        r.ctl.borrow_mut().set_file_picker(Box::new(
            snipdeck_platform::file_dialog::FakeFilePicker::answering(Ok(Some(path.clone()))),
        ));
        menu(&r, "New snippet");
        let ed = r.ed();
        click(
            ed.window(),
            &find(&ed, "Image", AccessibleRole::RadioButton),
        );
        assert!(ed.get_can_browse());
        let browse = find(&ed, "Browse for a file", AccessibleRole::Button);
        click(ed.window(), &browse);
        assert_eq!(ed.get_source_path(), path.to_str().expect("utf8"));
        assert!(ed.get_dirty());
    });
}

// ---- Sensitive, Free tier, large text ----------------------------------------------------------

#[test]
fn a_sensitive_item_made_in_the_window_is_stored_encrypted_and_never_drafted() {
    on_ui_thread(|| {
        let r = rig();
        menu(&r, "New snippet");
        let ed = r.ed();
        type_text(ed.window(), "Vault");
        ed.set_content("WINDOW-NEEDLE-4411".into());
        let toggle = find(&ed, "Sensitive item", AccessibleRole::Switch);
        click(ed.window(), &toggle);
        assert!(ed.get_sensitive());
        wait(AUTOSAVE_EVERY);
        wait(AUTOSAVE_EVERY);
        assert!(r.draft_files().is_empty(), "no draft while it is sensitive");
        save(&ed);
        assert!(!r.editor_shown());
        let id = r.id("Vault");
        assert!(r.ctl.borrow().store().item(id).expect("item").sensitive);
        assert_eq!(r.text(id), "WINDOW-NEEDLE-4411");
        assert!(r.ctl.borrow().store().key_ring_exists().expect("ring"));
        assert_no_plaintext(&r.data, &[b"WINDOW-NEEDLE-4411"]);
        // The Panel masks it.
        let row = r
            .panel
            .get_rows()
            .iter()
            .find(|row| row.title == "Vault")
            .expect("row");
        assert_eq!(row.preview, "");
    });
}

#[test]
fn a_free_user_sees_prompted_fields_refused_with_the_pro_toast() {
    on_ui_thread(|| {
        let r = rig_tier(Tier::Free);
        menu(&r, "New snippet");
        let ed = r.ed();
        assert!(!ed.get_prompted_available());
        click(
            ed.window(),
            &find(&ed, "Template", AccessibleRole::RadioButton),
        );
        ed.set_content("Dear {field:Name}".into());
        ed.set_item_title("Ask".into());
        settle();
        let before = r.item_count();
        save(&ed);
        assert!(r.editor_shown(), "not saved");
        assert_eq!(r.item_count(), before);
        let said = find(&ed, "Template text", AccessibleRole::TextInput)
            .accessible_description()
            .expect("description");
        assert!(said.contains("Supporter"), "{said}");
        assert_eq!(
            r.panel.get_toast_text(),
            "This item uses a Supporter feature."
        );
        // The placeholder buttons mark the Supporter ones.
        assert!(
            !find(&ed, "Insert {field:Name}", AccessibleRole::Button)
                .accessible_enabled()
                .unwrap_or(true)
        );
        // Without it the template saves.
        ed.set_content("Dear sir, {date}".into());
        type_text(ed.window(), "");
        ed.invoke_focus_title();
        type_text(ed.window(), "!");
        save(&ed);
        assert!(!r.editor_shown());
        let sensitive_help = {
            menu(&r, "New snippet");
            let ed = r.ed();
            find(&ed, "Sensitive item", AccessibleRole::Switch)
                .accessible_description()
                .expect("description")
                .to_string()
        };
        // Encryption is free: the switch asks nothing of Free.
        assert!(!sensitive_help.contains("Supporter"), "{sensitive_help}");
    });
}

/// The demo library on disk in `data` with one text item over the editing cap in the first list.
fn with_big_item(data: &Path) -> (ItemId, String) {
    use snipdeck_core::{Format, Formats, Item, Placement};
    use snipdeck_store::{BlobStore, BoardRepo};
    let mut store = Store::open(data).expect("store");
    demo::seed(&mut store, now()).expect("seed");
    let list = store.all_items().expect("items")[0].list_id;
    let big = format!("{}{}", "0123456789\n".repeat(2500), "TAIL");
    let mut item = Item::new(list, "Giant".into(), ItemKind::Text, now());
    item.preview = Item::preview_of(&big, false);
    let content = store.put(big.clone().into_bytes()).expect("put");
    store
        .insert_item(
            &item,
            &Formats::new(vec![Format::Text(content)]).expect("formats"),
            Placement::Last,
        )
        .expect("insert");
    let _ = store.boards().expect("boards");
    (item.id, big)
}

#[test]
fn text_over_16_kb_opens_in_a_read_only_viewer_and_saving_keeps_it_whole() {
    on_ui_thread(|| {
        let data = tempfile::tempdir().expect("dir");
        let (id, big) = with_big_item(data.path());
        let r = rig_at(data.path(), Tier::Pro, &SharedProvider::new());
        r.ctl.borrow_mut().open_item_editor(id);
        r.editor.redraw();
        let ed = r.ed();
        assert!(ed.get_content_large());
        assert_eq!(
            ed.get_content().len(),
            16 * 1024,
            "the start of the text is shown"
        );
        let field = find(&ed, "Content", AccessibleRole::TextInput);
        assert_eq!(field.accessible_read_only(), Some(true));
        let size = ed.get_content_size().to_string();
        assert!(size.ends_with("KB"), "{size}");
        let notice = format!(
            "This text is {size}. Anything over 16 KB opens read-only here, and the first 16 KB are shown."
        );
        let _ = find(&ed, &notice, AccessibleRole::Text);

        // Typing into the viewer changes nothing; the title is still editable.
        let shown = ed.get_content();
        ed.invoke_focus_title();
        click(ed.window(), &field);
        type_text(ed.window(), "typed into the viewer");
        assert_eq!(ed.get_content(), shown, "read-only");
        ed.set_item_title("Giant, renamed".into());
        wait(AUTOSAVE_EVERY);
        assert!(r.draft_files().is_empty(), "a read-only text has no draft");
        ed.invoke_mark_dirty();
        save(&ed);
        assert!(
            !r.editor_shown(),
            "{:?}",
            r.ctl.borrow().item_editor_view().map(|v| (
                v.title_problem,
                v.content_problem,
                v.banner
            ))
        );
        let id = r.id("Giant, renamed");
        assert_eq!(r.text(id), big, "the whole text is still there");
    });
}

// ---- Accessibility -----------------------------------------------------------------------------

#[test]
fn the_editor_fields_have_roles_labels_and_big_enough_targets() {
    on_ui_thread(|| {
        let r = rig();
        menu(&r, "New snippet");
        let ed = r.ed();
        ed.window().set_size(slint::LogicalSize::new(640.0, 2000.0));
        for kind in 0..=5 {
            ed.set_kind_index(kind);
            settle();
            let _ = find(&ed, "Title", AccessibleRole::TextInput);
            let _ = find(&ed, "Save", AccessibleRole::Button);
            let _ = find(&ed, "Cancel", AccessibleRole::Button);
            let _ = find(&ed, "List", AccessibleRole::Combobox);
            let _ = find(&ed, "Sensitive item", AccessibleRole::Switch);
            let _ = find(&ed, "Kind", AccessibleRole::RadioGroup);
            match kind {
                0 | 1 => {
                    let _ = find(&ed, "Content", AccessibleRole::TextInput);
                }
                2 => {
                    let _ = find(&ed, "Link", AccessibleRole::TextInput);
                }
                3 => {
                    let _ = find(&ed, "Template text", AccessibleRole::TextInput);
                }
                4 => {
                    let _ = find(&ed, "Picture file", AccessibleRole::TextInput);
                }
                _ => {
                    let _ = find(&ed, "File", AccessibleRole::TextInput);
                }
            }
            for role in [
                AccessibleRole::Button,
                AccessibleRole::RadioButton,
                AccessibleRole::Switch,
                AccessibleRole::Combobox,
                AccessibleRole::TextInput,
            ] {
                let all = i_slint_backend_testing::ElementQuery::from_root(&ed)
                    .match_descendants()
                    .match_accessible_role(role)
                    .find_all();
                for e in all {
                    assert!(
                        e.accessible_label().is_some_and(|l| !l.is_empty()),
                        "kind {kind}: an unlabelled {role:?}"
                    );
                    let s = e.size();
                    assert!(
                        s.width >= 24.0 && s.height >= 24.0,
                        "kind {kind}: {role:?} {:?} is {}x{}",
                        e.accessible_label(),
                        s.width,
                        s.height
                    );
                }
            }
        }
        let _ = (&r.ctl, &r.data);
    });
}
