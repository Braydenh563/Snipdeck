//! The Panel's multi-selection, the bulk bar, the item menu's Move / Colour / Sensitive rows,
//! the quick-add box and its toast. Time is the fixed clock of the world.

use super::*;
use crate::{item_editor::Origin, main_window::SelectMode};

fn marked(w: &World) -> Vec<usize> {
    w.ctl
        .view()
        .rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.marked)
        .map(|(i, _)| i)
        .collect()
}

#[test]
fn ctrl_click_marks_and_a_second_ctrl_click_unmarks() {
    let mut w = world();
    w.ctl.panel_mark(0, SelectMode::Toggle);
    w.ctl.panel_mark(2, SelectMode::Toggle);
    assert_eq!(marked(&w), [0, 2]);
    assert_eq!(w.ctl.view().marked_count, 2);
    w.ctl.panel_mark(0, SelectMode::Toggle);
    assert_eq!(marked(&w), [2]);
}

#[test]
fn shift_click_marks_the_range_from_the_last_click() {
    let mut w = world();
    w.ctl.panel_mark(1, SelectMode::Toggle);
    w.ctl.panel_mark(3, SelectMode::Extend);
    assert_eq!(marked(&w), [1, 2, 3]);
}

#[test]
fn ctrl_a_marks_every_row_and_escape_clears() {
    let mut w = world();
    w.ctl.panel_mark_all();
    assert_eq!(marked(&w), [0, 1, 2, 3]);
    w.ctl.panel_clear_marks();
    assert_eq!(marked(&w), Vec::<usize>::new());
    assert_eq!(w.ctl.view().marked_count, 0);
}

#[test]
fn shift_down_and_up_extend_and_shrink_from_the_cursor() {
    let mut w = world();
    w.ctl.panel_extend_marks(1);
    w.ctl.panel_extend_marks(1);
    assert_eq!(marked(&w), [0, 1, 2]);
    assert_eq!(w.ctl.view().selected, 2);
    w.ctl.panel_extend_marks(-1);
    assert_eq!(marked(&w), [0, 1]);
}

#[test]
fn switching_list_ends_the_selection() {
    let mut w = world();
    w.ctl.panel_mark_all();
    w.ctl.select_tab(1);
    assert_eq!(w.ctl.view().marked_count, 0);
}

#[test]
fn delete_removes_the_selection_with_an_undo_toast_and_no_confirmation() {
    let mut w = world();
    w.ctl.panel_mark(0, SelectMode::Toggle);
    w.ctl.panel_mark(1, SelectMode::Toggle);
    w.ctl.delete_row(3);
    let view = w.ctl.view();
    assert_eq!(view.rows.len(), 2, "both marked rows left the list");
    assert_eq!(view.marked_count, 0);
    let toast = view.toast.expect("toast");
    assert_eq!(toast.message, ToastMessage::ItemsTrashed { count: 2 });
    assert!(toast.undo);
    w.ctl.main_undo();
    assert_eq!(w.ctl.view().rows.len(), 4, "Undo brings both back");
}

#[test]
fn the_bulk_bar_deletes_the_marked_rows() {
    let mut w = world();
    w.ctl.panel_mark_all();
    w.ctl.bulk_delete();
    assert_eq!(w.ctl.view().rows.len(), 0);
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::ItemsTrashed { count: 4 }
    );
}

#[test]
fn the_bulk_bar_moves_the_marked_rows_to_a_chosen_list() {
    let mut w = world();
    w.ctl.panel_mark(0, SelectMode::Toggle);
    w.ctl.panel_mark(1, SelectMode::Toggle);
    w.ctl.bulk_open_move();
    let menu = w.ctl.view().choose_menu.expect("lists to choose from");
    assert_eq!(menu.entries.len(), 2);
    assert_eq!(menu.entries[0].label, ChooseLabel::List("Replies".into()));
    assert!(menu.entries[0].checked, "both are in Replies");
    w.ctl.choose_item_menu(1);
    assert!(w.ctl.view().choose_menu.is_none());
    assert_eq!(w.ctl.view().rows.len(), 2);
    w.ctl.select_tab(1);
    assert_eq!(w.ctl.view().rows.len(), 3);
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::ItemsMoved {
            count: 2,
            list: "Links".into()
        }
    );
}

#[test]
fn the_bulk_bar_colours_the_marked_rows_and_no_label_clears_it() {
    let mut w = world();
    w.ctl.panel_mark(0, SelectMode::Toggle);
    w.ctl.panel_mark(2, SelectMode::Toggle);
    w.ctl.bulk_open_colour();
    let menu = w.ctl.view().choose_menu.expect("colours");
    assert_eq!(menu.entries.len(), 9, "no label and eight colours");
    // Entry 6 is Red (no label, then blue teal green amber orange red).
    w.ctl.choose_item_menu(6);
    let rows = w.ctl.view().rows;
    assert_eq!(rows[0].label, Some(snipdeck_core::ItemLabel::Red));
    assert_eq!(rows[1].label, None);
    assert_eq!(rows[2].label, Some(snipdeck_core::ItemLabel::Red));
    assert_eq!(
        marked(&w),
        [0, 2],
        "the selection stays for the next action"
    );
    w.ctl.bulk_open_colour();
    w.ctl.choose_item_menu(0);
    assert!(w.ctl.view().rows.iter().all(|r| r.label.is_none()));
}

#[test]
fn copy_all_joins_the_marked_texts_in_row_order() {
    let mut w = world();
    w.ctl.panel_mark(2, SelectMode::Toggle);
    w.ctl.panel_mark(0, SelectMode::Toggle);
    w.ctl.services.clipboard.written.clear();
    w.ctl.bulk_copy();
    let written = &w.ctl.services.clipboard.written;
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].data(), [Data::Text("first\nthird".into())]);
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::Copied
    );
}

#[test]
fn the_item_menu_on_a_marked_row_acts_on_every_marked_row() {
    let mut w = world();
    w.ctl.panel_mark(0, SelectMode::Toggle);
    w.ctl.panel_mark(1, SelectMode::Toggle);
    w.ctl.open_item_menu(1, false);
    let entries = w.ctl.view().item_menu.expect("menu");
    let at = entries
        .iter()
        .position(|e| *e == ItemMenuEntry::Trash)
        .expect("a Delete row");
    w.ctl.choose_item_menu(at);
    assert_eq!(w.ctl.view().rows.len(), 2);
}

#[test]
fn the_item_menu_offers_move_to_colour_and_sensitive() {
    let mut w = world();
    w.ctl.open_item_menu(0, false);
    let entries = w.ctl.view().item_menu.expect("menu");
    assert!(entries.contains(&ItemMenuEntry::MoveTo));
    assert!(entries.contains(&ItemMenuEntry::Colour));
    assert!(entries.contains(&ItemMenuEntry::Sensitive {
        on: false,
        enabled: true
    }));
    let at = entries
        .iter()
        .position(|e| *e == ItemMenuEntry::MoveTo)
        .expect("row");
    w.ctl.choose_item_menu(at);
    assert!(
        w.ctl.view().item_menu.is_none(),
        "the second level took its place"
    );
    assert!(w.ctl.view().choose_menu.is_some());
    w.ctl.close_item_menu();
    assert!(w.ctl.view().choose_menu.is_none());
}

