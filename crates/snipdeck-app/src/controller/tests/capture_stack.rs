//! Quick capture, stacks and paste next.

use super::*;

#[test]
fn quick_capture_saves_the_clipboard_text_at_the_top_of_the_shown_list() {
    let mut w = build(Opts {
        clipboard: Some("remember the milk"),
        ..Opts::default()
    });
    assert_eq!(w.ctl.on_hotkey(HotkeyAction::Capture), []);
    let view = w.ctl.view();
    assert_eq!(view.rows.len(), 5);
    assert_eq!(view.rows[0].preview, "remember the milk");
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
    // The list the Panel shows is the default: switch tabs and capture again.
    w.ctl.select_tab(1);
    w.ctl.on_hotkey(HotkeyAction::Capture);
    let view = w.ctl.view();
    assert_eq!(view.rows.len(), 2);
    assert_eq!(view.rows[0].preview, "remember the milk");
    assert_eq!(
        view.toast.expect("toast").message,
        ToastMessage::Saved {
            list: "Links".into()
        }
    );
}

#[test]
fn quick_capture_with_nothing_on_the_clipboard_saves_nothing() {
    let mut w = world();
    w.ctl.on_hotkey(HotkeyAction::Capture);
    assert_eq!(w.ctl.view().rows.len(), 4);
    let toast = w.ctl.view().toast.expect("toast");
    assert_eq!(
        (toast.kind, toast.message),
        (ToastKind::Neutral, ToastMessage::NothingToSave)
    );
}

#[test]
fn quick_capture_without_any_list_says_so() {
    let now = at("2026-10-17T09:00:00Z");
    let mut store = Store::open_in_memory().expect("store");
    let board = Board::new(Name::new("Empty").expect("n"), now);
    store.insert_board(&board, Placement::Last).expect("board");
    let mut clipboard = FakeClipboard::default();
    clipboard.written.push(
        snipdeck_platform::payload::Payload::new(vec![Data::Text("x".into())]).expect("payload"),
    );
    let mut ctl = Controller::new(
        store,
        Services {
            clipboard,
            keyboard: FakeKeyboard::default(),
            timing: FakeTiming::default(),
        },
        Box::new(FakeTargets::new(None)),
        Box::new(FixedClock::new(now)),
        Settings::default(),
        Entitlements::new(Tier::Pro),
    )
    .expect("controller");
    ctl.on_hotkey(HotkeyAction::Capture);
    assert_eq!(
        ctl.view().toast.expect("toast").message,
        ToastMessage::NoListToSave
    );
}

#[test]
fn a_marked_stack_is_pasted_in_the_order_it_was_marked_and_then_ends() {
    let mut w = world();
    w.ctl.toggle_stack_mark(w.ids[2]);
    w.ctl.toggle_stack_mark(w.ids[0]);
    w.ctl.toggle_stack_mark(w.ids[1]);
    w.ctl.toggle_stack_mark(w.ids[1]);
    let stack = w.ctl.view().stack.expect("marks show");
    assert_eq!((stack.count, stack.running), (2, false));

    w.ctl.start_stack();
    let stack = w.ctl.view().stack.expect("running");
    assert_eq!(
        (stack.running, stack.position, stack.next_title.as_str()),
        (true, 0, "Three")
    );

    w.ctl.paste_next();
    assert_eq!(w.ctl.view().stack.expect("running").position, 1);
    w.ctl.paste_next();
    assert_eq!(pasted(&w), ["third", "first"]);

    assert_eq!(w.ctl.paste_next(), None);
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::PasteNextFinished
    );
    assert_eq!(
        w.ctl.view().stack,
        None,
        "finishing the queue ends the stack"
    );
}

#[test]
fn ending_a_stack_forgets_the_marks_and_paste_next_goes_back_to_the_list() {
    let mut w = world();
    w.ctl.toggle_stack_mark(w.ids[2]);
    w.ctl.start_stack();
    w.ctl.end_stack();
    assert_eq!(w.ctl.view().stack, None);
    w.ctl.paste_next();
    assert_eq!(pasted(&w), ["first"]);
}

