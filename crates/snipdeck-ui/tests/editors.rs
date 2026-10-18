//! Screenshot tests for the editors (item, list, board, tags) and the settings page, in the states the design calls
//! out (light, dark, 225% text, errors, dialogs, long values, every item kind), plus pixel measurements.
//!
//! Goldens: `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test editors`, then look at every PNG.
//! Goldens use only glyphs IBM Plex has (Latin placeholders; no CJK): system fallback fonts differ per machine.

use slint::{ComponentHandle, ModelRc, VecModel, platform::Key};
use snipdeck_ui::{
    BoardEditor, ItemEditor, ListEditor, SettingsPage, SystemLook, TagManager,
    testing::{Frame, Harness, assert_golden},
};

#[path = "support/mod.rs"]
mod support;
use support::{chord, key, press, type_text};

#[path = "support/editor_fixture.rs"]
mod fixture;

const ITEM: (u32, u32) = (560, 760);
const SMALL: (u32, u32) = (460, 440);
const SETTINGS: (u32, u32) = (640, 1000);

// Palette values from design/tokens/palette.json.
const APP_LIGHT: [u8; 3] = [0xF5, 0xF3, 0xF1];
const SURFACE_LIGHT: [u8; 3] = [0xFB, 0xFA, 0xF7];

fn item(size: (u32, u32), dark: bool) -> Harness<ItemEditor> {
    let h = Harness::new(ItemEditor::new, size.0, size.1);
    h.component.set_dark(dark);
    fixture::text_item(&h.component);
    h
}

fn render<C: ComponentHandle>(h: &Harness<C>) -> Frame {
    h.render_settled()
}

#[test]
fn item_editor_text_light_and_dark() {
    let h = item(ITEM, false);
    assert_golden("item-editor-text-light", &render(&h));
    drop(h);
    let h = item(ITEM, true);
    h.component.set_sensitive(true);
    assert_golden("item-editor-text-dark", &render(&h));
}

#[test]
fn item_editor_each_kind() {
    let h = item(ITEM, false);
    h.component.set_kind_index(1);
    h.component.set_item_title("Retry helper".into());
    h.component.set_content(
        "async fn retry<T>(f: impl Fn() -> T, n: usize) -> T {\n    for _ in 0..n { /* try */ }\n}"
            .into(),
    );
    assert_golden("item-editor-code-light", &render(&h));
    drop(h);

    let h = item(ITEM, false);
    h.component.set_kind_index(2);
    h.component.set_item_title("Design spec".into());
    h.component
        .set_content("figma.com/file/snipdeck-index-deck".into());
    h.component
        .set_content_error("A link needs a scheme such as https://".into());
    assert_golden("item-editor-link-error-light", &render(&h));
    drop(h);

    let h = item((560, 980), false);
    h.component.set_kind_index(3);
    h.component.set_item_title("Invoice reminder".into());
    h.component
        .set_content("A gentle reminder that invoice {field:Number} is due on {date}.".into());
    assert_golden("item-editor-template-light", &render(&h));
    drop(h);

    let h = item((560, 980), false);
    h.component.set_kind_index(3);
    h.component.set_prompted_available(false);
    h.component.set_content("Due on {date}.".into());
    assert_golden("item-editor-template-free-light", &render(&h));
    drop(h);

    let h = item(ITEM, false);
    h.component.set_kind_index(4);
    h.component.set_item_title("Screenshot 14:02".into());
    h.component.set_preview(fixture::gradient());
    h.component
        .set_attachment_name("screenshot-1402.png".into());
    h.component
        .set_attachment_detail("1280 x 720 PNG, copied from the browser".into());
    assert_golden("item-editor-image-light", &render(&h));
    drop(h);

    let h = item(ITEM, true);
    h.component.set_kind_index(5);
    h.component.set_item_title("Q3 report".into());
    h.component
        .set_attachment_name("report-q3-final.pdf".into());
    h.component
        .set_attachment_detail("PDF, 2.4 MB. Pasting copies the file itself.".into());
    assert_golden("item-editor-file-dark", &render(&h));
}

#[test]
fn item_editor_new_item_shows_what_is_missing() {
    let h = Harness::new(ItemEditor::new, ITEM.0, ITEM.1);
    h.component.set_is_new(true);
    h.component.set_list_names(fixture::list_names());
    h.component.set_tags(fixture::tag_model(Vec::new()));
    let _ = render(&h);
    // Ctrl+Enter on an empty item: the content field says what is missing.
    chord(h.component.window(), &[Key::Control], "\n");
    assert_golden("item-editor-new-errors-light", &render(&h));
}