#[test]
fn keep_on_top_is_a_menu_row_that_flips_the_pin() {
    let mut w = world();
    let before = w.ctl.view().on_top;
    let at = APP_MENU
        .iter()
        .position(|e| *e == AppMenuEntry::KeepOnTop)
        .expect("row");
    w.ctl.open_app_menu();
    w.ctl.choose_app_menu(at);
    assert_eq!(w.ctl.view().on_top, !before);
    assert_eq!(w.ctl.settings().panel_pinned, !before);
    assert!(w.ctl.view().app_menu.is_none(), "the menu closed");
    w.ctl.open_app_menu();
    w.ctl.choose_app_menu(at);
    assert_eq!(w.ctl.view().on_top, before);
}

// ---- Quick-add ---------------------------------------------------------------------------------

#[test]
fn quick_add_saves_text_at_the_end_of_the_shown_list_and_selects_it() {
    let mut w = world();
    assert!(
        w.ctl
            .quick_add("remember the milk\nand eggs", None, false, Origin::Panel)
    );
    let view = w.ctl.view();
    assert_eq!(view.rows.len(), 5);
    let row = &view.rows[4];
    assert_eq!(
        row.title, "remember the milk",
        "the title is the first line"
    );
    assert_eq!(row.kind, ItemKind::Text);
    assert_eq!(view.selected, 4);
    let toast = view.toast.expect("toast");
    assert_eq!(
        (toast.kind, toast.message),
        (
            ToastKind::Success,
            ToastMessage::Saved {
                list: "Replies".into()
            }
        )
    );
    assert!(toast.undo && toast.edit, "Undo and Edit");
}

#[test]
fn quick_add_makes_a_lone_address_a_link_and_a_placeholder_a_template() {
    let mut w = world();
    w.ctl
        .quick_add("  https://example.org/a  ", None, false, Origin::Panel);
    w.ctl
        .quick_add("Dear {field:Name}", None, false, Origin::Panel);
    w.ctl
        .quick_add("see https://example.org", None, false, Origin::Panel);
    let kinds: Vec<ItemKind> = w.ctl.view().rows[4..].iter().map(|r| r.kind).collect();
    assert_eq!(kinds, [ItemKind::Link, ItemKind::Template, ItemKind::Text]);
}

#[test]
fn quick_add_takes_a_kind_the_user_chose() {
    let mut w = world();
    w.ctl.quick_add(
        "https://example.org",
        Some(ItemKind::Text),
        false,
        Origin::Panel,
    );
    assert_eq!(w.ctl.view().rows[4].kind, ItemKind::Text);
}

#[test]
fn quick_add_with_blank_text_does_nothing() {
    let mut w = world();
    assert!(!w.ctl.quick_add("  \n\t ", None, false, Origin::Panel));
    assert_eq!(w.ctl.view().rows.len(), 4);
    assert!(w.ctl.view().toast.is_none());
}

#[test]
fn quick_add_goes_to_the_list_on_the_tab() {
    let mut w = world();
    w.ctl.select_tab(1);
    w.ctl
        .quick_add("https://example.net", None, false, Origin::Panel);
    assert_eq!(w.ctl.view().rows.len(), 2);
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::Saved {
            list: "Links".into()
        }
    );
}

#[test]
fn the_saved_toast_undo_moves_the_new_item_to_the_trash() {
    let mut w = world();
    w.ctl.quick_add("oops", None, false, Origin::Panel);
    assert_eq!(w.ctl.view().rows.len(), 5);
    w.ctl.toast_action();
    assert_eq!(w.ctl.view().rows.len(), 4);
}

#[test]
fn the_saved_toast_edit_opens_the_new_item_in_the_editor() {
    let mut w = world();
    w.ctl.quick_add("polish me", None, false, Origin::Panel);
    let id = w.ctl.view().rows[4].id;
    assert_eq!(w.ctl.toast_edit(), [Effect::EditItem(id)]);
    assert!(w.ctl.view().toast.is_none(), "the toast is gone");
    assert!(w.ctl.toast_edit().is_empty(), "once");
}

#[test]
fn ctrl_shift_n_opens_the_full_editor_prefilled_with_what_was_typed() {
    let mut w = world();
    w.ctl
        .open_new_item_with(Origin::Panel, "https://example.org\n");
    let view = w.ctl.item_editor_view().expect("editor");
    assert!(view.is_new);
    assert_eq!(view.fields.content, "https://example.org");
    assert_eq!(view.fields.kind, ItemKind::Link);
}

#[test]
fn more_options_is_remembered_in_the_settings() {
    let mut w = world();
    assert!(!w.ctl.settings().editor_more_open);
    w.ctl.set_editor_more_open(true);
    assert!(w.ctl.settings().editor_more_open);
    w.ctl.open_new_item(Origin::Panel);
    assert!(w.ctl.item_editor_view().expect("editor").more_open);
}

#[test]
fn reset_size_and_position_clears_the_stored_geometry_of_this_monitor_only() {
    use crate::frame::{Monitor, Rect};
    let mut w = world();
    let first = Monitor {
        id: Monitor::id_of("DISPLAY1", 1920, 1080),
        work_area: Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        },
        scale: 1.0,
        taskbar: None,
    };
    let second = Monitor {
        id: Monitor::id_of("DISPLAY2", 1280, 1024),
        work_area: Rect {
            x: 1920,
            y: 0,
            w: 1280,
            h: 984,
        },
        scale: 1.0,
        taskbar: None,
    };
    w.ctl.frame_changed(
        &second,
        Rect {
            x: 2000,
            y: 30,
            w: 450,
            h: 700,
        },
    );
    w.ctl.frame_changed(
        &first,
        Rect {
            x: 10,
            y: 20,
            w: 300,
            h: 500,
        },
    );
    let index = crate::controller::APP_MENU
        .iter()
        .position(|e| *e == crate::controller::AppMenuEntry::ResetGeometry)
        .expect("the kebab has the row");
    w.ctl.open_app_menu();
    let effects = w.ctl.choose_app_menu(index);
    assert!(matches!(effects.as_slice(), [Effect::PlaceWindow(_)]));
    let ids: Vec<_> = w
        .ctl
        .settings()
        .panel_geometry
        .iter()
        .map(|g| g.monitor_id.clone())
        .collect();
    assert_eq!(
        ids,
        std::slice::from_ref(&second.id),
        "only the current monitor's entry is gone"
    );
    assert!(w.ctl.view().toast.is_none(), "no Undo toast");
}

fn pasting(text: &'static str) -> World {
    build(Opts {
        clipboard: Some(text),
        ..Opts::default()
    })
}

