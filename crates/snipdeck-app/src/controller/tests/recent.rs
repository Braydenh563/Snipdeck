//! The Recent inbox through the controller: on for a new install, off after an opt-out, what a clipboard change does with
//! the opt-in, pause, and promoting to a list. The clock is the fixed one the other controller
//! tests use.

use super::*;
use crate::recent::{ClipboardObservation, DEFAULT_MAX_ENTRIES};

fn on() -> Settings {
    Settings {
        recent_history: true,
        no_history_apps: vec!["keepass.exe".to_owned()],
        ..Settings::default()
    }
}

/// A world whose user switched Recent off (a new install has it on).
fn off_world() -> World {
    world_with(
        None,
        Settings {
            recent_history: false,
            ..Settings::default()
        },
    )
}

/// The inbox on, with key protection (the test provider stands in for DPAPI): the inbox is
/// sealed with the library key, so nothing is kept without one.
pub(super) fn recent_world() -> World {
    let mut w = world_with(None, on());
    w.ctl.set_key_provider(Box::new(
        snipdeck_store::crypto::MemoryKeyProvider::new().expect("provider"),
    ));
    w
}

pub(super) fn copied(text: &str) -> ClipboardObservation {
    ClipboardObservation {
        text: Some(text.to_owned()),
        source_app: Some("notepad.exe".to_owned()),
        ..ClipboardObservation::default()
    }
}

#[test]
fn nothing_is_kept_until_the_user_opts_in() {
    let mut w = world();
    w.ctl.on_clipboard_change(&copied("secret plan"));
    assert_eq!(w.ctl.recent_entries().len(), 0);
    assert_eq!(w.ctl.store().recent().expect("recent").len(), 0);
}

#[test]
fn with_the_opt_in_a_copy_is_kept_and_a_recopy_moves_to_the_top() {
    let mut w = recent_world();
    w.ctl.on_clipboard_change(&copied("one"));
    w.clock.set(at("2026-10-17T10:31:00Z"));
    w.ctl.on_clipboard_change(&copied("two"));
    w.clock.set(at("2026-10-17T10:32:00Z"));
    w.ctl.on_clipboard_change(&copied("one"));
    let texts: Vec<_> = w.ctl.recent_entries().into_iter().map(|e| e.text).collect();
    assert_eq!(texts, ["one", "two"]);
}

#[test]
fn pause_stops_collecting_until_resumed() {
    let mut w = recent_world();
    w.ctl.set_recent_paused(true);
    w.ctl.on_clipboard_change(&copied("while paused"));
    assert_eq!(w.ctl.recent_entries().len(), 0);
    w.ctl.set_recent_paused(false);
    w.ctl.on_clipboard_change(&copied("after"));
    assert_eq!(w.ctl.recent_entries().len(), 1);
}

#[test]
fn an_excluded_app_and_an_exclusion_format_are_never_kept() {
    let mut w = recent_world();
    let mut from_keepass = copied("hunter2");
    from_keepass.source_app = Some("KeePass.exe".to_owned());
    w.ctl.on_clipboard_change(&from_keepass);
    let mut flagged = copied("token");
    flagged.excluded_format = true;
    w.ctl.on_clipboard_change(&flagged);
    assert_eq!(w.ctl.recent_entries().len(), 0);
}

#[test]
fn switching_the_inbox_off_hides_what_was_kept() {
    let mut w = recent_world();
    w.ctl.on_clipboard_change(&copied("kept"));
    assert_eq!(w.ctl.recent_entries().len(), 1);
    w.ctl.update_settings(|s| s.recent_history = false);
    assert_eq!(w.ctl.recent_entries().len(), 0);
}

#[test]
fn the_cap_holds_at_the_default() {
    let mut w = recent_world();
    for i in 0..=DEFAULT_MAX_ENTRIES {
        w.ctl.on_clipboard_change(&copied(&format!("t{i}")));
    }
    assert_eq!(w.ctl.recent_entries().len(), DEFAULT_MAX_ENTRIES);
}

