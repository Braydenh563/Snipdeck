//! Hiding with a hint, the toast timeline and hotkey problems.

use super::*;

#[test]
fn close_hides_with_a_tray_icon_and_show_brings_it_back() {
    let mut w = world();
    with_tray(&mut w);
    assert_eq!(w.ctl.close(), [Effect::HidePanel]);
    assert!(!w.ctl.visible());
    assert_eq!(w.ctl.show(), [Effect::ShowPanel]);
}

fn store_reminder(w: &World) -> bool {
    w.ctl
        .view()
        .toast
        .is_some_and(|t| t.message == ToastMessage::StoreDataReminder)
}

#[test]
fn the_store_build_is_reminded_once_that_uninstalling_removes_the_library() {
    let mut w = world();
    w.ctl.remind_store_data(true);
    assert!(store_reminder(&w));
    assert!(w.ctl.settings().store_data_reminded);
    w.ctl.show_toast(Toast {
        kind: ToastKind::Neutral,
        message: ToastMessage::Copied,
    });
    w.ctl.remind_store_data(true);
    assert!(!store_reminder(&w), "not a second time");
}

#[test]
fn other_builds_are_not_reminded() {
    let mut w = world();
    w.ctl.remind_store_data(false);
    assert!(!store_reminder(&w));
    assert!(!w.ctl.settings().store_data_reminded);
}

fn hide_hint(w: &World) -> Option<ToastMessage> {
    w.ctl
        .view()
        .toast
        .map(|t| t.message)
        .filter(|m| matches!(m, ToastMessage::PanelHidden { .. }))
}

#[test]
fn the_first_close_of_a_session_tells_how_to_get_the_panel_back() {
    let mut w = world();
    with_tray(&mut w);
    assert_eq!(w.ctl.close(), [Effect::HidePanel]);
    assert_eq!(
        hide_hint(&w),
        Some(ToastMessage::PanelHidden {
            chord: Some(Settings::new_install().hotkeys.toggle_panel)
        })
    );
    assert!(
        w.ctl.settings().hide_hint_shown,
        "remembered in the settings"
    );
}

#[test]
fn a_second_hide_says_nothing() {
    let mut w = world();
    with_tray(&mut w);
    w.ctl.close();
    let _ = w.ctl.show();
    w.ctl.show_toast(Toast {
        kind: ToastKind::Neutral,
        message: ToastMessage::Copied,
    });
    w.ctl.close();
    assert_eq!(hide_hint(&w), None);
}

#[test]
fn the_toggle_hotkey_and_tray_hide_say_nothing() {
    let mut w = world();
    with_tray(&mut w);
    assert_eq!(
        w.ctl.on_hotkey(HotkeyAction::TogglePanel),
        [Effect::HidePanel]
    );
    assert_eq!(hide_hint(&w), None);
    let _ = w.ctl.show();
    assert_eq!(w.ctl.on_tray(TrayEvent::TogglePanel), [Effect::HidePanel]);
    assert_eq!(hide_hint(&w), None);
    assert!(!w.ctl.settings().hide_hint_shown);
}

#[test]
fn without_a_toggle_chord_the_toast_names_only_the_tray() {
    let mut settings = Settings::default();
    settings.hotkeys.toggle_panel = String::new();
    let mut w = world_with(Some(outlook()), settings);
    with_tray(&mut w);
    w.ctl.close();
    assert_eq!(
        hide_hint(&w),
        Some(ToastMessage::PanelHidden { chord: None })
    );
}

#[test]
fn closing_without_a_tray_quits_and_shows_no_toast() {
    let mut w = world();
    assert_eq!(w.ctl.close(), [Effect::Quit]);
    assert_eq!(hide_hint(&w), None);
}

#[test]
fn a_toast_walks_enter_hold_exit_and_goes() {
    let mut w = world();
    w.ctl.show_toast(Toast {
        kind: ToastKind::Neutral,
        message: ToastMessage::Copied,
    });
    let seq = w.ctl.view().toast.expect("toast").seq;
    assert_eq!(w.ctl.view().toast.expect("toast").phase, ToastPhase::Enter);
    assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_HOLD));
    assert_eq!(w.ctl.view().toast.expect("toast").phase, ToastPhase::Hold);
    assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_EXIT));
    assert_eq!(w.ctl.view().toast.expect("toast").phase, ToastPhase::Exit);
    assert_eq!(w.ctl.toast_tick(seq), None);
    assert!(w.ctl.view().toast.is_none());
}