#[test]
fn paste_create_adds_to_the_shown_list_with_undo_and_edit() {
    let mut w = pasting("a fresh note");
    assert!(w.ctl.history_view().undo.is_none());
    w.ctl.paste_create(Origin::Panel);
    let view = w.ctl.view();
    assert_eq!(view.rows.len(), 5, "the shown list gained one item");
    assert_eq!(view.rows[0].preview, "a fresh note", "at the top");
    let toast = view.toast.expect("toast");
    assert_eq!(
        toast.message,
        ToastMessage::AddedTo {
            list: "Replies".into(),
            count: 1
        }
    );
    assert!(toast.undo && toast.edit, "Undo and Edit are offered");
    assert!(w.ctl.history_view().undo.is_some(), "one history step");
    let id = view.rows[0].id;
    assert_eq!(w.ctl.toast_edit(), [Effect::EditItem(id)]);

    // Undo takes the item back to the Trash and leaves no step behind.
    let mut w = pasting("a fresh note");
    w.ctl.paste_create(Origin::Panel);
    w.ctl.toast_action();
    assert_eq!(w.ctl.view().rows.len(), 4);
    assert!(w.ctl.history_view().undo.is_none(), "exactly one step");
    assert!(w.ctl.history_view().redo.is_some());
}

#[test]
fn a_pasted_address_becomes_a_link() {
    let mut w = pasting("https://example.org/page");
    w.ctl.paste_create(Origin::Panel);
    let id = w.ctl.view().rows[0].id;
    assert_eq!(w.ctl.store().item(id).expect("item").kind, ItemKind::Link);
}

#[test]
fn an_empty_clipboard_creates_nothing() {
    let mut w = world();
    w.ctl.paste_create(Origin::Panel);
    let view = w.ctl.view();
    assert_eq!(view.rows.len(), 4);
    assert_eq!(
        view.toast.expect("toast").message,
        ToastMessage::NothingToSave
    );
}

#[test]
fn a_secret_paste_opens_the_editor_sensitive() {
    let secret = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
    let mut w = pasting(secret);
    w.ctl.paste_create(Origin::Panel);
    let view = w.ctl.view();
    assert_eq!(view.rows.len(), 4, "nothing was saved");
    assert_eq!(
        view.toast.expect("toast").message,
        ToastMessage::LooksSecret
    );
    let editor = w.ctl.item_editor_view().expect("the editor is open");
    assert!(editor.is_new);
    assert!(editor.fields.sensitive);
    assert_eq!(editor.fields.content, secret);
}

/// The kinds a stored item holds, in order.
fn stored_kinds(w: &World, id: snipdeck_core::ItemId) -> Vec<snipdeck_core::FormatKind> {
    w.ctl
        .store
        .formats(id)
        .expect("formats")
        .iter()
        .map(snipdeck_core::Format::kind)
        .collect()
}

#[test]
fn paste_create_keeps_html_and_rtf() {
    use snipdeck_core::FormatKind as K;
    let mut w = pasting("bold words");
    w.ctl.services.clipboard.held_html = Some("<b>bold</b> words".into());
    w.ctl.services.clipboard.held_rtf = Some(b"{\\rtf1 bold words}".to_vec());
    w.ctl.paste_create(Origin::Panel);
    let row = w.ctl.view().rows[0].clone();
    assert_eq!(row.preview, "bold words");
    assert_eq!(stored_kinds(&w, row.id), [K::Text, K::Html, K::Rtf]);
    assert!(row.formatted, "the row carries the Formatted mark");
    // Plain text stays plain.
    let mut w = pasting("just text");
    w.ctl.paste_create(Origin::Panel);
    let row = w.ctl.view().rows[0].clone();
    assert_eq!(stored_kinds(&w, row.id), [K::Text]);
    assert!(!row.formatted);
}

#[test]
fn a_secret_rich_paste_still_opens_the_sensitive_editor() {
    let secret = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
    let mut w = pasting(secret);
    w.ctl.services.clipboard.held_html = Some(format!("<b>{secret}</b>"));
    w.ctl.services.clipboard.held_rtf = Some(b"{\\rtf1 secret}".to_vec());
    w.ctl.paste_create(Origin::Panel);
    assert_eq!(w.ctl.view().rows.len(), 4, "nothing was saved");
    let editor = w.ctl.item_editor_view().expect("the editor is open");
    assert!(editor.is_new && editor.fields.sensitive);
    assert_eq!(editor.fields.content, secret);
}

#[test]
fn ctrl_c_one_item_uses_item_copy() {
    let mut w = world();
    w.ctl.services.clipboard.written.clear();
    w.ctl.panel_copy_selection();
    let written = &w.ctl.services.clipboard.written;
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].data(), [Data::Text("first".into())]);
    assert_eq!(
        uses(&w, w.ids[0]),
        1,
        "counted as a use, like the Copy button"
    );
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::Copied
    );
}

#[test]
fn ctrl_c_many_joins_like_copy_all() {
    let mut w = world();
    w.ctl.panel_mark(2, SelectMode::Toggle);
    w.ctl.panel_mark(0, SelectMode::Toggle);
    w.ctl.services.clipboard.written.clear();
    w.ctl.panel_copy_selection();
    let written = &w.ctl.services.clipboard.written;
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].data(), [Data::Text("first\nthird".into())]);
}

#[test]
fn ctrl_c_on_a_single_sensitive_item_applies_the_clear_rules() {
    let mut w = world();
    let id = w.ids[0];
    super::use_paste::make_sensitive(&mut w, id);
    let used = w.ctl.panel_copy_selection();
    assert!(matches!(used, Some(UseResult::Done(_))), "{used:?}");
    assert_eq!(
        w.ctl.clipboard_clear_wait(),
        Some(std::time::Duration::from_secs(30))
    );
}

#[test]
fn a_sensitive_item_in_a_multi_selection_is_skipped() {
    let mut w = world();
    let id = w.ids[1];
    super::use_paste::make_sensitive(&mut w, id);
    for row in 0..3 {
        w.ctl.panel_mark(row, SelectMode::Toggle);
    }
    w.ctl.services.clipboard.written.clear();
    assert!(w.ctl.panel_copy_selection().is_none());
    assert_eq!(
        w.ctl.services.clipboard.written[0].data(),
        [Data::Text("first\nthird".into())]
    );
}

fn items_in(w: &World, list: snipdeck_core::ListId) -> usize {
    w.ctl
        .store()
        .all_items()
        .expect("items")
        .iter()
        .filter(|i| i.list_id == list)
        .count()
}

#[test]
fn paste_create_in_the_main_window_adds_to_the_open_list() {
    let mut w = pasting("from main");
    w.ctl
        .main_open(crate::main_window::Section::List(w.links_list));
    w.ctl.paste_create(Origin::Main);
    assert_eq!(items_in(&w, w.links_list), 2);
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::AddedTo {
            list: "Links".into(),
            count: 1
        }
    );
}

#[test]
fn main_copy_selection_copies_one_item_or_joins_many() {
    let mut w = world();
    w.ctl
        .main_open(crate::main_window::Section::List(w.links_list));
    w.ctl.services.clipboard.written.clear();
    let used = w.ctl.main_copy_selection(0);
    assert!(matches!(used, Some(UseResult::Done(_))), "{used:?}");
    assert_eq!(
        w.ctl.services.clipboard.written[0].data(),
        [Data::Text("https://example.com".into())]
    );
    assert_eq!(uses(&w, w.ids[4]), 1);

    w.ctl.main_open(crate::main_window::Section::All);
    w.ctl.main_select_all();
    w.ctl.services.clipboard.written.clear();
    assert!(w.ctl.main_copy_selection(0).is_none());
    let text = match &w.ctl.services.clipboard.written[0].data()[0] {
        Data::Text(t) => t.clone(),
        other => panic!("{other:?}"),
    };
    for want in ["first", "second", "third", "https://example.com"] {
        assert!(text.lines().any(|l| l == want), "{want} in {text:?}");
    }
}

