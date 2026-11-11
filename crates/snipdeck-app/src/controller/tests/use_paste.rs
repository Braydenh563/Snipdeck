//! Using an item: pasting, copying, the clipboard clear timer and the template fill form.

use super::*;

#[test]
fn the_view_shows_the_board_the_rows_and_where_a_click_goes() {
    let w = world();
    let view = w.ctl.view();
    assert_eq!(view.board_name, "Work");
    assert_eq!(view.tabs.len(), 2);
    assert_eq!(view.current_tab, 0);
    assert_eq!(view.rows.len(), 4);
    assert_eq!(view.rows[0].number, 1);
    assert_eq!(
        view.footer,
        Footer::PastesInto {
            app: "Outlook".into(),
            title: "New message".into()
        }
    );
    assert_eq!(view.use_label, UseLabel::Paste);
    assert!(view.on_top && !view.dark && view.toast.is_none() && view.board_menu.is_none());
}

#[test]
fn the_target_changing_updates_the_footer() {
    let mut w = world();
    w.targets.set(None);
    w.ctl.on_target_changed();
    let view = w.ctl.view();
    assert_eq!(view.footer, Footer::Copies);
    assert_eq!(view.use_label, UseLabel::Copy);
}

#[test]
fn activating_a_row_pastes_it_counts_the_use_and_toasts() {
    let mut w = world();
    let result = w.ctl.activate_row(1, UseMode::Paste);
    assert_eq!(result, UseResult::Done(UseOutcome::Pasted));
    let services = w.ctl.services();
    assert_eq!(
        services.clipboard.written[0].data(),
        [Data::Text("second".into())]
    );
    assert_eq!(
        services.keyboard.sent.len(),
        1,
        "one batch of key events: {:?}",
        services.keyboard.sent
    );
    assert!(
        services.keyboard.sent[0].contains(&KeyEvent {
            key: Key::V,
            down: true
        }),
        "Ctrl+V was injected"
    );
    assert!(services.timing.slept.contains(&REFOCUS_WAIT));
    assert_eq!(w.targets.refocus_count(), 1);
    assert_eq!(uses(&w, w.ids[1]), 1);
    assert_eq!(
        w.ctl.store().item(w.ids[1]).expect("item").last_used_at,
        Some(w.clock.now())
    );
    let toast = w.ctl.view().toast.expect("toast");
    assert_eq!(toast.kind, ToastKind::Success);
    assert_eq!(
        toast.message,
        ToastMessage::Pasted {
            app: "Outlook".into()
        }
    );
}

#[test]
fn a_template_fills_clipboard_from_what_the_user_copied() {
    let mut w = build(Opts {
        clipboard: Some("from the clipboard"),
        ..Opts::default()
    });
    let list = w.ctl.model.list().expect("list").id;
    let id = add(
        &mut w.ctl.store,
        list,
        ItemKind::Template,
        "Echo",
        "Got: {clipboard}",
    );
    assert_eq!(
        w.ctl.use_item(id, UseMode::Copy),
        UseResult::Done(UseOutcome::Copied)
    );
    assert_eq!(
        w.ctl.services.clipboard.read_text().as_deref(),
        Some("Got: from the clipboard")
    );
}

pub(super) fn make_sensitive(w: &mut World, id: ItemId) {
    let mut item = w.ctl.store.item(id).expect("item");
    item.sensitive = true;
    item.preview.clear();
    w.ctl.store.update_item(&item, None).expect("update");
}

#[test]
fn the_item_menu_offers_paste_as_and_pastes_the_chosen_transform() {
    use snipdeck_core::template::transform::Transform;
    let mut w = world();
    w.ctl.open_item_menu(0, false);
    let entries = w.ctl.view().item_menu.expect("menu");
    let at = entries
        .iter()
        .position(|e| *e == ItemMenuEntry::PasteAs)
        .expect("a Paste as row");
    assert_eq!(entries[at - 1], ItemMenuEntry::Copy, "right after Copy");
    // The row opens the choices in the menu's place; nothing is pasted yet.
    w.ctl.choose_item_menu(at);
    let view = w.ctl.view();
    assert!(view.item_menu.is_none());
    let choices = view.choose_menu.expect("choices");
    let labels: Vec<_> = choices.entries.iter().map(|e| e.label.clone()).collect();
    let want: Vec<_> = Transform::ALL
        .iter()
        .map(|t| ChooseLabel::Transform(*t))
        .collect();
    assert_eq!(labels, want);
    assert_eq!(w.ctl.services.clipboard.written.len(), 0);
    // Row 1 is UPPERCASE.
    w.ctl.choose_item_menu(1);
    assert_eq!(
        w.ctl.services.clipboard.written[0].data(),
        [Data::Text("FIRST".into())]
    );
    assert!(w.ctl.view().choose_menu.is_none());
}

#[test]
fn a_template_with_questions_asks_first_then_pastes_as_the_transform() {
    use snipdeck_core::template::transform::Transform;
    let mut w = world();
    let list = w.ctl.model.list().expect("list").id;
    let id = add(
        &mut w.ctl.store,
        list,
        ItemKind::Template,
        "Greeting",
        "hello {field:Name}",
    );
    assert_eq!(
        w.ctl.use_item_as(id, Transform::Title),
        UseResult::NeedsAnswers
    );
    assert_eq!(w.ctl.services.clipboard.written.len(), 0);
    assert_eq!(
        w.ctl.submit_fill(&["ada lovelace".to_owned()]),
        UseResult::Done(UseOutcome::Pasted)
    );
    assert_eq!(
        w.ctl.services.clipboard.written[0].data(),
        [Data::Text("Hello Ada Lovelace".into())]
    );
}

#[test]
fn a_template_field_a_choice_and_a_date_offset_all_go_through_the_transform() {
    use snipdeck_core::template::transform::Transform;
    let mut w = world();
    let list = w.ctl.model.list().expect("list").id;
    let id = add(
        &mut w.ctl.store,
        list,
        ItemKind::Template,
        "Ticket",
        "{field:Subject} {choice:Tone|Warm|Firm} {date+1d:%Y-%m-%d}",
    );
    assert_eq!(
        w.ctl.use_item_as(id, Transform::Slug),
        UseResult::NeedsAnswers
    );
    assert_eq!(
        w.ctl
            .submit_fill(&["Café Menu".to_owned(), "Firm".to_owned()]),
        UseResult::Done(UseOutcome::Pasted)
    );
    // The clock is frozen at the fixture's day; the transform sees the rendered text.
    let Data::Text(text) = &w.ctl.services.clipboard.written[0].data()[0] else {
        panic!("plain text")
    };
    assert!(text.starts_with("café-menu-firm-"), "{text:?}");
    assert!(!text.contains(' ') && !text.contains('{'), "{text:?}");
}