#[test]
fn a_toast_under_the_pointer_stays_and_goes_a_moment_after_the_pointer_leaves() {
    let mut w = world();
    w.ctl.delete_row(1);
    let seq = w.ctl.view().toast.expect("toast").seq;
    assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_UNDO_HOLD));
    w.ctl.toast_hover(true);
    // The hold is over, but the pointer is on the toast: it looks again, and again.
    assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_HOVER_RECHECK));
    assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_HOVER_RECHECK));
    let toast = w.ctl.view().toast.expect("still there");
    assert_eq!((toast.phase, toast.undo), (ToastPhase::Hold, true));
    // The pointer leaves: a fresh hold, then the fade-out as usual.
    w.ctl.toast_hover(false);
    assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_HOLD));
    assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_EXIT));
    assert_eq!(w.ctl.toast_tick(seq), None);
    assert!(w.ctl.view().toast.is_none());
}

#[test]
fn dismissing_a_toast_drops_it_and_its_undo_and_edit() {
    let mut w = world();
    w.ctl.delete_row(1);
    let seq = w.ctl.view().toast.expect("toast").seq;
    assert!(w.ctl.view().toast.is_some_and(|t| t.undo));
    w.ctl.toast_dismiss();
    assert!(w.ctl.view().toast.is_none(), "the toast is gone");
    assert_eq!(
        w.ctl.toast_tick(seq),
        None,
        "its timers have nothing left to do"
    );
    assert!(w.ctl.toast_edit().is_empty(), "no Edit behind it");
    let rows = w.ctl.view().rows.len();
    w.ctl.main_undo();
    assert_eq!(
        w.ctl.view().rows.len(),
        rows,
        "the Undo is gone with the toast"
    );
}

#[test]
fn a_hover_that_ends_before_the_hold_does_does_not_change_the_timing() {
    let mut w = world();
    w.ctl.delete_row(1);
    let seq = w.ctl.view().toast.expect("toast").seq;
    assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_UNDO_HOLD));
    w.ctl.toast_hover(true);
    w.ctl.toast_hover(false);
    assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_EXIT));
}

#[test]
fn a_pointer_leave_that_never_comes_cannot_keep_a_toast_for_good() {
    let mut w = world();
    w.ctl.delete_row(1);
    let seq = w.ctl.view().toast.expect("toast").seq;
    w.ctl.toast_tick(seq);
    w.ctl.toast_hover(true);
    for _ in 0..TOAST_HOVER_RECHECKS {
        assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_HOVER_RECHECK));
    }
    assert_eq!(
        w.ctl.toast_tick(seq),
        Some(TOAST_EXIT),
        "a minute is enough"
    );
}

#[test]
fn a_newer_toast_does_not_inherit_the_hover() {
    let mut w = world();
    w.ctl.delete_row(1);
    let old = w.ctl.view().toast.expect("toast").seq;
    w.ctl.toast_tick(old);
    w.ctl.toast_hover(true);
    w.ctl.show_toast(Toast {
        kind: ToastKind::Neutral,
        message: ToastMessage::Copied,
    });
    let seq = w.ctl.view().toast.expect("toast").seq;
    assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_HOLD));
    assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_EXIT));
}

#[test]
fn a_stale_tick_does_not_touch_a_newer_toast() {
    let mut w = world();
    w.ctl.show_toast(Toast {
        kind: ToastKind::Neutral,
        message: ToastMessage::Copied,
    });
    let old = w.ctl.view().toast.expect("toast").seq;
    w.ctl.show_toast(Toast {
        kind: ToastKind::Danger,
        message: ToastMessage::Failed,
    });
    assert_eq!(w.ctl.toast_tick(old), None);
    let now = w.ctl.view().toast.expect("toast");
    assert_eq!(now.phase, ToastPhase::Enter);
    assert_ne!(now.seq, old);
}