#[test]
fn undo_falls_back_to_the_trash_when_the_history_step_is_gone() {
    let mut w = pasting("a fresh note");
    w.ctl.paste_create(Origin::Panel);
    assert_eq!(w.ctl.view().rows.len(), 5);
    w.ctl.clear_history();
    w.ctl.toast_action();
    assert_eq!(w.ctl.view().rows.len(), 4, "the new item went to the Trash");
}

#[test]
fn a_secret_paste_leaves_an_open_editor_alone_and_says_so() {
    let mut w = pasting("ghp_abcdefghijklmnopqrstuvwxyz0123456789");
    w.ctl.open_new_item(Origin::Panel);
    let before = w.ctl.item_editor_view().expect("editor").fields;
    w.ctl.paste_create(Origin::Panel);
    let after = w.ctl.item_editor_view().expect("still open").fields;
    assert_eq!(before, after, "the open editor is untouched");
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::PasteNotAddedEditorOpen
    );
}

#[test]
fn a_secret_paste_opens_a_fresh_editor_and_leaves_the_new_item_draft_alone() {
    let dir = tempfile::tempdir().expect("dir");
    let mut w = pasting("password: hunter2");
    w.ctl
        .set_drafts(crate::drafts::Drafts::in_data_dir(dir.path()));
    // A half-typed new item is parked: its draft stays on disk.
    w.ctl.open_new_item(Origin::Panel);
    let mut typed = w.ctl.item_editor_view().expect("editor").fields;
    typed.content = "half a thought".into();
    w.ctl.item_editor_park(&typed);
    let draft = dir.path().join("drafts").join("new.json");
    assert!(draft.exists());
    let kept = std::fs::read(&draft).expect("draft");

    w.ctl.paste_create(Origin::Panel);
    let editor = w.ctl.item_editor_view().expect("editor");
    assert!(!editor.restored, "the draft was not mixed in");
    assert_eq!(editor.fields.content, "password: hunter2");
    assert!(editor.fields.sensitive);
    // Autosave and cancel of the secret editor do not touch the old draft.
    w.ctl.item_editor_autosave(&editor.fields);
    assert_eq!(std::fs::read(&draft).expect("draft"), kept);
    w.ctl.item_editor_cancel();
    assert_eq!(std::fs::read(&draft).expect("draft"), kept);
}

/// A PNG of one pixel, for a pasted screenshot.
fn tiny_png() -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, 1, 1);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("header");
    writer.write_image_data(&[1, 2, 3]).expect("pixel");
    writer.finish().expect("png");
    out
}

/// Files in a temp folder, named and filled as given, and their paths as Explorer lists them.
fn files_in(dir: &tempfile::TempDir, files: &[(&str, &[u8])]) -> Vec<String> {
    files
        .iter()
        .map(|(name, bytes)| {
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).expect("file");
            path.to_string_lossy().into_owned()
        })
        .collect()
}

#[test]
fn pasting_two_files_adds_two_file_items_one_step() {
    let dir = tempfile::tempdir().expect("dir");
    let mut w = world();
    w.ctl.services.clipboard.held_files =
        files_in(&dir, &[("a.txt", b"alpha"), ("b.bin", &[0, 1, 2, 3])]);
    w.ctl.paste_create(Origin::Panel);
    let view = w.ctl.view();
    assert_eq!(view.rows.len(), 6, "two items joined the four");
    assert_eq!(
        (view.rows[0].title.as_str(), view.rows[1].title.as_str()),
        ("a.txt", "b.bin"),
        "in the order copied, at the top"
    );
    assert!(view.rows[..2].iter().all(|r| r.kind == ItemKind::File));
    assert_eq!(
        view.toast.expect("toast").message,
        ToastMessage::AddedTo {
            list: "Replies".into(),
            count: 2
        }
    );
    assert!(w.ctl.history_view().undo.is_some(), "one history step");
    w.ctl.toast_action();
    assert_eq!(w.ctl.view().rows.len(), 4, "one Undo takes both back");
    assert!(w.ctl.history_view().undo.is_none(), "exactly one step");
}

#[test]
fn a_pasted_png_file_becomes_an_image_item() {
    let dir = tempfile::tempdir().expect("dir");
    let mut w = world();
    let png = tiny_png();
    w.ctl.services.clipboard.held_files = files_in(&dir, &[("shot.png", &png)]);
    w.ctl.paste_create(Origin::Panel);
    let id = w.ctl.view().rows[0].id;
    assert_eq!(w.ctl.store().item(id).expect("item").kind, ItemKind::Image);
}

#[test]
fn a_pasted_screenshot_adds_an_image_item() {
    let mut w = world();
    w.ctl.services.clipboard.held_png = Some(tiny_png());
    w.ctl.paste_create(Origin::Panel);
    let view = w.ctl.view();
    assert_eq!(view.rows.len(), 5);
    assert_eq!(view.rows[0].kind, ItemKind::Image);
    assert!(view.toast.expect("toast").undo);
}

#[test]
fn text_on_the_clipboard_wins_over_a_picture_next_to_it() {
    let mut w = pasting("cells copied from a sheet");
    w.ctl.services.clipboard.held_png = Some(tiny_png());
    w.ctl.paste_create(Origin::Panel);
    assert_eq!(w.ctl.view().rows[0].kind, ItemKind::Text);
}

#[test]
fn a_file_over_the_limit_is_skipped_and_said() {
    let dir = tempfile::tempdir().expect("dir");
    let mut w = world();
    let mut paths = files_in(&dir, &[("ok.txt", b"fine")]);
    let big = dir.path().join("big.bin");
    let file = std::fs::File::create(&big).expect("big");
    file.set_len(crate::item_editor::max_attachment_bytes() + 1)
        .expect("sparse");
    paths.insert(0, big.to_string_lossy().into_owned());
    paths.push(dir.path().join("gone.txt").to_string_lossy().into_owned());
    w.ctl.services.clipboard.held_files = paths;
    w.ctl.paste_create(Origin::Panel);
    let view = w.ctl.view();
    assert_eq!(view.rows.len(), 5, "only the usable file was added");
    assert_eq!(view.rows[0].title, "ok.txt");
    assert_eq!(
        view.toast.expect("toast").message,
        ToastMessage::AddedSomeFiles {
            list: "Replies".into()
        }
    );
    // Nothing usable at all: nothing changes and the toast says why.
    let mut w = world();
    w.ctl.services.clipboard.held_files =
        vec![dir.path().join("gone.txt").to_string_lossy().into_owned()];
    w.ctl.paste_create(Origin::Panel);
    assert_eq!(w.ctl.view().rows.len(), 4);
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::FilesNotAdded
    );
    assert!(w.ctl.history_view().undo.is_none(), "no step for nothing");
}