#[test]
fn item_editor_long_values_and_the_inline_limit() {
    let h = item((420, 760), false);
    h.component.set_item_title(
        "A very long item title that keeps going well past the width of the field and then some"
            .into(),
    );
    h.component.set_content_large(true);
    h.component.set_content_size("2.4 MB".into());
    h.component.set_content(
        "Supercalifragilisticexpialidocious_Supercalifragilisticexpialidocious_Snippet https://example.com/a/very/long/unbroken/path".into(),
    );
    h.component
        .set_title_error("Titles can't be longer than 200 characters.".into());
    assert_golden("item-editor-long-light", &render(&h));
}

#[test]
fn item_editor_at_225_percent_text_size() {
    let h = item((1100, 1500), false);
    h.component.global::<SystemLook<'_>>().set_text_scale(2.25);
    assert_golden("item-editor-scale-light", &render(&h));
}

#[test]
fn item_editor_asks_before_discarding_and_scrolls_to_the_focus() {
    let h = item(ITEM, false);
    let _ = render(&h);
    h.component.invoke_focus_title();
    type_text(h.component.window(), "!");
    press(h.component.window(), &key(Key::Escape));
    assert!(
        h.component.get_discard_open(),
        "Esc with unsaved changes asks first"
    );
    let f = render(&h);
    assert_golden("item-editor-discard-light", &f);
    // The 40% scrim covers the whole window, corners included.
    for (x, y) in [
        (2, 2),
        (ITEM.0 - 3, 2),
        (2, ITEM.1 - 3),
        (ITEM.0 - 3, ITEM.1 - 3),
    ] {
        let p = f.pixel(x, y);
        assert!(
            p[0] < APP_LIGHT[0] - 40 && p[1] < APP_LIGHT[1] - 40,
            "scrim at ({x}, {y}) is {p:?}, expected darker than the page"
        );
    }
}

#[test]
fn item_editor_footer_is_a_surface_below_the_page() {
    let h = item(ITEM, false);
    let f = render(&h);
    assert_eq!(f.pixel(4, 4), APP_LIGHT, "page background at the top left");
    assert_eq!(
        f.pixel(4, ITEM.1 - 4),
        SURFACE_LIGHT,
        "footer surface at the bottom left"
    );
}

#[test]
fn list_editor_new_existing_error_and_confirmation() {
    let h = Harness::new(ListEditor::new, SMALL.0, SMALL.1);
    h.component.set_is_new(true);
    assert_golden("list-editor-new-light", &render(&h));
    chord(h.component.window(), &[Key::Control], "\n");
    assert_golden("list-editor-new-errors-light", &render(&h));
    drop(h);

    let h = Harness::new(ListEditor::new, SMALL.0, SMALL.1);
    h.component.set_dark(true);
    fixture::list_editor_existing(&h.component);
    h.component
        .set_name_error("A list named Email replies already exists.".into());
    h.component.set_sort_index(1);
    assert_golden("list-editor-existing-dark", &render(&h));
    drop(h);

    let h = Harness::new(ListEditor::new, SMALL.0, SMALL.1);
    fixture::list_editor_existing(&h.component);
    h.component.set_delete_open(true);
    assert_golden("list-editor-delete-light", &render(&h));
}

#[test]
fn board_editor_and_the_last_board() {
    let h = Harness::new(BoardEditor::new, SMALL.0, SMALL.1);
    h.component.set_name("Work".into());
    h.component.set_accent(4);
    h.component.set_item_count(42);
    h.component.set_delete_open(true);
    assert_golden("board-editor-delete-light", &render(&h));
    drop(h);

    let h = Harness::new(BoardEditor::new, SMALL.0, SMALL.1);
    h.component.set_dark(true);
    h.component.set_name("Personal".into());
    h.component.set_can_delete(false);
    h.component
        .set_delete_hint("The last board can't be deleted.".into());
    assert_golden("board-editor-last-dark", &render(&h));
}