#[test]
fn hotkey_problems_show_a_toast_naming_the_chord() {
    let mut w = world();
    w.ctl.report_hotkey_problems(&[]);
    assert!(w.ctl.view().toast.is_none());
    w.ctl.report_hotkey_problems(&[crate::host::HotkeyProblem {
        action: HotkeyAction::Picker,
        chord: "Ctrl+Shift+Space".into(),
        failure: crate::host::HotkeyFailure::InUse,
    }]);
    let toast = w.ctl.view().toast.expect("toast");
    assert_eq!(toast.kind, ToastKind::Danger);
    assert_eq!(
        toast.message,
        ToastMessage::HotkeyInUse {
            chord: "Ctrl+Shift+Space".into()
        }
    );
}

#[test]
fn every_failed_hotkey_is_named_in_the_toast_and_kept_for_settings() {
    use crate::host::{HotkeyFailure, HotkeyProblem};
    let mut w = world();
    let problem = |action, chord: &str, failure| HotkeyProblem {
        action,
        chord: chord.into(),
        failure,
    };
    w.ctl.report_hotkey_problems(&[
        problem(
            HotkeyAction::TogglePanel,
            "Ctrl+Alt+Space",
            HotkeyFailure::InUse,
        ),
        problem(
            HotkeyAction::PasteNext,
            "Ctrl+Alt+V",
            HotkeyFailure::Invalid,
        ),
    ]);
    let toast = w.ctl.view().toast.expect("toast");
    assert_eq!(toast.kind, ToastKind::Danger);
    assert_eq!(
        toast.message,
        ToastMessage::HotkeysFailed {
            chords: "Ctrl+Alt+Space, Ctrl+Alt+V".into()
        }
    );
    assert_eq!(
        w.ctl
            .hotkey_problem(HotkeyAction::TogglePanel)
            .map(|p| p.failure),
        Some(HotkeyFailure::InUse)
    );
    assert_eq!(
        w.ctl
            .hotkey_problem(HotkeyAction::PasteNext)
            .map(|p| p.failure),
        Some(HotkeyFailure::Invalid)
    );
    assert_eq!(w.ctl.hotkey_problem(HotkeyAction::Picker), None);
    // A later registration that works for everything forgets them.
    w.ctl.report_hotkey_problems(&[]);
    assert_eq!(w.ctl.hotkey_problem(HotkeyAction::TogglePanel), None);
}

#[test]
fn a_hotkey_problem_is_worded_by_why_it_failed() {
    use crate::host::{HotkeyFailure, HotkeyProblem};
    let mut w = world();
    let mut message = |failure| {
        w.ctl.report_hotkey_problems(&[HotkeyProblem {
            action: HotkeyAction::Picker,
            chord: "F13".into(),
            failure,
        }]);
        w.ctl.view().toast.expect("toast").message
    };
    let chord = String::from("F13");
    assert_eq!(
        message(HotkeyFailure::Invalid),
        ToastMessage::HotkeyInvalid {
            chord: chord.clone()
        }
    );
    assert_eq!(
        message(HotkeyFailure::OwnAction),
        ToastMessage::HotkeyOwnAction {
            chord: chord.clone()
        }
    );
    // The Windows error code reaches the words.
    assert_eq!(
        message(HotkeyFailure::Refused(5)),
        ToastMessage::HotkeyRefused {
            chord: chord.clone(),
            code: 5
        }
    );
    assert_eq!(
        message(HotkeyFailure::Unavailable),
        ToastMessage::HotkeyUnavailable { chord }
    );
}

#[test]
fn the_dark_theme_setting_reaches_the_view() {
    let settings = Settings {
        theme: Theme::Dark,
        ..Settings::default()
    };
    assert!(world_with(None, settings).ctl.view().dark);
}

#[test]
fn an_error_toast_holds_longer_than_a_neutral_one() {
    let mut w = world();
    w.ctl.show_toast(Toast {
        kind: ToastKind::Danger,
        message: ToastMessage::NotSaved,
    });
    let seq = w.ctl.view().toast.expect("toast").seq;
    // The hold is the guard (nothing plain replaces the error in it) and the rest, 10 s together.
    assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_ERROR_GUARD));
    assert_eq!(
        w.ctl.toast_tick(seq),
        TOAST_ERROR_HOLD.checked_sub(TOAST_ERROR_GUARD)
    );
    assert!(TOAST_ERROR_HOLD >= TOAST_HOLD * 4);
    // Under the pointer it stays, and after the pointer leaves it gets a fresh error hold.
    w.ctl.toast_hover(true);
    assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_HOVER_RECHECK));
    w.ctl.toast_hover(false);
    assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_ERROR_HOLD));
    assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_EXIT));
}