#[test]
fn a_drop_to_a_given_list_targets_that_list() {
    let dir = tempfile::tempdir().expect("dir");
    let mut w = world();
    let paths = files_in(&dir, &[("dropped.txt", b"x")]);
    w.ctl.drop_create(&paths, Origin::Panel, Some(w.links_list));
    assert_eq!(items_in(&w, w.links_list), 2);
    assert_eq!(w.ctl.view().rows.len(), 4, "the shown list is untouched");
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::AddedTo {
            list: "Links".into(),
            count: 1
        }
    );
    // No list given: the list shown. A list that is gone falls back to the list shown too.
    w.ctl.drop_create(&paths, Origin::Panel, None);
    assert_eq!(w.ctl.view().rows.len(), 5);
    w.ctl.drop_create(
        &paths,
        Origin::Panel,
        Some(snipdeck_core::ListId::generate()),
    );
    assert_eq!(w.ctl.view().rows.len(), 6);
    assert!(w.ctl.history_view().undo.is_some());
}

#[test]
fn a_drag_over_the_window_names_the_list_until_it_ends() {
    let mut w = world();
    assert_eq!(w.ctl.drop_hint(), None);
    w.ctl.drop_hover(Origin::Panel, Some(w.links_list));
    assert_eq!(w.ctl.drop_hint(), Some(w.links_list));
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::DropTo {
            list: "Links".into()
        }
    );
    w.ctl.drop_end();
    assert_eq!(w.ctl.drop_hint(), None);
    assert!(w.ctl.view().toast.is_none(), "the hint goes with the drag");
    // A toast that is something else stays.
    w.ctl.paste_create(Origin::Panel);
    w.ctl.drop_end();
    assert!(w.ctl.view().toast.is_some());
}

#[test]
fn a_picture_over_the_attachment_cap_is_skipped_and_said() {
    let mut w = world();
    // The size is checked before the bytes are looked at, so these need not be a real PNG.
    let big = vec![0_u8; usize::try_from(crate::item_editor::max_attachment_bytes()).unwrap() + 1];
    w.ctl.services.clipboard.held_png = Some(big);
    w.ctl.paste_create(Origin::Panel);
    assert_eq!(w.ctl.view().rows.len(), 4);
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::FilesNotAdded
    );
    assert!(w.ctl.history_view().undo.is_none());
}

#[test]
fn a_dropped_folder_is_skipped_like_an_unusable_file() {
    let dir = tempfile::tempdir().expect("dir");
    let mut w = world();
    let mut paths = files_in(&dir, &[("ok.txt", b"fine")]);
    let folder = dir.path().join("sub");
    std::fs::create_dir(&folder).expect("folder");
    paths.push(folder.to_string_lossy().into_owned());
    w.ctl.drop_create(&paths, Origin::Panel, None);
    assert_eq!(w.ctl.view().rows.len(), 5);
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::AddedSomeFiles {
            list: "Replies".into()
        }
    );
}

#[test]
fn too_many_files_are_refused_with_their_own_message() {
    let mut w = world();
    w.ctl.services.clipboard.files_too_many = true;
    w.ctl.paste_create(Origin::Panel);
    assert_eq!(w.ctl.view().rows.len(), 4);
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::TooManyFiles
    );
    let many: Vec<String> = (0..=snipdeck_platform::win_formats::MAX_DROP_FILES)
        .map(|i| format!("f{i}"))
        .collect();
    w.ctl.drop_create(&many, Origin::Panel, None);
    assert_eq!(w.ctl.view().rows.len(), 4);
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::TooManyFiles
    );
}

// ---- Header Select and Delete, "Clear list…" (O50) ---------------------------------------------

fn select_or_delete_row() -> usize {
    APP_MENU
        .iter()
        .position(|e| *e == AppMenuEntry::SelectOrDelete)
        .expect("a Select or delete row in the Panel menu")
}

#[test]
fn select_mode_checkbox_marks_like_ctrl_click() {
    let mut w = world();
    w.ctl.panel_select_mode(true);
    assert!(w.ctl.view().select_mode);
    // A plain click on a row in Select mode toggles it, as Ctrl+click does.
    w.ctl.panel_mark(0, SelectMode::Replace);
    w.ctl.panel_mark(2, SelectMode::Replace);
    assert_eq!(marked(&w), [0, 2]);
    assert_eq!(
        w.ctl.view().marked_count,
        2,
        "the same marks drive the bulk bar"
    );
    w.ctl.panel_mark(0, SelectMode::Replace);
    assert_eq!(marked(&w), [2]);
    // Leaving the mode drops the marks.
    w.ctl.panel_select_mode(false);
    assert!(!w.ctl.view().select_mode);
    assert_eq!(marked(&w), Vec::<usize>::new());
}

#[test]
fn the_two_modes_exclude_each_other() {
    let mut w = world();
    w.ctl.panel_select_mode(true);
    w.ctl.panel_delete_mode(true);
    assert!(w.ctl.view().delete_mode);
    assert!(!w.ctl.view().select_mode);
    w.ctl.panel_select_mode(true);
    assert!(w.ctl.view().select_mode);
    assert!(!w.ctl.view().delete_mode);
}

#[test]
fn delete_mode_row_undo_restores_it() {
    let mut w = world();
    let before: Vec<String> = w.ctl.view().rows.iter().map(|r| r.title.clone()).collect();
    w.ctl.panel_delete_mode(true);
    w.ctl.panel_delete_row(1);
    let view = w.ctl.view();
    assert_eq!(view.rows.len(), 4, "the row stays, collapsed");
    assert!(view.rows[1].deleted_pending);
    assert_eq!(view.rows[1].title, before[1]);
    assert_eq!(view.rows.iter().filter(|r| r.deleted_pending).count(), 1);
    // Deleting the collapsed row again does nothing; the next live row is one step on.
    w.ctl.panel_delete_row(1);
    w.ctl.panel_delete_row(2);
    assert_eq!(
        w.ctl
            .view()
            .rows
            .iter()
            .map(|r| r.deleted_pending)
            .collect::<Vec<_>>(),
        [false, true, true, false]
    );
    // Undo on the first collapsed row brings back that one only, in its place.
    w.ctl.panel_undo_row(1);
    let view = w.ctl.view();
    let titles: Vec<String> = view.rows.iter().map(|r| r.title.clone()).collect();
    assert_eq!(view.rows.iter().filter(|r| r.deleted_pending).count(), 1);
    assert!(!view.rows[1].deleted_pending);
    assert_eq!(titles.len(), 4);
    assert!(titles.contains(&before[1]));
}

#[test]
fn leaving_delete_mode_drops_the_collapsed_rows() {
    let mut w = world();
    w.ctl.panel_delete_mode(true);
    w.ctl.panel_delete_row(0);
    assert_eq!(w.ctl.view().rows.len(), 4);
    w.ctl.panel_delete_mode(false);
    assert_eq!(w.ctl.view().rows.len(), 3);
    assert!(w.ctl.view().rows.iter().all(|r| !r.deleted_pending));
}