#[test]
fn paste_as_on_a_sensitive_item_stays_private() {
    use snipdeck_core::template::transform::Transform;
    let mut w = world();
    let id = w.ids[0];
    make_sensitive(&mut w, id);
    w.ctl.use_item_as(id, Transform::Upper);
    let written = &w.ctl.services.clipboard.written[0];
    assert!(written.is_private());
    assert_eq!(written.data(), [Data::Text("FIRST".into())]);
}

#[test]
fn a_copied_sensitive_item_leaves_the_clipboard_after_thirty_seconds() {
    let mut w = world();
    let id = w.ids[0];
    make_sensitive(&mut w, id);
    w.ctl.activate_row(0, UseMode::Copy);
    assert_eq!(w.ctl.clipboard_clear_wait(), Some(Duration::from_secs(30)));
    w.clock.set(at("2026-10-17T10:30:29Z"));
    w.ctl.clear_clipboard_if_due();
    assert_eq!(
        w.ctl.services.clipboard.read_text().as_deref(),
        Some("first"),
        "not yet"
    );
    w.clock.set(at("2026-10-17T10:30:30Z"));
    w.ctl.clear_clipboard_if_due();
    assert_eq!(w.ctl.services.clipboard.read_text(), None);
    assert_eq!(w.ctl.clipboard_clear_wait(), None);
}

#[test]
fn the_clipboard_clear_time_is_the_setting_and_zero_turns_it_off() {
    let mut w = world();
    let id = w.ids[0];
    make_sensitive(&mut w, id);
    w.ctl.update_settings(|s| s.clipboard_clear_secs = 10);
    w.ctl.activate_row(0, UseMode::Copy);
    assert_eq!(w.ctl.clipboard_clear_wait(), Some(Duration::from_secs(10)));
    w.clock.set(at("2026-10-17T10:30:10Z"));
    w.ctl.clear_clipboard_if_due();
    assert_eq!(w.ctl.services.clipboard.read_text(), None);

    w.ctl.update_settings(|s| s.clipboard_clear_secs = 0);
    w.ctl.activate_row(0, UseMode::Copy);
    assert_eq!(w.ctl.clipboard_clear_wait(), None, "0 = never clear");
}

#[test]
fn a_sensitive_paste_whose_restore_failed_still_arms_the_clear_timer() {
    let mut w = world();
    let id = w.ids[0];
    make_sensitive(&mut w, id);
    w.ctl.services.clipboard.fail_snapshot = true;
    assert_eq!(
        w.ctl.activate_row(0, UseMode::Paste),
        UseResult::Done(UseOutcome::PastedNotRestored)
    );
    assert_eq!(w.ctl.clipboard_clear_wait(), Some(Duration::from_secs(30)));
    w.clock.set(at("2026-10-17T10:30:30Z"));
    w.ctl.clear_clipboard_if_due();
    assert_eq!(w.ctl.services.clipboard.read_text(), None);
}

#[test]
fn a_sensitive_paste_that_restored_the_clipboard_arms_no_timer() {
    let mut w = world();
    let id = w.ids[0];
    make_sensitive(&mut w, id);
    assert_eq!(
        w.ctl.activate_row(0, UseMode::Paste),
        UseResult::Done(UseOutcome::Pasted)
    );
    assert_eq!(w.ctl.clipboard_clear_wait(), None);
}

#[test]
fn the_timer_empties_a_clipboard_with_no_text_if_nothing_changed_since_our_write() {
    use snipdeck_platform::payload::{Data, Payload};
    let mut w = world();
    let id = w.ids[0];
    make_sensitive(&mut w, id);
    w.ctl.activate_row(0, UseMode::Copy);
    // The clipboard now holds an image only (no text): the digest cannot match.
    w.ctl.services.clipboard.written.clear();
    w.ctl
        .services
        .clipboard
        .written
        .push(Payload::new(vec![Data::Png(vec![1, 2, 3])]).expect("payload"));
    let sequence = w.ctl.services.clipboard.sequence;
    w.ctl.clipboard_clear.as_mut().expect("armed").sequence = Some(sequence);
    w.clock.set(at("2026-10-17T10:30:30Z"));
    w.ctl.clear_clipboard_if_due();
    assert!(w.ctl.services.clipboard.written.is_empty(), "cleared");
}

#[test]
fn the_timer_leaves_a_clipboard_that_changed_since_our_write() {
    use snipdeck_platform::payload::{Data, Payload};
    let mut w = world();
    let id = w.ids[0];
    make_sensitive(&mut w, id);
    w.ctl.activate_row(0, UseMode::Copy);
    w.ctl.services.clipboard.written.clear();
    w.ctl
        .services
        .clipboard
        .written
        .push(Payload::new(vec![Data::Png(vec![1, 2, 3])]).expect("payload"));
    w.ctl.services.clipboard.sequence += 1; // someone else wrote it
    w.clock.set(at("2026-10-17T10:30:30Z"));
    w.ctl.clear_clipboard_if_due();
    assert_eq!(w.ctl.services.clipboard.written.len(), 1, "left alone");
}

#[test]
fn a_newer_copy_is_not_cleared_with_the_sensitive_one() {
    let mut w = world();
    let id = w.ids[0];
    make_sensitive(&mut w, id);
    w.ctl.activate_row(0, UseMode::Copy);
    w.ctl.activate_row(1, UseMode::Copy);
    w.clock.set(at("2026-10-17T10:31:00Z"));
    w.ctl.clear_clipboard_if_due();
    assert_eq!(
        w.ctl.services.clipboard.read_text().as_deref(),
        Some("second")
    );
}

#[test]
fn copy_only_leaves_the_item_on_the_clipboard_and_sends_no_keys() {
    let mut w = world();
    let result = w.ctl.activate_row(0, UseMode::Copy);
    assert_eq!(result, UseResult::Done(UseOutcome::Copied));
    assert_eq!(
        w.ctl.services().keyboard.sent,
        Vec::<Vec<snipdeck_platform::keys::KeyEvent>>::new()
    );
    assert_eq!(w.ctl.services().clipboard.log, ["persistent:1"]);
    assert_eq!(
        w.targets.refocus_count(),
        0,
        "a copy never touches the Target"
    );
    assert_eq!(uses(&w, w.ids[0]), 1);
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::Copied
    );
}