#[test]
fn a_refused_move_or_reorder_names_its_cause() {
    use std::io::{Error as Io, ErrorKind};
    let toast = change_failed(&snipdeck_store::Error::Io(Io::from(ErrorKind::StorageFull)));
    assert_eq!(
        (toast.kind, toast.message),
        (
            ToastKind::Danger,
            ToastMessage::ChangeFailed(crate::failure::Cause::DiskFull)
        )
    );
    assert_eq!(
        change_failed(&snipdeck_store::Error::Corrupt("x".into())).message,
        ToastMessage::ChangeFailed(crate::failure::Cause::Other)
    );
}

#[test]
fn pasting_a_picture_into_a_text_field_says_so_instead_of_doing_nothing() {
    use snipdeck_platform::payload::{Data, Payload};
    let mut w = world();
    let png = Payload::new(vec![Data::Png(vec![1, 2, 3])]).expect("payload");
    w.ctl.services.clipboard.written.push(png);
    assert!(w.ctl.refuse_pasted_image());
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::ImageNotPasteable
    );
}

#[test]
fn pasting_text_into_a_text_field_is_left_to_the_field() {
    use snipdeck_platform::payload::{Data, Payload};
    let mut w = world();
    assert!(!w.ctl.refuse_pasted_image(), "an empty clipboard: no toast");
    let text = Payload::new(vec![Data::Text("hi".into())]).expect("payload");
    w.ctl.services.clipboard.written.push(text);
    assert!(!w.ctl.refuse_pasted_image());
    assert!(w.ctl.view().toast.is_none());
}

// ---- The single toast slot and the notices around it ---------------------------------------

use super::recent::{copied, recent_world};
use crate::controller::panel_ctl::FAIL_REFRESH;

fn shown(w: &World) -> Option<ToastMessage> {
    w.ctl.view().toast.map(|t| t.message)
}

fn success(message: ToastMessage) -> Toast {
    Toast {
        kind: ToastKind::Success,
        message,
    }
}

#[test]
fn the_latest_toast_wins_across_actions() {
    let mut w = world();
    // A paste failed, the retry worked: the user sees "Pasted" once the error has had its guard
    // time (MAIN-43), not the old error for good.
    w.ctl.show_toast(failed());
    w.ctl.show_toast(success(ToastMessage::Pasted {
        app: "Outlook".into(),
    }));
    assert_eq!(
        shown(&w),
        Some(ToastMessage::Failed),
        "the error is guarded"
    );
    let seq = w.ctl.view().toast.expect("toast").seq;
    w.ctl.toast_tick(seq);
    w.ctl.toast_tick(seq);
    assert_eq!(
        shown(&w),
        Some(ToastMessage::Pasted {
            app: "Outlook".into()
        })
    );
    // Settings and reorder confirmations take over from an error the same way.
    w.ctl.show_toast(failed());
    w.ctl.show_toast(neutral(ToastMessage::PanelOnTop(true)));
    let seq = w.ctl.view().toast.expect("toast").seq;
    w.ctl.toast_tick(seq);
    w.ctl.toast_tick(seq);
    assert_eq!(shown(&w), Some(ToastMessage::PanelOnTop(true)));
    // An error replaces a success.
    w.ctl.show_toast(failed());
    assert_eq!(shown(&w), Some(ToastMessage::Failed));
}

#[test]
fn a_retry_that_works_replaces_the_error_of_the_failed_try() {
    let mut w = world();
    w.ctl.show_toast(failed());
    let before = w.ctl.view().toast.expect("toast").seq;
    let result = w.ctl.activate_row(0, UseMode::Paste);
    assert_eq!(result, UseResult::Done(UseOutcome::Pasted));
    // The error keeps the screen for its guard time, then the retry's toast takes over.
    w.ctl.toast_tick(before);
    w.ctl.toast_tick(before);
    let now = w.ctl.view().toast.expect("toast");
    assert_ne!(now.seq, before, "a new toast");
    assert!(matches!(now.message, ToastMessage::Pasted { .. }));
}