#[test]
fn esc_leaves_delete_mode() {
    let mut w = world();
    w.ctl.panel_delete_mode(true);
    w.ctl.panel_delete_row(0);
    w.ctl.panel_end_modes();
    assert!(!w.ctl.view().delete_mode);
    assert_eq!(w.ctl.view().rows.len(), 3);
    w.ctl.panel_select_mode(true);
    w.ctl.panel_mark(1, SelectMode::Toggle);
    w.ctl.panel_end_modes();
    assert!(!w.ctl.view().select_mode);
    assert_eq!(w.ctl.view().marked_count, 0);
}

#[test]
fn clear_list_asks_once_with_the_count() {
    let mut w = world();
    w.ctl.panel_delete_mode(true);
    w.ctl.panel_open_clear_list();
    let ask = w.ctl.view().clear_list.expect("the confirmation asks");
    assert_eq!(ask.count, 4);
    assert_eq!(ask.name, "Replies");
    assert_eq!(
        w.ctl.view().rows.len(),
        4,
        "nothing is deleted before the answer"
    );
    w.ctl.cancel_clear_list();
    assert!(w.ctl.view().clear_list.is_none());
    assert_eq!(w.ctl.view().rows.len(), 4);
}

#[test]
fn clear_list_is_one_step_and_undoes_whole() {
    let mut w = world();
    w.ctl.panel_delete_mode(true);
    w.ctl.panel_open_clear_list();
    w.ctl.panel_clear_list();
    let view = w.ctl.view();
    assert!(view.clear_list.is_none());
    assert_eq!(view.rows.len(), 0);
    let toast = view.toast.expect("an Undo toast");
    assert_eq!(toast.message, ToastMessage::ItemsTrashed { count: 4 });
    assert!(toast.undo, "the toast offers Undo");
    w.ctl.undo();
    assert_eq!(
        w.ctl.view().rows.len(),
        4,
        "one Undo brings every item back"
    );
}

#[test]
fn clear_list_on_an_empty_list_asks_nothing() {
    let mut w = world();
    w.ctl.select_tab(1);
    w.ctl.panel_mark_all();
    w.ctl.bulk_delete();
    w.ctl.panel_delete_mode(true);
    w.ctl.panel_open_clear_list();
    assert!(w.ctl.view().clear_list.is_none());
}

#[test]
fn the_kebab_row_select_or_delete_opens_select_mode() {
    let mut w = world();
    w.ctl.open_app_menu();
    w.ctl.choose_app_menu(select_or_delete_row());
    assert!(w.ctl.view().select_mode);
    assert!(w.ctl.view().app_menu.is_none(), "the menu closed");
}

fn shown(w: &World) -> Vec<(String, bool)> {
    w.ctl
        .view()
        .rows
        .iter()
        .map(|r| (r.title.clone(), r.deleted_pending))
        .collect()
}

#[test]
fn up_and_down_skip_the_collapsed_rows() {
    let mut w = world();
    w.ctl.panel_delete_mode(true);
    w.ctl.panel_delete_row(1);
    assert_eq!(w.ctl.view().selected, 2, "the cursor went to the next row");
    w.ctl.move_selection(-1);
    assert_eq!(
        w.ctl.view().selected,
        0,
        "Up steps back over the collapsed row"
    );
    w.ctl.move_selection(1);
    assert_eq!(w.ctl.view().selected, 2, "and Down steps over it");
}

#[test]
fn the_cursor_never_rests_on_a_collapsed_row() {
    let mut w = world();
    w.ctl.panel_delete_mode(true);
    w.ctl.panel_delete_row(3);
    assert_eq!(
        w.ctl.view().selected,
        2,
        "the previous row when the last one went"
    );
    w.ctl.panel_delete_row(0);
    let v = w.ctl.view();
    assert!(!v.rows[v.selected].deleted_pending);
    assert_eq!(v.selected, 1);
}

#[test]
fn an_action_on_the_row_below_a_collapsed_one_acts_on_that_row() {
    let mut w = world();
    w.ctl.panel_delete_mode(true);
    w.ctl.panel_delete_row(1);
    let below = w.ctl.view().rows[2].id;
    assert_eq!(w.ctl.edit_row(2), vec![Effect::EditItem(below)]);
    assert!(
        w.ctl.edit_row(1).is_empty(),
        "a collapsed row refuses actions"
    );
    // The row menu on the row below, then Delete in it, trashes that very item.
    w.ctl.open_item_menu(2, false);
    let at = w
        .ctl
        .view()
        .item_menu
        .expect("menu")
        .iter()
        .position(|e| *e == ItemMenuEntry::Trash)
        .expect("Delete row");
    w.ctl.choose_item_menu(at);
    assert!(
        w.ctl.view().rows.iter().all(|r| r.id != below),
        "that item went"
    );
    assert_eq!(w.ctl.view().rows.len(), 3);
    w.ctl.open_item_menu(1, false);
    assert!(
        w.ctl.view().item_menu.is_none(),
        "no menu on a collapsed row"
    );
}

#[test]
fn quick_numbers_skip_the_collapsed_rows() {
    let mut w = world();
    w.ctl.panel_delete_mode(true);
    w.ctl.panel_delete_row(0);
    let numbers: Vec<u8> = w.ctl.view().rows.iter().map(|r| r.number).collect();
    assert_eq!(numbers, [0, 1, 2, 3], "the row numbers do not skip");
    let first_live = w.ctl.view().rows[1].id;
    w.ctl.quick_use(1);
    assert_eq!(
        w.ctl
            .view()
            .rows
            .iter()
            .find(|r| r.id == first_live)
            .map(|r| r.number),
        Some(1)
    );
    assert!(w.ctl.view().rows[0].deleted_pending);
}

#[test]
fn collapsed_rows_keep_their_order_whatever_is_deleted_first() {
    let mut w = world();
    let before: Vec<String> = shown(&w).into_iter().map(|(t, _)| t).collect();
    w.ctl.panel_delete_mode(true);
    w.ctl.panel_delete_row(2);
    w.ctl.panel_delete_row(1);
    let rows = shown(&w);
    assert_eq!(
        rows.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>(),
        before,
        "the same order as before"
    );
    assert_eq!(
        rows.iter().map(|(_, d)| *d).collect::<Vec<_>>(),
        [false, true, true, false]
    );
    w.ctl.panel_delete_row(0);
    assert_eq!(shown(&w).len(), 4);
    assert_eq!(
        shown(&w).iter().map(|(_, d)| *d).collect::<Vec<_>>(),
        [true, true, true, false]
    );
}

#[test]
fn collapsed_rows_belong_to_their_list_and_the_mode_ends_with_a_list_change() {
    let mut w = world();
    w.ctl.panel_delete_mode(true);
    w.ctl.panel_delete_row(0);
    w.ctl.select_tab(1);
    assert!(!w.ctl.view().delete_mode, "another list ends Delete mode");
    assert!(w.ctl.view().rows.iter().all(|r| !r.deleted_pending));
    w.ctl.select_tab(0);
    assert_eq!(w.ctl.view().rows.len(), 3, "the old collapsed row is gone");
    w.ctl.panel_delete_mode(true);
    w.ctl.panel_delete_row(0);
    w.ctl.add_list();
    assert!(!w.ctl.view().delete_mode, "a new list ends it too");
    assert!(w.ctl.view().rows.iter().all(|r| !r.deleted_pending));
}

