//! Searching, tabs, the board menu, the toggle hotkey and the Picker.

use super::*;

#[test]
fn searching_filters_rows_across_lists_and_resets_the_selection() {
    let mut w = world();
    w.ctl.move_selection(2);
    assert_eq!(w.ctl.view().selected, 2);
    w.ctl.set_query("docs");
    let view = w.ctl.view();
    assert_eq!(view.query, "docs");
    assert_eq!(view.rows.len(), 1);
    assert_eq!(view.rows[0].id, w.ids[4]);
    assert_eq!(view.selected, 0);
}

#[test]
fn picking_a_tab_shows_that_list_and_ends_the_search() {
    let mut w = world();
    w.ctl.set_query("one");
    w.ctl.select_tab(1);
    let view = w.ctl.view();
    assert_eq!(view.current_tab, 1);
    assert_eq!(view.query, "");
    assert_eq!(view.rows.len(), 1);
    assert_eq!(view.rows[0].id, w.ids[4]);
    w.ctl.select_tab(9);
    assert_eq!(
        w.ctl.view().current_tab,
        1,
        "an unknown tab changes nothing"
    );
    assert_eq!(w.ctl.model().list().map(|l| l.id), Some(w.links_list));
}

#[test]
fn the_board_menu_lists_boards_and_choosing_one_closes_it() {
    let mut w = world();
    w.ctl.open_board_menu();
    assert_eq!(w.ctl.view().board_menu, Some(vec!["Work".to_owned()]));
    w.ctl.choose_board(0);
    assert_eq!(w.ctl.view().board_menu, None);
    w.ctl.open_board_menu();
    w.ctl.close_board_menu();
    assert_eq!(w.ctl.view().board_menu, None);
}

#[test]
fn the_toggle_hotkey_hides_and_shows_and_rereads_the_target_on_show() {
    let mut w = world();
    assert!(w.ctl.visible());
    assert_eq!(
        w.ctl.on_hotkey(HotkeyAction::TogglePanel),
        [Effect::HidePanel]
    );
    assert!(!w.ctl.visible());
    w.targets.set(None);
    assert_eq!(
        w.ctl.on_hotkey(HotkeyAction::TogglePanel),
        [Effect::ShowPanel]
    );
    assert!(w.ctl.visible());
    assert_eq!(
        w.ctl.view().footer,
        Footer::Copies,
        "the Target was read on show"
    );
}

fn picker_titles(w: &World) -> Vec<String> {
    w.ctl
        .view()
        .picker
        .expect("the Picker is open")
        .rows
        .into_iter()
        .map(|r| r.title)
        .collect()
}