#[test]
fn quick_use_pastes_the_nth_row() {
    let mut w = world();
    assert_eq!(w.ctl.quick_use(3), UseResult::Done(UseOutcome::Pasted));
    assert_eq!(uses(&w, w.ids[2]), 1);
    assert_eq!(w.ctl.quick_use(9), UseResult::Failed, "no ninth row");
    assert_eq!(uses(&w, w.ids[0]), 0);
}

#[test]
fn with_no_target_a_click_copies_and_says_so() {
    let mut w = world_with(None, Settings::default());
    assert_eq!(w.ctl.view().footer, Footer::Copies);
    assert_eq!(
        w.ctl.activate_row(0, UseMode::Paste),
        UseResult::Done(UseOutcome::Copied)
    );
    assert_eq!(
        w.ctl.services().keyboard.sent,
        Vec::<Vec<snipdeck_platform::keys::KeyEvent>>::new()
    );
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::Copied
    );
}

#[test]
fn an_app_the_user_excluded_is_copied_into_not_pasted() {
    let settings = Settings {
        no_paste_apps: vec!["outlook.exe".into()],
        ..Settings::default()
    };
    let mut w = world_with(Some(outlook()), settings);
    assert_eq!(w.ctl.view().footer, Footer::Copies);
    w.ctl.activate_row(0, UseMode::Paste);
    assert_eq!(
        w.ctl.services().keyboard.sent,
        Vec::<Vec<snipdeck_platform::keys::KeyEvent>>::new()
    );
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::CopiedExcluded {
            app: "Outlook".into()
        }
    );
}

#[test]
fn a_failed_clipboard_is_a_danger_toast_and_not_counted() {
    let mut w = world();
    w.ctl.services.clipboard.fail_write = true;
    assert_eq!(w.ctl.activate_row(0, UseMode::Paste), UseResult::Failed);
    assert_eq!(uses(&w, w.ids[0]), 0);
    let toast = w.ctl.view().toast.expect("toast");
    assert_eq!(
        (toast.kind, toast.message),
        (ToastKind::Danger, ToastMessage::ClipboardBusy)
    );
}

#[test]
fn a_row_that_does_not_exist_is_ignored() {
    let mut w = world();
    assert_eq!(w.ctl.activate_row(40, UseMode::Paste), UseResult::Failed);
    assert_eq!(w.ctl.activate_pinned(0, UseMode::Paste), UseResult::Failed);
    assert!(w.ctl.view().toast.is_none());
}

#[test]
fn a_template_with_prompts_opens_the_fill_form_then_pastes_with_the_answers() {
    let mut w = world();
    let greeting = w.ids[3];
    assert_eq!(
        w.ctl.activate_row(3, UseMode::Paste),
        UseResult::NeedsAnswers
    );
    let view = w.ctl.view();
    let form = view.fill.expect("the form is open");
    assert_eq!(form.title, "Greeting");
    assert_eq!(form.fields.len(), 1);
    assert_eq!(form.fields[0].label, "Name");
    assert!(form.fields[0].required);
    assert_eq!(form.preview, "Hi ");
    assert_eq!(view.toast, None, "the form is the answer, not a toast");
    assert_eq!(uses(&w, greeting), 0);
    assert!(
        w.ctl.services().clipboard.log.is_empty(),
        "nothing touched the clipboard"
    );

    w.ctl.fill_changed(0, "Sam");
    assert_eq!(w.ctl.view().fill.expect("form").preview, "Hi Sam");
    assert_eq!(
        w.ctl.submit_fill(&["Sam".to_owned()]),
        UseResult::Done(UseOutcome::Pasted)
    );
    assert!(w.ctl.fill().is_none() && w.ctl.view().fill.is_none());
    assert_eq!(
        w.ctl.services().clipboard.written[0].data(),
        [Data::Text("Hi Sam".into())]
    );
    assert_eq!(uses(&w, greeting), 1);
}

#[test]
fn a_required_field_left_empty_keeps_the_form_open_and_pastes_nothing() {
    let mut w = world();
    w.ctl.activate_row(3, UseMode::Paste);
    assert_eq!(
        w.ctl.submit_fill(&["  ".to_owned()]),
        UseResult::NeedsAnswers
    );
    assert!(w.ctl.view().fill.is_some());
    assert_eq!(w.ctl.services().clipboard.log, Vec::<String>::new());
    assert_eq!(uses(&w, w.ids[3]), 0);
    // Fixing it works.
    assert_eq!(
        w.ctl.submit_fill(&["Ada".to_owned()]),
        UseResult::Done(UseOutcome::Pasted)
    );
}

#[test]
fn copying_a_template_asks_the_same_questions_and_copies() {
    let mut w = world();
    w.ctl.activate_row(3, UseMode::Copy);
    assert_eq!(
        w.ctl.submit_fill(&["Sam".to_owned()]),
        UseResult::Done(UseOutcome::Copied)
    );
    assert_eq!(w.ctl.services().keyboard.sent, Vec::<Vec<KeyEvent>>::new());
}

#[test]
fn cancelling_the_form_forgets_it_and_hands_the_focus_back_once() {
    let mut w = world();
    w.ctl.activate_row(3, UseMode::Paste);
    let refocused = w.targets.refocus_count();
    w.ctl.cancel_fill();
    assert!(w.ctl.view().fill.is_none());
    assert_eq!(w.targets.refocus_count(), refocused + 1);
    w.ctl.cancel_fill();
    assert_eq!(w.targets.refocus_count(), refocused + 1, "nothing was open");
    assert_eq!(
        w.ctl.submit_fill(&["Sam".to_owned()]),
        UseResult::NeedsAnswers
    );
    assert_eq!(uses(&w, w.ids[3]), 0);
}

#[test]
fn every_form_that_opens_is_a_new_one_for_the_window() {
    let mut w = world();
    w.ctl.activate_row(3, UseMode::Paste);
    let first = w.ctl.view().fill.expect("form").seq;
    w.ctl.fill_changed(0, "typed");
    assert_eq!(w.ctl.view().fill.expect("form").seq, first, "same form");
    w.ctl.cancel_fill();
    w.ctl.activate_row(3, UseMode::Paste);
    assert_ne!(w.ctl.view().fill.expect("form").seq, first);
    assert_eq!(w.ctl.view().fill.expect("form").fields[0].value, "");
}

#[test]
fn a_free_user_is_told_about_pro_instead_of_being_asked() {
    let mut w = build(Opts {
        tier: Tier::Free,
        ..Opts::default()
    });
    assert_eq!(w.ctl.activate_row(3, UseMode::Paste), UseResult::Failed);
    assert!(w.ctl.view().fill.is_none());
    let toast = w.ctl.view().toast.expect("toast");
    assert_eq!(
        (toast.kind, toast.message),
        (
            ToastKind::Danger,
            ToastMessage::NeedsPro(Feature::PromptedFields)
        )
    );
}