#[test]
fn hiding_the_panel_ends_delete_mode() {
    let mut w = world();
    w.ctl.panel_delete_mode(true);
    w.ctl.panel_delete_row(0);
    let _ = w.ctl.hide();
    assert!(!w.ctl.view().delete_mode);
    assert_eq!(w.ctl.view().rows.len(), 3);
}

#[test]
fn undo_on_a_collapsed_row_works_when_the_history_step_is_gone() {
    let mut w = world();
    w.ctl.panel_delete_mode(true);
    w.ctl.panel_delete_row(1);
    w.ctl.clear_history();
    w.ctl.panel_undo_row(1);
    let rows = shown(&w);
    assert_eq!(rows.len(), 4);
    assert!(
        rows.iter().all(|(_, d)| !d),
        "the item came back from the Trash"
    );
    assert!(w.ctl.view().toast.is_some(), "and it said so");
}

#[test]
fn a_delete_during_a_search_offers_the_undo_toast() {
    let mut w = world();
    w.ctl.set_query("a");
    let rows = w.ctl.view().rows;
    let gone = rows.first().expect("the search finds a row").id;
    w.ctl.panel_delete_mode(true);
    w.ctl.panel_delete_row(0);
    let view = w.ctl.view();
    assert!(view.rows.iter().all(|r| r.id != gone && !r.deleted_pending));
    let toast = view.toast.expect("a toast says it");
    assert_eq!(toast.message, ToastMessage::ItemsTrashed { count: 1 });
    assert!(toast.undo, "with Undo");
    w.ctl.undo();
    w.ctl.set_query("a");
    assert!(
        w.ctl.view().rows.iter().any(|r| r.id == gone),
        "Undo brings it back"
    );
}

#[test]
fn after_a_delete_the_focus_goes_to_the_next_row_or_the_list() {
    let mut w = world();
    w.ctl.panel_delete_mode(true);
    w.ctl.panel_delete_row(1);
    assert_eq!(
        w.ctl.view().focus_row,
        Some(2),
        "the next row's delete button"
    );
    let seq = w.ctl.view().focus_seq;
    w.ctl.panel_delete_row(3);
    assert_eq!(
        w.ctl.view().focus_row,
        Some(2),
        "the previous live row at the end"
    );
    assert!(w.ctl.view().focus_seq > seq);
    w.ctl.panel_delete_row(2);
    assert_eq!(w.ctl.view().focus_row, Some(0), "the one row left");
    w.ctl.panel_delete_row(0);
    assert_eq!(w.ctl.view().focus_row, None, "nothing left: the list");
}

#[test]
fn hiding_the_panel_ends_select_mode_and_clears_the_marks() {
    let mut w = world();
    w.ctl.panel_select_mode(true);
    w.ctl.panel_mark(0, SelectMode::Toggle);
    w.ctl.panel_mark(2, SelectMode::Toggle);
    assert_eq!(w.ctl.view().marked_count, 2);
    let _ = w.ctl.hide();
    assert!(!w.ctl.view().select_mode);
    assert_eq!(w.ctl.view().marked_count, 0);
}

// ---- Groups ---------------------------------------------------------------------------------------

fn group_first_two(w: &mut World) {
    w.ctl.panel_mark(0, SelectMode::Toggle);
    w.ctl.panel_mark(1, SelectMode::Toggle);
    w.ctl.group_marked(0);
}

fn headers(w: &World) -> Vec<usize> {
    w.ctl
        .view()
        .rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.header.is_some())
        .map(|(i, _)| i)
        .collect()
}

#[test]
fn group_from_selection_is_one_undo_step() {
    let mut w = world();
    group_first_two(&mut w);
    let view = w.ctl.view();
    assert_eq!(view.rows.len(), 5, "a header joins the four items");
    let header = view.rows[0].header.clone().expect("the header is first");
    assert_eq!(header.count, 2);
    assert!(view.rows[1].grouped && view.rows[2].grouped && !view.rows[3].grouped);
    assert_eq!(view.rename_row, Some(0), "the new header starts renaming");
    w.ctl.undo();
    assert_eq!(headers(&w), Vec::<usize>::new(), "one Undo ungroups");
    assert_eq!(w.ctl.view().rows.len(), 4);
    w.ctl.redo();
    assert_eq!(headers(&w), [0]);
}

#[test]
fn delete_group_keep_items_ungroups() {
    let mut w = world();
    group_first_two(&mut w);
    w.ctl.group_ungroup(0);
    assert_eq!(headers(&w), Vec::<usize>::new());
    assert_eq!(w.ctl.view().rows.len(), 4, "the items stay");
    w.ctl.undo();
    assert_eq!(headers(&w), [0], "Undo brings the header back");
}

#[test]
fn delete_group_with_its_items_is_one_undo() {
    let mut w = world();
    group_first_two(&mut w);
    w.ctl.group_delete(0);
    assert_eq!(w.ctl.view().rows.len(), 2);
    assert!(w.ctl.view().toast.expect("toast").undo);
    w.ctl.undo();
    assert_eq!(w.ctl.view().rows.len(), 5, "items and header are back");
}

#[test]
fn collapsed_group_hides_rows_but_search_shows_them() {
    let mut w = world();
    let title = w.ctl.view().rows[0].title.clone();
    group_first_two(&mut w);
    w.ctl.group_collapse(0, Some(true));
    assert_eq!(w.ctl.view().rows.len(), 3, "header and the two others");
    assert!(w.ctl.view().rows[0].header.as_ref().unwrap().collapsed);
    w.ctl.set_query(&title);
    let rows = w.ctl.view().rows;
    let at = rows
        .iter()
        .position(|r| r.title == title && r.header.is_none())
        .expect("a search finds the item inside the collapsed group");
    assert!(at > 0, "the match is not first");
    let header = rows[at - 1]
        .header
        .as_ref()
        .expect("the header sits before the match");
    assert!(!header.collapsed, "the header is open in results");
    assert!(rows[at].grouped, "the match is drawn under the header");
    w.ctl.set_query("");
    w.ctl.group_collapse(0, Some(false));
    assert_eq!(w.ctl.view().rows.len(), 5);
}

#[test]
fn quick_numbers_skip_headers() {
    let mut w = world();
    group_first_two(&mut w);
    let numbers: Vec<u8> = w.ctl.view().rows.iter().map(|r| r.number).collect();
    assert_eq!(numbers, [0, 1, 2, 3, 4], "the header takes no number");
    w.ctl.group_collapse(0, Some(true));
    let numbers: Vec<u8> = w.ctl.view().rows.iter().map(|r| r.number).collect();
    assert_eq!(numbers, [0, 1, 2], "numbers count the rows shown");
}

#[test]
fn renaming_a_group_is_undoable_and_a_header_is_never_marked() {
    let mut w = world();
    group_first_two(&mut w);
    w.ctl.group_rename(0, "Work");
    assert_eq!(w.ctl.view().rows[0].title, "Work");
    w.ctl.undo();
    assert_ne!(w.ctl.view().rows[0].title, "Work");
    w.ctl.panel_mark_all();
    assert_eq!(w.ctl.view().marked_count, 4, "only items are marked");
    w.ctl.panel_mark(0, SelectMode::Toggle);
    assert_eq!(w.ctl.view().marked_count, 4);
}

