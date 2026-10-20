//! Hiding with a hint, the toast timeline and hotkey problems.

use super::*;

#[test]
fn close_hides_with_a_tray_icon_and_drag_asks_the_host() {
    let mut w = world();
    with_tray(&mut w);
    assert_eq!(w.ctl.close(), [Effect::HidePanel]);
    assert!(!w.ctl.visible());
    assert_eq!(w.ctl.drag(), [Effect::DragWindow]);
    assert_eq!(w.ctl.show(), [Effect::ShowPanel]);
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
            chord: Some(Settings::default().hotkeys.toggle_panel)
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
