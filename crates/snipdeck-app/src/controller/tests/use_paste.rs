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

fn make_sensitive(w: &mut World, id: ItemId) {
    let mut item = w.ctl.store.item(id).expect("item");
    item.sensitive = true;
    item.preview.clear();
    w.ctl.store.update_item(&item, None).expect("update");
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
    assert_eq!(w.ctl.services.clipboard.written.len(), 1, "busy: still there");
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