#[test]
fn a_program_rule_in_settings_decides_the_paste_key() {
    use crate::settings::{ProgramPaste, ProgramRule};
    use snipdeck_platform::keys::Modifier;
    let wt = Target {
        app: "wt.exe".into(),
        title: "PowerShell".into(),
        elevated: false,
        focus: Focus::Caret,
    };
    let rule = || ProgramRule {
        exe: "wt.exe".into(),
        paste: ProgramPaste::CtrlShiftV,
    };
    let shift_down = KeyEvent {
        key: Key::Modifier(Modifier::Shift),
        down: true,
    };

    // A rule changed while running (Settings > Programs).
    let mut w = world_with(Some(wt.clone()), Settings::default());
    w.ctl.activate_row(0, UseMode::Paste);
    assert!(
        !w.ctl.services().keyboard.sent[0].contains(&shift_down),
        "Ctrl+V by default: {:?}",
        w.ctl.services().keyboard.sent
    );
    w.ctl.update_settings(|s| s.programs.push(rule()));
    w.ctl.activate_row(0, UseMode::Paste);
    let sent = &w.ctl.services().keyboard.sent;
    assert!(
        sent[1].contains(&shift_down),
        "Ctrl+Shift+V after the rule: {sent:?}"
    );

    // A rule already in the settings file at start.
    let mut settings = Settings::default();
    settings.programs.push(rule());
    let mut w = world_with(Some(wt), settings);
    w.ctl.activate_row(0, UseMode::Paste);
    assert!(w.ctl.services().keyboard.sent[0].contains(&shift_down));
}

#[test]
fn a_template_reads_the_clipboard_only_when_it_says_clipboard() {
    let mut w = build(Opts {
        clipboard: Some("private"),
        ..Opts::default()
    });
    let list = w.ctl.model.list().expect("list").id;
    let plain = add(
        &mut w.ctl.store,
        list,
        ItemKind::Template,
        "Plain",
        "Hello {cursor}",
    );
    let echo = add(
        &mut w.ctl.store,
        list,
        ItemKind::Template,
        "Echo",
        "Got: {clipboard}",
    );
    let before = w.ctl.services.clipboard.text_reads;
    w.ctl.use_item(plain, UseMode::Copy);
    assert_eq!(
        w.ctl.services.clipboard.text_reads, before,
        "no {{clipboard}}, no read"
    );
    w.ctl.use_item(echo, UseMode::Copy);
    assert_eq!(w.ctl.services.clipboard.text_reads, before + 1);
}

#[test]
fn a_busy_clipboard_is_tried_again_until_the_secret_is_gone() {
    let mut w = world();
    let id = w.ids[0];
    make_sensitive(&mut w, id);
    w.ctl.activate_row(0, UseMode::Copy);
    w.ctl.services.clipboard.clear_busy = 2;
    w.clock.set(at("2026-10-17T10:30:30Z"));
    w.ctl.clear_clipboard_if_due();
    assert_eq!(
        w.ctl.services.clipboard.written.len(),
        1,
        "busy: still there"
    );
    assert_eq!(
        w.ctl.clipboard_clear_wait(),
        Some(CLIPBOARD_CLEAR_RETRY),
        "re-armed, not dropped"
    );
    w.clock.set(at("2026-10-17T10:30:31Z"));
    w.ctl.clear_clipboard_if_due();
    assert_eq!(w.ctl.services.clipboard.written.len(), 1, "busy again");
    w.clock.set(at("2026-10-17T10:30:32Z"));
    w.ctl.clear_clipboard_if_due();
    assert!(w.ctl.services.clipboard.written.is_empty(), "cleared");
    assert_eq!(w.ctl.clipboard_clear_wait(), None, "done");
}

#[test]
fn a_clipboard_that_stays_busy_is_given_up_on() {
    let mut w = world();
    let id = w.ids[0];
    make_sensitive(&mut w, id);
    w.ctl.activate_row(0, UseMode::Copy);
    w.ctl.services.clipboard.clear_busy = u32::MAX;
    let mut now = at("2026-10-17T10:30:30Z");
    for _ in 0..CLIPBOARD_CLEAR_ATTEMPTS {
        w.clock.set(now);
        w.ctl.clear_clipboard_if_due();
        now = now.checked_add(Duration::from_secs(1)).expect("later");
    }
    assert_eq!(w.ctl.clipboard_clear_wait(), None, "no timer left running");
}

#[test]
fn the_clear_compares_with_the_sequence_at_our_write_not_at_the_arming() {
    use snipdeck_platform::payload::{Data, Payload};
    let mut w = world();
    let id = w.ids[0];
    make_sensitive(&mut w, id);
    w.ctl.activate_row(0, UseMode::Copy);
    let at_write = w.ctl.services.clipboard.write_sequence;
    // Something else copied after our write and before the timer was armed would move the
    // current number; the pending clear keeps the number from our write.
    assert_eq!(
        w.ctl.clipboard_clear.as_ref().expect("armed").sequence,
        at_write
    );
    w.ctl.services.clipboard.written.clear();
    w.ctl
        .services
        .clipboard
        .written
        .push(Payload::new(vec![Data::Png(vec![9])]).expect("payload"));
    w.ctl.services.clipboard.sequence += 1;
    w.clock.set(at("2026-10-17T10:30:30Z"));
    w.ctl.clear_clipboard_if_due();
    assert_eq!(w.ctl.services.clipboard.written.len(), 1, "left alone");
}

#[test]
fn a_first_paste_whose_write_is_not_on_the_clipboard_injects_nothing_and_says_so() {
    let mut w = world();
    w.ctl.services.clipboard.never_confirm = true;
    assert_eq!(w.ctl.activate_row(0, UseMode::Paste), UseResult::Failed);
    assert!(
        w.ctl.services().keyboard.sent.is_empty(),
        "no Ctrl+V with stale clipboard content: {:?}",
        w.ctl.services().keyboard.sent
    );
    assert_eq!(uses(&w, w.ids[0]), 0, "not counted as a use");
    let toast = w.ctl.view().toast.expect("toast");
    assert_eq!(
        (toast.kind, toast.message),
        (ToastKind::Danger, ToastMessage::ClipboardBusy)
    );
}

#[test]
fn a_write_that_lands_late_still_pastes_the_right_item() {
    let mut w = world();
    w.ctl.services.clipboard.unconfirmed_polls = 4;
    assert_eq!(
        w.ctl.activate_row(1, UseMode::Paste),
        UseResult::Done(UseOutcome::Pasted)
    );
    assert_eq!(
        w.ctl.services().clipboard.written[0].data(),
        [Data::Text("second".into())]
    );
    assert_eq!(w.ctl.services().keyboard.sent.len(), 1);
}