#[test]
fn an_operation_whose_refresh_fails_ends_with_the_not_refreshed_notice() {
    for which in 0..3 {
        let mut w = world();
        let id = w.ids[0];
        FAIL_REFRESH.set(true);
        match which {
            0 => w.ctl.pin_row(0),
            1 => w.ctl.duplicate_row(0),
            _ => w.ctl.trash_items(&[id]),
        }
        FAIL_REFRESH.set(false);
        assert_eq!(
            shown(&w),
            Some(ToastMessage::NotRefreshed),
            "action {which}: the change worked, so its success comes first and the notice last"
        );
    }
}

#[test]
fn a_keyboard_reorder_whose_refresh_fails_ends_with_the_not_refreshed_notice() {
    let mut w = world();
    // A first move works and says where the row went; the second one's refresh fails.
    w.ctl.panel_move_selected(1);
    assert!(matches!(shown(&w), Some(ToastMessage::ItemPlaced { .. })));
    FAIL_REFRESH.set(true);
    w.ctl.panel_move_selected(1);
    FAIL_REFRESH.set(false);
    assert_eq!(
        shown(&w),
        Some(ToastMessage::NotRefreshed),
        "the move worked but the list on screen is stale: never \"moved to position N\""
    );
}

#[test]
fn a_toast_with_an_undo_button_always_shows() {
    let mut w = world();
    w.ctl.show_toast(failed());
    w.ctl
        .show_undo_toast(success(ToastMessage::ItemsTrashed { count: 1 }), Some(7));
    assert_eq!(shown(&w), Some(ToastMessage::ItemsTrashed { count: 1 }));
}

#[test]
fn a_failed_refresh_after_a_paste_keeps_the_success() {
    let mut w = world();
    FAIL_REFRESH.set(true);
    let result = w.ctl.activate_row(0, UseMode::Paste);
    FAIL_REFRESH.set(false);
    assert_eq!(result, UseResult::Done(UseOutcome::Pasted));
    assert_eq!(
        shown(&w),
        Some(ToastMessage::Pasted {
            app: "Outlook".into()
        }),
        "the paste worked; the refresh error is only logged"
    );
}

#[test]
fn a_failed_refresh_after_a_quick_save_keeps_the_saved_toast() {
    let mut w = world();
    FAIL_REFRESH.set(true);
    w.ctl.save_text(Some("something to keep"));
    FAIL_REFRESH.set(false);
    assert!(matches!(shown(&w), Some(ToastMessage::Saved { .. })));
}

#[test]
fn a_refresh_that_fails_after_a_change_says_the_view_is_stale() {
    let mut w = world();
    assert!(w.ctl.refresh_or_notice("a test"));
    assert_eq!(shown(&w), None);
    FAIL_REFRESH.set(true);
    assert!(!w.ctl.refresh_or_notice("a test"));
    FAIL_REFRESH.set(false);
    assert_eq!(shown(&w), Some(ToastMessage::NotRefreshed));
    assert_eq!(w.ctl.view().toast.map(|t| t.kind), Some(ToastKind::Danger));
}

#[test]
fn locking_the_library_with_a_failed_refresh_says_the_view_is_stale() {
    let mut w = world();
    w.ctl.lock.set = true;
    FAIL_REFRESH.set(true);
    w.ctl.lock_now();
    FAIL_REFRESH.set(false);
    assert_eq!(shown(&w), Some(ToastMessage::NotRefreshed));
}

// ---- Confirmations (feedback audit gaps 1-6) -------------------------------------------------

#[test]
fn pinning_and_unpinning_in_the_panel_say_so() {
    let mut w = world();
    w.ctl.pin_row(0);
    assert_eq!(shown(&w), Some(ToastMessage::Pinned(true)));
    assert_eq!(w.ctl.view().toast.map(|t| t.kind), Some(ToastKind::Success));
    w.ctl.pin_row(0);
    assert_eq!(shown(&w), Some(ToastMessage::Pinned(false)));
}

#[test]
fn duplicating_in_the_panel_says_so() {
    let mut w = world();
    w.ctl.duplicate_row(0);
    assert_eq!(shown(&w), Some(ToastMessage::Duplicated));
}