#[test]
fn promoting_saves_an_item_at_the_top_of_the_list_and_keeps_the_entry() {
    let mut w = recent_world();
    w.ctl.on_clipboard_change(&copied("promote me"));
    let id = w.ctl.recent_entries()[0].id;
    let before = w.ctl.store().items(w.links_list).expect("items").len();
    w.ctl.recent_promote(id, w.links_list);
    let items = w.ctl.store().items(w.links_list).expect("items");
    assert_eq!(items.len(), before + 1);
    assert_eq!(items[0].preview, "promote me");
    assert_eq!(w.ctl.recent_entries().len(), 1);
}

#[test]
fn delete_and_clear_remove_entries() {
    let mut w = recent_world();
    w.ctl.on_clipboard_change(&copied("a"));
    w.ctl.on_clipboard_change(&copied("b"));
    let first = w.ctl.recent_entries()[0].id;
    w.ctl.recent_delete(first);
    assert_eq!(w.ctl.recent_entries().len(), 1);
    assert_eq!(w.ctl.recent_clear(), 1);
    assert_eq!(w.ctl.recent_entries().len(), 0);
}

#[test]
fn the_listener_gate_follows_the_opt_in_and_the_pause() {
    use snipdeck_platform::clipboard_watch::WatchGate;
    let gate = WatchGate::new();
    let mut w = off_world();
    w.ctl.set_recent_gate(gate.clone());
    assert!(!gate.is_open(), "switched off: nothing is read");
    w.ctl.update_settings(|s| s.recent_history = true);
    assert!(gate.is_open());
    w.ctl.set_recent_paused(true);
    assert!(!gate.is_open());
    w.ctl.set_recent_paused(false);
    assert!(gate.is_open());
    w.ctl.update_settings(|s| s.recent_history = false);
    assert!(!gate.is_open());
}

#[test]
fn the_view_has_no_recent_part_until_the_opt_in() {
    let w = off_world();
    assert!(w.ctl.view().recent.is_none());
    let w = recent_world();
    let recent = w.ctl.view().recent.expect("on");
    assert!(!recent.open);
    assert_eq!(recent.rows.len(), 0);
}

#[test]
fn the_view_lists_entries_newest_first_with_the_lists_to_save_to() {
    let mut w = recent_world();
    w.ctl.on_clipboard_change(&copied("alpha"));
    w.clock.set(at("2026-10-17T10:35:00Z"));
    w.ctl.on_clipboard_change(&copied("beta"));
    w.clock.set(at("2026-10-17T10:40:00Z"));
    // A closed inbox lists nothing (and decrypts nothing); the clock tab opens it.
    let closed = w.ctl.view().recent.expect("on");
    assert!(closed.rows.is_empty() && closed.targets.is_empty());
    w.ctl.set_recent_open(true);
    let recent = w.ctl.view().recent.expect("on");
    let rows: Vec<_> = recent
        .rows
        .iter()
        .map(|r| (r.preview.as_str(), r.age_minutes))
        .collect();
    assert_eq!(rows, [("beta", 5), ("alpha", 10)]);
    assert_eq!(recent.rows[0].app, "Notepad");
    let targets: Vec<_> = recent.targets.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(targets, ["Replies", "Links"]);
}

#[test]
fn the_search_box_narrows_the_recent_list_and_rows_act_on_what_is_shown() {
    let mut w = recent_world();
    w.ctl.on_clipboard_change(&copied("alpha one"));
    w.ctl.on_clipboard_change(&copied("beta two"));
    w.ctl.set_query("beta");
    w.ctl.set_recent_open(true);
    let recent = w.ctl.view().recent.expect("on");
    assert_eq!(recent.rows.len(), 1);
    // Row 0 of the narrowed list is "beta two": saving it saves that text.
    w.ctl.recent_promote_to(recent.rows[0].id, 1);
    let items = w.ctl.store().items(w.links_list).expect("items");
    assert_eq!(items[0].preview, "beta two");
}