// ---- Hide after use -------------------------------------------------------------------------

/// A world whose Panel is unpinned (a pinned one stays) and has been shown by the hotkey.
fn unpinned_and_shown() -> World {
    let mut w = build(Opts {
        pinned: Vec::new(),
        ..Opts::default()
    });
    w.ctl.set_on_top(false);
    w.ctl.on_hotkey(HotkeyAction::TogglePanel);
    w.ctl.on_hotkey(HotkeyAction::TogglePanel);
    assert!(w.ctl.visible());
    w
}

#[test]
fn an_unpinned_panel_hides_after_a_paste_or_a_copy() {
    for mode in [UseMode::Paste, UseMode::Copy] {
        let mut w = unpinned_and_shown();
        let used = w.ctl.activate_row(0, mode);
        assert!(matches!(used, UseResult::Done(_)));
        assert_eq!(w.ctl.after_use(&used), [Effect::HidePanel], "{mode:?}");
        assert!(!w.ctl.visible());
    }
}

#[test]
fn a_pinned_panel_stays_after_a_use() {
    let mut w = unpinned_and_shown();
    w.ctl.set_on_top(true);
    let used = w.ctl.activate_row(0, UseMode::Paste);
    assert_eq!(w.ctl.after_use(&used), Vec::new());
    assert!(w.ctl.visible());
}

#[test]
fn the_setting_off_keeps_the_panel_open_and_a_failed_use_never_hides() {
    let settings = Settings {
        panel_hide_after_use: false,
        ..Settings::default()
    };
    let mut w = world_with(Some(outlook()), settings);
    w.ctl.set_on_top(false);
    let used = w.ctl.activate_row(0, UseMode::Paste);
    assert_eq!(w.ctl.after_use(&used), Vec::new());

    let mut w = unpinned_and_shown();
    assert_eq!(w.ctl.after_use(&UseResult::Failed), Vec::new());
    assert_eq!(w.ctl.after_use(&UseResult::NeedsAnswers), Vec::new());
    assert!(w.ctl.visible());
}

// ---- Typed-trigger expansion (slice 5b) --------------------------------------------------------

use crate::typing_worker::Typed;
use snipdeck_core::{Fired, Trigger};
use snipdeck_store::TriggerRepo;

/// A world where typing expansion is on and item `n` has the trigger `code`.
fn typing_world(n: usize, code: &str) -> World {
    let mut w = world();
    w.ctl.update_settings(|s| s.expand_triggers = true);
    let trigger = Trigger::new(code).expect("trigger");
    w.ctl
        .store
        .set_trigger(w.ids[n], Some(&trigger))
        .expect("set trigger");
    w
}

/// What typing `text` makes the matcher fire.
fn typed(w: &World, text: &str) -> Fired {
    let mut matcher = w.ctl.trigger_matcher();
    text.chars()
        .find_map(|c| matcher.feed(c))
        .expect("the trigger fires")
}

fn backspaces_sent(w: &mut World) -> Vec<Vec<KeyEvent>> {
    w.ctl.services().keyboard.sent.clone()
}

#[test]
fn a_typed_trigger_backspaces_then_pastes() {
    let mut w = typing_world(1, ";two");
    let fired = typed(&w, ";two");
    assert_eq!((fired.item, fired.chars), (w.ids[1], 4));
    let out = w
        .ctl
        .expand_typed(&Typed::new(fired.clone(), false, || true));
    assert_eq!(
        out,
        TypedOutcome::Expanded(UseResult::Done(UseOutcome::Pasted))
    );
    let sent = backspaces_sent(&mut w);
    assert_eq!(sent.len(), 1, "one injection: erase and paste together");
    assert!(
        sent[0].starts_with(&snipdeck_platform::keys::backspaces(4)),
        "erased first"
    );
    assert!(
        sent[0].contains(&KeyEvent {
            key: Key::V,
            down: true
        }),
        "then the normal paste"
    );
    assert_eq!(
        w.ctl.services().clipboard.written[0].data(),
        [Data::Text("second".into())]
    );
    assert_eq!(uses(&w, w.ids[1]), 1);
}

#[test]
fn a_trigger_counts_graphemes_not_characters() {
    let mut w = typing_world(1, ";e\u{301}");
    let fired = typed(&w, ";e\u{301}");
    assert_eq!(fired.chars, 2);
    w.ctl
        .expand_typed(&Typed::new(fired.clone(), false, || true));
    assert!(backspaces_sent(&mut w)[0].starts_with(&snipdeck_platform::keys::backspaces(2)));
}

#[test]
fn never_paste_app_does_not_expand() {
    let mut w = typing_world(1, ";two");
    w.ctl.update_settings(|s| {
        s.programs = vec![crate::settings::ProgramRule {
            exe: "outlook.exe".into(),
            paste: crate::settings::ProgramPaste::Never,
        }];
    });
    let fired = typed(&w, ";two");
    assert_eq!(
        w.ctl
            .expand_typed(&Typed::new(fired.clone(), false, || true)),
        TypedOutcome::Skipped
    );
    assert_eq!(
        backspaces_sent(&mut w),
        Vec::<Vec<KeyEvent>>::new(),
        "nothing was typed into the app"
    );
    assert_eq!(uses(&w, w.ids[1]), 0);
}

#[test]
fn password_box_does_not_expand() {
    let mut w = typing_world(1, ";two");
    let fired = typed(&w, ";two");
    assert_eq!(
        w.ctl
            .expand_typed(&Typed::new(fired.clone(), true, || true)),
        TypedOutcome::Skipped
    );
    assert_eq!(backspaces_sent(&mut w), Vec::<Vec<KeyEvent>>::new());
    assert_eq!(uses(&w, w.ids[1]), 0);
}

#[test]
fn sensitive_item_never_expands() {
    let mut w = typing_world(1, ";two");
    let fired = typed(&w, ";two");
    let id = w.ids[1];
    make_sensitive(&mut w, id);
    assert_eq!(
        w.ctl
            .expand_typed(&Typed::new(fired.clone(), false, || true)),
        TypedOutcome::Skipped
    );
    assert_eq!(backspaces_sent(&mut w), Vec::<Vec<KeyEvent>>::new());
    let mut matcher = w.ctl.trigger_matcher();
    assert!(
        ";two".chars().all(|c| matcher.feed(c).is_none()),
        "and the matcher does not hold it"
    );
}