#[test]
fn a_panel_drag_reorder_announces_the_new_place_like_the_keyboard_does() {
    let mut w = world();
    w.ctl.panel_drop_row(0, 2);
    assert_eq!(
        shown(&w),
        Some(ToastMessage::ItemPlaced {
            position: 3,
            total: 4
        })
    );
}

#[test]
fn adding_a_list_with_the_plus_tab_says_so() {
    let mut w = world();
    w.ctl.add_list();
    assert_eq!(shown(&w), Some(ToastMessage::ListAdded));
}

#[test]
fn deleting_and_clearing_recent_entries_say_so() {
    let mut w = recent_world();
    w.ctl.on_clipboard_change(&copied("a"));
    w.ctl.on_clipboard_change(&copied("b"));
    let first = w.ctl.recent_entries()[0].id;
    w.ctl.recent_delete(first);
    assert_eq!(shown(&w), Some(ToastMessage::RecentDeleted));
    assert_eq!(w.ctl.recent_clear(), 1);
    assert_eq!(shown(&w), Some(ToastMessage::RecentCleared));
}

#[test]
fn clearing_picker_recents_empties_emoji_and_colours_and_says_so() {
    let mut w = world();
    w.ctl.update_settings(|s| {
        s.colours_recent = vec![snipdeck_colour::RecentColour {
            hex: "#3a7bd5".into(),
            used_at: 1,
        }];
        s.symbols_recent = vec![snipdeck_symbols::RecentEntry {
            id: "emoji:grinning".into(),
            used_at: 1,
        }];
    });
    w.ctl.picker_recents_clear();
    assert_eq!(w.ctl.settings().colours_recent, []);
    assert_eq!(w.ctl.settings().symbols_recent, []);
    assert_eq!(shown(&w), Some(ToastMessage::PickerRecentsCleared));
}

#[test]
fn docking_and_floating_again_say_which() {
    let mut w = world();
    w.ctl.frame_changed(
        &display(),
        Rect {
            x: 600,
            y: 100,
            w: 320,
            h: 500,
        },
    );
    w.ctl.dock_to(Some(crate::settings::DockEdge::Left));
    assert_eq!(
        shown(&w),
        Some(ToastMessage::DockSet(Some(crate::settings::DockEdge::Left)))
    );
    w.ctl.dock_to(None);
    assert_eq!(shown(&w), Some(ToastMessage::DockSet(None)));
}

// ---- Hotkey health: "press it now" -------------------------------------------------------

#[test]
fn a_hotkey_is_ready_until_the_test_hears_it_and_the_press_does_nothing_else() {
    use crate::hotkey_health::HotkeyHealth;
    let mut w = world();
    assert_eq!(
        w.ctl.hotkey_health(HotkeyAction::TogglePanel),
        HotkeyHealth::Ready
    );
    assert_eq!(
        w.ctl.hotkey_health(HotkeyAction::Capture),
        HotkeyHealth::Off,
        "capture has no chord until the user sets one"
    );
    assert!(w.ctl.visible());
    w.ctl.start_hotkey_test(HotkeyAction::TogglePanel);
    assert_eq!(w.ctl.hotkey_test(), Some(HotkeyAction::TogglePanel));
    // Another shortcut is not the one asked for: it works as usual.
    assert_eq!(w.ctl.on_hotkey(HotkeyAction::Picker), []);
    assert_eq!(
        w.ctl.hotkey_health(HotkeyAction::TogglePanel),
        HotkeyHealth::Ready
    );
    // The asked-for one is heard and swallowed: the Panel stays.
    assert_eq!(w.ctl.on_hotkey(HotkeyAction::TogglePanel), []);
    assert!(w.ctl.visible(), "the test press did not hide the Panel");
    assert_eq!(w.ctl.hotkey_test(), None);
    assert_eq!(
        w.ctl.hotkey_health(HotkeyAction::TogglePanel),
        HotkeyHealth::Heard
    );
    // Back to normal afterwards.
    assert_eq!(
        w.ctl.on_hotkey(HotkeyAction::TogglePanel),
        [Effect::HidePanel]
    );
    // Testing again forgets the result.
    w.ctl.start_hotkey_test(HotkeyAction::TogglePanel);
    assert_eq!(
        w.ctl.hotkey_health(HotkeyAction::TogglePanel),
        HotkeyHealth::Ready
    );
    w.ctl.stop_hotkey_test();
    assert_eq!(w.ctl.hotkey_test(), None);
}