#[test]
fn clicking_an_entry_puts_its_text_on_the_clipboard_and_deleting_removes_it() {
    let mut w = recent_world();
    w.ctl.on_clipboard_change(&copied("copy me"));
    w.ctl.set_recent_open(true);
    let id = w.ctl.view().recent.expect("on").rows[0].id;
    w.ctl.recent_copy(id);
    let written = &w.ctl.services().clipboard.written;
    assert_eq!(written.len(), 1);
    w.ctl.recent_delete(id);
    assert_eq!(w.ctl.recent_entries().len(), 0);
    // An id that is gone does nothing.
    w.ctl.recent_copy(id);
    w.ctl.recent_delete(id);
    w.ctl.recent_promote_to(id, 0);
}

fn recent_world_at_outlook() -> World {
    let mut w = world_with(Some(outlook()), on());
    w.ctl.set_key_provider(Box::new(
        snipdeck_store::crypto::MemoryKeyProvider::new().expect("provider"),
    ));
    w
}

#[test]
fn using_an_entry_pastes_it_into_the_target_and_shift_only_copies() {
    let mut w = recent_world_at_outlook();
    w.ctl.on_clipboard_change(&copied("paste me"));
    let id = w.ctl.recent_entries()[0].id;
    w.ctl.recent_use(id, UseMode::Paste);
    let services = w.ctl.services();
    assert_eq!(services.keyboard.sent.len(), 1, "Ctrl+V went out");
    assert!(
        services
            .clipboard
            .log
            .iter()
            .any(|l| l.starts_with("transient")),
        "written without the history markers: {:?}",
        services.clipboard.log
    );
    assert_eq!(services.clipboard.written.len(), 1);
    // Shift: copy only, no key.
    w.ctl.recent_use(id, UseMode::Copy);
    let services = w.ctl.services();
    assert_eq!(services.keyboard.sent.len(), 1, "no second paste");
    assert_eq!(services.clipboard.written.len(), 2);
    // A gone id does nothing.
    w.ctl.recent_delete(id);
    w.ctl.recent_use(id, UseMode::Paste);
    assert_eq!(w.ctl.services().keyboard.sent.len(), 1);
}

#[test]
fn a_recent_use_closes_the_panel_like_an_item_use_and_a_gone_entry_does_not() {
    let mut w = recent_world_at_outlook();
    w.ctl.update_settings(|s| s.panel_hide_after_use = true);
    w.ctl.set_on_top(false);
    w.ctl.on_hotkey(HotkeyAction::TogglePanel);
    w.ctl.on_hotkey(HotkeyAction::TogglePanel);
    assert!(w.ctl.visible());
    w.ctl.on_clipboard_change(&copied("close after me"));
    let id = w.ctl.recent_entries()[0].id;
    let used = w.ctl.recent_use(id, UseMode::Paste);
    assert_eq!(used, UseResult::Done(snipdeck_core::UseOutcome::Pasted));
    assert_eq!(w.ctl.after_use(&used), [Effect::HidePanel]);
    w.ctl.recent_delete(id);
    let gone = w.ctl.recent_use(id, UseMode::Paste);
    assert_eq!(gone, UseResult::Failed);
    assert_eq!(w.ctl.after_use(&gone), Vec::new());
}

#[test]
fn using_an_entry_with_no_target_copies_it() {
    let mut w = recent_world();
    w.targets.set(None);
    w.ctl.on_clipboard_change(&copied("no target"));
    let id = w.ctl.recent_entries()[0].id;
    w.ctl.recent_use(id, UseMode::Paste);
    assert_eq!(
        w.ctl.services().keyboard.sent,
        Vec::<Vec<snipdeck_platform::keys::KeyEvent>>::new()
    );
    assert_eq!(w.ctl.services().clipboard.written.len(), 1);
}