#[test]
fn no_expansion_without_a_paste_target_or_while_snipdeck_is_in_front() {
    let mut w = typing_world(1, ";two");
    let fired = typed(&w, ";two");
    w.ctl.set_own_window_active(true);
    assert_eq!(
        w.ctl
            .expand_typed(&Typed::new(fired.clone(), false, || true)),
        TypedOutcome::Skipped
    );
    w.ctl.set_own_window_active(false);
    w.targets.set(None);
    assert_eq!(
        w.ctl
            .expand_typed(&Typed::new(fired.clone(), false, || true)),
        TypedOutcome::Skipped
    );
    let mut elevated = outlook();
    elevated.elevated = true;
    w.targets.set(Some(elevated));
    assert_eq!(
        w.ctl
            .expand_typed(&Typed::new(fired.clone(), false, || true)),
        TypedOutcome::Skipped
    );
    assert_eq!(backspaces_sent(&mut w), Vec::<Vec<KeyEvent>>::new());
}

#[test]
fn an_item_trashed_since_the_matcher_was_built_does_not_expand() {
    let mut w = typing_world(1, ";two");
    let fired = typed(&w, ";two");
    w.ctl
        .store
        .trash(snipdeck_store::Trashable::Item(w.ids[1]), w.clock.now())
        .expect("trash");
    assert_eq!(
        w.ctl
            .expand_typed(&Typed::new(fired.clone(), false, || true)),
        TypedOutcome::Skipped
    );
    assert_eq!(backspaces_sent(&mut w), Vec::<Vec<KeyEvent>>::new());
}

#[test]
fn off_by_default_installs_no_hook() {
    let mut w = world();
    let typing = crate::host::FakeTyping::default();
    w.ctl.set_typing(Box::new(typing.clone()));
    assert!(!w.ctl.settings().expand_triggers);
    assert_eq!(typing.enabled_history(), vec![false], "told: off");
    // A trigger exists but the feature is off: still no hook, and a stray event does nothing.
    let trigger = Trigger::new(";two").expect("trigger");
    w.ctl
        .store
        .set_trigger(w.ids[1], Some(&trigger))
        .expect("set");
    let mut matcher = w.ctl.trigger_matcher();
    let fired = ";two".chars().find_map(|c| matcher.feed(c)).expect("fires");
    assert_eq!(
        w.ctl
            .expand_typed(&Typed::new(fired.clone(), false, || true)),
        TypedOutcome::Skipped
    );
    assert!(!typing.ever_enabled());
    w.ctl.update_settings(|s| s.expand_triggers = true);
    assert_eq!(typing.enabled_history().last(), Some(&true));
    w.ctl.update_settings(|s| s.expand_triggers = false);
    assert_eq!(typing.enabled_history().last(), Some(&false));
}

#[test]
fn a_prompted_trigger_opens_the_fill_form_and_erases_only_once_it_is_confirmed() {
    let mut w = typing_world(3, ";hi");
    let fired = typed(&w, ";hi");
    let out = w
        .ctl
        .expand_typed(&Typed::new(fired.clone(), false, || true));
    assert_eq!(out, TypedOutcome::Expanded(UseResult::NeedsAnswers));
    assert!(w.ctl.fill().is_some());
    assert_eq!(
        backspaces_sent(&mut w),
        Vec::<Vec<KeyEvent>>::new(),
        "the typed code stays until the form is confirmed"
    );
    assert_eq!(
        uses(&w, w.ids[3]),
        0,
        "not used until the form is submitted"
    );
    w.ctl.submit_fill(&["Sam".to_owned()]);
    let sent = backspaces_sent(&mut w);
    assert_eq!(sent.len(), 1, "erase and paste in one injection");
    assert!(sent[0].starts_with(&snipdeck_platform::keys::backspaces(3)));
    assert_eq!(uses(&w, w.ids[3]), 1);
}

#[test]
fn a_cancelled_fill_form_erases_nothing_and_later_fills_do_not_erase() {
    let mut w = typing_world(3, ";hi");
    let fired = typed(&w, ";hi");
    w.ctl.expand_typed(&Typed::new(fired, false, || true));
    w.ctl.cancel_fill();
    assert_eq!(backspaces_sent(&mut w), Vec::<Vec<KeyEvent>>::new());
    // The same template used by hand afterwards must not erase what the user never typed.
    assert_eq!(
        w.ctl.use_item(w.ids[3], UseMode::Paste),
        UseResult::NeedsAnswers
    );
    w.ctl.submit_fill(&["Sam".to_owned()]);
    let sent = backspaces_sent(&mut w);
    assert_eq!(sent.len(), 1);
    assert!(!sent[0].starts_with(&snipdeck_platform::keys::backspaces(1)));
}

#[test]
fn a_fill_form_confirmed_in_another_app_erases_nothing() {
    let mut w = typing_world(3, ";hi");
    let fired = typed(&w, ";hi");
    w.ctl.expand_typed(&Typed::new(fired, false, || true));
    let mut elsewhere = outlook();
    elsewhere.app = "other.exe".into();
    w.targets.set(Some(elsewhere));
    w.ctl.submit_fill(&["Sam".to_owned()]);
    for batch in backspaces_sent(&mut w) {
        assert!(!batch.starts_with(&snipdeck_platform::keys::backspaces(1)));
    }
}

#[test]
fn a_key_after_the_fire_erases_nothing() {
    let mut w = typing_world(1, ";two");
    let fired = typed(&w, ";two");
    let out = w.ctl.expand_typed(&Typed::new(fired, false, || false));
    assert_eq!(out, TypedOutcome::Skipped);
    assert_eq!(backspaces_sent(&mut w), Vec::<Vec<KeyEvent>>::new());
    assert_eq!(uses(&w, w.ids[1]), 0);
}

#[test]
fn a_key_typed_while_the_clipboard_is_confirmed_sends_nothing() {
    let mut w = typing_world(1, ";two");
    let fired = typed(&w, ";two");
    // Current when the controller first asks, stale by the time the keys are about to go out.
    let asked = std::sync::atomic::AtomicUsize::new(0);
    let typed = Typed::new(fired, false, move || {
        asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0
    });
    let out = w.ctl.expand_typed(&typed);
    assert_eq!(out, TypedOutcome::Skipped);
    assert_eq!(backspaces_sent(&mut w), Vec::<Vec<KeyEvent>>::new());
    assert_eq!(uses(&w, w.ids[1]), 0, "not counted");
}