#[test]
fn a_failed_or_paused_hotkey_cannot_be_heard() {
    use crate::{
        host::{HotkeyFailure, HotkeyProblem},
        hotkey_health::HotkeyHealth,
    };
    let mut w = world();
    w.ctl.report_hotkey_problems(&[HotkeyProblem {
        action: HotkeyAction::Picker,
        chord: "Ctrl+Shift+Space".into(),
        failure: HotkeyFailure::InUse,
    }]);
    assert_eq!(
        w.ctl.hotkey_health(HotkeyAction::Picker),
        HotkeyHealth::Failed(HotkeyFailure::InUse)
    );
    w.ctl.on_tray(TrayEvent::HotkeysPaused(true));
    assert_eq!(
        w.ctl.hotkey_health(HotkeyAction::TogglePanel),
        HotkeyHealth::Paused
    );
    // A press queued before the pause is ignored, even during a test.
    w.ctl.start_hotkey_test(HotkeyAction::TogglePanel);
    assert_eq!(w.ctl.on_hotkey(HotkeyAction::TogglePanel), []);
    assert_eq!(w.ctl.hotkey_test(), Some(HotkeyAction::TogglePanel));
}

#[test]
fn pausing_the_hotkeys_ends_a_running_test_so_a_press_after_resume_is_not_swallowed() {
    for pause in [
        |w: &mut World, on: bool| {
            w.ctl.on_tray(TrayEvent::HotkeysPaused(on));
        },
        |w: &mut World, _on: bool| {
            // The Panel menu's "Pause hotkeys" toggles.
            let at = crate::controller::APP_MENU
                .iter()
                .position(|e| *e == crate::controller::AppMenuEntry::PauseHotkeys)
                .expect("pause row");
            w.ctl.choose_app_menu(at);
        },
    ] {
        let mut w = world();
        assert!(w.ctl.visible());
        w.ctl.start_hotkey_test(HotkeyAction::TogglePanel);
        pause(&mut w, true);
        assert_eq!(w.ctl.hotkey_test(), None, "the pause ended the test");
        pause(&mut w, false);
        assert_eq!(
            w.ctl.on_hotkey(HotkeyAction::TogglePanel),
            [Effect::HidePanel],
            "the press after the resume does its job"
        );
    }
}

fn danger(message: ToastMessage) -> Toast {
    Toast {
        kind: ToastKind::Danger,
        message,
    }
}

fn plain(message: ToastMessage) -> Toast {
    Toast {
        kind: ToastKind::Neutral,
        message,
    }
}

/// MAIN-43: a long message is held for the time it takes to read; a short one keeps its hold.
#[test]
fn a_long_message_is_held_by_its_length() {
    use crate::controller::{TOAST_READ_BASE, TOAST_READ_MAX, TOAST_READ_PER_CHAR};
    let mut w = world();
    w.ctl.show_toast(plain(ToastMessage::Copied));
    let seq = w.ctl.view().toast.expect("toast").seq;
    assert!(w.ctl.claim_toast_timer(seq, 6));
    assert_eq!(
        w.ctl.toast_tick(seq),
        Some(TOAST_HOLD),
        "six characters need less than the base hold"
    );
    w.ctl.show_toast(plain(ToastMessage::Copied));
    let seq = w.ctl.view().toast.expect("toast").seq;
    assert!(w.ctl.claim_toast_timer(seq, 100));
    assert_eq!(
        w.ctl.toast_tick(seq),
        Some(TOAST_READ_BASE + TOAST_READ_PER_CHAR * 100)
    );
    w.ctl.show_toast(plain(ToastMessage::Copied));
    let seq = w.ctl.view().toast.expect("toast").seq;
    assert!(w.ctl.claim_toast_timer(seq, 100_000));
    assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_READ_MAX), "capped");
}