#[test]
fn actions_follow_the_entry_id_when_the_list_shifts_under_an_open_menu() {
    let mut w = recent_world();
    w.ctl.on_clipboard_change(&copied("original"));
    w.ctl.set_recent_open(true);
    let id = w.ctl.view().recent.expect("on").rows[0].id;
    // The menu is open on "original"; a new copy pushes it down a row.
    w.ctl.on_clipboard_change(&copied("newcomer"));
    let rows = w.ctl.view().recent.expect("on").rows;
    assert_eq!(rows[1].id, id, "the entry moved");
    w.ctl.recent_delete(id);
    let left: Vec<_> = w
        .ctl
        .recent_entries()
        .iter()
        .map(|e| e.text.clone())
        .collect();
    assert_eq!(left, ["newcomer"], "Remove hit the original entry");
}

#[test]
fn the_clock_tab_cannot_open_while_history_is_off() {
    let mut w = off_world();
    w.ctl.set_recent_open(true);
    w.ctl.update_settings(|s| s.recent_history = true);
    assert!(!w.ctl.view().recent.expect("on").open);
}

#[test]
fn the_main_window_lists_the_entries_and_counts_them() {
    let mut w = recent_world();
    w.ctl.on_clipboard_change(&copied("alpha one"));
    w.clock.set(at("2026-10-17T10:35:00Z"));
    w.ctl.on_clipboard_change(&copied("beta two"));
    w.clock.set(at("2026-10-17T10:40:00Z"));
    // The count is real in every section; the rows only in the Recent section.
    let screen = w.ctl.main_screen();
    assert_eq!(screen.counts.recent, 2);
    assert_eq!(screen.recent_rows.len(), 0);
    w.ctl.main_open(crate::main_window::Section::Recent);
    let screen = w.ctl.main_screen();
    let rows: Vec<_> = screen
        .recent_rows
        .iter()
        .map(|r| (r.preview.as_str(), r.app.as_str(), r.age_minutes))
        .collect();
    assert_eq!(
        rows,
        [("beta two", "Notepad", 5), ("alpha one", "Notepad", 10)]
    );
    let targets: Vec<_> = screen
        .recent_targets
        .iter()
        .map(|t| t.label.as_str())
        .collect();
    assert_eq!(targets, ["Replies", "Links"]);
}

#[test]
fn the_main_window_search_narrows_recent_and_rows_act_on_what_is_shown() {
    let mut w = recent_world();
    w.ctl.on_clipboard_change(&copied("alpha one"));
    w.ctl.on_clipboard_change(&copied("beta two"));
    w.ctl.main_open(crate::main_window::Section::Recent);
    w.ctl.main_set_query("beta");
    let screen = w.ctl.main_screen();
    assert_eq!(screen.recent_rows.len(), 1);
    // The count stays the whole inbox.
    assert_eq!(screen.counts.recent, 2);
    let id = screen.recent_rows[0].id;
    w.ctl.recent_promote_to(id, 1);
    let items = w.ctl.store().items(w.links_list).expect("items");
    assert_eq!(items[0].preview, "beta two");
    w.ctl.recent_copy(id);
    assert_eq!(w.ctl.services().clipboard.written.len(), 1);
    w.ctl.recent_delete(id);
    assert_eq!(w.ctl.recent_entries().len(), 1);
    // An unknown id or target does nothing.
    w.ctl.recent_copy(id);
    w.ctl.recent_delete(id);
    w.ctl.recent_promote_to(id, 0);
    let other = w.ctl.recent_entries()[0].id;
    w.ctl.recent_promote_to(other, 99);
    assert_eq!(w.ctl.recent_entries().len(), 1);
}