#[test]
fn another_window_in_front_before_the_keys_go_out_sends_nothing() {
    let mut w = typing_world(1, ";two");
    let fired = typed(&w, ";two");
    // The user switches window during the clipboard check: the flag is raised on the second ask.
    let raised = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut other = outlook();
    other.app = "other.exe".into();
    w.targets.switch_when(std::sync::Arc::clone(&raised), other);
    let asked = std::sync::atomic::AtomicUsize::new(0);
    let typed = Typed::new(fired, false, move || {
        if asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst) > 0 {
            raised.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        true
    });
    assert_eq!(w.ctl.expand_typed(&typed), TypedOutcome::Skipped);
    assert_eq!(backspaces_sent(&mut w), Vec::<Vec<KeyEvent>>::new());
}

#[test]
fn a_failed_clipboard_erases_nothing() {
    let mut w = typing_world(1, ";two");
    let fired = typed(&w, ";two");
    w.ctl.services.clipboard.fail_write = true;
    w.ctl.expand_typed(&Typed::new(fired, false, || true));
    assert_eq!(backspaces_sent(&mut w), Vec::<Vec<KeyEvent>>::new());
    assert_eq!(uses(&w, w.ids[1]), 0);
}

#[test]
fn a_trigger_changed_since_the_fire_does_not_expand() {
    let mut w = typing_world(1, ";two");
    let fired = typed(&w, ";two");
    let other = Trigger::new(";zzz").expect("trigger");
    w.ctl
        .store
        .set_trigger(w.ids[1], Some(&other))
        .expect("set");
    let out = w.ctl.expand_typed(&Typed::new(fired, false, || true));
    assert_eq!(out, TypedOutcome::Skipped);
    assert_eq!(backspaces_sent(&mut w), Vec::<Vec<KeyEvent>>::new());
}

#[test]
fn an_unpinned_panel_that_is_shown_does_not_stop_an_expansion_in_another_app() {
    let mut w = typing_world(1, ";two");
    let fired = typed(&w, ";two");
    w.ctl.on_top = false;
    w.ctl.show();
    assert!(!w.ctl.own_window_active);
    let out = w.ctl.expand_typed(&Typed::new(fired, false, || true));
    assert_eq!(
        out,
        TypedOutcome::Expanded(UseResult::Done(UseOutcome::Pasted))
    );
}

// ---- Suggest as you type (slice 6) ----------------------------------------------------------

use crate::controller::HintOutcome;
use snipdeck_platform::route::ScreenRect;

const CARET: ScreenRect = ScreenRect {
    x: 400,
    y: 300,
    w: 2,
    h: 18,
};

/// Suggestions on (expansion off), item 1 has the trigger `code`, and the Target has a caret.
fn suggest_world(code: &str) -> (World, crate::host::FakeTyping) {
    let mut w = world();
    let typing = crate::host::FakeTyping::default();
    w.ctl.set_typing(Box::new(typing.clone()));
    w.ctl.update_settings(|s| s.suggest_triggers = true);
    let trigger = Trigger::new(code).expect("trigger");
    w.ctl
        .store
        .set_trigger(w.ids[1], Some(&trigger))
        .expect("set trigger");
    w.targets.set_caret(Some(CARET));
    (w, typing)
}

/// What the worker would hand over once `chars` characters of the code are typed.
fn suggestion(w: &World, code: &str, chars: usize, password: bool) -> Typed {
    let fired = Fired {
        item: w.ids[1],
        trigger: Trigger::new(code).expect("trigger"),
        chars,
    };
    Typed::new(fired, password, || true)
}

#[test]
fn suggestions_alone_install_the_hook_and_hand_over_the_candidates() {
    let (mut w, typing) = suggest_world(";two");
    assert!(!w.ctl.settings().expand_triggers);
    assert_eq!(typing.enabled_history().last(), Some(&true), "hook is on");
    let _ = w.ctl.refresh_panel();
    let candidates = typing.candidates();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].item, w.ids[1]);
    w.ctl.update_settings(|s| s.suggest_triggers = false);
    assert_eq!(
        typing.enabled_history().last(),
        Some(&false),
        "and off again"
    );
    assert_eq!(backspaces_sent(&mut w), Vec::<Vec<KeyEvent>>::new());
}

#[test]
fn with_only_suggestions_on_a_complete_trigger_never_expands() {
    let (mut w, _) = suggest_world(";two");
    assert!(!w.ctl.settings().expand_triggers);
    let typed = suggestion(&w, ";two", 4, false);
    assert_eq!(w.ctl.expand_typed(&typed), TypedOutcome::Skipped);
    assert_eq!(backspaces_sent(&mut w), Vec::<Vec<KeyEvent>>::new());
}

#[test]
fn tab_inserts_and_backspaces_the_prefix() {
    let (mut w, _) = suggest_world(";two");
    let typed = suggestion(&w, ";two", 2, false);
    let out = w.ctl.accept_suggested(&typed);
    assert_eq!(
        out,
        TypedOutcome::Expanded(UseResult::Done(UseOutcome::Pasted))
    );
    let sent = backspaces_sent(&mut w);
    assert_eq!(sent.len(), 1, "erase and paste in one injection");
    assert!(sent[0].starts_with(&snipdeck_platform::keys::backspaces(2)));
    assert!(sent[0].contains(&KeyEvent {
        key: Key::V,
        down: true
    }));
    assert_eq!(uses(&w, w.ids[1]), 1);
}

#[test]
fn accepting_needs_the_suggestion_setting_and_the_same_gates() {
    let (mut w, _) = suggest_world(";two");
    let typed = suggestion(&w, ";two", 2, false);
    w.ctl.update_settings(|s| s.suggest_triggers = false);
    assert_eq!(w.ctl.accept_suggested(&typed), TypedOutcome::Skipped);
    w.ctl.update_settings(|s| s.suggest_triggers = true);
    // A password box, a changed trigger and a stale key all stop it.
    let in_password = suggestion(&w, ";two", 2, true);
    assert_eq!(w.ctl.accept_suggested(&in_password), TypedOutcome::Skipped);
    let stale = Typed::new(typed.fired.clone(), false, || false);
    assert_eq!(w.ctl.accept_suggested(&stale), TypedOutcome::Skipped);
    let other = Trigger::new(";zzz").expect("trigger");
    w.ctl
        .store
        .set_trigger(w.ids[1], Some(&other))
        .expect("set");
    assert_eq!(w.ctl.accept_suggested(&typed), TypedOutcome::Skipped);
    assert_eq!(backspaces_sent(&mut w), Vec::<Vec<KeyEvent>>::new());
}

#[test]
fn the_hint_names_the_item_at_the_caret() {
    let (mut w, _) = suggest_world(";two");
    let typed = suggestion(&w, ";two", 2, false);
    assert_eq!(
        w.ctl.suggest_typed(&typed),
        HintOutcome::Show {
            title: "Two".into(),
            caret: CARET
        }
    );
}