/// The shown rows as text: a header is `[name]`, an item is its title.
fn shape(w: &World) -> Vec<String> {
    w.ctl
        .view()
        .rows
        .iter()
        .map(|r| {
            if r.header.is_some() {
                format!("[{}]", r.title)
            } else {
                r.title.clone()
            }
        })
        .collect()
}

/// Two groups: `[Group] a b [Second] c d` (the item titles come from the fixture).
fn two_groups(w: &mut World) {
    group_first_two(w);
    w.ctl.panel_mark(3, SelectMode::Toggle);
    w.ctl.panel_mark(4, SelectMode::Toggle);
    w.ctl.group_marked(3);
    w.ctl.group_rename(3, "Second");
}

#[test]
fn dragging_a_group_down_onto_the_next_group_moves_it_below() {
    let mut w = world();
    two_groups(&mut w);
    let before = shape(&w);
    w.ctl.panel_drop_row(0, 3);
    let after = shape(&w);
    assert_eq!(
        after,
        [&before[3..], &before[..3]].concat(),
        "the whole first group sits below the second"
    );
    assert!(w.ctl.view().toast.is_none(), "no failure toast");
}

#[test]
fn dragging_a_group_up_onto_the_group_above_moves_it_above() {
    let mut w = world();
    two_groups(&mut w);
    let before = shape(&w);
    w.ctl.panel_drop_row(3, 0);
    assert_eq!(shape(&w), [&before[3..], &before[..3]].concat());
}

#[test]
fn dragging_a_group_to_the_top_and_to_the_end() {
    let mut w = world();
    two_groups(&mut w);
    let before = shape(&w);
    // Onto the last item: the group goes to the end.
    w.ctl.panel_drop_row(0, 5);
    assert_eq!(shape(&w), [&before[3..], &before[..3]].concat());
    // Back to the top: second group (now first) is row 0; drag the first-group (row 3) onto row 1.
    w.ctl.panel_drop_row(3, 1);
    assert_eq!(
        shape(&w),
        before,
        "dropped onto the item at the top it goes above it"
    );
}

#[test]
fn a_group_drag_is_one_undo_step_and_redo_repeats_it() {
    let mut w = world();
    two_groups(&mut w);
    let before = shape(&w);
    w.ctl.panel_drop_row(0, 3);
    let moved = shape(&w);
    assert_ne!(moved, before);
    w.ctl.undo();
    assert_eq!(shape(&w), before, "Undo puts the group back");
    w.ctl.redo();
    assert_eq!(shape(&w), moved);
}

#[test]
fn an_item_dropped_on_a_header_joins_and_undo_takes_it_out() {
    let mut w = world();
    group_first_two(&mut w);
    let before = shape(&w);
    // Row 4 is the last item; drop it on the header.
    w.ctl.panel_drop_row(4, 0);
    assert_eq!(
        w.ctl.view().rows[0].header.as_ref().expect("header").count,
        3
    );
    w.ctl.undo();
    assert_eq!(shape(&w), before);
    assert_eq!(
        w.ctl.view().rows[0].header.as_ref().expect("header").count,
        2
    );
}

#[test]
fn undoing_ctrl_g_on_apart_rows_puts_them_back_and_redo_groups_again() {
    let mut w = world();
    let before = shape(&w);
    // Rows 0 and 2 are not neighbours.
    w.ctl.panel_mark(0, SelectMode::Toggle);
    w.ctl.panel_mark(2, SelectMode::Toggle);
    w.ctl.group_marked(0);
    let grouped = shape(&w);
    assert_eq!(grouped[1], before[0]);
    assert_eq!(grouped[2], before[2], "gathered under the topmost");
    w.ctl.undo();
    assert_eq!(shape(&w), before, "same order and no header");
    w.ctl.redo();
    assert_eq!(shape(&w), grouped);
}

/// Like `shape`, but a member is shown as `+title`, so membership is compared too.
fn members_shape(w: &World) -> Vec<String> {
    w.ctl
        .view()
        .rows
        .iter()
        .map(|r| match (&r.header, r.grouped) {
            (Some(h), _) => format!("[{}:{}]", r.title, h.count),
            (None, true) => format!("+{}", r.title),
            (None, false) => r.title.clone(),
        })
        .collect()
}

#[test]
fn undoing_a_drop_on_a_header_keeps_an_item_below_another_group_ungrouped() {
    let mut w = world();
    group_first_two(&mut w);
    // `[Group] a b c [Second] d`: c is ungrouped, right below the first group.
    w.ctl.panel_mark(4, SelectMode::Toggle);
    w.ctl.group_marked(4);
    w.ctl.group_rename(4, "Second");
    let before = members_shape(&w);
    assert!(
        !before[3].starts_with('+'),
        "c starts ungrouped: {before:?}"
    );
    w.ctl.panel_drop_row(3, 4);
    let joined = members_shape(&w);
    assert_ne!(joined, before);
    w.ctl.undo();
    assert_eq!(members_shape(&w), before, "c is ungrouped at its old place");
    w.ctl.redo();
    assert_eq!(members_shape(&w), joined);
    w.ctl.undo();
    assert_eq!(members_shape(&w), before);
}

#[test]
fn undoing_ctrl_g_puts_a_member_of_another_group_back_in_it() {
    let mut w = world();
    group_first_two(&mut w);
    let before = members_shape(&w);
    // b (row 2, in the first group) and d (row 4) become a new group.
    w.ctl.panel_mark(2, SelectMode::Toggle);
    w.ctl.panel_mark(4, SelectMode::Toggle);
    w.ctl.group_marked(2);
    let grouped = members_shape(&w);
    assert_ne!(grouped, before);
    w.ctl.undo();
    assert_eq!(members_shape(&w), before, "b is back in the first group");
    w.ctl.redo();
    assert_eq!(members_shape(&w), grouped);
    w.ctl.undo();
    assert_eq!(members_shape(&w), before);
}

#[test]
fn the_rename_request_follows_its_group_and_ends() {
    let mut w = world();
    group_first_two(&mut w);
    assert_eq!(w.ctl.view().rename_row, Some(0));
    let seq = w.ctl.view().rename_seq;
    // The header moves down a row; the request is still on it.
    w.ctl.panel_mark(0, SelectMode::Toggle);
    w.ctl.panel_drop_row(3, 1);
    let at = headers(&w)[0];
    assert_eq!(
        w.ctl.view().rename_row,
        Some(at),
        "keyed by group, not by row"
    );
    assert_eq!(w.ctl.view().rename_seq, seq);
    w.ctl.group_rename_ended();
    assert_eq!(w.ctl.view().rename_row, None, "spent once the rename ends");
}

#[test]
fn ctrl_g_on_a_header_renames_its_group() {
    let mut w = world();
    group_first_two(&mut w);
    w.ctl.group_rename_ended();
    w.ctl.group_marked(0);
    assert_eq!(headers(&w), [0], "no second group");
    assert_eq!(w.ctl.view().rename_row, Some(0));
}