#[test]
fn the_main_window_clear_empties_recent_and_off_shows_nothing() {
    let mut w = recent_world();
    w.ctl.on_clipboard_change(&copied("one"));
    w.ctl.main_open(crate::main_window::Section::Recent);
    assert_eq!(w.ctl.recent_clear(), 1);
    let screen = w.ctl.main_screen();
    assert_eq!(screen.counts.recent, 0);
    assert!(screen.recent_rows.is_empty() && screen.recent_on);
    w.ctl.on_clipboard_change(&copied("two"));
    w.ctl.update_settings(|s| s.recent_history = false);
    let screen = w.ctl.main_screen();
    assert_eq!((screen.counts.recent, screen.recent_rows.len()), (0, 0));
    assert!(!screen.recent_on);
}

#[test]
fn the_tray_menu_is_told_when_history_is_on_and_when_it_is_paused() {
    use std::{cell::RefCell, rc::Rc};
    let seen: Rc<RefCell<Vec<(bool, bool)>>> = Rc::default();
    let mut w = off_world();
    w.ctl.set_tray_history_sink({
        let seen = Rc::clone(&seen);
        move |available, paused| seen.borrow_mut().push((available, paused))
    });
    // Told at once: switched off, so no entry.
    assert_eq!(seen.borrow().last(), Some(&(false, false)));
    w.ctl.update_settings(|s| s.recent_history = true);
    assert_eq!(seen.borrow().last(), Some(&(true, false)));
    // The tray's own toggle pauses collecting and the sink hears the new state.
    assert_eq!(
        w.ctl
            .on_tray(snipdeck_platform::tray::TrayEvent::HistoryPaused(true))
            .len(),
        0
    );
    assert!(w.ctl.recent_paused());
    assert_eq!(seen.borrow().last(), Some(&(true, true)));
    w.ctl.on_clipboard_change(&copied("while paused"));
    assert_eq!(w.ctl.recent_entries().len(), 0);
    w.ctl
        .on_tray(snipdeck_platform::tray::TrayEvent::HistoryPaused(false));
    assert_eq!(seen.borrow().last(), Some(&(true, false)));
    // Turning the inbox off takes the entry away.
    w.ctl.update_settings(|s| s.recent_history = false);
    assert_eq!(seen.borrow().last(), Some(&(false, false)));
}

#[test]
fn a_copy_is_kept_sealed_and_nothing_is_kept_without_key_protection() {
    let mut w = recent_world();
    w.ctl.on_clipboard_change(&copied("sealed"));
    assert!(
        w.ctl.store().key_ring_exists().expect("ring"),
        "made on the first copy"
    );
    assert_eq!(w.ctl.recent_entries().len(), 1);

    // No key protection on the machine: nothing is written in plain text.
    let mut bare = world_with(None, on());
    bare.ctl.on_clipboard_change(&copied("unsealed"));
    assert_eq!(bare.ctl.recent_entries().len(), 0);
    assert_eq!(bare.ctl.store().recent().expect("recent").len(), 0);
}

/// A small RGBA PNG of one colour.
fn png_of(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("header");
    writer
        .write_image_data(&vec![90; (width * height * 4) as usize])
        .expect("data");
    writer.finish().expect("finish");
    out
}

#[test]
fn using_a_rich_entry_writes_text_html_rtf() {
    use snipdeck_platform::payload::Data;
    let mut w = recent_world();
    w.ctl.on_clipboard_change(&ClipboardObservation {
        html: Some("<b>bold</b>".to_owned()),
        rtf: Some(b"{\\rtf1 bold}".to_vec()),
        ..copied("bold")
    });
    let id = w.ctl.recent_entries()[0].id;
    w.ctl.recent_use(id, UseMode::Copy);
    let written = &w.ctl.services().clipboard.written;
    assert_eq!(written.len(), 1);
    assert_eq!(
        written[0].data(),
        [
            Data::Text("bold".to_owned()),
            Data::Html("<b>bold</b>".to_owned()),
            Data::Rtf(b"{\\rtf1 bold}".to_vec()),
        ]
    );
}