#[test]
fn no_caret_no_hint() {
    let (mut w, _) = suggest_world(";two");
    w.targets.set_caret(None);
    let typed = suggestion(&w, ";two", 2, false);
    assert_eq!(w.ctl.suggest_typed(&typed), HintOutcome::Hide);
}

#[test]
fn no_hint_in_a_password_box_when_off_or_with_snipdeck_in_front() {
    let (mut w, _) = suggest_world(";two");
    let typed = suggestion(&w, ";two", 2, false);
    assert_eq!(
        w.ctl.suggest_typed(&suggestion(&w, ";two", 2, true)),
        HintOutcome::Hide
    );
    w.ctl.set_own_window_active(true);
    assert_eq!(w.ctl.suggest_typed(&typed), HintOutcome::Hide);
    w.ctl.set_own_window_active(false);
    let mut elevated = outlook();
    elevated.elevated = true;
    w.targets.set(Some(elevated));
    assert_eq!(w.ctl.suggest_typed(&typed), HintOutcome::Hide);
    w.targets.set(Some(outlook()));
    w.ctl.update_settings(|s| s.suggest_triggers = false);
    assert_eq!(w.ctl.suggest_typed(&typed), HintOutcome::Hide);
}

#[test]
fn sensitive_items_are_never_suggested() {
    let (mut w, typing) = suggest_world(";two");
    let typed = suggestion(&w, ";two", 2, false);
    let id = w.ids[1];
    make_sensitive(&mut w, id);
    assert_eq!(w.ctl.suggest_typed(&typed), HintOutcome::Hide);
    assert_eq!(w.ctl.accept_suggested(&typed), TypedOutcome::Skipped);
    // And it is not among the candidates the worker matches prefixes against.
    let _ = w.ctl.refresh_panel();
    w.ctl.update_settings(|s| s.suggest_triggers = false);
    w.ctl.update_settings(|s| s.suggest_triggers = true);
    assert_eq!(typing.candidates(), Vec::new());
}

#[test]
fn the_ui_thread_arms_the_hint_flag_only_for_a_hint_it_shows() {
    let (mut w, typing) = suggest_world(";two");
    assert_eq!(
        typing.hint_up_history(),
        Vec::<u64>::new(),
        "nothing armed by itself"
    );
    w.ctl.hint_up(3);
    w.ctl.hint_down(3);
    w.ctl.hint_down(9);
    assert_eq!(typing.hint_up_history(), vec![3]);
    assert_eq!(typing.hint_history(), vec![3, 9]);
}

#[test]
fn a_skipped_expansion_is_reported_back_so_the_next_trigger_gets_its_boundary() {
    let mut w = typing_world(1, ";two");
    let typing = crate::host::FakeTyping::default();
    w.ctl.set_typing(Box::new(typing.clone()));
    let fired = typed(&w, ";two");
    // A Snipdeck window is in use: nothing is typed.
    w.ctl.set_own_window_active(true);
    let out = w
        .ctl
        .expand_typed(&Typed::new(fired.clone(), false, || true));
    assert_eq!(out, TypedOutcome::Skipped);
    assert_eq!(typing.not_expanded_history(), vec![fired.clone()]);
    // An expansion that goes out is not reported.
    w.ctl.set_own_window_active(false);
    let out = w.ctl.expand_typed(&Typed::new(fired, false, || true));
    assert!(matches!(out, TypedOutcome::Expanded(_)));
    assert_eq!(typing.not_expanded_history().len(), 1);
}

#[test]
fn a_window_that_is_no_longer_in_front_stops_an_expansion_and_a_hint() {
    let mut w = typing_world(1, ";two");
    let fired = typed(&w, ";two");
    let front = std::sync::Arc::new(std::sync::atomic::AtomicIsize::new(5));
    let seen = std::sync::Arc::clone(&front);
    let at_fire: crate::typing_worker::Foreground =
        std::sync::Arc::new(move || seen.load(std::sync::atomic::Ordering::SeqCst));
    let typed_in = |fired: Fired| Typed::new(fired, false, || true).in_window(5, at_fire.clone());
    // Another window (same app, a second Notepad) came to the front after the trigger.
    front.store(6, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        w.ctl.expand_typed(&typed_in(fired.clone())),
        TypedOutcome::Skipped
    );
    assert!(backspaces_sent(&mut w).is_empty(), "nothing was typed");
    front.store(5, std::sync::atomic::Ordering::SeqCst);
    assert!(matches!(
        w.ctl.expand_typed(&typed_in(fired)),
        TypedOutcome::Expanded(_)
    ));
}

fn tab_batch() -> Vec<KeyEvent> {
    [true, false]
        .map(|down| KeyEvent {
            key: snipdeck_platform::keys::Key::Tab,
            down,
        })
        .to_vec()
}

#[test]
fn a_swallowed_tab_that_cannot_insert_is_given_back_exactly_once() {
    let (mut w, _typing) = suggest_world(";two");
    w.ctl.update_settings(|s| s.suggest_triggers = true);
    let typed = suggestion(&w, ";two", 2, false);
    // A Snipdeck window is in use: the insertion is skipped, the Tab must not be lost.
    w.ctl.set_own_window_active(true);
    assert_eq!(w.ctl.accept_hinted(&typed, true), TypedOutcome::Skipped);
    assert_eq!(backspaces_sent(&mut w), vec![tab_batch()]);
}

#[test]
fn a_swallowed_tab_for_a_hint_not_on_screen_is_given_back_exactly_once() {
    let (mut w, _typing) = suggest_world(";two");
    let typed = suggestion(&w, ";two", 2, false);
    assert_eq!(w.ctl.accept_hinted(&typed, false), TypedOutcome::Skipped);
    assert_eq!(backspaces_sent(&mut w), vec![tab_batch()]);
}

#[test]
fn a_tab_that_inserts_is_not_given_back() {
    let (mut w, _typing) = suggest_world(";two");
    let typed = suggestion(&w, ";two", 2, false);
    let out = w.ctl.accept_hinted(&typed, true);
    assert!(matches!(out, TypedOutcome::Expanded(_)));
    assert!(
        !backspaces_sent(&mut w).contains(&tab_batch()),
        "no Tab after an insertion"
    );
}

#[test]
fn a_hint_from_another_worker_generation_is_stale() {
    let (w, typing) = suggest_world(";two");
    typing.set_generation(3);
    assert!(!w.ctl.hint_is_stale(3 << 32 | 1));
    assert!(w.ctl.hint_is_stale(2 << 32 | 1), "an older worker");
    assert!(
        w.ctl.hint_is_stale(4 << 32 | 1),
        "a newer one is not the running one either"
    );
}