#[test]
fn starting_a_stack_is_a_pro_feature_and_needs_marks() {
    let mut w = build(Opts {
        tier: Tier::Free,
        ..Opts::default()
    });
    w.ctl.toggle_stack_mark(w.ids[0]);
    w.ctl.start_stack();
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::NeedsPro(Feature::PasteStack)
    );
    assert!(!w.ctl.view().stack.expect("marks stay").running);

    let mut w = world();
    w.ctl.start_stack();
    assert_eq!(w.ctl.view().stack, None, "nothing marked, nothing starts");
}

#[test]
fn the_item_menu_marks_and_unmarks_for_the_stack() {
    let mut w = world();
    let id = w.ids[0];
    assert!(
        w.ctl
            .item_menu_entries(id)
            .contains(&ItemMenuEntry::Stack { marked: false })
    );
    w.ctl.toggle_stack_mark(id);
    assert!(
        w.ctl
            .item_menu_entries(id)
            .contains(&ItemMenuEntry::Stack { marked: true })
    );
}

#[test]
fn paste_next_is_a_pro_feature() {
    let mut w = build(Opts {
        tier: Tier::Free,
        ..Opts::default()
    });
    w.ctl.on_hotkey(HotkeyAction::PasteNext);
    let toast = w.ctl.view().toast.expect("toast");
    assert_eq!(
        (toast.kind, toast.message),
        (
            ToastKind::Danger,
            ToastMessage::NeedsPro(Feature::PasteStack)
        )
    );
    assert_eq!(pasted(&w), Vec::<String>::new());
}

#[test]
fn paste_next_walks_the_shown_list_then_says_it_is_done_and_starts_again() {
    let mut w = world();
    for _ in 0..3 {
        assert_eq!(w.ctl.on_hotkey(HotkeyAction::PasteNext), []);
    }
    assert_eq!(pasted(&w), ["first", "second", "third"]);
    assert_eq!(uses(&w, w.ids[2]), 1);

    // The fourth is a template: the form opens, and answering it moves the run on.
    assert_eq!(w.ctl.paste_next(), Some(UseResult::NeedsAnswers));
    assert!(w.ctl.view().fill.is_some());
    assert_eq!(
        w.ctl.submit_fill(&["Sam".to_owned()]),
        UseResult::Done(UseOutcome::Pasted)
    );
    assert_eq!(pasted(&w).last().map(String::as_str), Some("Hi Sam"));

    assert_eq!(w.ctl.paste_next(), None);
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::PasteNextFinished
    );
    assert_eq!(
        w.ctl.paste_next(),
        Some(UseResult::Done(UseOutcome::Pasted))
    );
    assert_eq!(pasted(&w).last().map(String::as_str), Some("first"));
}

#[test]
fn paste_next_prefers_the_pinned_strip_and_reads_the_target_when_pressed() {
    let mut w = build(Opts {
        pinned: vec![1, 4],
        ..Opts::default()
    });
    // The window in front now is not the one the controller last heard about.
    w.targets.set(None);
    w.ctl.on_hotkey(HotkeyAction::PasteNext);
    assert!(
        w.ctl.services().keyboard.sent.is_empty(),
        "no Target now, so it is copied, not pasted"
    );
    w.targets.set(Some(outlook()));
    w.ctl.on_hotkey(HotkeyAction::PasteNext);
    assert_eq!(pasted(&w), ["second", "https://example.com"]);
    assert_eq!(w.ctl.services().keyboard.sent.len(), 1);
    assert_eq!(w.ctl.paste_next(), None);
}

#[test]
fn a_clipboard_that_is_busy_fails_the_paste_and_counts_nothing() {
    let mut w = build(Opts {
        clipboard_busy: true,
        ..Opts::default()
    });
    assert_eq!(w.ctl.paste_next(), Some(UseResult::Failed));
    assert_eq!(
        w.ctl.view().toast.expect("toast").message,
        ToastMessage::ClipboardBusy
    );
    assert_eq!(uses(&w, w.ids[0]), 0);
}