#[test]
fn promoting_an_image_makes_an_image_item() {
    use snipdeck_platform::payload::Data;
    let mut w = recent_world();
    let png = png_of(12, 8);
    w.ctl.on_clipboard_change(&ClipboardObservation {
        image: Some(png.clone()),
        source_app: Some("snippingtool.exe".to_owned()),
        ..ClipboardObservation::default()
    });
    let entries = w.ctl.recent_entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, snipdeck_store::RecentKind::Image);
    let before = w.ctl.store().items(w.links_list).expect("items").len();
    w.ctl.recent_promote(entries[0].id, w.links_list);
    let items = w.ctl.store().items(w.links_list).expect("items");
    assert_eq!(items.len(), before + 1);
    assert_eq!(items[0].kind, snipdeck_core::ItemKind::Image);
    assert_eq!(w.ctl.recent_entries().len(), 1, "the entry stays");
    // Using it puts the same PNG back.
    w.ctl.recent_copy(entries[0].id);
    assert_eq!(
        w.ctl.services().clipboard.written[0].data(),
        [Data::Png(png)]
    );
}

#[test]
fn a_moved_file_says_so() {
    let dir = tempfile::tempdir().expect("dir");
    let kept = dir.path().join("kept.txt");
    std::fs::write(&kept, b"still here").expect("write");
    let gone = dir.path().join("gone.txt");
    let mut w = recent_world();
    w.ctl.on_clipboard_change(&ClipboardObservation {
        files: vec![gone.to_string_lossy().into_owned()],
        ..ClipboardObservation::default()
    });
    let id = w.ctl.recent_entries()[0].id;
    assert_eq!(w.ctl.recent_use(id, UseMode::Copy), UseResult::Failed);
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::FileMoved
    );
    assert_eq!(w.ctl.services().clipboard.written.len(), 0);
    let before = w.ctl.store().items(w.links_list).expect("items").len();
    w.ctl.recent_promote(id, w.links_list);
    assert_eq!(
        w.ctl.store().items(w.links_list).expect("items").len(),
        before
    );
    // A file that is still there is read now and saved as a File item.
    w.ctl.on_clipboard_change(&ClipboardObservation {
        files: vec![kept.to_string_lossy().into_owned()],
        ..ClipboardObservation::default()
    });
    let id = w.ctl.recent_entries()[0].id;
    w.ctl.recent_promote(id, w.links_list);
    let items = w.ctl.store().items(w.links_list).expect("items");
    assert_eq!(items.len(), before + 1);
    assert_eq!(items[0].kind, snipdeck_core::ItemKind::File);
}

#[test]
fn some_missing_files_still_paste_the_rest_and_say_so() {
    use snipdeck_platform::payload::Data;
    let dir = tempfile::tempdir().expect("dir");
    let kept = dir.path().join("kept.txt");
    std::fs::write(&kept, b"still here").expect("write");
    let gone = dir.path().join("gone.txt");
    let mut w = recent_world();
    w.ctl.on_clipboard_change(&ClipboardObservation {
        files: vec![
            gone.to_string_lossy().into_owned(),
            kept.to_string_lossy().into_owned(),
        ],
        ..ClipboardObservation::default()
    });
    let id = w.ctl.recent_entries()[0].id;
    assert!(matches!(
        w.ctl.recent_use(id, UseMode::Copy),
        UseResult::Done(_)
    ));
    let written = &w.ctl.services().clipboard.written;
    assert_eq!(written.len(), 1);
    let Data::Files(files) = &written[0].data()[0] else {
        panic!("{:?}", written[0]);
    };
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "kept.txt");
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::FileMoved
    );
    // Saving does the same: the file that exists is saved, and the toast still says one is gone.
    let before = w.ctl.store().items(w.links_list).expect("items").len();
    w.ctl.recent_promote(id, w.links_list);
    assert_eq!(
        w.ctl.store().items(w.links_list).expect("items").len(),
        before + 1
    );
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::FileMoved
    );
}