#[test]
fn tag_manager_rows_renaming_and_deleting() {
    let h = Harness::new(TagManager::new, 440, 520);
    fixture::tag_manager(&h.component);
    assert_golden("tag-manager-light", &render(&h));
    h.component.set_editing(1);
    h.component.set_rename_text("meetings".into());
    h.component
        .set_rename_error("A tag named meetings already exists.".into());
    assert_golden("tag-manager-rename-light", &render(&h));
    h.component.set_editing(-1);
    h.component.set_delete_index(0);
    assert_golden("tag-manager-delete-light", &render(&h));
    drop(h);

    let h = Harness::new(TagManager::new, 440, 520);
    h.component.set_dark(true);
    h.component.set_tags(ModelRc::new(VecModel::default()));
    assert_golden("tag-manager-empty-dark", &render(&h));
}

#[test]
fn settings_page_light_dark_and_scaled() {
    let h = Harness::new(SettingsPage::new, SETTINGS.0, SETTINGS.1);
    fixture::settings(&h.component);
    assert_golden("settings-light", &render(&h));
    drop(h);

    let h = Harness::new(SettingsPage::new, SETTINGS.0, SETTINGS.1);
    h.component.set_dark(true);
    fixture::settings(&h.component);
    h.component.set_theme_index(2);
    h.component.set_autostart(false);
    assert_golden("settings-dark", &render(&h));
    drop(h);

    let h = Harness::new(SettingsPage::new, 1200, 1800);
    fixture::settings(&h.component);
    h.component.global::<SystemLook<'_>>().set_text_scale(2.25);
    assert_golden("settings-scale-light", &render(&h));
}

#[test]
fn settings_page_licence_backups_data_and_privacy() {
    // Tall enough for every section.
    let h = Harness::new(SettingsPage::new, SETTINGS.0, 2400);
    fixture::settings(&h.component);
    assert_golden("settings-full-light", &render(&h));
    drop(h);

    let h = Harness::new(SettingsPage::new, SETTINGS.0, 2400);
    h.component.set_dark(true);
    fixture::settings(&h.component);
    h.component.set_pro(true);
    h.component.set_erase_open(true);
    h.component.set_toast_text("Backed up".into());
    h.component.set_toast_kind(snipdeck_ui::ToastKind::Success);
    assert_golden("settings-full-pro-erase-dark", &render(&h));
    drop(h);

    let h = Harness::new(SettingsPage::new, SETTINGS.0, 2400);
    fixture::settings(&h.component);
    h.component.set_has_data_folder(false);
    h.component.set_backups(ModelRc::new(VecModel::default()));
    h.component.set_licence_error(
        "That licence key is not valid. Check that you copied all of it.".into(),
    );
    assert_golden("settings-demo-light", &render(&h));
}

#[test]
fn settings_page_conflict_empty_and_narrow() {
    let h = Harness::new(SettingsPage::new, 420, 900);
    fixture::settings(&h.component);
    let mut hotkeys = fixture::hotkeys();
    hotkeys[1].conflict = "Another program already uses Ctrl+Shift+Space.".into();
    h.component
        .set_hotkeys(ModelRc::new(VecModel::from(hotkeys)));
    h.component
        .set_no_paste_apps(ModelRc::new(VecModel::default()));
    h.component
        .set_no_paste_error("keepass.exe is already in the list.".into());
    assert_golden("settings-conflict-narrow-light", &render(&h));
}

#[test]
fn settings_page_recording_a_shortcut() {
    let h = Harness::new(SettingsPage::new, SETTINGS.0, SETTINGS.1);
    fixture::settings(&h.component);
    let _ = render(&h);
    // Tab to the first recorder (the page itself is the first stop) and start recording with Enter.
    press(h.component.window(), &key(Key::Tab));
    press(h.component.window(), &key(Key::Tab));
    press(h.component.window(), "\n");
    assert_golden("settings-recording-light", &render(&h));
}

#[test]
fn form_controls_in_every_state() {
    use snipdeck_ui::FormControlsGallery;
    let h = Harness::new(FormControlsGallery::new, 520, 760);
    assert_golden("formctl-light", &render(&h));
    h.component.invoke_focus_seg();
    assert_golden("formctl-focus-segmented-light", &render(&h));
    h.component.invoke_focus_num();
    assert_golden("formctl-focus-stepper-light", &render(&h));
    h.component.invoke_focus_sel();
    h.component.set_sel_open(true);
    assert_golden("formctl-select-open-light", &render(&h));
    drop(h);

    let h = Harness::new(FormControlsGallery::new, 520, 760);
    h.component.set_dark(true);
    h.component.set_sel(3);
    h.component.set_num(480);
    assert_golden("formctl-dark", &render(&h));
    h.component.set_sel_open(true);
    h.component.invoke_focus_sel();
    assert_golden("formctl-select-open-dark", &render(&h));
}