fn pasted(w: &World) -> Vec<String> {
    w.ctl
        .services()
        .clipboard
        .written
        .iter()
        .filter_map(|p| match &p.data()[0] {
            Data::Text(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_picker_hotkey_opens_the_picker_over_the_library_and_again_closes_it() {
    let mut w = world();
    assert!(w.ctl.view().picker.is_none());
    assert_eq!(w.ctl.on_hotkey(HotkeyAction::Picker), []);
    let picker = w.ctl.view().picker.expect("open");
    assert_eq!(picker.rows.len(), 5, "every item of every list");
    assert_eq!(
        (picker.scope, picker.query.as_str(), picker.selected),
        (Scope::All, "", 0)
    );
    assert_eq!(
        picker.footer,
        Footer::PastesInto {
            app: "Outlook".into(),
            title: "New message".into()
        }
    );
    assert!(w.ctl.visible(), "the Panel is not touched");
    assert_eq!(w.ctl.on_hotkey(HotkeyAction::Picker), []);
    assert!(w.ctl.view().picker.is_none());
}

#[test]
fn the_picker_searches_by_scope_and_moves_a_selection() {
    let mut w = build(Opts {
        pinned: vec![1, 4],
        ..Opts::default()
    });
    w.ctl.on_hotkey(HotkeyAction::Picker);
    w.ctl.picker_query("docs");
    assert_eq!(picker_titles(&w), ["Docs"]);
    w.ctl.picker_query("");
    w.ctl.picker_scope(Scope::Pinned);
    assert_eq!(picker_titles(&w), ["Two", "Docs"]);
    assert_eq!(w.ctl.view().picker.expect("open").scope, Scope::Pinned);
    w.ctl.picker_scope(Scope::Board);
    assert_eq!(
        picker_titles(&w).len(),
        5,
        "everything is on the Work board"
    );
    w.ctl.picker_move(2);
    assert_eq!(w.ctl.view().picker.expect("open").selected, 2);
    w.ctl.picker_query("o");
    assert_eq!(w.ctl.view().picker.expect("open").selected, 0);
    // With the Picker shut these are ignored.
    w.ctl.close_picker();
    w.ctl.picker_query("x");
    w.ctl.picker_scope(Scope::All);
    w.ctl.picker_move(1);
    assert!(w.ctl.view().picker.is_none());
}

#[test]
fn enter_in_the_picker_pastes_counts_the_use_and_closes_it() {
    let mut w = world();
    w.ctl.on_hotkey(HotkeyAction::Picker);
    w.ctl.picker_query("two");
    let refocused = w.targets.refocus_count();
    assert_eq!(
        w.ctl.picker_use(0, UseMode::Paste),
        UseResult::Done(UseOutcome::Pasted)
    );
    assert!(w.ctl.view().picker.is_none());
    assert_eq!(pasted(&w), ["second"]);
    assert_eq!(uses(&w, w.ids[1]), 1);
    assert_eq!(w.ctl.services().keyboard.sent.len(), 1, "Ctrl+V was sent");
    assert_eq!(w.targets.refocus_count(), refocused + 1);
    assert!(w.ctl.view().toast.is_some());
}

#[test]
fn shift_enter_in_the_picker_only_copies() {
    let mut w = world();
    w.ctl.on_hotkey(HotkeyAction::Picker);
    assert_eq!(
        w.ctl.picker_use(0, UseMode::Copy),
        UseResult::Done(UseOutcome::Copied)
    );
    assert_eq!(w.ctl.services().keyboard.sent, Vec::<Vec<KeyEvent>>::new());
    assert_eq!(pasted(&w), ["first"]);
}

#[test]
fn a_number_key_takes_the_nth_row_and_a_missing_row_leaves_the_picker_open() {
    let mut w = world();
    w.ctl.on_hotkey(HotkeyAction::Picker);
    assert_eq!(w.ctl.picker_take_number(10), None);
    assert_eq!(w.ctl.picker_take_number(0), None);
    assert!(w.ctl.view().picker.is_some());
    assert_eq!(w.ctl.picker_use(9, UseMode::Paste), UseResult::Failed);
    assert!(w.ctl.view().picker.is_some(), "no such row: still open");
    assert_eq!(w.ctl.picker_take_number(2), Some(w.ids[1]));
    assert!(w.ctl.view().picker.is_none());
    // The use itself is a separate call, after the host has hidden the window.
    assert_eq!(
        w.ctl.use_item(w.ids[1], UseMode::Paste),
        UseResult::Done(UseOutcome::Pasted)
    );
}

#[test]
fn a_template_chosen_in_the_picker_closes_it_and_opens_the_fill_form() {
    let mut w = world();
    w.ctl.on_hotkey(HotkeyAction::Picker);
    w.ctl.picker_query("greeting");
    assert_eq!(w.ctl.picker_use(0, UseMode::Paste), UseResult::NeedsAnswers);
    let view = w.ctl.view();
    assert!(view.picker.is_none());
    assert_eq!(view.fill.expect("form").title, "Greeting");
    // And the other way: summoning the Picker drops a form that was open.
    w.ctl.on_hotkey(HotkeyAction::Picker);
    assert!(w.ctl.view().fill.is_none() && w.ctl.view().picker.is_some());
}

#[test]
fn escape_closes_the_picker_and_hands_the_focus_back_once() {
    let mut w = world();
    w.ctl.on_hotkey(HotkeyAction::Picker);
    let refocused = w.targets.refocus_count();
    w.ctl.close_picker();
    assert!(w.ctl.view().picker.is_none());
    assert_eq!(w.targets.refocus_count(), refocused + 1);
    w.ctl.close_picker();
    assert_eq!(w.targets.refocus_count(), refocused + 1);
    assert_eq!(uses(&w, w.ids[0]), 0);
}

#[test]
fn the_target_is_read_when_the_picker_opens_and_kept_while_it_is_open() {
    let mut w = world();
    // The user moved to a window with nothing to paste into; the controller has not heard.
    w.targets.set(None);
    assert!(matches!(w.ctl.view().footer, Footer::PastesInto { .. }));
    w.ctl.on_hotkey(HotkeyAction::Picker);
    assert_eq!(w.ctl.view().picker.expect("open").footer, Footer::Copies);

    // The Picker itself takes the foreground: that must not become the Target.
    w.targets.set(Some(outlook()));
    w.ctl.on_target_changed();
    assert_eq!(w.ctl.view().picker.expect("open").footer, Footer::Copies);
    w.ctl.close_picker();
    w.ctl.on_target_changed();
    assert!(matches!(w.ctl.view().footer, Footer::PastesInto { .. }));
}