/// MAIN-43: an error is not replaced at once by a plain message; the message shows once the
/// error has had its guard time, and the error stays readable in the history.
#[test]
fn a_plain_message_waits_behind_a_fresh_error() {
    use crate::controller::TOAST_ERROR_GUARD;
    let mut w = world();
    w.ctl.show_toast(danger(ToastMessage::ClipboardBusy));
    let error = w.ctl.view().toast.expect("error").seq;
    assert!(w.ctl.claim_toast_timer(error, 10));
    w.ctl.show_toast(plain(ToastMessage::Copied));
    assert_eq!(
        w.ctl.view().toast.expect("still the error").message,
        ToastMessage::ClipboardBusy
    );
    // The fade-in, then the guard.
    assert_eq!(w.ctl.toast_tick(error), Some(TOAST_ERROR_GUARD));
    assert_eq!(
        w.ctl.toast_tick(error),
        None,
        "the guard is over: the message takes over"
    );
    assert_eq!(
        w.ctl.view().toast.expect("the message").message,
        ToastMessage::Copied
    );
    assert_eq!(
        w.ctl.recent_toasts(),
        [ToastMessage::Copied, ToastMessage::ClipboardBusy],
        "newest first, the error still there"
    );
}

/// A message that waited behind a guarded error is not lost when the pointer rests on the error
/// through the whole hold.
#[test]
fn a_message_that_waited_is_shown_when_the_hovered_error_ends() {
    let mut w = world();
    w.ctl.show_toast(danger(ToastMessage::ClipboardBusy));
    let error = w.ctl.view().toast.expect("error").seq;
    assert!(w.ctl.claim_toast_timer(error, 10));
    w.ctl.show_toast(plain(ToastMessage::Copied));
    w.ctl.toast_hover(true);
    // Tick until the error leaves (bounded: the hover extension is finite).
    let mut reached_exit = false;
    for _ in 0..200 {
        let Some(shown) = w.ctl.view().toast.filter(|t| t.seq == error) else {
            break;
        };
        if shown.phase == ToastPhase::Exit {
            // The next tick is the exit itself: the message must still be waiting for it.
            assert!(w.ctl.pending_toast.is_some(), "queued until the exit");
            reached_exit = true;
        }
        w.ctl.toast_tick(error);
    }
    assert!(reached_exit, "the error went through its exit phase");
    assert_eq!(
        w.ctl.view().toast.expect("the message that waited").message,
        ToastMessage::Copied
    );
}

#[test]
fn a_newer_error_wins_and_a_message_after_the_guard_wins_again() {
    let mut w = world();
    w.ctl.show_toast(danger(ToastMessage::ClipboardBusy));
    w.ctl.show_toast(danger(ToastMessage::TemplateBroken));
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::TemplateBroken,
        "a newer error wins"
    );
    let seq = w.ctl.view().toast.expect("toast").seq;
    w.ctl.toast_tick(seq);
    // The guard ends with nothing waiting: the rest of the hold follows.
    assert!(w.ctl.toast_tick(seq).is_some());
    w.ctl.show_toast(plain(ToastMessage::Copied));
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::Copied,
        "past the guard the latest toast wins again"
    );
}

#[test]
fn dismissing_a_guarded_error_shows_the_message_that_waited() {
    let mut w = world();
    w.ctl.show_toast(danger(ToastMessage::ClipboardBusy));
    w.ctl.show_toast(plain(ToastMessage::Copied));
    w.ctl.toast_dismiss();
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::Copied
    );
}

#[test]
fn a_toast_with_undo_is_not_held_back_by_an_error() {
    let mut w = world();
    w.ctl.show_toast(danger(ToastMessage::ClipboardBusy));
    w.ctl.delete_row(1);
    let toast = w.ctl.view().toast.expect("toast");
    assert!(toast.undo, "the delete's Undo shows at once");
}

/// MAIN-43: the history keeps the last ten, newest first, without a repeat of the same words in a row.
#[test]
fn the_history_keeps_the_last_ten_messages() {
    let app = |n: u32| ToastMessage::Pasted {
        app: format!("App {n}"),
    };
    let mut w = world();
    for i in 0..12 {
        w.ctl.show_toast(plain(app(i)));
    }
    w.ctl.show_toast(plain(app(11)));
    let recent = w.ctl.recent_toasts();
    assert_eq!(recent.len(), 10);
    assert_eq!(recent[0], app(11));
    assert_eq!(recent[9], app(2));
}
